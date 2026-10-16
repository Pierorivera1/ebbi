use chrono::NaiveDate;
use ebbi::{
    analytics::{fixed_review_dates, numeric_success},
    domain::*,
    persistence::Database,
};
use rusqlite::Connection;
fn day(value: &str) -> NaiveDate {
    value.parse().unwrap()
}

#[test]
fn fixed_dates_and_numeric_boundaries() {
    assert_eq!(
        fixed_review_dates(day("2028-02-28")).unwrap(),
        [day("2028-02-29"), day("2028-03-02"), day("2028-03-04")]
    );
    assert_eq!(
        fixed_review_dates(day("2026-12-30")).unwrap(),
        [day("2026-12-31"), day("2027-01-02"), day("2027-01-04")]
    );
    assert!(fixed_review_dates(NaiveDate::MAX).is_err());
    for (goal, value, expected) in [
        (NumericGoal::AtLeast(2.0), Some(2.0), true),
        (NumericGoal::AtLeast(2.0), Some(1.0), false),
        (NumericGoal::AtMost(0.0), None, false),
        (NumericGoal::AtMost(0.0), Some(0.0), true),
        (NumericGoal::Between { min: 1.0, max: 2.0 }, Some(1.0), true),
        (NumericGoal::Between { min: 1.0, max: 2.0 }, Some(2.0), true),
        (
            NumericGoal::Between { min: 1.0, max: 2.0 },
            Some(3.0),
            false,
        ),
        (NumericGoal::AtLeast(0.0), Some(f64::NAN), false),
    ] {
        assert_eq!(numeric_success(&goal, value), expected);
    }
    assert_eq!(Counts::default().percentage(), None);
}

#[test]
fn study_actions_preserve_completed_history_and_unknown_imports() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("e.db");
    let mut db = Database::open(&path).unwrap();
    let initial = day("2026-12-30");
    db.apply(
        Action::AddStudyTopic {
            title: "Rust".into(),
            notes: "First\nSecond\n".into(),
            initial_date: initial,
        },
        initial,
    )
    .unwrap();
    let s = db.snapshot(initial).unwrap();
    assert_eq!(
        s.study_reviews
            .iter()
            .map(|r| r.scheduled_date)
            .collect::<Vec<_>>(),
        fixed_review_dates(initial).unwrap()
    );
    let completed = s.study_reviews[2].id;
    let completed_day = day("2027-01-04");
    db.apply(
        Action::SetReviewCompleted {
            id: completed,
            completed: true,
        },
        completed_day,
    )
    .unwrap();
    let original = db
        .snapshot(completed_day)
        .unwrap()
        .study_reviews
        .into_iter()
        .find(|r| r.id == completed)
        .unwrap();
    db.apply(
        Action::SetReviewCompleted {
            id: completed,
            completed: true,
        },
        day("2027-01-06"),
    )
    .unwrap();
    db.apply(
        Action::UpdateStudyTopic {
            id: 1,
            title: "Renamed".into(),
            notes: "Keep\n".into(),
        },
        completed_day,
    )
    .unwrap();
    db.apply(
        Action::RescheduleReview {
            id: 1,
            date: day("2027-01-10"),
        },
        completed_day,
    )
    .unwrap();
    assert!(db
        .apply(
            Action::RescheduleReview {
                id: completed,
                date: day("2027-01-11")
            },
            completed_day
        )
        .is_err());
    let check = Connection::open(&path).unwrap();
    check.execute_batch("CREATE TRIGGER reject_reschedule BEFORE INSERT ON review_reschedules WHEN NEW.review_id=2 BEGIN SELECT RAISE(ABORT,'synthetic reschedule failure'); END;").unwrap();
    assert!(db
        .apply(
            Action::RescheduleStudyTopic {
                id: 1,
                initial_date: day("2027-01-01")
            },
            completed_day
        )
        .is_err());
    let unchanged = db.snapshot(completed_day).unwrap();
    assert_eq!(unchanged.study_topics[0].initial_date, Some(initial));
    assert_eq!(
        unchanged
            .study_reviews
            .iter()
            .find(|r| r.id == 1)
            .unwrap()
            .scheduled_date,
        day("2027-01-10")
    );
    check
        .execute_batch("DROP TRIGGER reject_reschedule")
        .unwrap();
    drop(check);
    db.apply(
        Action::RescheduleStudyTopic {
            id: 1,
            initial_date: day("2027-01-01"),
        },
        completed_day,
    )
    .unwrap();
    let s = db.snapshot(completed_day).unwrap();
    let preserved = s.study_reviews.iter().find(|r| r.id == completed).unwrap();
    assert_eq!(preserved.scheduled_date, original.scheduled_date);
    assert_eq!(preserved.completed_at, original.completed_at);
    assert_eq!(preserved.completed_date, Some(completed_day));
    assert_eq!(
        s.study_reviews
            .iter()
            .find(|r| r.id == 1)
            .unwrap()
            .original_due_date,
        Some(day("2026-12-31"))
    );
    assert_eq!(
        s.study_reviews
            .iter()
            .find(|r| r.id == 1)
            .unwrap()
            .scheduled_date,
        day("2027-01-02")
    );
    assert_eq!((s.reviews_due, s.reviews_overdue), (1, 1));
    assert!(!s.study_topics[0].completed);
    for id in [1, 2] {
        db.apply(
            Action::SetReviewCompleted {
                id,
                completed: true,
            },
            completed_day,
        )
        .unwrap();
    }
    assert!(db.snapshot(completed_day).unwrap().study_topics[0].completed);
    db.apply(
        Action::SetReviewCompleted {
            id: 2,
            completed: false,
        },
        completed_day,
    )
    .unwrap();
    assert!(!db.snapshot(completed_day).unwrap().study_topics[0].completed);
    let known = db
        .analytics(day("2027-01-06"), 7, day("2027-01-06"))
        .unwrap();
    assert_eq!(
        known.study.on_time,
        Counts {
            successful: 1,
            eligible: 3
        }
    );
    assert_eq!(known.study.completed, 2);
    let check = Connection::open(&path).unwrap();
    check
        .execute(
            "UPDATE study_reviews SET original_due_date=NULL,completed_date=NULL WHERE id=?1",
            [completed],
        )
        .unwrap();
    let report = db
        .analytics(day("2027-01-06"), 7, day("2027-01-06"))
        .unwrap();
    assert_eq!(
        report.study.on_time,
        Counts {
            successful: 0,
            eligible: 2
        }
    );
    assert_eq!(report.study.unknown_deadlines, 1);
    assert_eq!(report.study.unknown_completion_dates, 1);
    assert_eq!(report.study.completed, 1);
    assert_eq!(
        report
            .study
            .activity
            .iter()
            .map(|d| d.completed)
            .sum::<usize>(),
        1
    );
    drop(check);
    drop(db);
    let mut db = Database::open(&path).unwrap();
    assert_eq!(
        db.snapshot(initial).unwrap().study_topics[0].notes,
        "Keep\n"
    );
    db.apply(Action::DeleteStudyTopic(1), completed_day)
        .unwrap();
    assert!(db.snapshot(initial).unwrap().study_reviews.is_empty());
    let check = Connection::open(path).unwrap();
    assert_eq!(
        check
            .query_row("SELECT count(*) FROM review_reschedules", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        0
    );
}

#[test]
fn numeric_history_streaks_rates_and_periods_match_hand_calculation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("e.db");
    let mut db = Database::open(&path).unwrap();
    let first = day("2026-10-01");
    let second = day("2026-10-02");
    let third = day("2026-10-03");
    db.apply(Action::AddHabit("Read".into()), first).unwrap();
    db.apply(
        Action::AddNumericHabit {
            name: "Limit".into(),
            unit: "kcal".into(),
            goal: NumericGoal::AtMost(10.0),
        },
        first,
    )
    .unwrap();
    for (id, date) in [(1, first), (1, second)] {
        db.apply(
            Action::SetHabitCompletion {
                id,
                date,
                completed: true,
            },
            third,
        )
        .unwrap();
    }
    db.apply(
        Action::SetHabitValue {
            id: 2,
            date: first,
            value: Some(0.0),
        },
        third,
    )
    .unwrap();
    db.apply(
        Action::SetNumericGoal {
            id: 2,
            effective_from: third,
            goal: NumericGoal::AtLeast(20.0),
        },
        second,
    )
    .unwrap();
    assert_eq!(
        db.snapshot(second).unwrap().habits[1].next_goal,
        Some((third, NumericGoal::AtLeast(20.0)))
    );
    db.apply(
        Action::SetHabitValue {
            id: 2,
            date: third,
            value: Some(20.0),
        },
        third,
    )
    .unwrap();
    for goal in [
        NumericGoal::AtLeast(f64::NAN),
        NumericGoal::AtMost(f64::INFINITY),
        NumericGoal::Between { min: 3.0, max: 2.0 },
        NumericGoal::AtLeast(-1.0),
    ] {
        assert!(db
            .apply(
                Action::SetNumericGoal {
                    id: 2,
                    effective_from: day("2026-10-04"),
                    goal
                },
                third
            )
            .is_err());
    }
    for value in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(db
            .apply(
                Action::SetHabitValue {
                    id: 2,
                    date: third,
                    value: Some(value)
                },
                third
            )
            .is_err());
    }
    assert!(db
        .apply(
            Action::SetNumericGoal {
                id: 2,
                effective_from: third,
                goal: NumericGoal::AtMost(0.0)
            },
            third
        )
        .is_err());
    assert!(db
        .apply(
            Action::SetHabitCompletion {
                id: 2,
                date: third,
                completed: true
            },
            third
        )
        .is_err());
    assert!(db
        .apply(
            Action::SetHabitValue {
                id: 1,
                date: third,
                value: Some(1.0)
            },
            third
        )
        .is_err());
    let report = db.analytics(third, 4, third).unwrap();
    assert_eq!(report.daily.len(), 4);
    assert_eq!(report.daily[0].state, DayState::Untracked);
    assert_eq!(
        report.daily.iter().map(|d| d.counts).collect::<Vec<_>>(),
        vec![
            Counts::default(),
            Counts {
                successful: 2,
                eligible: 2
            },
            Counts {
                successful: 1,
                eligible: 2
            },
            Counts {
                successful: 1,
                eligible: 2
            }
        ]
    );
    assert_eq!(
        report.total,
        Counts {
            successful: 4,
            eligible: 6
        }
    );
    assert_eq!(report.habits[0].current_streak, 2); // today is pending
    assert_eq!(report.habits[0].longest_streak, 2);
    assert_eq!(
        report.habits[1].consistency,
        Counts {
            successful: 2,
            eligible: 3
        }
    );
    assert_eq!(
        report.habits[1].numeric[1].goal,
        Some(NumericGoal::AtMost(10.0))
    );
    assert_eq!(report.habits[1].numeric[2].value, None);
    assert!(!report.habits[1].numeric[2].successful);
    assert_eq!(
        report.habits[1].numeric[3].goal,
        Some(NumericGoal::AtLeast(20.0))
    );
    let future = db.analytics(day("2026-10-04"), 1, third).unwrap();
    assert_eq!(future.daily[0].state, DayState::Future);
    assert_eq!(future.total.percentage(), None);
    assert!(db.analytics(third, 0, third).is_err());
    assert!(db.analytics(third, 367, third).is_err());
    db.apply(
        Action::SetHabitArchived {
            id: 1,
            archived: true,
        },
        third,
    )
    .unwrap();
    assert_eq!(
        db.analytics(day("2026-10-04"), 4, day("2026-10-04"))
            .unwrap()
            .habits[0]
            .current_streak,
        0
    );
    db.apply(
        Action::SetHabitValue {
            id: 2,
            date: third,
            value: None,
        },
        third,
    )
    .unwrap();
    assert!(!db.snapshot(third).unwrap().habits[1].completed);
    let unknown = Connection::open(&path).unwrap();
    unknown.execute_batch("INSERT INTO habits(id,name,position,created_at,tracked_from,unknown_before) VALUES(4,'Unknown',4,'2026-10-03','2026-10-03','2026-10-03'); INSERT INTO habit_rules VALUES(4,'2026-10-03',1);").unwrap();
    let known = db.analytics(first, 1, third).unwrap();
    assert_eq!(
        known.total,
        Counts {
            successful: 2,
            eligible: 2
        }
    );
    assert_eq!(known.daily[0].unknown_habits, 1);
    drop(unknown);
    // A complete September contributes one success to thirty opportunities.
    let check = Connection::open(&path).unwrap();
    check.execute_batch("INSERT INTO habits(id,name,position,created_at,tracked_from) VALUES(3,'Older',3,'2026-09-01','2026-09-01'); INSERT INTO habit_rules VALUES(3,'2026-09-01',1); INSERT INTO habit_entries VALUES(3,'2026-09-30',1);").unwrap();
    let report = db
        .analytics(day("2026-10-15"), 7, day("2026-10-15"))
        .unwrap();
    assert_eq!(report.monthly.current.start, day("2026-09-01"));
    assert_eq!(report.monthly.current.end, day("2026-09-30"));
    assert_eq!(
        report.monthly.current.counts,
        Counts {
            successful: 1,
            eligible: 30
        }
    );
    assert_eq!(report.monthly.previous.counts.percentage(), None);
    assert_eq!(report.monthly.percentage_point_change, None);
    assert_eq!(report.weekly.current.start, day("2026-10-05"));
    assert_eq!(report.weekly.current.end, day("2026-10-11"));
    check.execute_batch("INSERT INTO habits(id,name,position,created_at,tracked_from) VALUES(5,'Ancient',5,'0001-01-01','0001-01-01'); INSERT INTO habit_rules VALUES(5,'0001-01-01',1); INSERT INTO habit_entries VALUES(5,'0001-01-01',1),(5,'0001-01-02',1),(5,'2026-10-14',1);").unwrap();
    let earliest = NaiveDate::MIN;
    check
        .execute("UPDATE habits SET tracked_from=?1 WHERE id=5", [earliest])
        .unwrap();
    check
        .execute(
            "UPDATE habit_rules SET effective_from=?1 WHERE habit_id=5",
            [earliest],
        )
        .unwrap();
    check
        .execute(
            "UPDATE habit_entries SET date=?1 WHERE habit_id=5 AND date='0001-01-01'",
            [earliest],
        )
        .unwrap();
    check
        .execute(
            "UPDATE habit_entries SET date=?1 WHERE habit_id=5 AND date='0001-01-02'",
            [earliest.succ_opt().unwrap()],
        )
        .unwrap();
    let extreme = db
        .analytics(day("2026-10-15"), 7, day("2026-10-15"))
        .unwrap();
    let ancient = extreme.habits.iter().find(|h| h.id == 5).unwrap();
    assert_eq!((ancient.current_streak, ancient.longest_streak), (1, 2));
    let elapsed = db
        .analytics(day("2026-10-15"), 7, day("2026-10-16"))
        .unwrap();
    assert_eq!(
        elapsed
            .habits
            .iter()
            .find(|h| h.id == 5)
            .unwrap()
            .current_streak,
        0
    );

    drop(check);
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.snapshot(first).unwrap().habits[1].value, Some(0.0));
    assert_eq!(
        db.snapshot(first).unwrap().habits[1].goal,
        Some(NumericGoal::AtMost(10.0))
    );
}

#[test]
fn migration_keeps_verified_v1_backup_and_rolls_back_failure() {
    for fail in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../src/persistence/schema.sql"))
            .unwrap();
        conn.execute_batch("INSERT INTO habits(id,name,position,created_at,tracked_from) VALUES(1,'Original',0,'2026-01-01','2026-01-01'); INSERT INTO habit_rules VALUES(1,'2026-01-01',1); INSERT INTO habit_entries VALUES(1,'2026-01-02',1); INSERT INTO study_topics VALUES(1,'Study','Notes',NULL,'2026-01-01','active'); INSERT INTO study_reviews(topic_id,review_stage,scheduled_date,completed_at) VALUES(1,1,'2026-01-02','2026-01-02T10:00:00Z');").unwrap();
        if fail {
            conn.execute_batch("CREATE TRIGGER reject_v2 BEFORE INSERT ON schema_migrations WHEN NEW.version=2 BEGIN SELECT RAISE(ABORT,'synthetic upgrade failure'); END;").unwrap();
        }
        drop(conn);
        let result = Database::open(&path);
        assert_eq!(result.is_err(), fail);
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().contains("pre-v2-"))
            .collect();
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(&backups[0]).unwrap();
        assert_eq!(
            backup
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            backup
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            backup
                .query_row("SELECT name FROM habits", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "Original"
        );
        let check = Connection::open(&path).unwrap();
        assert_eq!(
            check
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            if fail { 1 } else { 2 }
        );
        if fail {
            assert!(check.prepare("SELECT kind FROM habits").is_err());
            assert_eq!(
                check
                    .query_row("SELECT max(version) FROM schema_migrations", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        } else {
            let db = result.unwrap();
            let s = db.snapshot(day("2026-01-02")).unwrap();
            assert!(s.habits[0].completed);
            assert_eq!(s.habits[0].kind, HabitKind::Binary);
            assert_eq!(s.study_reviews[0].original_due_date, None);
            assert_eq!(s.study_reviews[0].completed_date, None);
        }
    }
}
