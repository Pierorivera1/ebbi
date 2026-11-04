//! The bounded S3 head format. Revisions are immutable, opaque random identifiers.
use crate::persistence::Result;
use serde::{Deserialize, Serialize};

pub const HEAD_KEY: &str = "ebbi/head.json";
pub const MAX_HEAD_BYTES: usize = 16 * 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Head {
    pub protocol_version: u32,
    pub schema_version: i64,
    pub workspace_id: String,
    pub revision: String,
    pub parent_revision: Option<String>,
    pub snapshot_key: String,
    pub content_sha256: String,
    pub logical_sha256: String,
    pub device_id: String,
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Head {
    pub fn validate(&self) -> Result<()> {
        if self.protocol_version != 1 || self.schema_version != 2 {
            return Err("Unsupported cloud sync protocol or database schema".into());
        }
        if !hex(&self.workspace_id, 32)
            || !hex(&self.revision, 32)
            || self
                .parent_revision
                .as_ref()
                .is_some_and(|value| !hex(value, 32) || value == &self.revision)
            || !hex(&self.content_sha256, 64)
            || !hex(&self.logical_sha256, 64)
            || self.snapshot_key != format!("ebbi/snapshots/{}.db", self.revision)
            || self.device_id.is_empty()
            || self.device_id.len() > 255
            || !self
                .device_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_ .".contains(&c))
        {
            return Err("Invalid cloud sync head".into());
        }
        Ok(())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_HEAD_BYTES {
            return Err("Cloud sync head is too large".into());
        }
        let head: Self = serde_json::from_slice(bytes)
            .map_err(|_| "Invalid cloud sync head JSON".to_string())?;
        head.validate()?;
        Ok(head)
    }
}

pub fn new_revision() -> Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("Could not create a sync revision: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
