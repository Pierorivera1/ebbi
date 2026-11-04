//! Background worker integration for device-local sync settings and pairing.
use ebbi::{
    persistence::{Database, Result},
    sync::{
        engine::{self, JoinPreview, Outcome},
        protocol::new_revision,
        transport::S3Transport,
        LocalChanges, LocalState,
    },
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::{ffi::OsStrExt, fs::DirBuilderExt},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub profile: String,
    pub bucket: String,
    pub region: String,
    pub owner: String,
}
#[derive(Debug, Clone)]
pub enum SyncAction {
    Status,
    Configure(SyncConfig),
    CheckConnection,
    Initial,
    PreviewJoin,
    Join(JoinPreview),
    Push,
    Pull,
    Recover,
}
#[derive(Debug, Clone)]
pub struct SyncRequest {
    pub action: SyncAction,
    pub state_path: PathBuf,
    pub scratch_dir: PathBuf,
}
#[derive(Debug, Clone)]
pub struct SyncReport {
    pub state: Option<LocalState>,
    pub changes: Option<LocalChanges>,
    pub preview: Option<JoinPreview>,
    pub notice: Option<String>,
}

pub fn paths(db_path: &Path) -> Result<(PathBuf, PathBuf)> {
    let base = dirs::data_local_dir().ok_or("Cannot locate the device data directory")?;
    let hash = format!("{:x}", Sha256::digest(db_path.as_os_str().as_bytes()));
    let dir = base.join("ebbi/sync").join(hash);
    Ok((dir.join("state.json"), dir.join("scratch")))
}
fn ensure_scratch(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|e| e.to_string())
}
fn inspect(db: &Database, request: &SyncRequest, state: Option<LocalState>) -> Result<SyncReport> {
    let Some(ref current) = state else {
        return Ok(SyncReport {
            state,
            changes: None,
            preview: None,
            notice: None,
        });
    };
    ensure_scratch(&request.scratch_dir)?;
    let path = request
        .scratch_dir
        .join(format!("status-{}.db", new_revision()?));
    let result = db.sync_snapshot(&path);
    if result.is_ok() {
        let _ = fs::remove_file(&path);
    }
    let changes = current.changes(&result?);
    Ok(SyncReport {
        state,
        changes: Some(changes),
        preview: None,
        notice: None,
    })
}
fn notice(outcome: Outcome) -> String {
    match outcome {
        Outcome::UpToDate => "The workspace is already up to date.".into(),
        Outcome::Pushed => "Changes pushed to the cloud.".into(),
        Outcome::Pulled { recovery } => format!("Changes pulled. Recovery backup: {}", recovery.display()),
        Outcome::PendingRecovered => "The interrupted sync operation was recovered. Select your action again.".into(),
        Outcome::NothingPending => "There is no interrupted sync operation to recover.".into(),
        Outcome::PendingConflict => "Another device published first. Your local changes were preserved; resolve the conflict before syncing.".into(),
        Outcome::InitialPushed { backup } => format!("Starting workspace published. Local backup: {}", backup.display()),
        Outcome::Joined { recovery } => format!("Joined the cloud workspace. Previous workspace backup: {}", recovery.display()),
    }
}

pub fn run(db: Database, request: &SyncRequest) -> (Result<Database>, Result<SyncReport>) {
    let state = match LocalState::load(&request.state_path) {
        Ok(state) => state,
        Err(error) => return (Ok(db), Err(error)),
    };
    if matches!(request.action, SyncAction::Status) {
        let result = inspect(&db, request, state);
        return (Ok(db), result);
    }
    if let SyncAction::Configure(config) = &request.action {
        let result = (|| {
            let mut state = match state {
                Some(state) => {
                    if state.accepted.is_some() || state.pending.is_some() {
                        return Err("Paired or pending sync settings cannot be changed here".into());
                    }
                    state
                }
                None => LocalState {
                    format_version: 1,
                    device_id: new_revision()?,
                    profile: String::new(),
                    bucket: String::new(),
                    region: String::new(),
                    expected_bucket_owner: String::new(),
                    accepted: None,
                    last_success: None,
                    pending: None,
                },
            };
            state.profile = config.profile.trim().into();
            state.bucket = config.bucket.trim().into();
            state.region = config.region.trim().into();
            state.expected_bucket_owner = config.owner.trim().into();
            state.save(&request.state_path)?;
            let mut report = inspect(&db, request, Some(state)).map_err(|e| {
                format!("Sync settings were saved, but refreshing status failed: {e}")
            })?;
            report.notice = Some("Sync settings saved on this device.".into());
            Ok(report)
        })();
        return (Ok(db), result);
    }
    let Some(mut state) = state else {
        return (
            Ok(db),
            Err("Save sync settings on this device first".into()),
        );
    };
    if let Err(error) = ensure_scratch(&request.scratch_dir) {
        return (Ok(db), Err(error));
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(v) => v,
        Err(e) => return (Ok(db), Err(format!("Cannot start sync runtime: {e}"))),
    };
    let s3 = match runtime.block_on(S3Transport::new(&state)) {
        Ok(v) => v,
        Err(e) => return (Ok(db), Err(e)),
    };
    if matches!(request.action, SyncAction::CheckConnection) {
        let result = runtime.block_on(s3.head()).and_then(|head| {
            let mut report = inspect(&db, request, Some(state))?;
            report.notice = Some(
                if head.is_some() {
                    "Connected. A cloud workspace is available."
                } else {
                    "Connected. No cloud workspace has been published yet."
                }
                .into(),
            );
            Ok(report)
        });
        return (Ok(db), result);
    }
    let recovered = runtime.block_on(engine::recover_pending(
        &db,
        &mut state,
        &request.state_path,
        &request.scratch_dir,
        &s3,
    ));
    match recovered {
        Err(e) => return (Ok(db), Err(e)),
        Ok(Some(outcome)) => {
            let result = inspect(&db, request, Some(state)).map(|mut r| {
                r.notice = Some(notice(outcome));
                r
            });
            return (Ok(db), result);
        }
        Ok(None) if matches!(request.action, SyncAction::Recover) => {
            let result = inspect(&db, request, Some(state)).map(|mut r| {
                r.notice = Some(notice(Outcome::NothingPending));
                r
            });
            return (Ok(db), result);
        }
        _ => {}
    }
    if matches!(request.action, SyncAction::PreviewJoin) {
        let result = runtime
            .block_on(engine::preview_join(&db, &state, &request.scratch_dir, &s3))
            .and_then(|preview| {
                let mut report = inspect(&db, request, Some(state))?;
                report.preview = Some(preview);
                Ok(report)
            });
        return (Ok(db), result);
    }
    let (reopened, outcome) = match &request.action {
        SyncAction::Initial => {
            let result = runtime.block_on(engine::initial_push(
                &db,
                &mut state,
                &request.state_path,
                &request.scratch_dir,
                &s3,
            ));
            (Ok(db), result)
        }
        SyncAction::Push => {
            let result = runtime.block_on(engine::push(
                &db,
                &mut state,
                &request.state_path,
                &request.scratch_dir,
                &s3,
            ));
            (Ok(db), result)
        }
        SyncAction::Pull => runtime.block_on(engine::pull(
            db,
            &mut state,
            &request.state_path,
            &request.scratch_dir,
            &s3,
        )),
        SyncAction::Join(preview) => runtime.block_on(engine::join(
            db,
            &mut state,
            &request.state_path,
            &request.scratch_dir,
            &s3,
            preview,
        )),
        _ => unreachable!(),
    };
    let result = outcome.and_then(|outcome| {
        let db = reopened.as_ref().map_err(|e| e.clone())?;
        // If status refresh fails after success, say the transfer committed.
        let mut report = inspect(db, request, Some(state))
            .map_err(|e| format!("{} Status refresh failed: {e}", notice(outcome.clone())))?;
        report.notice = Some(notice(outcome));
        Ok(report)
    });
    (reopened, result)
}
