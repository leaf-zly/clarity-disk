//! One-shot elevated broker executable.

mod maintenance;
mod partition;
mod request_store;
mod windows_process;

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_privileged_protocol::{
    MaintenanceCapability, PROTOCOL_SCHEMA_VERSION, PrivilegedCapabilities,
    PrivilegedExecutionReport, PrivilegedExecutionStatus, PrivilegedOperation,
};
use thiserror::Error;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), BrokerError> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments == ["--capabilities"] {
        let encoded = serde_json::to_string(&capabilities())?;
        println!("{encoded}");
        return Ok(());
    }
    let (request_id, expected_digest) = parse_request_arguments(&arguments)?;
    if !is_process_elevated() {
        return Err(BrokerError::ElevationRequired);
    }
    let claim = request_store::claim(&request_id, &expected_digest)?;
    let now = unix_ms();
    let report = match claim.request.validate(now) {
        Ok(()) => execute_request(&claim.request.operation, &request_id, now),
        Err(error) => PrivilegedExecutionReport {
            request_id: request_id.clone(),
            status: PrivilegedExecutionStatus::Rejected,
            message: error.to_string(),
            completed_at_unix_ms: now,
            recovery_state: None,
        },
    };
    request_store::complete(claim, &report)?;
    Ok(())
}

fn execute_request(
    operation: &PrivilegedOperation,
    request_id: &str,
    now_unix_ms: u64,
) -> PrivilegedExecutionReport {
    match operation {
        PrivilegedOperation::Maintenance(operation) => maintenance::execute(*operation)
            .map_or_else(
                |error| PrivilegedExecutionReport {
                    request_id: request_id.to_owned(),
                    status: PrivilegedExecutionStatus::SafeStopped,
                    message: error.to_string(),
                    completed_at_unix_ms: unix_ms(),
                    recovery_state: None,
                },
                |message| PrivilegedExecutionReport {
                    request_id: request_id.to_owned(),
                    status: PrivilegedExecutionStatus::Completed,
                    message,
                    completed_at_unix_ms: unix_ms(),
                    recovery_state: None,
                },
            ),
        PrivilegedOperation::PartitionMerge(operation) => {
            partition::execute(operation, request_id, now_unix_ms)
        }
    }
}

fn capabilities() -> PrivilegedCapabilities {
    PrivilegedCapabilities {
        schema_version: PROTOCOL_SCHEMA_VERSION,
        service_version: env!("CARGO_PKG_VERSION").to_owned(),
        service_available: cfg!(windows),
        maintenance_operations: vec![
            MaintenanceCapability::Hibernation,
            MaintenanceCapability::WindowsUpdateDownloadCache,
            MaintenanceCapability::SystemRestorePoint,
        ],
        partition_writer_compiled: cfg!(feature = "partition-writes") && cfg!(windows),
        partition_writer_runtime_enabled: partition::runtime_gate_enabled(),
    }
}

fn parse_request_arguments(arguments: &[String]) -> Result<(String, String), BrokerError> {
    if arguments.len() != 2 {
        return Err(BrokerError::InvalidArguments);
    }
    let request_id = arguments[0]
        .strip_prefix("--request-id=")
        .ok_or(BrokerError::InvalidArguments)?;
    let digest = arguments[1]
        .strip_prefix("--request-digest=")
        .ok_or(BrokerError::InvalidArguments)?;
    if !is_lower_hex(request_id, 32) || !is_lower_hex(digest, 64) {
        return Err(BrokerError::InvalidArguments);
    }
    Ok((request_id.to_owned(), digest.to_owned()))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn is_process_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = std::ptr::null_mut();
    // SAFETY: Windows initializes `token` on success; every successful handle
    // is closed below, and the fixed-size output buffer matches TOKEN_ELEVATION.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut returned = 0_u32;
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&raw mut elevation).cast(),
            u32::try_from(std::mem::size_of::<TOKEN_ELEVATION>()).unwrap_or(u32::MAX),
            &raw mut returned,
        )
    };
    unsafe { CloseHandle(token) };
    result != 0 && elevation.TokenIsElevated != 0
}

#[cfg(not(windows))]
fn is_process_elevated() -> bool {
    false
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Debug, Error)]
enum BrokerError {
    #[error("expected --capabilities or exact request-id and request-digest arguments")]
    InvalidArguments,
    #[error("privileged operation requires an elevated administrator token")]
    ElevationRequired,
    #[error(transparent)]
    Store(#[from] request_store::RequestStoreError),
    #[error("capability response could not be serialized: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_accepts_only_fixed_hex_arguments() {
        let arguments = vec![
            format!("--request-id={}", "a".repeat(32)),
            format!("--request-digest={}", "b".repeat(64)),
        ];
        assert!(parse_request_arguments(&arguments).is_ok());
        assert!(parse_request_arguments(&["--request-id=C:\\request.json".to_owned()]).is_err());
    }

    #[test]
    fn default_build_keeps_partition_writer_closed() {
        if !cfg!(feature = "partition-writes") {
            assert!(!capabilities().partition_writer_compiled);
            assert!(!capabilities().partition_writer_runtime_enabled);
        }
    }
}
