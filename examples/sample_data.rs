use chrono::{Days, Local, NaiveDate};
use ebbi::{
    domain::{Action, NumericGoal},
    persistence::{Database, Result},
};
use std::{fs, os::unix::fs::DirBuilderExt, path::Path};

fn create_sample(directory: &Path, today: NaiveDate) -> Result<()> {
    // Refuse every existing destination, including symlinks and empty directories.
    fs::DirBuilder::new()
        .mode(0o700)
        .create(directory)
        .map_err(|e| format!("Choose a new sample directory: {e}"))?;
    let mut db = Database::open(&directory.join("ebbi.db"))?;
    let first = today - Days::new(89);
    for name in ["Read a chapter", "Stretch"] {
        db.apply(Action::AddHabit(name.into()), first)?;
    }
    db.apply(
        Action::AddNumericHabit {
            name: "Focused study".into(),
            unit: "minutes".into(),
            goal: NumericGoal::AtLeast(30.0),
        },
        first,
    )?;
    for offset in 0..90 {
        let date = first + Days::new(offset);
        for (id, completed) in [(1, offset % 5 != 0), (2, offset % 3 != 0)] {
            db.apply(
                Action::SetHabitCompletion {
                    id,
                    date,
                    completed,
                },
                today,
            )?;
        }
        if offset % 9 != 0 {
            db.apply(
                Action::SetHabitValue {
                    id: 3,
                    date,
                    value: Some(if offset % 11 == 0 {
                        0.0
                    } else {
                        15.0 + (offset % 7) as f64 * 5.0
                    }),
                },
                today,
            )?;
        }
    }
    for title in [
        "Plan the week",
        "Practice Rust exercises",
        "Review study notes",
    ] {
        db.apply(
            Action::AddTask {
                title: title.into(),
                notes: String::new(),
                due_date: None,
            },
            today,
        )?;
    }
    for (title, days_ago) in [
        ("Rust collections", 10),
        ("SQLite transactions", 3),
        ("Ownership basics", 1),
        ("Wayland input", 0),
    ] {
        db.apply(
            Action::AddStudyTopic {
                title: title.into(),
                notes: format!("# {title}\n\nSynthetic practice notes.\n\n- Explain the idea\n- Try a small example"),
                initial_date: today - Days::new(days_ago),
            },
            today,
        )?;
    }
    for review in db.snapshot(today)?.study_reviews {
        if review.topic_id == 1 || (review.topic_id == 3 && review.stage == 1) {
            db.apply(
                Action::SetReviewCompleted {
                    id: review.id,
                    completed: true,
                },
                today,
            )?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let directory = args
        .next()
        .ok_or("Usage: cargo run --example sample_data -- NEW_DIRECTORY")?;
    if args.next().is_some() {
        return Err("Expected exactly one new directory".into());
    }
    create_sample(Path::new(&directory), Local::now().date_naive())?;
    println!(
        "Created synthetic sample database: {}",
        Path::new(&directory).join("ebbi.db").display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_persists_and_refuses_to_overwrite_existing_data() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("sample");
        let today = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        create_sample(&directory, today).unwrap();
        let path = directory.join("ebbi.db");
        let before = fs::read(&path).unwrap();
        assert!(create_sample(&directory, today).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let db = Database::open(&path).unwrap();
        let snapshot = db.snapshot(today).unwrap();
        assert_eq!(snapshot.habits.len(), 3);
        assert_eq!(snapshot.tasks.len(), 3);
        assert_eq!(snapshot.study_topics.len(), 4);
        assert_eq!(snapshot.study_reviews.len(), 12);
        assert_eq!((snapshot.reviews_due, snapshot.reviews_overdue), (1, 1));
        let report = db.analytics(today, 90, today).unwrap();
        assert_eq!(report.total.eligible, 270);
        assert_eq!(report.study.completed, 4);
        let numeric = &report.habits[2].numeric;
        assert!(numeric.iter().any(|point| point.value == Some(0.0)));
        assert!(numeric.iter().any(|point| point.value.is_none()));
        assert!(numeric.iter().any(|point| point.successful));
    }
}
