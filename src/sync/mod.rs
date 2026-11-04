//! Manual sync primitives. Pairing and UI integration are future work.
//! Run database snapshot and inspection operations on the database worker, never the UI thread.

use crate::persistence::{readonly, validate, Result};
pub mod engine;
pub mod protocol;
pub mod transport;
use rusqlite::{types::ValueRef, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
};

fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A checksum of the transferable bytes and a stable digest of their logical content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub workspace_id: String,
    pub schema_version: i64,
    pub content_sha256: String,
    pub logical_sha256: String,
}

/// Device-specific information. Never put AWS credentials in this file or the database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalState {
    pub format_version: u32,
    pub device_id: String,
    pub profile: String,
    pub bucket: String,
    pub region: String,
    pub expected_bucket_owner: String,
    pub accepted: Option<AcceptedRevision>,
    #[serde(default)]
    pub last_success: Option<String>,
    #[serde(default)]
    pub pending: Option<PendingTransfer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum PendingTransfer {
    Push {
        head: protocol::Head,
    },
    InitialPush {
        head: protocol::Head,
    },
    Pull {
        head: protocol::Head,
        previous_logical_sha256: String,
    },
    Join {
        head: protocol::Head,
        previous_workspace_id: String,
        previous_logical_sha256: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptedRevision {
    pub workspace_id: String,
    pub revision: String,
    pub logical_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalChanges {
    Unpaired,
    Unchanged,
    Changed,
    DifferentWorkspace,
}

impl LocalState {
    pub fn changes(&self, snapshot: &SnapshotInfo) -> LocalChanges {
        match &self.accepted {
            None => LocalChanges::Unpaired,
            Some(base) if base.workspace_id != snapshot.workspace_id => {
                LocalChanges::DifferentWorkspace
            }
            Some(base) if base.logical_sha256 == snapshot.logical_sha256 => LocalChanges::Unchanged,
            Some(_) => LocalChanges::Changed,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.format_version != 1 {
            return Err("Unsupported local sync state format".into());
        }
        for (label, value) in [
            ("device ID", &self.device_id),
            ("profile", &self.profile),
            ("bucket", &self.bucket),
            ("region", &self.region),
        ] {
            if value.is_empty()
                || value.len() > 255
                || !value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            {
                return Err(format!("Invalid sync {label}"));
            }
        }
        if self.expected_bucket_owner.len() != 12
            || !self
                .expected_bucket_owner
                .bytes()
                .all(|b| b.is_ascii_digit())
        {
            return Err("Invalid expected bucket owner".into());
        }
        if let Some(base) = &self.accepted {
            if base.workspace_id.is_empty()
                || base.revision.is_empty()
                || base.revision.len() > 255
                || base.logical_sha256.len() != 64
                || !base
                    .logical_sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err("Invalid accepted sync revision".into());
            }
        }
        if let Some(pending) = &self.pending {
            if let PendingTransfer::Join {
                previous_workspace_id,
                ..
            } = pending
            {
                if previous_workspace_id.len() != 32
                    || !previous_workspace_id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err("Invalid previous workspace identity".into());
                }
            }
            let head = match pending {
                PendingTransfer::Push { head } | PendingTransfer::InitialPush { head } => head,
                PendingTransfer::Pull {
                    head,
                    previous_logical_sha256,
                }
                | PendingTransfer::Join {
                    head,
                    previous_logical_sha256,
                    ..
                } => {
                    if previous_logical_sha256.len() != 64
                        || !previous_logical_sha256
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        return Err("Invalid pending local fingerprint".into());
                    }
                    head
                }
            };
            head.validate()?;
            if matches!(pending, PendingTransfer::InitialPush { .. })
                && (self.accepted.is_some() || head.parent_revision.is_some())
            {
                return Err("Invalid pending initial publication".into());
            }
            if matches!(pending, PendingTransfer::Join { .. }) && self.accepted.is_some() {
                return Err("Cannot join an already paired workspace".into());
            }
        }
        Ok(())
    }

    /// Loads only explicitly configured state. Absence is different from malformed state.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(error(e)),
        };
        let mut bytes = Vec::new();
        file.take(16385).read_to_end(&mut bytes).map_err(error)?;
        if bytes.len() > 16384 {
            return Err("Sync state file is too large".into());
        }
        let state: Self = serde_json::from_slice(&bytes).map_err(error)?;
        state.validate()?;
        Ok(Some(state))
    }

    /// Replaces local state atomically after its contents have been flushed to disk.
    /// Call only after the corresponding cloud publication or local apply is confirmed.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or("Sync state needs a parent directory")?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(error)?;
        let data = serde_json::to_vec(self).map_err(error)?;
        let mut suffix = path.as_os_str().to_os_string();
        suffix.push(format!(".pending-{}", protocol::new_revision()?));
        let pending = std::path::PathBuf::from(suffix);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending)
            .map_err(error)?;
        let result = (|| {
            file.write_all(&data).map_err(error)?;
            file.sync_all().map_err(error)?;
            fs::rename(&pending, path).map_err(error)?;
            fs::File::open(parent)
                .map_err(error)?
                .sync_all()
                .map_err(error)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&pending);
        }
        result
    }
}

fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}

fn row_hash(row: &rusqlite::Row<'_>, columns: usize) -> rusqlite::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    for index in 0..columns {
        match row.get_ref(index)? {
            ValueRef::Null => field(&mut hash, b"null"),
            ValueRef::Integer(v) => {
                field(&mut hash, b"integer");
                field(&mut hash, &v.to_be_bytes());
            }
            ValueRef::Real(v) => {
                field(&mut hash, b"real");
                field(&mut hash, &v.to_bits().to_be_bytes());
            }
            ValueRef::Text(v) => {
                field(&mut hash, b"text");
                field(&mut hash, v);
            }
            ValueRef::Blob(v) => {
                field(&mut hash, b"blob");
                field(&mut hash, v);
            }
        }
    }
    Ok(hash.finalize().into())
}

fn hash_query(conn: &Connection, digest: &mut Sha256, sql: &str) -> Result<()> {
    let mut stmt = conn.prepare(sql).map_err(error)?;
    let columns = stmt.column_count();
    let mut rows = stmt.query([]).map_err(error)?;
    let mut hashes = Vec::new();
    while let Some(row) = rows.next().map_err(error)? {
        hashes.push(row_hash(row, columns).map_err(error)?);
    }
    hashes.sort_unstable();
    field(digest, &(hashes.len() as u64).to_be_bytes());
    for row in hashes {
        field(digest, &row);
    }
    Ok(())
}

/// Inspect a closed snapshot created by `Database::sync_snapshot`.
/// The logical digest ignores SQLite page layout and includes schema and every table row.
pub fn inspect_snapshot(path: &Path) -> Result<SnapshotInfo> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        match fs::symlink_metadata(sidecar) {
            Ok(_) => return Err("Snapshot has SQLite sidecars and may be in use".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(error(e)),
        }
    }
    let mut file = fs::File::open(path).map_err(error)?;
    let mut bytes_hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).map_err(error)?;
        if n == 0 {
            break;
        }
        bytes_hash.update(&buf[..n]);
    }
    let conn = readonly(path)?;
    let app_id: i64 = conn
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(error)?;
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(error)?;
    if app_id != 1161970249 || version != 2 {
        return Err("Snapshot is not a supported Ebbi database".into());
    }
    validate(&conn)?;
    let ledger: i64 = conn
        .query_row("SELECT max(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })
        .map_err(error)?;
    if ledger != version {
        return Err("Snapshot migration ledger disagrees with its schema".into());
    }
    let workspace_id: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key='database_id'",
            [],
            |r| r.get(0),
        )
        .map_err(error)?;
    if workspace_id.is_empty() {
        return Err("Snapshot has no workspace identity".into());
    }
    let mut digest = Sha256::new();
    field(&mut digest, b"ebbi-logical-snapshot-v1");
    field(&mut digest, &app_id.to_be_bytes());
    field(&mut digest, &version.to_be_bytes());
    hash_query(
        &conn,
        &mut digest,
        "SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name",
    )?;
    let mut stmt = conn.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND (name NOT LIKE 'sqlite_%' OR name='sqlite_sequence') ORDER BY name").map_err(error)?;
    let tables = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(error)?;
    for name in tables {
        field(&mut digest, name.as_bytes());
        let quoted = name.replace('"', "\"\"");
        hash_query(&conn, &mut digest, &format!("SELECT * FROM \"{quoted}\""))?;
    }
    Ok(SnapshotInfo {
        workspace_id,
        schema_version: version,
        content_sha256: format!("{:x}", bytes_hash.finalize()),
        logical_sha256: format!("{:x}", digest.finalize()),
    })
}
