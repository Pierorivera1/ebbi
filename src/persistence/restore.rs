use super::*;
use std::os::unix::fs::MetadataExt;

pub(super) fn lock_path(path: &Path) -> Result<fs::File> {
    let canonical = if path.exists() {
        fs::canonicalize(path).map_err(err)?
    } else {
        fs::canonicalize(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .map_err(err)?
        .join(path.file_name().ok_or("Missing database filename")?)
    };
    let mut name = canonical.into_os_string();
    name.push(".lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(Path::new(&name))
        .map_err(err)?;
    file.try_lock()
        .map_err(|e| format!("Database is already in use, or cannot be locked: {e}"))?;
    Ok(file)
}

fn no_sidecars(path: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        if fs::symlink_metadata(Path::new(&name)).is_ok() {
            return Err(format!(
                "SQLite sidecar exists for {}; close its users before restoring",
                path.display()
            ));
        }
    }
    Ok(())
}

fn identity(conn: &Connection) -> Result<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key='database_id'",
        [],
        |r| r.get(0),
    )
    .map_err(err)
}

impl Database {
    /// Returns a reopened database even when restore fails. Recovery snapshots are permanent.
    pub fn restore(self, source: &Path) -> (Result<Self>, Result<PathBuf>) {
        let pending = unique_path(&self.path, "restore-pending");
        let candidate = unique_path(&self.path, "restore-candidate");
        let recovery = unique_path(&self.path, "pre-restore");
        let prepared = (|| {
            let source = fs::canonicalize(source).map_err(err)?;
            let info = fs::metadata(&source).map_err(err)?;
            let destination = fs::metadata(&self.path).map_err(err)?;
            if !info.is_file() || (info.dev(), info.ino()) == (destination.dev(), destination.ino())
            {
                return Err("Restore source must be a separate regular Ebbi backup".into());
            }
            let _source_stable = lock_path(&source)?;
            let mut source_file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&source)
                .map_err(err)?;
            source_file
                .try_lock()
                .map_err(|_| "Restore source is in use".to_string())?;
            no_sidecars(&source)?;
            // Copy only a closed backup, then validate/migrate the private copy, never the selected file.
            let mut candidate_file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&candidate)
                .map_err(err)?;
            std::io::copy(&mut source_file, &mut candidate_file).map_err(err)?;
            candidate_file.sync_all().map_err(err)?;
            drop(candidate_file);
            no_sidecars(&source)?;
            let raw = readonly(&candidate)?;
            let version: i64 = raw
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .map_err(err)?;
            if !(1..=2).contains(&version) || identity(&raw)? != identity(&self.conn)? {
                return Err(
                    "Restore source is not a supported backup of this Ebbi database".into(),
                );
            }
            validate(&raw)?;
            let app_id: i64 = raw
                .pragma_query_value(None, "application_id", |r| r.get(0))
                .map_err(err)?;
            let ledger: i64 = raw
                .query_row("SELECT max(version) FROM schema_migrations", [], |r| {
                    r.get(0)
                })
                .map_err(err)?;
            if app_id != 1161970249 || ledger != version {
                return Err("Invalid Ebbi restore identity or migration ledger".into());
            }
            drop(raw);
            // The candidate is private and disposable, so migrate it without a permanent v1 snapshot.
            if version == 1 {
                let mut migration = Connection::open(&candidate).map_err(err)?;
                let tx = migration.transaction().map_err(err)?;
                tx.execute_batch(include_str!("v2.sql")).map_err(err)?;
                validate(&tx)?;
                tx.commit().map_err(err)?;
            }
            let validated = Database::open(&candidate)?;
            validated.snapshot(chrono::Local::now().date_naive())?;
            validated.backup(&pending)?;
            drop(validated);
            self.backup(&recovery)?;
            let saved = readonly(&recovery)?;
            validate(&saved)?;
            if identity(&saved)? != identity(&self.conn)? {
                return Err("Pre-restore recovery snapshot identity check failed".into());
            }
            drop(saved);
            // DELETE mode checkpoints WAL and refuses readers that prevent safe replacement.
            self.conn
                .pragma_update(None, "journal_mode", "DELETE")
                .map_err(err)?;
            let replacement_lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&pending)
                .map_err(err)?;
            replacement_lock.try_lock().map_err(err)?;
            Ok(replacement_lock)
        })();
        let _ = fs::remove_file(&candidate);
        let mut candidate_lock = candidate.as_os_str().to_os_string();
        candidate_lock.push(".lock");
        let _ = fs::remove_file(Path::new(&candidate_lock));
        let replacement_lock = match prepared {
            Ok(lock) => lock,
            Err(error) => {
                let _ = fs::remove_file(&pending);
                let error = if recovery.exists() {
                    format!(
                        "{error}. Verified recovery snapshot: {}",
                        recovery.display()
                    )
                } else {
                    error
                };
                return (Ok(self), Err(error));
            }
        };
        let Self {
            conn,
            _lock,
            stable_lock,
            path,
            ..
        } = self;
        if let Err((conn, error)) = conn.close() {
            let db = Self {
                conn,
                _lock,
                stable_lock,
                path,
                dirty: true,
                backup_date: None,
            };
            let _ = fs::remove_file(&pending);
            return (
                Ok(db),
                Err(format!("Could not close destination SQLite: {error}")),
            );
        }
        let mut renamed = false;
        let replaced = (|| {
            no_sidecars(&path)?;
            fs::rename(&pending, &path).map_err(err)?;
            renamed = true;
            sync_parent(&path)
        })();
        // Keep both locks until replacement is complete; the stable lock spans reopen.
        drop(_lock);
        let reopened = Self::open_locked(
            &path,
            stable_lock,
            if renamed {
                Some(replacement_lock)
            } else {
                None
            },
        );
        let outcome = replaced.map(|()| recovery.clone()).map_err(|error| {
            format!(
                "Restore failed: {error}. Verified recovery snapshot: {}",
                recovery.display()
            )
        });
        let outcome = if let Err(error) = &reopened {
            Err(format!(
                "Restore reopen failed: {error}. Verified recovery snapshot: {}",
                recovery.display()
            ))
        } else {
            outcome
        };
        let _ = fs::remove_file(&pending);
        (reopened, outcome)
    }
}
