use chrono::NaiveDate;
use ebbi::{
    domain::{Action, LegacyKind},
    persistence::Database,
};
use rusqlite::{params, Connection};
use std::{fs, path::Path};
fn day(s: &str) -> NaiveDate {
    s.parse().unwrap()
}
fn habitodo(path: &Path) -> Connection {
    let c = Connection::open(path).unwrap();
    c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE habits(id INTEGER PRIMARY KEY,name TEXT,position INTEGER,created_at TEXT,archived INTEGER);CREATE TABLE habit_completions(habit_id INTEGER,date TEXT,completed INTEGER);CREATE TABLE todos(id INTEGER PRIMARY KEY,title TEXT,notes TEXT,completed INTEGER,created_at TEXT,completed_at TEXT,due_date TEXT,position INTEGER); CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT); INSERT INTO habits VALUES(7,'Read',0,'2026-01-01 09:00:00',1);INSERT INTO habit_completions VALUES(7,'2026-01-02',1);INSERT INTO todos VALUES(4,'Task','Notes',1,'2026-01-01 09:00:00',NULL,'2026-01-05',0);INSERT INTO settings VALUES('theme','dark');").unwrap();
    c
}
#[test]
fn habit_deletion_removes_binary_and_numeric_history_and_preserves_other_data() {
    use ebbi::domain::NumericGoal;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("delete.db");
    let today = day("2026-10-19");
    let mut db = Database::open(&path).unwrap();
    db.apply(Action::AddHabit("Read".into()), today).unwrap();
    db.apply(
        Action::AddNumericHabit {
            name: "Walk".into(),
            unit: "minutes".into(),
            goal: NumericGoal::AtLeast(30.0),
        },
        today,
    )
    .unwrap();
    db.apply(Action::AddHabit("Keep".into()), today).unwrap();
    db.apply(
        Action::SetHabitCompletion {
            id: 1,
            date: today,
            completed: true,
        },
        today,
    )
    .unwrap();
    db.apply(
        Action::SetHabitValue {
            id: 2,
            date: today,
            value: Some(40.0),
        },
        today,
    )
    .unwrap();
    db.apply(
        Action::AddTask {
            title: "Keep task".into(),
            notes: String::new(),
            due_date: None,
        },
        today,
    )
    .unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER block_habit_delete BEFORE DELETE ON habits WHEN OLD.id=1 BEGIN SELECT RAISE(ABORT,'blocked'); END;").unwrap();
    assert!(db.apply(Action::DeleteHabit(1), today).is_err());
    assert!(db.snapshot(today).unwrap().habits[0].completed);
    conn.execute_batch("DROP TRIGGER block_habit_delete")
        .unwrap();
    drop(conn);
    db.apply(Action::DeleteHabit(1), today).unwrap();
    db.apply(Action::DeleteHabit(2), today).unwrap();
    assert!(db.apply(Action::DeleteHabit(999), today).is_err());
    drop(db);
    let db = Database::open(&path).unwrap();
    let snapshot = db.snapshot(today).unwrap();
    assert_eq!(snapshot.habits.len(), 1);
    assert_eq!(snapshot.habits[0].name, "Keep");
    assert_eq!(snapshot.tasks.len(), 1);
    let conn = Connection::open(&path).unwrap();
    for table in [
        "habit_entries",
        "habit_values",
        "habit_goals",
        "habit_rules",
    ] {
        let count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE habit_id IN (1,2)"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    assert!(conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query([])
        .unwrap()
        .next()
        .unwrap()
        .is_none());
}

#[test]
fn daily_edits_reopen_and_reorder() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("ebbi.db");
    let today = day("2026-10-15");
    let mut db = Database::open(&p).unwrap();
    db.apply(Action::AddHabit(" Read ".into()), today).unwrap();
    db.apply(Action::AddHabit("Walk".into()), today).unwrap();
    db.apply(Action::MoveHabit { id: 2, delta: -1 }, today)
        .unwrap();
    db.apply(
        Action::RenameHabit {
            id: 1,
            name: "Book".into(),
        },
        today,
    )
    .unwrap();
    db.apply(
        Action::SetHabitCompletion {
            id: 1,
            date: today,
            completed: true,
        },
        today,
    )
    .unwrap();
    db.apply(
        Action::AddTask {
            title: "Task".into(),
            notes: "N".into(),
            due_date: Some(today),
        },
        today,
    )
    .unwrap();
    db.apply(
        Action::SetTaskCompleted {
            id: 1,
            completed: true,
        },
        today,
    )
    .unwrap();
    drop(db);
    let db = Database::open(&p).unwrap();
    let s = db.snapshot(today).unwrap();
    assert_eq!(s.habits[0].id, 2);
    assert!(s.habits[1].completed);
    assert_eq!(s.habits[1].name, "Book");
    assert!(s.tasks[0].completed);
    assert!(s.tasks[0].completed_at.is_some());
    assert_eq!(s.tasks[0].notes, "N");
}
#[test]
fn archive_changes_tomorrow_and_preserves_history() {
    let t = tempfile::tempdir().unwrap();
    let mut db = Database::open(&t.path().join("e.db")).unwrap();
    let a = day("2026-10-15");
    let b = a.succ_opt().unwrap();
    let c = b.succ_opt().unwrap();
    db.apply(Action::AddHabit("Read".into()), a).unwrap();
    db.apply(
        Action::SetHabitCompletion {
            id: 1,
            date: a,
            completed: true,
        },
        a,
    )
    .unwrap();
    db.apply(
        Action::SetHabitArchived {
            id: 1,
            archived: true,
        },
        a,
    )
    .unwrap();
    assert!(db.snapshot(a).unwrap().habits[0].eligible);
    assert!(!db.snapshot(b).unwrap().habits[0].eligible);
    db.apply(
        Action::SetHabitArchived {
            id: 1,
            archived: false,
        },
        b,
    )
    .unwrap();
    assert!(!db.snapshot(b).unwrap().habits[0].eligible);
    assert!(db.snapshot(c).unwrap().habits[0].eligible);
    assert!(db.snapshot(a).unwrap().habits[0].completed);
}
#[test]
fn invalid_writes_and_unrelated_databases_are_rejected() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("e.db");
    let mut db = Database::open(&p).unwrap();
    let d = day("2026-10-15");
    assert!(db.apply(Action::AddHabit(" \n".into()), d).is_err());
    db.apply(Action::AddHabit("A".into()), d).unwrap();
    for date in [d.pred_opt().unwrap(), d.succ_opt().unwrap()] {
        assert!(db
            .apply(
                Action::SetHabitCompletion {
                    id: 1,
                    date,
                    completed: true
                },
                d
            )
            .is_err());
    }
    assert!(db
        .apply(
            Action::AddTask {
                title: " ".into(),
                notes: String::new(),
                due_date: None
            },
            d
        )
        .is_err());
    let legacy = t.path().join("legacy.db");
    let c = habitodo(&legacy);
    c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
        .unwrap();
    drop(c);
    let bytes = fs::read(&legacy).unwrap();
    assert!(Database::open(&legacy).is_err());
    assert_eq!(fs::read(&legacy).unwrap(), bytes);
    assert!(!legacy.with_file_name("legacy.db-wal").exists());
    assert!(Database::open(Path::new(":memory:")).is_err());
    assert!(Database::open(t.path()).is_err());
}
#[test]
fn habitodo_wal_import_is_lossless_idempotent_and_refuses_changes() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("e.db");
    let src = t.path().join("legacy.db");
    let c = habitodo(&src);
    c.execute("UPDATE habits SET created_at='2026-01-01 00:30:00'", [])
        .unwrap();
    let before = fs::read(&src).unwrap();
    let wal = fs::read(t.path().join("legacy.db-wal")).unwrap();
    let mut db = Database::open(&p).unwrap();
    let preview = db.preview_import(LegacyKind::Habitodo, &src).unwrap();
    assert_eq!(preview.counts.iter().map(|(_, n)| n).sum::<usize>(), 4);
    db.import_legacy(&preview).unwrap();
    let s = db.snapshot(day("2026-01-02")).unwrap();
    assert!(s.habits[0].completed);
    assert!(!s.habits[0].eligibility_known);
    assert!(s.tasks[0].completed);
    assert!(s.tasks[0].completed_at.is_none());
    db.apply(
        Action::SetTaskCompleted {
            id: s.tasks[0].id,
            completed: true,
        },
        day("2026-01-02"),
    )
    .unwrap();
    assert!(db.snapshot(day("2026-01-02")).unwrap().tasks[0]
        .completed_at
        .is_none());
    let expected = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Local)
        .date_naive()
        .to_string();
    let check = Connection::open(&p).unwrap();
    assert_eq!(
        check
            .query_row("SELECT tracked_from FROM habits", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        expected
    );
    assert!(db
        .import_legacy(&preview)
        .unwrap()
        .contains("already imported"));
    assert_eq!(fs::read(&src).unwrap(), before);
    assert_eq!(fs::read(t.path().join("legacy.db-wal")).unwrap(), wal);
    let copied = t.path().join("moved-legacy.db");
    db.preview_import(LegacyKind::Habitodo, &src).unwrap();
    fs::copy(&preview.snapshot_path, &copied).unwrap();
    let same = db.preview_import(LegacyKind::Habitodo, &copied).unwrap();
    assert!(db
        .import_legacy(&same)
        .unwrap()
        .contains("already imported"));
    assert_eq!(db.snapshot(day("2026-01-02")).unwrap().habits.len(), 1);
    let alias = Connection::open(&copied).unwrap();
    alias
        .execute("UPDATE habits SET name='Changed alias'", [])
        .unwrap();
    let changed_alias = db.preview_import(LegacyKind::Habitodo, &copied).unwrap();
    assert!(db.import_legacy(&changed_alias).is_err());
    assert_eq!(db.snapshot(day("2026-01-02")).unwrap().habits.len(), 1);
    let inspection = Connection::open(&p).unwrap();
    assert_eq!(
        inspection
            .query_row("SELECT count(*) FROM legacy_rows", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        4
    );
    c.execute("UPDATE habits SET name='Changed'", []).unwrap();
    assert!(db.import_legacy(&preview).is_err());
    let changed = db.preview_import(LegacyKind::Habitodo, &src).unwrap();
    assert!(db.import_legacy(&changed).is_err());
    assert_eq!(
        db.snapshot(day("2026-01-02")).unwrap().habits[0].name,
        "Read"
    );
    for timestamp in ["2026-01-01T00:30:00+09:00", "2026-01-01T00:30:00"] {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("legacy.db");
        let c = habitodo(&source);
        c.execute("UPDATE habits SET archived=0,created_at=?1", [timestamp])
            .unwrap();
        let target = t.path().join("e.db");
        let mut db = Database::open(&target).unwrap();
        let preview = db.preview_import(LegacyKind::Habitodo, &source).unwrap();
        db.import_legacy(&preview).unwrap();
        let check = Connection::open(&target).unwrap();
        if let Ok(instant) = chrono::DateTime::parse_from_rfc3339(timestamp) {
            assert_eq!(
                check
                    .query_row("SELECT tracked_from FROM habits", [], |r| r
                        .get::<_, String>(0))
                    .unwrap(),
                instant
                    .with_timezone(&chrono::Local)
                    .date_naive()
                    .to_string()
            );
        } else {
            assert!(preview
                .warnings
                .iter()
                .any(|w| w.contains("timezone provenance")));
            assert!(!db.snapshot(day("2026-01-02")).unwrap().habits[0].eligibility_known);
        }
    }
}
#[test]
fn repasito_variants_preserve_dates_notes_stages_and_unknown_deadlines() {
    for initial_column in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("e.db");
        let src = t.path().join("study.db");
        let c = Connection::open(&src).unwrap();
        c.execute_batch("CREATE TABLE topics(id INTEGER PRIMARY KEY,title TEXT,notes TEXT,created_at TEXT,status TEXT);CREATE TABLE reviews(id INTEGER PRIMARY KEY,topic_id INTEGER,review_stage INTEGER,scheduled_date TEXT,completed_at TEXT); INSERT INTO topics VALUES(12,'Rust','# Notes','2026-01-01T09:00:00','completed'); INSERT INTO reviews VALUES(1,12,1,'2026-01-02',NULL),(2,12,2,'2026-01-04',NULL),(3,12,3,'2026-01-06','2026-01-06T10:00:00');").unwrap();
        if initial_column {
            c.execute_batch("ALTER TABLE topics ADD COLUMN initial_date TEXT;UPDATE topics SET initial_date='2026-01-01';").unwrap();
        }
        let bytes = fs::read(&src).unwrap();
        let mut db = Database::open(&p).unwrap();
        let preview = db.preview_import(LegacyKind::Repasito, &src).unwrap();
        db.import_legacy(&preview).unwrap();
        let s = db.snapshot(day("2026-01-04")).unwrap();
        assert_eq!((s.reviews_due, s.reviews_overdue), (1, 1));
        assert_eq!(fs::read(&src).unwrap(), bytes);
        let check = Connection::open(&p).unwrap();
        assert_eq!(
            check
                .query_row(
                    "SELECT count(*) FROM study_reviews WHERE original_due_date IS NULL",
                    [],
                    |r| r.get::<_, usize>(0)
                )
                .unwrap(),
            3
        );
        assert_eq!(
            check
                .query_row("SELECT notes FROM study_topics", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "# Notes"
        );
        assert_eq!(
            check
                .query_row("SELECT initial_date FROM study_topics", [], |r| r
                    .get::<_, Option<String>>(0))
                .unwrap()
                .is_some(),
            initial_column
        );
    }
}
#[test]
fn invalid_imports_and_transaction_failure_leave_no_partial_rows() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("e.db");
    let src = t.path().join("legacy.db");
    let c = habitodo(&src);
    let mut db = Database::open(&p).unwrap();
    c.execute("UPDATE habit_completions SET date='2026-02-30'", [])
        .unwrap();
    assert!(db.preview_import(LegacyKind::Habitodo, &src).is_err());
    c.execute("UPDATE habit_completions SET date='2026-01-02'", [])
        .unwrap();
    let preview = db.preview_import(LegacyKind::Habitodo, &src).unwrap();
    let check = Connection::open(&p).unwrap();
    check.execute_batch("CREATE TRIGGER reject_task BEFORE INSERT ON tasks BEGIN SELECT RAISE(ABORT,'synthetic disk failure'); END;").unwrap();
    assert!(db.import_legacy(&preview).is_err());
    assert!(db.snapshot(day("2026-01-02")).unwrap().habits.is_empty());
    assert_eq!(
        check
            .query_row("SELECT count(*) FROM legacy_imports", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    check.execute_batch("DROP TRIGGER reject_task").unwrap();
    db.import_legacy(&preview).unwrap();
    let snap = Connection::open(&preview.snapshot_path).unwrap();
    snap.execute("UPDATE habits SET name=?1", params!["Tampered"])
        .unwrap();
    assert!(db.import_legacy(&preview).is_err());
}
#[test]
fn backups_include_wal_restore_and_never_overwrite_permanent_files() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("e.db");
    let b = t.path().join("safe.db");
    let d = day("2026-10-15");
    let mut db = Database::open(&p).unwrap();
    db.apply(Action::AddHabit("A".into()), d).unwrap();
    db.backup(&b).unwrap();
    let bytes = fs::read(&b).unwrap();
    assert!(db.backup(&b).is_err());
    assert_eq!(fs::read(&b).unwrap(), bytes);
    assert_eq!(
        Database::open(&b)
            .unwrap()
            .snapshot(d)
            .unwrap()
            .habits
            .len(),
        1
    );
    db.backup_if_due(d).unwrap();
    db.apply(Action::AddHabit("B".into()), d).unwrap();
    db.backup_if_due(d).unwrap();
    assert_eq!(
        Database::open(&t.path().join(".ebbi-backups/e.db/daily.db"))
            .unwrap()
            .snapshot(d)
            .unwrap()
            .habits
            .len(),
        2
    );
    let rolling = t.path().join(".ebbi-backups/e.db/daily.db");
    let reader = Database::open(&rolling).unwrap();
    db.apply(Action::AddHabit("C".into()), d).unwrap();
    assert!(db.backup_if_due(d).is_err());
    drop(reader);
    db.backup_if_due(d).unwrap();
    let unrelated = t.path().join("other.db");
    let other = Database::open(&unrelated).unwrap();
    drop(other);
    fs::copy(&unrelated, &rolling).unwrap();
    let bytes = fs::read(&rolling).unwrap();
    db.apply(Action::AddHabit("D".into()), d).unwrap();
    assert!(db.backup_if_due(d).is_err());
    assert_eq!(fs::read(&rolling).unwrap(), bytes);
    let mut separate = Database::open(&t.path().join("e.sqlite")).unwrap();
    separate.backup_if_due(d).unwrap();
    assert!(t.path().join(".ebbi-backups/e.sqlite/daily.db").exists());
    assert!(Database::open(&p).is_err());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn restore_preserves_recovery_reopens_and_rejects_unsafe_sources() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("current.db");
    let backup = t.path().join("backup.db");
    let date = day("2026-10-15");
    let mut db = Database::open(&path).unwrap();
    db.apply(Action::AddHabit("Before".into()), date).unwrap();
    db.backup(&backup).unwrap();
    db.apply(Action::AddHabit("After".into()), date).unwrap();
    let source_bytes = fs::read(&backup).unwrap();
    let (reopened, restored) = db.restore(&backup);
    let mut db = reopened.unwrap();
    let recovery = restored.unwrap();
    assert_eq!(db.snapshot(date).unwrap().habits.len(), 1);
    assert_eq!(fs::read(&backup).unwrap(), source_bytes);
    assert_eq!(
        Database::open(&recovery)
            .unwrap()
            .snapshot(date)
            .unwrap()
            .habits
            .len(),
        2
    );
    assert!(Database::open(&path).is_err());
    let alias = t.path().join("alias.db");
    std::os::unix::fs::symlink(&path, &alias).unwrap();
    assert!(Database::open(&alias).is_err());
    let foreign = t.path().join("foreign.db");
    drop(Database::open(&foreign).unwrap());
    let corrupt = t.path().join("corrupt.db");
    fs::write(&corrupt, b"not sqlite").unwrap();
    let newer = t.path().join("newer.db");
    fs::copy(&backup, &newer).unwrap();
    Connection::open(&newer)
        .unwrap()
        .execute_batch("PRAGMA user_version=99")
        .unwrap();
    let live = Database::open(&backup).unwrap();
    for source in [&path, &alias, &foreign, &corrupt, &newer, &backup] {
        let (opened, outcome) = db.restore(source);
        db = opened.unwrap();
        assert!(outcome.is_err(), "{}", source.display());
        assert_eq!(db.snapshot(date).unwrap().habits.len(), 1);
    }
    drop(live);
    let sidecar = t.path().join("backup.db-wal");
    fs::write(&sidecar, b"untrusted WAL").unwrap();
    let (opened, outcome) = db.restore(&backup);
    db = opened.unwrap();
    assert!(outcome.is_err());
    assert_eq!(fs::read(&sidecar).unwrap(), b"untrusted WAL");
    db.apply(Action::AddHabit("Still writable".into()), date)
        .unwrap();
    drop(db);
    assert_eq!(
        Database::open(&path)
            .unwrap()
            .snapshot(date)
            .unwrap()
            .habits
            .len(),
        2
    );
}

#[test]
fn restore_reader_failure_preserves_current_data_and_recovery() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("current.db");
    let backup = t.path().join("backup.db");
    let date = day("2026-10-15");
    let mut db = Database::open(&path).unwrap();
    db.apply(Action::AddHabit("Original".into()), date).unwrap();
    db.backup(&backup).unwrap();
    db.apply(Action::AddHabit("Recent".into()), date).unwrap();
    let reader = Connection::open(&path).unwrap();
    reader
        .execute_batch("BEGIN; SELECT * FROM habits;")
        .unwrap();
    let (reopened, outcome) = db.restore(&backup);
    let mut db = reopened.unwrap();
    assert!(outcome.is_err());
    assert_eq!(db.snapshot(date).unwrap().habits.len(), 2);
    let recovery = fs::read_dir(t.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.to_string_lossy().contains("pre-restore") && p.extension().is_some_and(|e| e == "db")
        })
        .unwrap();
    assert_eq!(
        Database::open(&recovery)
            .unwrap()
            .snapshot(date)
            .unwrap()
            .habits
            .len(),
        2
    );
    drop(reader);
    db.apply(Action::AddHabit("Usable".into()), date).unwrap();
    let (reopened, outcome) = db.restore(&backup);
    assert!(outcome.is_ok());
    assert_eq!(reopened.unwrap().snapshot(date).unwrap().habits.len(), 1);
}

#[test]
fn restore_validates_and_migrates_private_v1_candidate() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("current.db");
    let backup = t.path().join("v1.db");
    let date = day("2026-10-15");
    let db = Database::open(&path).unwrap();
    let id: String = Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT value FROM settings WHERE key='database_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let old = Connection::open(&backup).unwrap();
    old.execute_batch(include_str!("../src/persistence/schema.sql"))
        .unwrap();
    old.execute("UPDATE settings SET value=?1 WHERE key='database_id'", [id])
        .unwrap();
    old.execute_batch("INSERT INTO tasks(title,notes,created_at,position) VALUES('V1 task','retained','2026-10-15T00:00:00Z',0)").unwrap();
    drop(old);
    let before = fs::read(&backup).unwrap();
    let (reopened, outcome) = db.restore(&backup);
    assert!(outcome.is_ok());
    assert_eq!(
        reopened.unwrap().snapshot(date).unwrap().tasks[0].title,
        "V1 task"
    );
    assert_eq!(fs::read(&backup).unwrap(), before);
    assert!(!fs::read_dir(t.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e
            .file_name()
            .to_string_lossy()
            .contains("restore-candidate")));
}
