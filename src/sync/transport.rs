//! Bounded S3 operations. Call from a background worker using a Tokio runtime.
use super::{
    protocol::{Head, HEAD_KEY, MAX_HEAD_BYTES, MAX_SNAPSHOT_BYTES},
    LocalState,
};
use crate::persistence::Result;
use aws_config::{profile::ProfileFileCredentialsProvider, BehaviorVersion};
use aws_sdk_s3::{
    config::{retry::RetryConfig, timeout::TimeoutConfig, Region},
    primitives::ByteStream,
    Client,
};
use std::time::Duration;

pub struct S3Transport {
    client: Client,
    bucket: String,
    owner: String,
}

#[derive(Debug, Clone)]
pub struct RemoteHead {
    pub head: Head,
    pub etag: String,
}

/// Operations the sync engine needs. Implementations must use conditional writes.
#[allow(async_fn_in_trait)]
pub trait Transport {
    async fn head(&self) -> Result<Option<RemoteHead>>;
    async fn snapshot(&self, head: &Head) -> Result<Vec<u8>>;
    async fn put_snapshot(&self, head: &Head, data: Vec<u8>) -> Result<()>;
    async fn publish(&self, head: &Head, previous_etag: Option<&str>) -> Result<()>;
}

impl S3Transport {
    pub async fn new(state: &LocalState) -> Result<Self> {
        state.validate()?;
        let credentials = ProfileFileCredentialsProvider::builder()
            .profile_name(&state.profile)
            .build();
        let config = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(credentials)
            .region(Region::new(state.region.clone()))
            .retry_config(RetryConfig::standard().with_max_attempts(2))
            .timeout_config(
                TimeoutConfig::builder()
                    .operation_timeout(Duration::from_secs(60))
                    .operation_attempt_timeout(Duration::from_secs(30))
                    .build(),
            )
            .load()
            .await;
        Ok(Self {
            client: Client::new(&config),
            bucket: state.bucket.clone(),
            owner: state.expected_bucket_owner.clone(),
        })
    }

    async fn download(&self, key: &str, max: usize) -> Result<Option<(Vec<u8>, String)>> {
        let output = match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .expected_bucket_owner(&self.owner)
            .send()
            .await
        {
            Ok(output) => output,
            Err(error) if error.as_service_error().is_some_and(|e| e.is_no_such_key()) => {
                return Ok(None)
            }
            Err(error) => {
                let status = error.raw_response().map(|r| r.status().as_u16());
                return Err(match status {
                    Some(403) => "S3 denied access. Check the profile, permissions and expected bucket owner.".into(),
                    Some(code) => format!("S3 download failed (HTTP {code})."),
                    None => "S3 download failed. Check the connection and selected profile.".into(),
                });
            }
        };
        if output
            .content_length()
            .is_none_or(|size| size < 0 || size as u64 > max as u64)
        {
            return Err("S3 object is too large or has no content length".into());
        }
        let etag = output.e_tag().ok_or("S3 object has no ETag")?.to_owned();
        let mut body = output.body;
        let collect = async {
            let mut bytes = Vec::new();
            while let Some(part) = body.next().await {
                let part = part.map_err(|_| "S3 download stream failed".to_string())?;
                if bytes.len().saturating_add(part.len()) > max {
                    return Err("S3 object exceeds the size limit".to_string());
                }
                bytes.extend_from_slice(&part);
            }
            Ok((bytes, etag))
        };
        tokio::time::timeout(Duration::from_secs(120), collect)
            .await
            .map_err(|_| "S3 download timed out".to_string())?
            .map(Some)
    }

    pub async fn head(&self) -> Result<Option<RemoteHead>> {
        match self.download(HEAD_KEY, MAX_HEAD_BYTES).await? {
            Some((bytes, etag)) => Ok(Some(RemoteHead {
                head: Head::from_bytes(&bytes)?,
                etag,
            })),
            None => Ok(None),
        }
    }

    pub async fn snapshot(&self, head: &Head) -> Result<Vec<u8>> {
        head.validate()?;
        let (data, _) = self
            .download(&head.snapshot_key, MAX_SNAPSHOT_BYTES)
            .await?
            .ok_or("Cloud snapshot is missing")?;
        use sha2::{Digest, Sha256};
        if format!("{:x}", Sha256::digest(&data)) != head.content_sha256 {
            return Err("Cloud snapshot checksum does not match its head".into());
        }
        Ok(data)
    }

    /// Never overwrites a snapshot. An error may mean the outcome is uncertain.
    pub async fn put_snapshot(&self, head: &Head, data: Vec<u8>) -> Result<()> {
        head.validate()?;
        use sha2::{Digest, Sha256};
        if data.len() > MAX_SNAPSHOT_BYTES
            || format!("{:x}", Sha256::digest(&data)) != head.content_sha256
        {
            return Err("Invalid snapshot bytes".into());
        }
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&head.snapshot_key)
            .expected_bucket_owner(&self.owner)
            .if_none_match("*")
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|_| "S3 snapshot upload failed or its outcome is unknown".to_string())?;
        Ok(())
    }

    /// A conditional head write is the cloud commit point. Errors can be ambiguous.
    pub async fn publish(&self, head: &Head, previous_etag: Option<&str>) -> Result<()> {
        head.validate()?;
        let data =
            serde_json::to_vec(head).map_err(|_| "Could not encode cloud head".to_string())?;
        if data.len() > MAX_HEAD_BYTES {
            return Err("Cloud head exceeds the size limit".into());
        }
        let request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(HEAD_KEY)
            .expected_bucket_owner(&self.owner)
            .body(ByteStream::from(data));
        let request = match previous_etag {
            Some(etag) => request.if_match(etag),
            None => request.if_none_match("*"),
        };
        request
            .send()
            .await
            .map_err(|_| "Cloud publication failed or its outcome is unknown".to_string())?;
        Ok(())
    }
}

impl Transport for S3Transport {
    async fn head(&self) -> Result<Option<RemoteHead>> {
        S3Transport::head(self).await
    }
    async fn snapshot(&self, head: &Head) -> Result<Vec<u8>> {
        S3Transport::snapshot(self, head).await
    }
    async fn put_snapshot(&self, head: &Head, data: Vec<u8>) -> Result<()> {
        S3Transport::put_snapshot(self, head, data).await
    }
    async fn publish(&self, head: &Head, previous_etag: Option<&str>) -> Result<()> {
        S3Transport::publish(self, head, previous_etag).await
    }
}
