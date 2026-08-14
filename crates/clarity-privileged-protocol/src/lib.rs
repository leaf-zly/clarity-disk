//! Versioned, path-free contract for the one-shot Windows privileged broker.
//!
//! The protocol intentionally cannot carry shell text, executable names,
//! registry paths, file-system paths, drive letters or free-form arguments.

use std::fmt::Write as _;

use clarity_core::ImmutablePartitionPlan;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Current serialized protocol schema accepted by client and broker.
pub const PROTOCOL_SCHEMA_VERSION: u16 = 1;
/// Maximum lifetime of an elevated request after it is written by the client.
pub const MAX_REQUEST_LIFETIME_MS: u64 = 2 * 60 * 1_000;
/// Exact confirmation for disabling Windows hibernation.
pub const DISABLE_HIBERNATION_CONFIRMATION: &str = "确认关闭休眠功能";
/// Exact confirmation for enabling Windows hibernation.
pub const ENABLE_HIBERNATION_CONFIRMATION: &str = "确认启用休眠功能";
/// Exact confirmation for resetting the Windows Update download cache.
pub const UPDATE_CACHE_CONFIRMATION: &str = "确认维护 Windows 更新缓存";
/// Exact confirmation for creating a system restore point.
pub const RESTORE_POINT_CONFIRMATION: &str = "确认创建系统还原点";
/// Exact high-friction confirmation for an experimental partition merge.
pub const PARTITION_MERGE_CONFIRMATION: &str = "我已验证独立备份并确认合并分区";

/// Request written by the ordinary desktop process and independently validated by the broker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegedRequestEnvelope {
    /// Exact serialized schema version.
    pub schema_version: u16,
    /// Lowercase 128-bit hexadecimal one-shot identity.
    pub request_id: String,
    /// Client creation timestamp in Unix milliseconds.
    pub created_at_unix_ms: u64,
    /// Strict request expiration timestamp in Unix milliseconds.
    pub expires_at_unix_ms: u64,
    /// Informational desktop version, bounded to printable ASCII.
    pub client_version: String,
    /// Exact operation-specific phrase displayed by the client.
    pub confirmation_phrase: String,
    /// One allow-listed operation; the contract has no generic command variant.
    pub operation: PrivilegedOperation,
}

impl PrivilegedRequestEnvelope {
    /// Validates protocol shape, freshness, confirmation and operation invariants.
    ///
    /// This is intentionally repeated in both client and elevated broker. A
    /// successful client validation never grants authority to the broker.
    ///
    /// # Errors
    ///
    /// Returns a stable error whenever any unknown, expired or malformed field
    /// would make execution ambiguous.
    pub fn validate(&self, now_unix_ms: u64) -> Result<(), ProtocolError> {
        if self.schema_version != PROTOCOL_SCHEMA_VERSION {
            return Err(ProtocolError::UnsupportedSchema(self.schema_version));
        }
        validate_hex(&self.request_id, 32).map_err(|()| ProtocolError::InvalidRequestId)?;
        if self.created_at_unix_ms > now_unix_ms
            || now_unix_ms > self.expires_at_unix_ms
            || self
                .expires_at_unix_ms
                .saturating_sub(self.created_at_unix_ms)
                > MAX_REQUEST_LIFETIME_MS
        {
            return Err(ProtocolError::ExpiredRequest);
        }
        if self.client_version.is_empty()
            || self.client_version.len() > 32
            || !self
                .client_version
                .bytes()
                .all(|byte| byte.is_ascii_graphic())
        {
            return Err(ProtocolError::InvalidClientVersion);
        }
        let expected_phrase = self.operation.confirmation_phrase();
        if self.confirmation_phrase != expected_phrase {
            return Err(ProtocolError::ConfirmationMismatch);
        }
        self.operation.validate(now_unix_ms)
    }

    /// Returns the SHA-256 digest of the exact canonical JSON request.
    ///
    /// The ordinary process passes this lowercase digest beside the opaque
    /// request ID. The broker rejects a request file changed after elevation
    /// was requested.
    ///
    /// # Errors
    ///
    /// Returns an error only if the strongly typed envelope cannot serialize.
    pub fn request_digest(&self) -> Result<String, ProtocolError> {
        let encoded = serde_json::to_vec(self).map_err(ProtocolError::Serialize)?;
        Ok(hex_digest(&Sha256::digest(encoded)))
    }
}

/// Allow-listed elevated operations. No variant contains a path or command string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "camelCase")]
pub enum PrivilegedOperation {
    /// Run one fixed Windows system-maintenance adapter.
    Maintenance(MaintenanceOperation),
    /// Run the experimental, digest-bound adjacent data-partition merge.
    PartitionMerge(PartitionMergeOperation),
}

impl PrivilegedOperation {
    fn confirmation_phrase(&self) -> &'static str {
        match self {
            Self::Maintenance(MaintenanceOperation::SetHibernation { enabled: false }) => {
                DISABLE_HIBERNATION_CONFIRMATION
            }
            Self::Maintenance(MaintenanceOperation::SetHibernation { enabled: true }) => {
                ENABLE_HIBERNATION_CONFIRMATION
            }
            Self::Maintenance(MaintenanceOperation::ResetWindowsUpdateDownloadCache) => {
                UPDATE_CACHE_CONFIRMATION
            }
            Self::Maintenance(MaintenanceOperation::CreateSystemRestorePoint) => {
                RESTORE_POINT_CONFIRMATION
            }
            Self::PartitionMerge(_) => PARTITION_MERGE_CONFIRMATION,
        }
    }

    fn validate(&self, now_unix_ms: u64) -> Result<(), ProtocolError> {
        match self {
            Self::Maintenance(_) => Ok(()),
            Self::PartitionMerge(operation) => operation.validate(now_unix_ms),
        }
    }
}

/// Fixed Windows maintenance adapters implemented by the privileged broker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum MaintenanceOperation {
    /// Enable or disable hibernation with the trusted in-box `powercfg.exe`.
    SetHibernation {
        /// Desired hibernation state.
        enabled: bool,
    },
    /// Stop update services, safely clear only the fixed download cache, then restart services.
    ResetWindowsUpdateDownloadCache,
    /// Create a restore point with a fixed application-owned description.
    CreateSystemRestorePoint,
}

/// Experimental partition operation bound to a plan-seven immutable digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionMergeOperation {
    /// Immutable plan containing all identities the broker must rediscover.
    pub plan: ImmutablePartitionPlan,
    /// SHA-256 digest of the client-held one-time authorization token.
    pub authorization_token_digest: String,
}

impl PartitionMergeOperation {
    fn validate(&self, now_unix_ms: u64) -> Result<(), ProtocolError> {
        if self.plan.schema_version != 2
            || self.plan.expires_at_unix_ms < now_unix_ms
            || self.plan.created_at_unix_ms > now_unix_ms
            || self.plan.plan_digest.len() != 64
            || validate_hex(&self.plan.plan_digest, 64).is_err()
            || validate_hex(&self.authorization_token_digest, 64).is_err()
        {
            return Err(ProtocolError::InvalidPartitionPlan);
        }
        let source = &self.plan.source_identity;
        let target = &self.plan.target_identity;
        let identities_valid = source.partition_id == self.plan.source_partition_id
            && target.partition_id == self.plan.target_partition_id
            && source.guid.as_deref().is_some_and(non_empty_bounded)
            && target.guid.as_deref().is_some_and(non_empty_bounded)
            && source.partition_number != target.partition_number
            && source.start_offset_bytes
                == target.start_offset_bytes.saturating_add(target.size_bytes)
            && target
                .free_bytes
                .is_some_and(|free| free >= self.plan.migration_bytes);
        let evidence_valid = matches!(
            self.plan.external_power_state,
            clarity_core::ExternalPowerState::Connected
                | clarity_core::ExternalPowerState::DesktopNoBattery
        ) && self.plan.pending_restart_state
            == clarity_core::PendingRestartState::Clear
            && self.plan.backup_evidence_state == clarity_core::BackupEvidenceState::Verified;
        if !identities_valid || !evidence_valid {
            return Err(ProtocolError::InvalidPartitionPlan);
        }
        Ok(())
    }
}

/// Capability handshake returned without exposing a generic RPC surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegedCapabilities {
    /// Broker protocol schema.
    pub schema_version: u16,
    /// Broker application version.
    pub service_version: String,
    /// Whether the one-shot elevated broker binary is installed beside the app.
    pub service_available: bool,
    /// Fixed maintenance operations compiled into the broker.
    pub maintenance_operations: Vec<MaintenanceCapability>,
    /// Compile-time partition-writer gate.
    pub partition_writer_compiled: bool,
    /// Runtime experimental gate; both gates must pass.
    pub partition_writer_runtime_enabled: bool,
}

/// Stable identifiers used by the UI to render maintenance availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceCapability {
    /// Hibernation on/off adapter.
    Hibernation,
    /// Windows Update download-cache adapter.
    WindowsUpdateDownloadCache,
    /// System restore-point adapter.
    SystemRestorePoint,
}

/// Completed elevated operation report written by the broker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegedExecutionReport {
    /// Original one-shot request identity.
    pub request_id: String,
    /// Terminal result; partial success is never represented as success.
    pub status: PrivilegedExecutionStatus,
    /// Localized result safe to display and persist in an audit log.
    pub message: String,
    /// Timestamp after postcondition verification.
    pub completed_at_unix_ms: u64,
    /// Partition recovery checkpoint, when the operation used a recovery journal.
    pub recovery_state: Option<clarity_core::PartitionRecoveryState>,
}

/// Terminal elevated execution status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivilegedExecutionStatus {
    /// Operation and postcondition verification completed.
    Completed,
    /// No protected mutation started and execution stopped safely.
    SafeStopped,
    /// A potential partition mutation started and requires manual recovery.
    ManualRecoveryRequired,
    /// Request was rejected before any operation started.
    Rejected,
}

/// Stable failures shared by ordinary and elevated protocol validators.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// Only the exact current schema is accepted.
    #[error("unsupported privileged protocol schema: {0}")]
    UnsupportedSchema(u16),
    /// Request identity was not exactly 128 bits of lowercase hexadecimal.
    #[error("invalid privileged request identity")]
    InvalidRequestId,
    /// Request is expired, from the future, or has an excessive lifetime.
    #[error("privileged request is outside its validity window")]
    ExpiredRequest,
    /// Client version is empty, too long or contains unsafe characters.
    #[error("invalid client version")]
    InvalidClientVersion,
    /// Exact high-friction phrase does not match the enumerated operation.
    #[error("privileged confirmation phrase does not match the operation")]
    ConfirmationMismatch,
    /// Partition plan is expired, malformed or lacks exact identity/headroom invariants.
    #[error("invalid experimental partition plan")]
    InvalidPartitionPlan,
    /// Strongly typed request serialization failed.
    #[error("privileged request could not be serialized: {0}")]
    Serialize(#[source] serde_json::Error),
}

fn validate_hex(value: &str, expected_len: usize) -> Result<(), ()> {
    if value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(())
    }
}

fn non_empty_bounded(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use clarity_core::{PartitionExecutionIdentity, PartitionOperationKind};

    fn plan(now: u64) -> ImmutablePartitionPlan {
        ImmutablePartitionPlan {
            schema_version: 2,
            plan_digest: "a".repeat(64),
            preview_id: "preview".to_owned(),
            topology_captured_at_unix_ms: now,
            evidence_captured_at_unix_ms: now,
            created_at_unix_ms: now,
            expires_at_unix_ms: now + 60_000,
            operation: PartitionOperationKind::MergeAdjacentDataPartitions,
            disk_id: "disk-0".to_owned(),
            disk_number: 0,
            source_partition_id: "source".to_owned(),
            target_partition_id: "target".to_owned(),
            source_identity: PartitionExecutionIdentity {
                partition_id: "source".to_owned(),
                guid: Some("{source}".to_owned()),
                partition_number: 2,
                start_offset_bytes: 200,
                size_bytes: 100,
                free_bytes: Some(80),
            },
            target_identity: PartitionExecutionIdentity {
                partition_id: "target".to_owned(),
                guid: Some("{target}".to_owned()),
                partition_number: 1,
                start_offset_bytes: 100,
                size_bytes: 100,
                free_bytes: Some(50),
            },
            migration_bytes: 40,
            recovery_schema_version: 1,
            external_power_state: clarity_core::ExternalPowerState::Connected,
            pending_restart_state: clarity_core::PendingRestartState::Clear,
            backup_evidence_state: clarity_core::BackupEvidenceState::Verified,
        }
    }

    fn envelope(now: u64) -> PrivilegedRequestEnvelope {
        PrivilegedRequestEnvelope {
            schema_version: PROTOCOL_SCHEMA_VERSION,
            request_id: "1".repeat(32),
            created_at_unix_ms: now,
            expires_at_unix_ms: now + 60_000,
            client_version: "0.1.0".to_owned(),
            confirmation_phrase: PARTITION_MERGE_CONFIRMATION.to_owned(),
            operation: PrivilegedOperation::PartitionMerge(PartitionMergeOperation {
                plan: plan(now),
                authorization_token_digest: "b".repeat(64),
            }),
        }
    }

    #[test]
    fn accepts_only_digest_bound_path_free_partition_request() {
        let request = envelope(1_000);
        request.validate(1_001).unwrap();
        assert_eq!(request.request_digest().unwrap().len(), 64);
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("path"));
        assert!(!json.contains("command"));
    }

    #[test]
    fn rejects_phrase_mismatch_expiry_and_insufficient_headroom() {
        let mut request = envelope(1_000);
        request.confirmation_phrase = "yes".to_owned();
        assert!(matches!(
            request.validate(1_001),
            Err(ProtocolError::ConfirmationMismatch)
        ));
        request = envelope(1_000);
        assert!(matches!(
            request.validate(70_000),
            Err(ProtocolError::ExpiredRequest)
        ));
        request = envelope(1_000);
        let PrivilegedOperation::PartitionMerge(operation) = &mut request.operation else {
            unreachable!();
        };
        operation.plan.target_identity.free_bytes = Some(1);
        assert!(matches!(
            request.validate(1_001),
            Err(ProtocolError::InvalidPartitionPlan)
        ));
    }
}
