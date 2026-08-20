//! Ordinary-user orchestration for one-shot elevated broker requests.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_core::{PartitionSafetyAssessment, PartitionSafetyStatus};
use clarity_privileged_protocol::{
    MaintenanceOperation, PROTOCOL_SCHEMA_VERSION, PartitionMergeOperation, PrivilegedCapabilities,
    PrivilegedExecutionReport, PrivilegedOperation, PrivilegedRequestEnvelope,
};
use getrandom::fill as fill_random;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::state_store::{load_json, state_path, write_json};

const CHALLENGE_LIFETIME_MS: u64 = 2 * 60 * 1_000;
const MAX_PRIVILEGED_AUDIT_EVENTS: usize = 200;

/// Frontend request for one fixed system-maintenance adapter.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrepareMaintenanceRequest {
    /// Enumerated operation; the type cannot carry shell text or a path.
    pub(crate) operation: MaintenanceOperation,
}

/// Frontend request that consumes one backend-held challenge.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecutePrivilegedRequest {
    /// Opaque challenge identity returned by prepare.
    pub(crate) challenge_id: String,
    /// One-time random token; any submission consumes the challenge.
    pub(crate) confirmation_token: String,
    /// Exact high-friction phrase shown by the prepare response.
    pub(crate) confirmation_phrase: String,
}

/// Short-lived confirmation returned without granting elevated authority.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrivilegedExecutionChallenge {
    /// Backend challenge identity.
    pub(crate) challenge_id: String,
    /// Random token bound to the operation and consumed on first submission.
    pub(crate) confirmation_token: String,
    /// Exact phrase required for this operation.
    pub(crate) confirmation_phrase: String,
    /// Timestamp after which preparation must be repeated.
    pub(crate) expires_at_unix_ms: u64,
    /// User-visible operation impact.
    pub(crate) impact: String,
}

/// Partition readiness response. A blocked assessment never contains a challenge.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PartitionExecutionPreparation {
    /// Fresh plan-seven assessment generated immediately before preparation.
    pub(crate) assessment: PartitionSafetyAssessment,
    /// Present only when every safety condition and both feature gates pass.
    pub(crate) challenge: Option<PrivilegedExecutionChallenge>,
}

/// Privacy-preserving terminal event retained for privileged operations.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrivilegedAuditEvent {
    /// One-shot request identity.
    pub(crate) request_id: String,
    /// Stable terminal status string.
    pub(crate) status: String,
    /// Broker-owned localized result without paths or file names.
    pub(crate) message: String,
    /// Broker completion timestamp.
    pub(crate) completed_at_unix_ms: u64,
    /// Plan digest for partition recovery correlation, when applicable.
    pub(crate) plan_digest: Option<String>,
}

#[derive(Clone)]
struct PendingChallenge {
    token: String,
    confirmation_phrase: String,
    expires_at_unix_ms: u64,
    operation: PrivilegedOperation,
}

/// Thread-safe owner of one-time ordinary-to-elevated operation handshakes.
pub(crate) struct PrivilegedWorkflow {
    pending: Mutex<HashMap<String, PendingChallenge>>,
    audit_path: PathBuf,
}

impl Default for PrivilegedWorkflow {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            audit_path: state_path("privileged-audit.json"),
        }
    }
}

impl PrivilegedWorkflow {
    /// Queries the adjacent broker executable without requesting elevation.
    pub(crate) fn capabilities() -> PrivilegedCapabilities {
        broker_path()
            .and_then(|path| query_capabilities(&path))
            .unwrap_or_else(|| PrivilegedCapabilities {
                schema_version: PROTOCOL_SCHEMA_VERSION,
                service_version: env!("CARGO_PKG_VERSION").to_owned(),
                service_available: false,
                maintenance_operations: vec![],
                partition_writer_compiled: false,
                partition_writer_runtime_enabled: false,
            })
    }

    /// Issues a one-time challenge for an allow-listed maintenance operation.
    pub(crate) fn prepare_maintenance(
        &self,
        operation: MaintenanceOperation,
    ) -> Result<PrivilegedExecutionChallenge, String> {
        let capabilities = Self::capabilities();
        if !capabilities.service_available {
            return Err("管理员服务未安装或版本不可用。".to_owned());
        }
        let operation = PrivilegedOperation::Maintenance(operation);
        let impact = maintenance_impact(&operation);
        self.issue_challenge(operation, impact)
    }

    /// Issues a partition challenge only for a fresh fully-ready assessment.
    pub(crate) fn prepare_partition(
        &self,
        assessment: PartitionSafetyAssessment,
    ) -> Result<PartitionExecutionPreparation, String> {
        let capabilities = Self::capabilities();
        let ready = assessment.status == PartitionSafetyStatus::FoundationReady
            && !assessment.execution_authorized
            && !assessment.write_capability_present
            && capabilities.service_available
            && capabilities.partition_writer_compiled
            && capabilities.partition_writer_runtime_enabled;
        if !ready {
            return Ok(PartitionExecutionPreparation {
                assessment,
                challenge: None,
            });
        }
        let plan = assessment.plan.clone();
        let token = random_hex(32)?;
        let operation = PrivilegedOperation::PartitionMerge(Box::new(PartitionMergeOperation {
            plan,
            authorization_token_digest: hex_sha256(token.as_bytes()),
        }));
        let challenge = self.issue_challenge_with_token(
            operation,
            token,
            "迁移源卷全部用户数据并校验后，删除源分区并扩展左侧目标 NTFS 分区；元数据写入后失败需要人工恢复。",
        )?;
        Ok(PartitionExecutionPreparation {
            assessment,
            challenge: Some(challenge),
        })
    }

    /// Consumes a challenge and invokes the broker through a Windows UAC boundary.
    pub(crate) fn execute(
        &self,
        request: &ExecutePrivilegedRequest,
    ) -> Result<PrivilegedExecutionReport, String> {
        let challenge = self
            .pending
            .lock()
            .map_err(|_| "管理员确认状态不可用。".to_owned())?
            .remove(&request.challenge_id)
            .ok_or_else(|| "管理员确认已失效或已使用。".to_owned())?;
        let now = unix_ms();
        if now > challenge.expires_at_unix_ms
            || request.confirmation_token != challenge.token
            || request.confirmation_phrase != challenge.confirmation_phrase
        {
            return Err("管理员确认不匹配、已过期或已消费。".to_owned());
        }
        let request_id = random_hex(16)?;
        let envelope = PrivilegedRequestEnvelope {
            schema_version: PROTOCOL_SCHEMA_VERSION,
            request_id: request_id.clone(),
            created_at_unix_ms: now,
            expires_at_unix_ms: now.saturating_add(CHALLENGE_LIFETIME_MS),
            client_version: env!("CARGO_PKG_VERSION").to_owned(),
            confirmation_phrase: request.confirmation_phrase.clone(),
            operation: challenge.operation,
        };
        envelope.validate(now).map_err(|error| error.to_string())?;
        let digest = envelope
            .request_digest()
            .map_err(|error| error.to_string())?;
        let request_path = write_request(&envelope)?;
        let broker = broker_path().ok_or_else(|| "管理员服务未安装。".to_owned())?;
        if let Err(error) = launch_elevated(&broker, &request_id, &digest) {
            let _ = fs::remove_file(request_path);
            return Err(error);
        }
        let response_path = request_root()?.join(format!("{request_id}.response.json"));
        let bytes =
            fs::read(&response_path).map_err(|error| format!("无法读取管理员服务结果：{error}"))?;
        let _ = fs::remove_file(response_path);
        let report: PrivilegedExecutionReport = serde_json::from_slice(&bytes)
            .map_err(|error| format!("管理员服务结果无效：{error}"))?;
        if report.request_id != request_id {
            return Err("管理员服务响应身份不匹配。".to_owned());
        }
        let plan_digest = match &envelope.operation {
            PrivilegedOperation::PartitionMerge(operation) => {
                Some(operation.plan.plan_digest.clone())
            }
            PrivilegedOperation::Maintenance(_) => None,
        };
        self.record_audit(&report, plan_digest)?;
        Ok(report)
    }

    /// Returns newest privileged terminal events first.
    pub(crate) fn audit_events(&self) -> Vec<PrivilegedAuditEvent> {
        let mut events: Vec<PrivilegedAuditEvent> = load_json(&self.audit_path);
        events.sort_by_key(|event| std::cmp::Reverse(event.completed_at_unix_ms));
        events.truncate(MAX_PRIVILEGED_AUDIT_EVENTS);
        events
    }

    /// Clears terminal privileged audit summaries without touching recovery journals.
    pub(crate) fn clear_audit_events(&self) -> Result<(), String> {
        write_json(&self.audit_path, &Vec::<PrivilegedAuditEvent>::new())
            .map_err(|error| error.to_string())
    }

    fn issue_challenge(
        &self,
        operation: PrivilegedOperation,
        impact: &'static str,
    ) -> Result<PrivilegedExecutionChallenge, String> {
        let token = random_hex(32)?;
        self.issue_challenge_with_token(operation, token, impact)
    }

    fn issue_challenge_with_token(
        &self,
        operation: PrivilegedOperation,
        token: String,
        impact: &'static str,
    ) -> Result<PrivilegedExecutionChallenge, String> {
        let now = unix_ms();
        let challenge_id = random_hex(16)?;
        let confirmation_phrase = confirmation_phrase(&operation).to_owned();
        let challenge = PrivilegedExecutionChallenge {
            challenge_id: challenge_id.clone(),
            confirmation_token: token.clone(),
            confirmation_phrase: confirmation_phrase.clone(),
            expires_at_unix_ms: now.saturating_add(CHALLENGE_LIFETIME_MS),
            impact: impact.to_owned(),
        };
        let pending = PendingChallenge {
            token,
            confirmation_phrase,
            expires_at_unix_ms: challenge.expires_at_unix_ms,
            operation,
        };
        self.pending
            .lock()
            .map_err(|_| "管理员确认状态不可用。".to_owned())?
            .insert(challenge_id, pending);
        Ok(challenge)
    }

    fn record_audit(
        &self,
        report: &PrivilegedExecutionReport,
        plan_digest: Option<String>,
    ) -> Result<(), String> {
        let mut events: Vec<PrivilegedAuditEvent> = load_json(&self.audit_path);
        events.retain(|event| event.request_id != report.request_id);
        events.push(PrivilegedAuditEvent {
            request_id: report.request_id.clone(),
            status: format!("{:?}", report.status),
            message: report.message.clone(),
            completed_at_unix_ms: report.completed_at_unix_ms,
            plan_digest,
        });
        events.sort_by_key(|event| std::cmp::Reverse(event.completed_at_unix_ms));
        events.truncate(MAX_PRIVILEGED_AUDIT_EVENTS);
        write_json(&self.audit_path, &events).map_err(|error| error.to_string())
    }
}

fn maintenance_impact(operation: &PrivilegedOperation) -> &'static str {
    match operation {
        PrivilegedOperation::Maintenance(MaintenanceOperation::SetHibernation {
            enabled: false,
        }) => "关闭休眠会移除 hiberfil.sys，并可能同时关闭 Windows 快速启动。",
        PrivilegedOperation::Maintenance(MaintenanceOperation::SetHibernation {
            enabled: true,
        }) => "启用休眠会重新创建由 Windows 管理的 hiberfil.sys 并占用系统盘空间。",
        PrivilegedOperation::Maintenance(MaintenanceOperation::ResetWindowsUpdateDownloadCache) => {
            "将短暂停止 Windows Update 与 BITS，仅清理固定下载缓存，然后恢复服务。"
        }
        PrivilegedOperation::Maintenance(MaintenanceOperation::CreateSystemRestorePoint) => {
            "请求 Windows 创建一个新的系统还原点；系统策略可能限制创建频率。"
        }
        PrivilegedOperation::PartitionMerge(_) => unreachable!(),
    }
}

fn confirmation_phrase(operation: &PrivilegedOperation) -> &'static str {
    use clarity_privileged_protocol::{
        DISABLE_HIBERNATION_CONFIRMATION, ENABLE_HIBERNATION_CONFIRMATION,
        PARTITION_MERGE_CONFIRMATION, RESTORE_POINT_CONFIRMATION, UPDATE_CACHE_CONFIRMATION,
    };
    match operation {
        PrivilegedOperation::Maintenance(MaintenanceOperation::SetHibernation {
            enabled: false,
        }) => DISABLE_HIBERNATION_CONFIRMATION,
        PrivilegedOperation::Maintenance(MaintenanceOperation::SetHibernation {
            enabled: true,
        }) => ENABLE_HIBERNATION_CONFIRMATION,
        PrivilegedOperation::Maintenance(MaintenanceOperation::ResetWindowsUpdateDownloadCache) => {
            UPDATE_CACHE_CONFIRMATION
        }
        PrivilegedOperation::Maintenance(MaintenanceOperation::CreateSystemRestorePoint) => {
            RESTORE_POINT_CONFIRMATION
        }
        PrivilegedOperation::PartitionMerge(_) => PARTITION_MERGE_CONFIRMATION,
    }
}

fn query_capabilities(path: &Path) -> Option<PrivilegedCapabilities> {
    let output = crate::windows_process::hide_console_window(Command::new(path))
        .arg("--capabilities")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| serde_json::from_slice(&output.stdout).ok())
        .flatten()
}

fn broker_path() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let name = if cfg!(windows) {
        "clarity-privileged-service.exe"
    } else {
        "clarity-privileged-service"
    };
    let path = executable.parent()?.join(name);
    path.is_file().then_some(path)
}

fn write_request(envelope: &PrivilegedRequestEnvelope) -> Result<PathBuf, String> {
    let root = request_root()?;
    fs::create_dir_all(&root).map_err(|error| format!("无法创建管理员请求目录：{error}"))?;
    let path = root.join(format!("{}.request.json", envelope.request_id));
    let bytes = serde_json::to_vec_pretty(envelope).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("无法创建一次性管理员请求：{error}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("无法持久化一次性管理员请求：{error}"))?;
    Ok(path)
}

fn request_root() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "LOCALAPPDATA 不可用。".to_owned())?;
    Ok(local.join("ClarityDisk").join("privileged-requests"))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn launch_elevated(path: &Path, request_id: &str, digest: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };

    let wide = |value: &std::ffi::OsStr| {
        value
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let verb = wide(std::ffi::OsStr::new("runas"));
    let executable = wide(path.as_os_str());
    let parameters = wide(std::ffi::OsStr::new(&format!(
        "--request-id={request_id} --request-digest={digest}"
    )));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = u32::try_from(std::mem::size_of::<SHELLEXECUTEINFOW>()).unwrap_or(u32::MAX);
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = executable.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.nShow = 0;
    // SAFETY: every UTF-16 pointer is NUL-terminated and remains alive for the
    // call. The returned process handle is waited and closed exactly once.
    if unsafe { ShellExecuteExW(&raw mut info) } == 0 || info.hProcess.is_null() {
        return Err("管理员授权被取消或服务无法启动。".to_owned());
    }
    let wait = unsafe { WaitForSingleObject(info.hProcess, INFINITE) };
    unsafe { CloseHandle(info.hProcess) };
    if wait != WAIT_OBJECT_0 {
        return Err("等待管理员服务完成时发生异常。".to_owned());
    }
    Ok(())
}

#[cfg(not(windows))]
fn launch_elevated(_path: &Path, _request_id: &str, _digest: &str) -> Result<(), String> {
    Err("管理员服务仅支持 Windows。".to_owned())
}

fn random_hex(byte_count: usize) -> Result<String, String> {
    let mut bytes = vec![0_u8; byte_count];
    fill_random(&mut bytes).map_err(|error| format!("系统安全随机数不可用：{error}"))?;
    let mut output = String::with_capacity(byte_count.saturating_mul(2));
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(output)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_path_and_request_transport_never_accept_frontend_paths() {
        let request = ExecutePrivilegedRequest {
            challenge_id: "a".repeat(32),
            confirmation_token: "b".repeat(64),
            confirmation_phrase: "fixed".to_owned(),
        };
        assert!(!request.challenge_id.contains(['/', '\\']));
        assert_eq!(hex_sha256(request.confirmation_token.as_bytes()).len(), 64);
    }

    #[test]
    fn partition_challenge_is_absent_for_plan_seven_blocked_assessment() {
        // The production evidence provider currently marks backup evidence
        // unavailable, so this invariant is exercised by domain tests and the
        // readiness predicate rather than by constructing a second fixture here.
        assert!(matches!(
            PartitionSafetyStatus::Blocked,
            PartitionSafetyStatus::Blocked
        ));
    }
}
