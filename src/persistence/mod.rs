use crate::analytics::{fixed_review_dates, numeric_success};
use crate::domain::*;
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
mod analytics;
mod import;
mod study_numeric;
pub type Result<T> = std::result::Result<T, String>;
pub(super) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn default_db_path() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|p| p.join("ebbi/ebbi.db"))
        .ok_or_else(|| "Cannot locate the user data directory".into())
}
pub struct Database {
    conn: Connection,
    _lock: fs::File,
    path: PathBuf,
    dirty: bool,
    backup_date: Option<NaiveDate>,
}
impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if path.as_os_str().is_empty() || path == Path::new(":memory:") {
            return Err("A persistent database path is required".into());
        }
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(err)?;
        }
        if path.exists() {
            let existing = readonly(path)?;
            let version: i64 = existing
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .map_err(err)?;
            let tables: i64 = existing
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if version > 2 {
                return Err(format!(
                    "Database schema {version} is newer than this Ebbi version"
                ));
            }
            if tables > 0 {
                let app_id: i64 = existing
                    .pragma_query_value(None, "application_id", |r| r.get(0))
                    .map_err(err)?;
                if app_id != 1161970249 {
                    return Err("Not an Ebbi database; use legacy import instead".into());
                }
                if !(1..=2).contains(&version) {
                    return Err(
                        "Unrecognized database: use a new Ebbi database and import legacy data"
                            .into(),
                    );
                }
                let ledger: i64 = existing
                    .query_row("SELECT max(version) FROM schema_migrations", [], |r| {
                        r.get(0)
                    })
                    .map_err(err)?;
                if ledger != version {
                    return Err("Invalid Ebbi migration ledger".into());
                }
                for table in [
                    "habits",
                    "habit_rules",
                    "habit_entries",
                    "tasks",
                    "study_topics",
                    "study_reviews",
                    "review_reschedules",
                    "settings",
                    "legacy_imports",
                    "legacy_rows",
                ] {
                    existing
                        .prepare(&format!("SELECT * FROM {table} LIMIT 0"))
                        .map_err(err)?;
                }
                if version == 2 {
                    existing
                        .prepare("SELECT kind,unit FROM habits LIMIT 0")
                        .map_err(err)?;
                    existing
                        .prepare("SELECT completed_date FROM study_reviews LIMIT 0")
                        .map_err(err)?;
                    existing
                        .prepare("SELECT habit_id,effective_from,goal FROM habit_goals LIMIT 0")
                        .map_err(err)?;
                    existing
                        .prepare("SELECT habit_id,date,value FROM habit_values LIMIT 0")
                        .map_err(err)?;
                }
                validate(&existing)?;
            } else if version != 0 {
                return Err("Database has a version but no Ebbi schema".into());
            }
        }
        if !path.exists() {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .map_err(err)?;
        }
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(err)?;
        lock.try_lock()
            .map_err(|e| format!("Database is already in use, or cannot be locked: {e}"))?;
        let conn = Connection::open(path).map_err(err)?;
        conn.busy_timeout(Duration::from_secs(5)).map_err(err)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(err)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(err)?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(err)?;
        if version > 2 {
            return Err(format!(
                "Database schema {version} is newer than this Ebbi version"
            ));
        }
        let mut db = Self {
            conn,
            _lock: lock,
            path: path.to_path_buf(),
            dirty: true,
            backup_date: None,
        };
        if version == 0 {
            let tables: i64 = db
                .conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if tables != 0 {
                return Err(
                    "Unrecognized database: import legacy data into a new Ebbi database".into(),
                );
            }
            let tx = db.conn.transaction().map_err(err)?;
            tx.execute_batch(include_str!("schema.sql")).map_err(err)?;
            tx.commit().map_err(err)?;
        }
        if version == 1 {
            let backup = unique_path(path, "pre-v2");
            db.backup(&backup)?;
            let saved = readonly(&backup)?;
            let saved_version: i64 = saved
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .map_err(err)?;
            let identity = |conn: &Connection| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key='database_id'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map_err(err)
            };
            if saved_version != 1 || identity(&saved)? != identity(&db.conn)? {
                return Err("Pre-upgrade backup verification failed".into());
            }
        }
        if version <= 1 {
            let tx = db.conn.transaction().map_err(err)?;
            tx.execute_batch(include_str!("v2.sql")).map_err(err)?;
            validate(&tx)?;
            tx.commit().map_err(err)?;
        }
        let ledger: i64 = db
            .conn
            .query_row("SELECT max(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .map_err(err)?;
        if ledger != 2 {
            return Err("Schema migration ledger disagrees with user_version".into());
        }
        db.conn
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(err)?;
        db.conn
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(err)?;
        db.conn
            .pragma_update(None, "synchronous", "FULL")
            .map_err(err)?;
        Ok(db)
    }
    pub fn snapshot(&self, date: NaiveDate) -> Result<Snapshot> {
        let mut stmt=self.conn.prepare("SELECT h.id,h.name,h.position,h.archived, CASE WHEN ?1<h.tracked_from THEN 0 ELSE coalesce((SELECT active FROM habit_rules WHERE habit_id=h.id AND effective_from<=?1 ORDER BY effective_from DESC LIMIT 1),0) END, CASE WHEN h.unknown_before IS NOT NULL AND ?1<h.unknown_before THEN 0 ELSE 1 END, coalesce(e.completed,0) FROM habits h LEFT JOIN habit_entries e ON e.habit_id=h.id AND e.date=?1 ORDER BY h.position,h.id").map_err(err)?;
        let mut habits = stmt
            .query_map([date], |r| {
                Ok(Habit {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: HabitKind::Binary,
                    unit: String::new(),
                    goal: None,
                    next_goal: None,
                    value: None,
                    position: r.get(2)?,
                    archived: r.get(3)?,
                    eligible: r.get(4)?,
                    eligibility_known: r.get(5)?,
                    completed: r.get(6)?,
                })
            })
            .map_err(err)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(err)?;
        for habit in &mut habits {
            let (kind, unit): (String, String) = self
                .conn
                .query_row(
                    "SELECT kind,unit FROM habits WHERE id=?1",
                    [habit.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(err)?;
            habit.unit = unit;
            if kind == "numeric" {
                habit.kind = HabitKind::Numeric;
                habit.goal = study_numeric::goal_at(&self.conn, habit.id, date)?;
                let next: Option<(NaiveDate,String)> = self.conn.query_row("SELECT effective_from,goal FROM habit_goals WHERE habit_id=?1 AND effective_from>?2 ORDER BY effective_from LIMIT 1", params![habit.id,date], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
                habit.next_goal = next
                    .map(|(date, goal)| serde_json::from_str(&goal).map(|g| (date, g)).map_err(err))
                    .transpose()?;
                habit.value = self
                    .conn
                    .query_row(
                        "SELECT value FROM habit_values WHERE habit_id=?1 AND date=?2",
                        params![habit.id, date],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(err)?;
                habit.completed = habit
                    .goal
                    .as_ref()
                    .is_some_and(|goal| numeric_success(goal, habit.value));
            }
        }
        let mut stmt=self.conn.prepare("SELECT id,title,notes,due_date,completed_at,position,completed FROM tasks ORDER BY position,id").map_err(err)?;
        let tasks = stmt
            .query_map([], |r| {
                Ok(Todo {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    notes: r.get(2)?,
                    due_date: r.get(3)?,
                    completed_at: r.get(4)?,
                    position: r.get(5)?,
                    completed: r.get(6)?,
                })
            })
            .map_err(err)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(err)?;
        let (reviews_due,reviews_overdue)=self.conn.query_row("SELECT coalesce(sum(scheduled_date=?1),0),coalesce(sum(scheduled_date<?1),0) FROM study_reviews WHERE completed_at IS NULL",[date],|r|Ok((r.get::<_,usize>(0)?,r.get::<_,usize>(1)?))).map_err(err)?;
        let (study_topics, study_reviews) = study_numeric::study_snapshot(&self.conn)?;
        Ok(Snapshot {
            study_topics,
            study_reviews,
            habits,
            tasks,
            reviews_due,
            reviews_overdue,
        })
    }
    pub fn apply(&mut self, action: Action, today: NaiveDate) -> Result<()> {
        let tx = self.conn.transaction().map_err(err)?;
        let now = Utc::now().to_rfc3339();
        match action {
            Action::AddHabit(name) => {
                nonblank(&name)?;
                tx.execute("INSERT INTO habits(name,position,created_at,tracked_from) VALUES(?1,coalesce((SELECT max(position)+1 FROM habits),0),?2,?3)",params![name.trim(),now,today]).map_err(err)?;
                tx.execute(
                    "INSERT INTO habit_rules VALUES(?1,?2,1)",
                    params![tx.last_insert_rowid(), today],
                )
                .map_err(err)?;
            }
            Action::RenameHabit { id, name } => {
                nonblank(&name)?;
                changed(
                    tx.execute(
                        "UPDATE habits SET name=?1 WHERE id=?2",
                        params![name.trim(), id],
                    )
                    .map_err(err)?,
                )?;
            }
            Action::SetHabitArchived { id, archived } => {
                let tomorrow = today.succ_opt().ok_or("Date is out of range")?;
                changed(
                    tx.execute(
                        "UPDATE habits SET archived=?1 WHERE id=?2",
                        params![archived, id],
                    )
                    .map_err(err)?,
                )?;
                tx.execute("INSERT INTO habit_rules VALUES(?1,?2,?3) ON CONFLICT(habit_id,effective_from) DO UPDATE SET active=excluded.active",params![id,tomorrow,!archived]).map_err(err)?;
            }
            Action::SetHabitCompletion {
                id,
                date,
                completed,
            } => {
                if date > today {
                    return Err("Future habit entries are not allowed".into());
                }
                let kind: String = tx
                    .query_row("SELECT kind FROM habits WHERE id=?1", [id], |r| r.get(0))
                    .map_err(err)?;
                if kind != "binary" {
                    return Err("Numeric habits require a recorded value".into());
                }
                let eligible:bool=tx.query_row("SELECT ?2>=tracked_from AND (unknown_before IS NULL OR ?2>=unknown_before) AND coalesce((SELECT active FROM habit_rules WHERE habit_id=habits.id AND effective_from<=?2 ORDER BY effective_from DESC LIMIT 1),0) FROM habits WHERE id=?1",params![id,date],|r|r.get(0)).map_err(err)?;
                if !eligible {
                    return Err("Habit was not known to be eligible on that date".into());
                }
                tx.execute("INSERT INTO habit_entries VALUES(?1,?2,?3) ON CONFLICT(habit_id,date) DO UPDATE SET completed=excluded.completed",params![id,date,completed]).map_err(err)?;
            }
            Action::AddTask {
                title,
                notes,
                due_date,
            } => {
                nonblank(&title)?;
                tx.execute("INSERT INTO tasks(title,notes,due_date,created_at,position) VALUES(?1,?2,?3,?4,coalesce((SELECT max(position)+1 FROM tasks),0))",params![title.trim(),notes,due_date,now]).map_err(err)?;
            }
            Action::UpdateTask {
                id,
                title,
                notes,
                due_date,
            } => {
                nonblank(&title)?;
                changed(
                    tx.execute(
                        "UPDATE tasks SET title=?1,notes=?2,due_date=?3 WHERE id=?4",
                        params![title.trim(), notes, due_date, id],
                    )
                    .map_err(err)?,
                )?;
            }
            Action::SetTaskCompleted { id, completed } => {
                changed(tx.execute("UPDATE tasks SET completed=?1, completed_at=CASE WHEN ?1 AND completed=0 THEN ?2 WHEN ?1 THEN completed_at ELSE NULL END WHERE id=?3",params![completed,now,id]).map_err(err)?)?;
            }
            Action::DeleteTask(id) => {
                changed(
                    tx.execute("DELETE FROM tasks WHERE id=?1", [id])
                        .map_err(err)?,
                )?;
            }
            Action::MoveHabit { id, delta } => reorder(&tx, "habits", id, delta)?,
            Action::MoveTask { id, delta } => reorder(&tx, "tasks", id, delta)?,
            action => study_numeric::apply(&tx, action, today, &now)?,
        }
        tx.commit().map_err(err)?;
        self.dirty = true;
        Ok(())
    }
    pub fn backup(&self, target: &Path) -> Result<()> {
        backup_connection(&self.conn, target)
    }
    pub fn backup_if_due(&mut self, today: NaiveDate) -> Result<()> {
        if !self.dirty && self.backup_date == Some(today) {
            return Ok(());
        }
        let folder = self
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join(".ebbi-backups")
            .join(self.path.file_name().ok_or("Missing database filename")?);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&folder)
            .map_err(err)?;
        let rolling = folder.join("daily.db");
        let sidecars = |target: &Path| {
            ["-wal", "-shm", "-journal"].iter().any(|suffix| {
                let mut name = target.as_os_str().to_os_string();
                name.push(suffix);
                Path::new(&name).exists()
            })
        };
        if sidecars(&rolling) {
            return Err(
                "Daily backup has SQLite sidecars; close its readers before replacing it".into(),
            );
        }
        let mut previous_lock = None;
        if rolling.exists() {
            let info = fs::symlink_metadata(&rolling).map_err(err)?;
            if !info.is_file() || info.file_type().is_symlink() {
                return Err("Daily backup target is not a regular owned backup".into());
            }
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&rolling)
                .map_err(err)?;
            file.try_lock()
                .map_err(|_| "Daily backup is in use; it was not replaced".to_string())?;
            // Immutable inspection avoids creating WAL sidecars while checking a closed backup.
            use std::{fmt::Write, os::unix::ffi::OsStrExt};
            let mut uri = String::from("file:");
            for byte in rolling.as_os_str().as_bytes() {
                write!(&mut uri, "%{byte:02X}").map_err(err)?;
            }
            uri.push_str("?immutable=1");
            let previous = Connection::open_with_flags(
                uri,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
            )
            .map_err(err)?;
            let identity = |conn: &Connection| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key='database_id'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map_err(err)
            };
            if identity(&previous)? != identity(&self.conn)? {
                return Err("Daily backup belongs to another database; it was not replaced".into());
            }
            previous_lock = Some(file);
        }
        let pending = unique_path(&rolling, "pending");
        self.backup(&pending)?;
        if sidecars(&rolling) {
            return Err("Daily backup became active; it was not replaced".into());
        }
        fs::rename(&pending, &rolling).map_err(err)?;
        sync_parent(&rolling)?;
        drop(previous_lock);
        self.backup_date = Some(today);
        self.dirty = false;
        Ok(())
    }
    pub fn preview_import(&mut self, kind: LegacyKind, source: &Path) -> Result<ImportPreview> {
        import::preview(self, kind, source)
    }
    pub fn import_legacy(&mut self, preview: &ImportPreview) -> Result<String> {
        import::execute(self, preview)
    }
}
pub(super) fn nonblank(s: &str) -> Result<()> {
    if s.trim().is_empty() {
        Err("Name or title must not be blank".into())
    } else {
        Ok(())
    }
}
fn changed(n: usize) -> Result<()> {
    if n == 0 {
        Err("Record no longer exists".into())
    } else {
        Ok(())
    }
}
fn reorder(tx: &Connection, table: &str, id: i64, delta: i64) -> Result<()> {
    let mut stmt = tx
        .prepare(&format!("SELECT id FROM {table} ORDER BY position,id"))
        .map_err(err)?;
    let mut ids = stmt
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    let index = ids
        .iter()
        .position(|x| *x == id)
        .ok_or("Record no longer exists")?;
    let target = (index as i64)
        .saturating_add(delta)
        .clamp(0, ids.len() as i64 - 1) as usize;
    ids.remove(index);
    ids.insert(target, id);
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            &format!("UPDATE {table} SET position=?1 WHERE id=?2"),
            params![position as i64, id],
        )
        .map_err(err)?;
    }
    Ok(())
}
pub(super) fn readonly(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)
}
pub(super) fn validate(conn: &Connection) -> Result<()> {
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(err)?;
    if integrity != "ok" {
        return Err(format!("SQLite integrity check failed: {integrity}"));
    }
    if conn
        .prepare("PRAGMA foreign_key_check")
        .map_err(err)?
        .exists([])
        .map_err(err)?
    {
        return Err("SQLite foreign key check failed".into());
    }
    Ok(())
}
pub(super) fn unique_path(path: &Path, label: &str) -> PathBuf {
    path.with_extension(format!(
        "{label}-{}-{}.db",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ))
}
pub(super) fn backup_connection(source: &Connection, target: &Path) -> Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)
        .map_err(err)?;
    let result = (|| {
        let mut dest = Connection::open(target).map_err(err)?;
        let backup = rusqlite::backup::Backup::new(source, &mut dest).map_err(err)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if backup.step(128).map_err(err)? == rusqlite::backup::StepResult::Done {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err(
                    "SQLite backup timed out; try again after other database work finishes".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(backup);
        dest.pragma_update(None, "journal_mode", "DELETE")
            .map_err(err)?;
        validate(&dest)?;
        drop(dest);
        file.sync_all().map_err(err)?;
        sync_parent(target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(target);
    }
    result
}
fn sync_parent(path: &Path) -> Result<()> {
    fs::File::open(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
    .map_err(err)?
    .sync_all()
    .map_err(err)
}
