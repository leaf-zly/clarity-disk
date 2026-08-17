//! Independent backup evidence accepted by destructive storage workflows.

use serde::{Deserialize, Serialize};
use thiserror::Error;

const BACKUP_RECEIPT_SCHEMA_VERSION: u16 = 1;
const MAX_RECEIPT_LIFETIME_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

/// Administrator-protected receipt emitted after an out-of-place restore drill.
///
/// The receipt is evidence produced by a separate backup workflow. It is not a
/// user acknowledgement and cannot be minted by the partition UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupVerificationReceipt {
    /// Version of the persisted receipt contract.
    pub schema_version: u16,
    /// Stable identifier of the disk whose data was backed up.
    pub source_disk_id: String,
    /// Stable identifier of the physically independent destination disk.
    pub destination_disk_id: String,
    /// Opaque recovery point identifier from the backup provider.
    pub recovery_point_id: String,
    /// SHA-256 digest of the provider-owned backup manifest.
    pub manifest_sha256: String,
    /// Time the backup completed, in Unix milliseconds.
    pub backup_completed_at_unix_ms: u64,
    /// Time an out-of-place sample restore was verified, in Unix milliseconds.
    pub restore_verified_at_unix_ms: u64,
    /// Hard expiry selected by the independent provider.
    pub expires_at_unix_ms: u64,
    /// Whether restored sample bytes matched the backup manifest.
    pub restore_digest_verified: bool,
}

impl BackupVerificationReceipt {
    /// Validates that this receipt can support a high-risk operation now.
    ///
    /// # Errors
    ///
    /// Returns a precise error when the schema, identity, restore proof,
    /// digest, chronology, or validity window is unsafe.
    pub fn verify_for_disk(
        &self,
        expected_source_disk_id: &str,
        now_unix_ms: u64,
    ) -> Result<(), BackupVerificationError> {
        if self.schema_version != BACKUP_RECEIPT_SCHEMA_VERSION {
            return Err(BackupVerificationError::UnsupportedSchema);
        }
        if expected_source_disk_id.trim().is_empty()
            || self.source_disk_id != expected_source_disk_id
        {
            return Err(BackupVerificationError::SourceIdentityMismatch);
        }
        if self.destination_disk_id.trim().is_empty()
            || self.destination_disk_id == self.source_disk_id
        {
            return Err(BackupVerificationError::DestinationNotIndependent);
        }
        if self.recovery_point_id.trim().is_empty() {
            return Err(BackupVerificationError::RecoveryPointMissing);
        }
        if !is_sha256(&self.manifest_sha256) {
            return Err(BackupVerificationError::ManifestDigestInvalid);
        }
        if !self.restore_digest_verified {
            return Err(BackupVerificationError::RestoreNotVerified);
        }
        if self.backup_completed_at_unix_ms > self.restore_verified_at_unix_ms
            || self.restore_verified_at_unix_ms > now_unix_ms
        {
            return Err(BackupVerificationError::InvalidChronology);
        }
        if now_unix_ms > self.expires_at_unix_ms {
            return Err(BackupVerificationError::Expired);
        }
        let lifetime = self
            .expires_at_unix_ms
            .saturating_sub(self.restore_verified_at_unix_ms);
        if lifetime > MAX_RECEIPT_LIFETIME_MS {
            return Err(BackupVerificationError::ValidityWindowTooLong);
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Reasons independent backup evidence cannot authorize destructive work.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum BackupVerificationError {
    /// Receipt schema is not understood by this application version.
    #[error("backup receipt schema is unsupported")]
    UnsupportedSchema,
    /// The receipt does not bind the disk selected by the immutable plan.
    #[error("backup receipt source disk identity does not match")]
    SourceIdentityMismatch,
    /// Backup destination is missing or resolves to the affected physical disk.
    #[error("backup destination is not an independent physical disk")]
    DestinationNotIndependent,
    /// Provider recovery point identity is absent.
    #[error("backup recovery point identity is missing")]
    RecoveryPointMissing,
    /// Provider manifest digest is not a SHA-256 value.
    #[error("backup manifest digest is invalid")]
    ManifestDigestInvalid,
    /// No byte-for-byte out-of-place restore proof was recorded.
    #[error("backup restore digest was not verified")]
    RestoreNotVerified,
    /// Receipt timestamps are inconsistent or claim evidence from the future.
    #[error("backup receipt chronology is invalid")]
    InvalidChronology,
    /// The provider-selected evidence lifetime elapsed.
    #[error("backup verification receipt expired")]
    Expired,
    /// The provider attempted to issue evidence for more than 30 days.
    #[error("backup verification validity window exceeds 30 days")]
    ValidityWindowTooLong,
}

#[cfg(test)]
mod tests {
    use super::{BackupVerificationError, BackupVerificationReceipt};

    fn receipt() -> BackupVerificationReceipt {
        BackupVerificationReceipt {
            schema_version: 1,
            source_disk_id: "disk-source".to_owned(),
            destination_disk_id: "disk-backup".to_owned(),
            recovery_point_id: "point-42".to_owned(),
            manifest_sha256: "a".repeat(64),
            backup_completed_at_unix_ms: 100,
            restore_verified_at_unix_ms: 200,
            expires_at_unix_ms: 1_000,
            restore_digest_verified: true,
        }
    }

    #[test]
    fn accepts_current_out_of_place_restore_evidence() {
        assert_eq!(receipt().verify_for_disk("disk-source", 500), Ok(()));
    }

    #[test]
    fn rejects_same_disk_and_unverified_restore() {
        let mut value = receipt();
        value.destination_disk_id.clone_from(&value.source_disk_id);
        assert_eq!(
            value.verify_for_disk("disk-source", 500),
            Err(BackupVerificationError::DestinationNotIndependent)
        );
        value.destination_disk_id = "disk-backup".to_owned();
        value.restore_digest_verified = false;
        assert_eq!(
            value.verify_for_disk("disk-source", 500),
            Err(BackupVerificationError::RestoreNotVerified)
        );
    }

    #[test]
    fn rejects_wrong_source_expiry_and_excessive_window() {
        let mut value = receipt();
        assert_eq!(
            value.verify_for_disk("another-disk", 500),
            Err(BackupVerificationError::SourceIdentityMismatch)
        );
        assert_eq!(
            value.verify_for_disk("disk-source", 1_001),
            Err(BackupVerificationError::Expired)
        );
        value.expires_at_unix_ms = 31 * 24 * 60 * 60 * 1_000;
        assert_eq!(
            value.verify_for_disk("disk-source", 500),
            Err(BackupVerificationError::ValidityWindowTooLong)
        );
    }
}
