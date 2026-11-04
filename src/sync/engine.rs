//! Conflict-safe operations for an already paired workspace.
//! Run every function on the database worker, serialized with all writes.
use super::{
    inspect_snapshot,
    protocol::{new_revision, Head, MAX_SNAPSHOT_BYTES},
    transport::Transport,
    AcceptedRevision, LocalChanges, LocalState, PendingTransfer, SnapshotInfo,
};
use crate::persistence::{Database, Result};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub enum Outcome {
    UpToDate,
    Pushed,
    Pulled { recovery: PathBuf },
    PendingRecovered,
    PendingConflict,
    NothingPending,
    InitialPushed { backup: PathBuf },
    Joined { recovery: PathBuf },
}

struct Scratch(PathBuf, Option<PathBuf>);
impl Scratch {
    fn reserve_restore_lock(&mut self) -> Result<()> {
        let mut name = self.0.as_os_str().to_os_string();
        name.push(".lock");
        let path = PathBuf::from(name);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| e.to_string())?;
        self.1 = Some(path);
        Ok(())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        if let Some(path) = &self.1 {
            let _ = fs::remove_file(path);
        }
    }
}

fn next_path(directory: &Path) -> Result<PathBuf> {
    Ok(directory.join(format!("ebbi-sync-{}.db", new_revision()?)))
}
fn snapshot(db: &Database, directory: &Path) -> Result<(Scratch, SnapshotInfo)> {
    let path = next_path(directory)?;
    let info = db.sync_snapshot(&path)?;
    Ok((Scratch(path, None), info))
}
fn accepted(head: &Head) -> AcceptedRevision {
    AcceptedRevision {
        workspace_id: head.workspace_id.clone(),
        revision: head.revision.clone(),
        logical_sha256: head.logical_sha256.clone(),
    }
}
fn persist(state: &LocalState, path: &Path) -> Result<()> {
    state
        .save(path)
        .map_err(|e| format!("Could not persist sync state: {e}"))
}
fn update_state(
    state: &mut LocalState,
    path: &Path,
    update: impl FnOnce(&mut LocalState),
) -> Result<()> {
    let mut next = state.clone();
    update(&mut next);
    if next.accepted != state.accepted && next.accepted.is_some() {
        next.last_success = Some(chrono::Utc::now().to_rfc3339());
    }
    persist(&next, path)?;
    *state = next;
    Ok(())
}
fn require_paired(state: &LocalState) -> Result<&AcceptedRevision> {
    state
        .accepted
        .as_ref()
        .ok_or_else(|| "This workspace is not paired. Use the pairing flow first.".into())
}

/// Resolve a recorded transfer before any new operation. Never guesses after an uncertain upload.
pub async fn recover_pending(
    db: &Database,
    state: &mut LocalState,
    state_path: &Path,
    scratch_dir: &Path,
    s3: &impl Transport,
) -> Result<Option<Outcome>> {
    let Some(pending) = state.pending.clone() else {
        return Ok(None);
    };
    let mut next = state.clone();
    match pending {
        PendingTransfer::InitialPush { head } => {
            let remote = s3.head().await?;
            if remote.is_none() {
                if s3.publish(&head, None).await.is_err()
                    && s3.head().await?.as_ref().map(|r| &r.head) != Some(&head)
                {
                    return Err(
                        "Initial publication is uncertain. Retry sync to check again.".into(),
                    );
                }
            } else if !remote.as_ref().is_some_and(|r| {
                r.head == head
                    || (r.head.workspace_id == head.workspace_id
                        && r.head.parent_revision.as_deref() == Some(head.revision.as_str()))
            }) {
                return Err("Initial publication is uncertain and the cloud has another workspace. Sync is paused.".into());
            }
            next.accepted = Some(accepted(&head));
        }
        PendingTransfer::Push { head } => {
            let remote = s3.head().await?;
            let published = remote.as_ref().is_some_and(|r| {
                r.head == head
                    || (r.head.workspace_id == head.workspace_id
                        && r.head.parent_revision.as_deref() == Some(head.revision.as_str()))
            });
            if !published {
                let base = require_paired(state)?;
                let lost_race = remote.as_ref().is_some_and(|r| {
                    r.head.workspace_id == base.workspace_id
                        && r.head.parent_revision.as_deref() == Some(base.revision.as_str())
                        && head.parent_revision.as_deref() == Some(base.revision.as_str())
                        && r.head.revision != head.revision
                });
                if lost_race {
                    next.pending = None;
                    persist(&next, state_path)?;
                    *state = next;
                    return Ok(Some(Outcome::PendingConflict));
                }
                let safe_retry = remote.as_ref().is_some_and(|r| {
                    r.head.workspace_id == base.workspace_id
                        && r.head.revision == base.revision
                        && r.head.logical_sha256 == base.logical_sha256
                        && head.parent_revision.as_deref() == Some(base.revision.as_str())
                });
                if !safe_retry {
                    return Err("A previous upload has an uncertain outcome. Sync is paused to protect both histories.".into());
                }
                let etag = &remote.as_ref().ok_or("Cloud head is missing")?.etag;
                if s3.publish(&head, Some(etag)).await.is_err()
                    && s3.head().await?.as_ref().map(|r| &r.head) != Some(&head)
                {
                    return Err(
                        "Cloud publication is still uncertain. Retry sync to check again.".into(),
                    );
                }
            }
            next.accepted = Some(accepted(&head));
        }
        PendingTransfer::Pull {
            head,
            previous_logical_sha256,
        } => {
            let (_tmp, local) = snapshot(db, scratch_dir)?;
            if local.workspace_id == head.workspace_id
                && local.logical_sha256 == head.logical_sha256
            {
                next.accepted = Some(accepted(&head));
            } else if local.logical_sha256 != previous_logical_sha256
                || local.workspace_id != head.workspace_id
            {
                return Err(
                    "A previous download was interrupted and local data changed. Sync is paused."
                        .into(),
                );
            }
        }
        PendingTransfer::Join {
            head,
            previous_workspace_id,
            previous_logical_sha256,
        } => {
            let (_tmp, local) = snapshot(db, scratch_dir)?;
            if local.workspace_id == head.workspace_id
                && local.logical_sha256 == head.logical_sha256
            {
                next.accepted = Some(accepted(&head));
            } else if local.workspace_id != previous_workspace_id
                || local.logical_sha256 != previous_logical_sha256
            {
                return Err(
                    "An interrupted join has unexpected local data. Sync is paused.".into(),
                );
            }
        }
    }
    next.pending = None;
    if next.accepted != state.accepted && next.accepted.is_some() {
        next.last_success = Some(chrono::Utc::now().to_rfc3339());
    }
    persist(&next, state_path)?;
    *state = next;
    Ok(Some(Outcome::PendingRecovered))
}

/// Publish only when the remote head is still the accepted parent.
pub async fn push(
    db: &Database,
    state: &mut LocalState,
    state_path: &Path,
    scratch_dir: &Path,
    s3: &impl Transport,
) -> Result<Outcome> {
    if state.pending.is_some() {
        return Err("Resolve the pending sync operation before pushing".into());
    }
    let base = require_paired(state)?.clone();
    let (tmp, local) = snapshot(db, scratch_dir)?;
    if local.workspace_id != base.workspace_id {
        return Err("Local workspace does not match the paired workspace".into());
    }
    let remote = s3
        .head()
        .await?
        .ok_or("Cloud head is missing. No data was uploaded.")?;
    if remote.head.workspace_id != base.workspace_id
        || remote.head.revision != base.revision
        || remote.head.logical_sha256 != base.logical_sha256
    {
        return Err(
            "Cloud data changed since this device last synced. Pull or resolve the conflict first."
                .into(),
        );
    }
    if state.changes(&local) == LocalChanges::Unchanged {
        return Ok(Outcome::UpToDate);
    }
    let metadata = fs::metadata(&tmp.0).map_err(|e| e.to_string())?;
    if metadata.len() > MAX_SNAPSHOT_BYTES as u64 {
        return Err("Local snapshot exceeds the sync size limit".into());
    }
    let data = fs::read(&tmp.0).map_err(|e| e.to_string())?;
    let revision = new_revision()?;
    let head = Head {
        protocol_version: 1,
        schema_version: local.schema_version,
        workspace_id: local.workspace_id,
        snapshot_key: format!("ebbi/snapshots/{revision}.db"),
        revision,
        parent_revision: Some(base.revision),
        content_sha256: local.content_sha256,
        logical_sha256: local.logical_sha256,
        device_id: state.device_id.clone(),
    };
    head.validate()?;
    s3.put_snapshot(&head, data).await?;
    update_state(state, state_path, |next| {
        next.pending = Some(PendingTransfer::Push { head: head.clone() });
    })?;
    if s3.publish(&head, Some(&remote.etag)).await.is_err() {
        return match recover_pending(db, state, state_path, scratch_dir, s3).await {
            Ok(Some(Outcome::PendingRecovered)) => Ok(Outcome::Pushed),
            Ok(Some(Outcome::PendingConflict)) => Err("Another device published first. Your local changes were preserved; resolve the conflict before syncing.".into()),
            _ => Err("Cloud publication was not confirmed. Sync is paused until its outcome can be established.".into()),
        };
    }
    update_state(state, state_path, |next| {
        next.accepted = Some(accepted(&head));
        next.pending = None;
    })?;
    Ok(Outcome::Pushed)
}

/// Download and safely replace an unchanged paired workspace, retaining a recovery backup.
/// The database is returned even if preparation or restoration fails whenever it can be reopened.
pub async fn pull(
    db: Database,
    state: &mut LocalState,
    state_path: &Path,
    scratch_dir: &Path,
    s3: &impl Transport,
) -> (Result<Database>, Result<Outcome>) {
    let prepare = async {
        if state.pending.is_some() { return Err("Resolve the pending sync operation before pulling".into()); }
        let base = require_paired(state)?.clone();
        let (_current, local) = snapshot(&db, scratch_dir)?;
        if state.changes(&local) != LocalChanges::Unchanged { return Err("Local data has unpublished changes or belongs to another workspace. Pull was refused.".into()); }
        let remote = s3.head().await?.ok_or("Cloud head is missing")?;
        if remote.head.workspace_id != base.workspace_id { return Err("Cloud data belongs to a different workspace".into()); }
        if remote.head.revision == base.revision {
            if remote.head.logical_sha256 != base.logical_sha256 { return Err("Cloud revision has inconsistent contents".into()); }
            return Ok(None);
        }
        let bytes = s3.snapshot(&remote.head).await?;
        let path = next_path(scratch_dir)?;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).map_err(|e| e.to_string())?;
        let mut tmp = Scratch(path, None);
        file.write_all(&bytes).and_then(|()| file.sync_all()).map_err(|e| e.to_string())?;
        drop(file);
        let info = inspect_snapshot(&tmp.0)?;
        if info.workspace_id != remote.head.workspace_id || info.schema_version != remote.head.schema_version || info.content_sha256 != remote.head.content_sha256 || info.logical_sha256 != remote.head.logical_sha256 {
            return Err("Downloaded snapshot does not match its cloud head".into());
        }
        tmp.reserve_restore_lock()?;
        update_state(state, state_path, |next| {
            next.pending = Some(PendingTransfer::Pull { head: remote.head.clone(), previous_logical_sha256: local.logical_sha256 });
        })?;
        Ok::<_, String>(Some((tmp, remote.head)))
    }.await;
    let (tmp, head) = match prepare {
        Ok(Some(v)) => v,
        Ok(None) => return (Ok(db), Ok(Outcome::UpToDate)),
        Err(e) => return (Ok(db), Err(e)),
    };
    let (reopened, result) = db.restore(&tmp.0);
    let result = result.and_then(|recovery| {
        update_state(state, state_path, |next| {
            next.accepted = Some(accepted(&head));
            next.pending = None;
        })?;
        Ok(Outcome::Pulled { recovery })
    });
    (reopened, result)
}

/// Capture both sides before the user confirms replacing the local workspace.
#[derive(Debug, Clone)]
pub struct JoinPreview {
    pub head: Head,
    pub local_workspace_id: String,
    pub local_logical_sha256: String,
    pub cloud_tasks: i64,
    pub cloud_habits: i64,
    pub cloud_topics: i64,
    pub cloud_reviews: i64,
}

async fn download_snapshot(
    head: &Head,
    scratch_dir: &Path,
    s3: &impl Transport,
) -> Result<Scratch> {
    let bytes = s3.snapshot(head).await?;
    let path = next_path(scratch_dir)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let tmp = Scratch(path, None);
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    let info = inspect_snapshot(&tmp.0)?;
    if info.workspace_id != head.workspace_id
        || info.schema_version != head.schema_version
        || info.content_sha256 != head.content_sha256
        || info.logical_sha256 != head.logical_sha256
    {
        return Err("Downloaded workspace does not match its cloud head".into());
    }
    Ok(tmp)
}

pub async fn preview_join(
    db: &Database,
    state: &LocalState,
    scratch_dir: &Path,
    s3: &impl Transport,
) -> Result<JoinPreview> {
    if state.accepted.is_some() || state.pending.is_some() {
        return Err("This device is already paired or has a pending sync operation".into());
    }
    let (_tmp, local) = snapshot(db, scratch_dir)?;
    let remote = s3
        .head()
        .await?
        .ok_or("The cloud workspace is empty. Publish the starting workspace first.")?;
    let cloud = download_snapshot(&remote.head, scratch_dir, s3).await?;
    let conn = crate::persistence::readonly(&cloud.0)?;
    let count = |table: &str| -> Result<i64> {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(|e| e.to_string())
    };
    let cloud_tasks = count("tasks")?;
    let cloud_habits = count("habits")?;
    let cloud_topics = count("study_topics")?;
    let cloud_reviews = count("study_reviews")?;
    Ok(JoinPreview {
        head: remote.head,
        local_workspace_id: local.workspace_id,
        local_logical_sha256: local.logical_sha256,
        cloud_tasks,
        cloud_habits,
        cloud_topics,
        cloud_reviews,
    })
}

/// First publication is allowed only into an empty cloud head and keeps a permanent local backup.
pub async fn initial_push(
    db: &Database,
    state: &mut LocalState,
    state_path: &Path,
    scratch_dir: &Path,
    s3: &impl Transport,
) -> Result<Outcome> {
    if state.accepted.is_some() || state.pending.is_some() {
        return Err("This device is already paired or has a pending sync operation".into());
    }
    if s3.head().await?.is_some() {
        return Err("A cloud workspace already exists. No local data was uploaded.".into());
    }
    let backup = db.sync_recovery_backup()?;
    let (tmp, local) = snapshot(db, scratch_dir)?;
    if fs::metadata(&tmp.0).map_err(|e| e.to_string())?.len() > MAX_SNAPSHOT_BYTES as u64 {
        return Err(format!(
            "Snapshot is too large. Local backup: {}",
            backup.display()
        ));
    }
    let data = fs::read(&tmp.0).map_err(|e| e.to_string())?;
    let revision = new_revision()?;
    let head = Head {
        protocol_version: 1,
        schema_version: local.schema_version,
        workspace_id: local.workspace_id,
        snapshot_key: format!("ebbi/snapshots/{revision}.db"),
        revision,
        parent_revision: None,
        content_sha256: local.content_sha256,
        logical_sha256: local.logical_sha256,
        device_id: state.device_id.clone(),
    };
    head.validate()?;
    s3.put_snapshot(&head, data).await?;
    update_state(state, state_path, |next| {
        next.pending = Some(PendingTransfer::InitialPush { head: head.clone() })
    })?;
    if s3.publish(&head, None).await.is_err() {
        match recover_pending(db, state, state_path, scratch_dir, s3).await {
            Ok(Some(Outcome::PendingRecovered)) => return Ok(Outcome::InitialPushed { backup }),
            _ => {
                return Err(format!(
                    "Initial publication was not confirmed. Sync is paused. Local backup: {}",
                    backup.display()
                ))
            }
        }
    }
    update_state(state, state_path, |next| {
        next.accepted = Some(accepted(&head));
        next.pending = None;
    })?;
    Ok(Outcome::InitialPushed { backup })
}

/// Join only the exact workspace and local contents reviewed at confirmation.
pub async fn join(
    db: Database,
    state: &mut LocalState,
    state_path: &Path,
    scratch_dir: &Path,
    s3: &impl Transport,
    preview: &JoinPreview,
) -> (Result<Database>, Result<Outcome>) {
    let prepare = async {
        if state.accepted.is_some() || state.pending.is_some() {
            return Err("This device is already paired or has a pending sync operation".into());
        }
        let (_tmp, local) = snapshot(&db, scratch_dir)?;
        if local.workspace_id != preview.local_workspace_id
            || local.logical_sha256 != preview.local_logical_sha256
        {
            return Err("Local data changed since the join preview. Review it again.".into());
        }
        let remote = s3.head().await?.ok_or("The cloud workspace is missing")?;
        if remote.head != preview.head {
            return Err(
                "The cloud workspace changed since the join preview. Review it again.".into(),
            );
        }
        let mut tmp = download_snapshot(&remote.head, scratch_dir, s3).await?;
        let current = s3.head().await?.ok_or("The cloud workspace is missing")?;
        if current.head != preview.head {
            return Err("The cloud workspace changed while downloading. Review it again.".into());
        }
        tmp.reserve_restore_lock()?;
        update_state(state, state_path, |next| {
            next.pending = Some(PendingTransfer::Join {
                head: remote.head.clone(),
                previous_workspace_id: local.workspace_id,
                previous_logical_sha256: local.logical_sha256,
            })
        })?;
        Ok::<_, String>(Some((tmp, remote.head)))
    }
    .await;
    let (tmp, head) = match prepare {
        Ok(Some(v)) => v,
        Ok(None) => return (Ok(db), Err("No cloud workspace was downloaded".into())),
        Err(e) => return (Ok(db), Err(e.to_string())),
    };
    let (reopened, result) = db.restore_join(&tmp.0, &head.workspace_id);
    let result = result.and_then(|recovery| {
        update_state(state, state_path, |next| {
            next.accepted = Some(accepted(&head));
            next.pending = None;
        })?;
        Ok(Outcome::Joined { recovery })
    });
    (reopened, result)
}
