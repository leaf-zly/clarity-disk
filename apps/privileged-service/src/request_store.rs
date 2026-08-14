//! One-shot untrusted transport store for elevated requests and responses.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use clarity_privileged_protocol::{PrivilegedExecutionReport, PrivilegedRequestEnvelope};
use thiserror::Error;

/// Claimed request whose original file can no longer be replayed.
pub(crate) struct ClaimedRequest {
    pub(crate) request: PrivilegedRequestEnvelope,
    processing_path: PathBuf,
    response_path: PathBuf,
}

/// Atomically claims and validates the digest of one request file.
///
/// The request directory is an untrusted same-user transport. Authority comes
/// from schema validation, UAC, target rediscovery and operation allow-lists.
pub(crate) fn claim(
    request_id: &str,
    expected_digest: &str,
) -> Result<ClaimedRequest, RequestStoreError> {
    let root = request_root()?;
    fs::create_dir_all(&root).map_err(RequestStoreError::CreateRoot)?;
    let request_path = root.join(format!("{request_id}.request.json"));
    let processing_path = root.join(format!("{request_id}.processing.json"));
    let response_path = root.join(format!("{request_id}.response.json"));
    if processing_path.exists() || response_path.exists() {
        return Err(RequestStoreError::AlreadyConsumed);
    }
    fs::rename(&request_path, &processing_path).map_err(RequestStoreError::Claim)?;
    let result = (|| {
        let bytes = fs::read(&processing_path).map_err(RequestStoreError::Read)?;
        let request: PrivilegedRequestEnvelope =
            serde_json::from_slice(&bytes).map_err(RequestStoreError::Parse)?;
        if request.request_id != request_id {
            return Err(RequestStoreError::IdentityMismatch);
        }
        let digest = request
            .request_digest()
            .map_err(RequestStoreError::Protocol)?;
        if digest != expected_digest {
            return Err(RequestStoreError::DigestMismatch);
        }
        Ok(ClaimedRequest {
            request,
            processing_path: processing_path.clone(),
            response_path,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(processing_path);
    }
    result
}

/// Persists a terminal response before removing the claimed request.
pub(crate) fn complete(
    claim: ClaimedRequest,
    report: &PrivilegedExecutionReport,
) -> Result<(), RequestStoreError> {
    let bytes = serde_json::to_vec_pretty(report).map_err(RequestStoreError::Serialize)?;
    write_new_synced(&claim.response_path, &bytes)?;
    fs::remove_file(claim.processing_path).map_err(RequestStoreError::RemoveClaim)
}

fn request_root() -> Result<PathBuf, RequestStoreError> {
    let local = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or(RequestStoreError::LocalAppDataUnavailable)?;
    if !local.is_absolute() {
        return Err(RequestStoreError::UnsafeRoot);
    }
    Ok(local.join("ClarityDisk").join("privileged-requests"))
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), RequestStoreError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(RequestStoreError::WriteResponse)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(RequestStoreError::WriteResponse)
}

/// Failures from the one-shot request transport.
#[derive(Debug, Error)]
pub(crate) enum RequestStoreError {
    #[error("LOCALAPPDATA is unavailable")]
    LocalAppDataUnavailable,
    #[error("privileged request root is not absolute")]
    UnsafeRoot,
    #[error("privileged request directory could not be created: {0}")]
    CreateRoot(std::io::Error),
    #[error("privileged request was already consumed")]
    AlreadyConsumed,
    #[error("privileged request could not be claimed exactly once: {0}")]
    Claim(std::io::Error),
    #[error("claimed privileged request could not be read: {0}")]
    Read(std::io::Error),
    #[error("claimed privileged request is invalid JSON: {0}")]
    Parse(serde_json::Error),
    #[error("claimed request identity does not match its opaque file identity")]
    IdentityMismatch,
    #[error("claimed request changed after elevation was requested")]
    DigestMismatch,
    #[error("claimed request protocol digest failed: {0}")]
    Protocol(clarity_privileged_protocol::ProtocolError),
    #[error("privileged response could not be serialized: {0}")]
    Serialize(serde_json::Error),
    #[error("privileged response could not be durably written: {0}")]
    WriteResponse(std::io::Error),
    #[error("consumed privileged request could not be removed: {0}")]
    RemoveClaim(std::io::Error),
}
