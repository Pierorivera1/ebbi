use super::*;
use rusqlite::types::ValueRef;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

type Row = Map<String, Value>;
struct Source {
    tables: Vec<(&'static str, Vec<Row>)>,
}
fn fingerprint(path: &Path, kind: LegacyKind) -> Result<String> {
    // SQLite backup rewrites header counters; hash canonical rows so copied snapshots deduplicate.
    let data = load(&readonly(path)?, kind)?;
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&data.tables).map_err(err)?)
    ))
}
fn kind_name(kind: LegacyKind) -> &'static str {
    match kind {
        LegacyKind::Habitodo => "habitodo",
        LegacyKind::Repasito => "repasito",
    }
}
fn table(conn: &Connection, name: &str) -> Result<Vec<Row>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT rowid AS __rowid__, * FROM {name} ORDER BY rowid"
        ))
        .map_err(err)?;
    let names = stmt
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let rows = stmt
        .query_map([], |r| {
            let mut map = Map::new();
            for (i, name) in names.iter().enumerate() {
                let value = match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => Value::from(v),
                    ValueRef::Real(v) => Value::from(v),
                    ValueRef::Text(v) => {
                        Value::String(String::from_utf8(v.to_vec()).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                i,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?)
                    }
                    ValueRef::Blob(v) => serde_json::json!({"blob":v}),
                };
                map.insert(name.clone(), value);
            }
            Ok(map)
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}
fn text<'a>(r: &'a Row, key: &str) -> Result<&'a str> {
    r.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing or invalid text column {key}"))
}
fn optional_text<'a>(r: &'a Row, key: &str) -> Result<Option<&'a str>> {
    match r.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(v)) => Ok(Some(v)),
        _ => Err(format!("Invalid text column {key}")),
    }
}
fn integer(r: &Row, key: &str) -> Result<i64> {
    r.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("Missing or invalid integer column {key}"))
}
fn flag(r: &Row, key: &str) -> Result<bool> {
    match integer(r, key)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(format!("Invalid boolean column {key}")),
    }
}
fn date(s: &str) -> Result<NaiveDate> {
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| format!("Invalid calendar date: {s}"))?;
    if d.to_string() != s {
        return Err(format!("Noncanonical calendar date: {s}"));
    }
    Ok(d)
}
fn timestamp(s: &str) -> Result<()> {
    if chrono::DateTime::parse_from_rfc3339(s).is_ok()
        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").is_ok()
        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").is_ok()
    {
        Ok(())
    } else {
        Err(format!("Invalid legacy timestamp: {s}"))
    }
}
fn habit_creation_date(value: &str) -> Result<Option<NaiveDate>> {
    if let Ok(instant) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(Some(instant.with_timezone(&chrono::Local).date_naive()));
    }
    // Habitodo's SQLite datetime('now') timestamps are UTC despite having no offset.
    if let Ok(utc) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(Some(
            utc.and_utc().with_timezone(&chrono::Local).date_naive(),
        ));
    }
    timestamp(value)?;
    Ok(None)
}
fn load(conn: &Connection, kind: LegacyKind) -> Result<Source> {
    validate(conn)?;
    let required: &[&'static str] = match kind {
        LegacyKind::Habitodo => &["habits", "habit_completions", "todos"],
        LegacyKind::Repasito => &["topics", "reviews"],
    };
    let mut tables = Vec::new();
    for name in required {
        tables.push((*name, table(conn, name)?));
    }
    if kind == LegacyKind::Habitodo
        && conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(err)?
            .is_some()
    {
        tables.push(("settings", table(conn, "settings")?));
    }
    let source = Source { tables };
    for (name, rows) in &source.tables {
        for r in rows {
            if *name != "habit_completions" && *name != "settings" && integer(r, "id")? <= 0 {
                return Err(format!("Invalid source ID in {name}"));
            }
            match *name {
                "habits" => {
                    nonblank(text(r, "name")?)?;
                    integer(r, "position")?;
                    flag(r, "archived")?;
                    timestamp(text(r, "created_at")?)?;
                    date(
                        text(r, "created_at")?
                            .get(..10)
                            .ok_or("Missing creation date")?,
                    )?;
                }
                "habit_completions" => {
                    integer(r, "habit_id")?;
                    date(text(r, "date")?)?;
                    flag(r, "completed")?;
                }
                "todos" => {
                    nonblank(text(r, "title")?)?;
                    optional_text(r, "notes")?;
                    integer(r, "position")?;
                    flag(r, "completed")?;
                    timestamp(text(r, "created_at")?)?;
                    if let Some(v) = optional_text(r, "due_date")? {
                        date(v)?;
                    }
                    if let Some(v) = optional_text(r, "completed_at")? {
                        timestamp(v)?;
                    }
                }
                "topics" => {
                    nonblank(text(r, "title")?)?;
                    optional_text(r, "notes")?;
                    timestamp(text(r, "created_at")?)?;
                    if let Some(v) = optional_text(r, "initial_date")?.filter(|v| !v.is_empty()) {
                        date(v)?;
                    }
                    let status = optional_text(r, "status")?.unwrap_or("active");
                    if !["active", "completed"].contains(&status) {
                        return Err(format!("Unsupported topic status {status}"));
                    }
                }
                "reviews" => {
                    integer(r, "topic_id")?;
                    if !(1..=3).contains(&integer(r, "review_stage")?) {
                        return Err("Review stage must be 1, 2, or 3".into());
                    }
                    date(text(r, "scheduled_date")?)?;
                    if let Some(v) = optional_text(r, "completed_at")? {
                        timestamp(v)?;
                    }
                }
                "settings" => {
                    text(r, "key")?;
                    text(r, "value")?;
                }
                _ => unreachable!(),
            }
        }
    }
    // Validate references even if a legacy database omitted its FK declarations.
    let (parents, children, key) = match kind {
        LegacyKind::Habitodo => ("habits", "habit_completions", "habit_id"),
        LegacyKind::Repasito => ("topics", "reviews", "topic_id"),
    };
    let ids = source
        .rows(parents)
        .iter()
        .map(|r| integer(r, "id"))
        .collect::<Result<Vec<_>>>()?;
    for r in source.rows(children) {
        if !ids.contains(&integer(r, key)?) {
            return Err(format!("Orphan legacy row in {children}"));
        }
    }
    Ok(source)
}
impl Source {
    fn rows(&self, table: &str) -> &[Row] {
        self.tables
            .iter()
            .find(|(name, _)| *name == table)
            .map(|(_, rows)| rows.as_slice())
            .unwrap_or(&[])
    }
}
pub(super) fn preview(db: &Database, kind: LegacyKind, path: &Path) -> Result<ImportPreview> {
    let source_path = fs::canonicalize(path).map_err(err)?;
    if source_path == fs::canonicalize(&db.path).map_err(err)? {
        return Err("Cannot import Ebbi into itself".into());
    }
    let source = readonly(&source_path)?;
    let snapshot_path = unique_path(&db.path, "legacy-snapshot");
    backup_connection(&source, &snapshot_path)?;
    let snapshot = readonly(&snapshot_path)?;
    let data = load(&snapshot, kind)?;
    let counts = data
        .tables
        .iter()
        .map(|(name, rows)| (name.to_string(), rows.len()))
        .collect();
    let mut warnings=match kind {
        LegacyKind::Habitodo=>vec!["Legacy archive dates are unknown; archived habits have unknown eligibility before import. Raw settings are preserved without activating legacy preferences.".into(),"A completed task without a timestamp retains its completion flag and an unknown completion timestamp.".into()],
        LegacyKind::Repasito=>vec!["Original review deadlines are unknown and excluded from exact on-time metrics. Current dates and raw topic status are preserved.".into()]
    };
    if kind == LegacyKind::Habitodo
        && data.rows("habits").iter().any(|r| {
            text(r, "created_at")
                .and_then(habit_creation_date)
                .is_ok_and(|date| date.is_none())
        })
    {
        warnings.push("Some habit creation timestamps have no known timezone provenance. Raw timestamps are preserved; historical eligibility remains unknown until import.".into());
    }
    Ok(ImportPreview {
        source_path,
        snapshot_path: snapshot_path.clone(),
        kind,
        fingerprint: fingerprint(&snapshot_path, kind)?,
        counts,
        warnings,
    })
}
pub(super) fn execute(db: &mut Database, preview: &ImportPreview) -> Result<String> {
    if fingerprint(&preview.snapshot_path, preview.kind)? != preview.fingerprint {
        return Err("Import snapshot changed; preview again".into());
    }
    let source_path = fs::canonicalize(&preview.source_path).map_err(err)?;
    if source_path == fs::canonicalize(&db.path).map_err(err)? {
        return Err("Cannot import Ebbi into itself".into());
    }
    // Re-snapshot the live source so WAL changes cannot slip past preview approval.
    let current = unique_path(&db.path, "source-check");
    backup_connection(&readonly(&source_path)?, &current)?;
    let current_hash = fingerprint(&current, preview.kind);
    fs::remove_file(&current).map_err(err)?;
    if current_hash? != preview.fingerprint {
        return Err("Legacy source changed since preview; preview again".into());
    }
    let data = load(&readonly(&preview.snapshot_path)?, preview.kind)?;
    let counts: Vec<_> = data
        .tables
        .iter()
        .map(|(name, rows)| (name.to_string(), rows.len()))
        .collect();
    if counts != preview.counts {
        return Err("Import preview counts do not match its snapshot".into());
    }
    let source_key = source_path
        .to_str()
        .ok_or("Source path is not valid UTF-8")?;
    let kind = kind_name(preview.kind);
    let previous: Option<String> = db
        .conn
        .query_row(
            "SELECT fingerprint FROM legacy_imports WHERE kind=?1 AND source_path=?2",
            params![kind, source_key],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    if let Some(hash) = previous {
        return if hash == preview.fingerprint {
            Ok("This source was already imported; no changes made".into())
        } else {
            Err("This source was imported previously and has changed; explicit reconciliation is required".into())
        };
    }
    let duplicate: bool = db
        .conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM legacy_imports WHERE kind=?1 AND fingerprint=?2)",
            params![kind, preview.fingerprint],
            |r| r.get(0),
        )
        .map_err(err)?;
    if duplicate {
        db.backup(&unique_path(&db.path, "pre-import"))?;
        db.conn
            .execute(
                "INSERT INTO legacy_imports(kind,source_path,fingerprint) VALUES(?1,?2,?3)",
                params![kind, source_key, preview.fingerprint],
            )
            .map_err(err)?;
        db.dirty = true;
        return Ok(
            "This snapshot was already imported; source alias recorded without duplicating records"
                .into(),
        );
    }
    db.backup(&unique_path(&db.path, "pre-import"))?;
    let tx = db.conn.transaction().map_err(err)?;
    tx.execute(
        "INSERT INTO legacy_imports(kind,source_path,fingerprint) VALUES(?1,?2,?3)",
        params![kind, source_key, preview.fingerprint],
    )
    .map_err(err)?;
    let batch = tx.last_insert_rowid();
    let observed = chrono::Local::now().date_naive();
    let mut ids = HashMap::new();
    for (name, rows) in &data.tables {
        for r in rows {
            let mut destination = None;
            match *name {
                "habits" => {
                    let archived = flag(r, "archived")?;
                    let created = text(r, "created_at")?;
                    let known_creation = habit_creation_date(created)?;
                    let from = known_creation.unwrap_or(observed);
                    let unknown_before = (archived || known_creation.is_none()).then_some(observed);
                    tx.execute("INSERT INTO habits(name,position,archived,created_at,tracked_from,unknown_before) VALUES(?1,?2,?3,?4,?5,?6)",params![text(r,"name")?,integer(r,"position")?,archived,created,from,unknown_before]).map_err(err)?;
                    let id = tx.last_insert_rowid();
                    ids.insert(integer(r, "id")?, id);
                    destination = Some(id);
                    tx.execute(
                        "INSERT INTO habit_rules VALUES(?1,?2,?3)",
                        params![id, if archived { observed } else { from }, !archived],
                    )
                    .map_err(err)?;
                }
                "habit_completions" => {
                    let id = ids
                        .get(&integer(r, "habit_id")?)
                        .ok_or("Missing imported habit")?;
                    tx.execute(
                        "INSERT INTO habit_entries VALUES(?1,?2,?3)",
                        params![id, text(r, "date")?, flag(r, "completed")?],
                    )
                    .map_err(err)?;
                    destination = Some(*id);
                }
                "todos" => {
                    let completed = flag(r, "completed")?;
                    let completion = optional_text(r, "completed_at")?;
                    tx.execute("INSERT INTO tasks(title,notes,due_date,completed_at,created_at,position,completed) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![text(r,"title")?,optional_text(r,"notes")?.unwrap_or(""),optional_text(r,"due_date")?,completion,text(r,"created_at")?,integer(r,"position")?,completed]).map_err(err)?;
                    destination = Some(tx.last_insert_rowid());
                }
                "topics" => {
                    tx.execute("INSERT INTO study_topics(title,notes,initial_date,created_at,status) VALUES(?1,?2,?3,?4,?5)",params![text(r,"title")?,optional_text(r,"notes")?.unwrap_or(""),optional_text(r,"initial_date")?.filter(|v|!v.is_empty()),text(r,"created_at")?,optional_text(r,"status")?.unwrap_or("active")]).map_err(err)?;
                    let id = tx.last_insert_rowid();
                    ids.insert(integer(r, "id")?, id);
                    destination = Some(id);
                }
                "reviews" => {
                    tx.execute("INSERT INTO study_reviews(topic_id,review_stage,scheduled_date,completed_at) VALUES(?1,?2,?3,?4)",params![ids.get(&integer(r,"topic_id")?).ok_or("Missing imported topic")?,integer(r,"review_stage")?,text(r,"scheduled_date")?,optional_text(r,"completed_at")?]).map_err(err)?;
                    destination = Some(tx.last_insert_rowid());
                }
                "settings" => {}
                _ => unreachable!(),
            }
            let source_id = if *name == "settings" {
                text(r, "key")?.to_owned()
            } else if *name == "habit_completions" {
                format!("{}:{}", integer(r, "habit_id")?, text(r, "date")?)
            } else {
                integer(r, "id")?.to_string()
            };
            tx.execute(
                "INSERT INTO legacy_rows VALUES(?1,?2,?3,?4,?5)",
                params![
                    batch,
                    name,
                    source_id,
                    destination,
                    serde_json::to_string(r).map_err(err)?
                ],
            )
            .map_err(err)?;
        }
    }
    let actual: usize = tx
        .query_row(
            "SELECT count(*) FROM legacy_rows WHERE batch_id=?1",
            [batch],
            |r| r.get(0),
        )
        .map_err(err)?;
    if actual != counts.iter().map(|(_, n)| *n).sum::<usize>() {
        return Err("Imported row count verification failed".into());
    }
    validate(&tx)?;
    tx.commit().map_err(err)?;
    db.dirty = true;
    Ok(format!(
        "Imported {actual} legacy records. Original source unchanged."
    ))
}
