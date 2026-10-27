use chrono::NaiveDate;
use ebbi::domain::{Action, AnalyticsReport, ImportPreview, LegacyKind, Snapshot};
use ebbi::persistence::Database;
use iced::futures::channel::oneshot;
use std::{path::PathBuf, sync::mpsc, thread};

#[derive(Debug, Clone)]
pub enum Operation {
    Refresh,
    Analytics(u32),
    Retry,
    Apply(Action),
    Preview(LegacyKind, PathBuf),
    Import(ImportPreview),
    Backup(PathBuf),
    Restore(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub snapshot: Result<Snapshot, String>,
    pub analytics: Option<Result<AnalyticsReport, String>>,
    pub preview: Option<ImportPreview>,
    pub notice: Option<String>,
}

struct Request {
    operation: Operation,
    date: NaiveDate,
    today: NaiveDate,
    result: oneshot::Sender<Result<Reply, String>>,
}

pub struct Worker {
    sender: Option<mpsc::Sender<Request>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    pub fn start(path: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel::<Request>();
        let thread = thread::spawn(move || {
            let mut database = Database::open(&path);
            for request in receiver {
                if matches!(request.operation, Operation::Retry) {
                    drop(database);
                    database = Database::open(&path);
                }
                if let Operation::Restore(source) = &request.operation {
                    let current =
                        std::mem::replace(&mut database, Err("Restore in progress".into()));
                    let reply = match current {
                        Ok(db) => {
                            let (reopened, outcome) = db.restore(source);
                            database = reopened;
                            outcome.and_then(|recovery| {
                                let db = database.as_mut().map_err(|e| e.clone())?;
                                let mut reply =
                                    execute(db, Operation::Refresh, request.date, request.today)?;
                                reply.notice = Some(format!(
                                    "Backup restored. Verified pre-restore recovery snapshot: {}",
                                    recovery.display()
                                ));
                                Ok(reply)
                            })
                        }
                        Err(error) => {
                            database = Err(error.clone());
                            Err(error)
                        }
                    };
                    let _ = request.result.send(reply);
                    continue;
                }
                let reply = match &mut database {
                    Ok(db) => {
                        let today = if matches!(request.operation, Operation::Apply(_)) {
                            chrono::Local::now().date_naive()
                        } else {
                            request.today
                        };
                        execute(db, request.operation, request.date, today)
                    }
                    Err(error) => Err(error.clone()),
                };
                let _ = request.result.send(reply);
            }
        });
        Self {
            sender: Some(sender),
            thread: Some(thread),
        }
    }

    pub fn request(
        &self,
        operation: Operation,
        date: NaiveDate,
        today: NaiveDate,
    ) -> impl std::future::Future<Output = Result<Reply, String>> + Send + 'static {
        let (result, receiver) = oneshot::channel();
        let sent = self.sender.as_ref().is_some_and(|sender| {
            sender
                .send(Request {
                    operation,
                    date,
                    today,
                    result,
                })
                .is_ok()
        });
        async move {
            if !sent {
                return Err("The database worker has stopped. Restart Ebbi to retry.".into());
            }
            receiver
                .await
                .map_err(|_| "The database worker stopped before replying.".to_string())?
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn execute(
    db: &mut Database,
    operation: Operation,
    date: NaiveDate,
    today: NaiveDate,
) -> Result<Reply, String> {
    let mut preview = None;
    let mut analytics = None;
    let mut notice = None;
    let changed = matches!(operation, Operation::Apply(_) | Operation::Import(_));
    match operation {
        Operation::Restore(_) => return Err("Restore must run through the worker lifecycle".into()),
        Operation::Refresh | Operation::Retry => {}
        Operation::Analytics(days) => {
            analytics = Some(db.analytics(date, days, today));
        }
        Operation::Apply(action) => db.apply(action, today)?,
        Operation::Preview(kind, path) => preview = Some(db.preview_import(kind, &path)?),
        Operation::Import(plan) => notice = Some(db.import_legacy(&plan)?),
        Operation::Backup(path) => {
            db.backup(&path)?;
            notice = Some(format!("Verified backup saved to {}", path.display()));
        }
    }
    if changed {
        if let Err(error) = db.backup_if_due(today) {
            notice = Some(format!("Changes saved. Daily backup failed: {error}"));
        }
    }
    // A committed write must not be reported as unsaved if its subsequent read fails.
    let snapshot = db.snapshot(date).map_err(|error| {
        if changed { format!("Changes were saved, but refreshing failed: {error}. Reload before retrying an action.") }
        else { error }
    });
    Ok(Reply {
        snapshot,
        analytics,
        preview,
        notice,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_worker_refreshes_and_remains_usable_after_failure() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("worker.db");
        let backup = temp.path().join("backup.db");
        let date = chrono::Local::now().date_naive();
        let worker = Worker::start(path.clone());
        let request =
            |operation| iced::futures::executor::block_on(worker.request(operation, date, date));
        request(Operation::Apply(Action::AddHabit("Original".into()))).unwrap();
        request(Operation::Backup(backup.clone())).unwrap();
        request(Operation::Apply(Action::AddHabit("Recent".into()))).unwrap();
        assert!(request(Operation::Restore(path)).is_err());
        assert_eq!(
            request(Operation::Refresh)
                .unwrap()
                .snapshot
                .unwrap()
                .habits
                .len(),
            2
        );
        let reply = request(Operation::Restore(backup)).unwrap();
        assert_eq!(reply.snapshot.unwrap().habits.len(), 1);
        let notice = reply.notice.unwrap();
        let recovery = notice.split("snapshot: ").nth(1).unwrap();
        assert_eq!(
            Database::open(std::path::Path::new(recovery))
                .unwrap()
                .snapshot(date)
                .unwrap()
                .habits
                .len(),
            2
        );
        request(Operation::Apply(Action::AddHabit("New".into()))).unwrap();
        assert_eq!(
            request(Operation::Retry)
                .unwrap()
                .snapshot
                .unwrap()
                .habits
                .len(),
            2
        );
    }

    #[test]
    fn storage_failure_stays_an_error_and_committed_write_is_not_reported_unsaved() {
        let temp = tempfile::tempdir().unwrap();
        let worker = Worker::start(temp.path().to_owned());
        let date = "2026-10-15".parse().unwrap();
        assert!(
            iced::futures::executor::block_on(worker.request(Operation::Refresh, date, date))
                .is_err()
        );
        let healthy = Worker::start(temp.path().join("healthy.db"));
        for operation in [Operation::Refresh, Operation::Retry] {
            assert!(
                iced::futures::executor::block_on(healthy.request(operation, date, date))
                    .unwrap()
                    .snapshot
                    .is_ok()
            );
        }
        let path = temp.path().join("test.db");
        let mut db = Database::open(&path).unwrap();
        let inspection = rusqlite::Connection::open(&path).unwrap();
        inspection.execute_batch("DROP TABLE tasks").unwrap();
        let reply = execute(
            &mut db,
            Operation::Apply(Action::AddHabit("Saved".into())),
            date,
            date,
        )
        .unwrap();
        assert!(reply.snapshot.unwrap_err().contains("Changes were saved"));
        assert_eq!(
            inspection
                .query_row("SELECT count(*) FROM habits", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
