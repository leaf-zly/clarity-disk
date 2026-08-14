//! Read-only cleanup rules for strictly allow-listed Windows locations.

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cleanup_adapters::allowed_roots;
use clarity_core::{
    CleanupCandidate, CleanupError, CleanupPreview, CleanupRuleAvailability, CleanupRuleStatus,
    RecoveryStrategy, ScanProgress, ScanStatus, SuggestionRisk,
};
use sha2::{Digest, Sha256};

const BROWSER_CACHE_RULE_ID: &str = "browser-cache.v1";
const THUMBNAIL_CACHE_RULE_ID: &str = "thumbnail-cache.v1";
const USER_TEMP_RULE_ID: &str = "user-temp.v1";
const RECYCLE_BIN_RULE_ID: &str = "recycle-bin.v1";
const BUILD_CACHE_RULE_ID: &str = "build-cache.v1";
const WINDOWS_UPDATE_CACHE_RULE_ID: &str = "windows-update-download-cache.v1";

// Rule definitions intentionally combine independent policy dimensions;
// splitting them would make every rule constructor harder to audit.
#[allow(clippy::struct_excessive_bools)]
struct CleanupRule {
    id: &'static str,
    version: &'static str,
    title: &'static str,
    description: &'static str,
    evidence: &'static str,
    paths: Vec<PathBuf>,
    unavailable_reason: Option<&'static str>,
    risk: SuggestionRisk,
    recoverable: bool,
    requires_admin: bool,
    recovery_strategy: RecoveryStrategy,
    quarantine_eligible: bool,
    default_selected: bool,
}

struct TreeMeasurement {
    bytes: u64,
    items: u64,
    metadata_digest: String,
}

/// Scans every enabled cleanup rule without deleting, moving, or opening files.
///
/// # Errors
///
/// Returns an error only for arithmetic overflow or an invalid domain result;
/// inaccessible allow-listed roots are represented as rule statuses instead.
#[allow(clippy::too_many_lines)]
pub fn scan_cleanup_preview() -> Result<CleanupPreview, CleanupScanError> {
    let now = current_unix_ms();
    let scan = ScanProgress {
        scan_id: format!("cleanup-{}-{now}", std::process::id()),
        status: ScanStatus::Scanning,
        scanned_items: 0,
        skipped_items: 0,
        message: "正在读取允许的清理目录".to_owned(),
        source_volume_id: std::env::var("SystemDrive").ok(),
    };
    let mut candidates = Vec::new();
    let mut rule_statuses = Vec::new();
    let mut scanned_items = 0_u64;
    let mut skipped_items = 0_u64;

    for rule in cleanup_rules() {
        if rule.paths.is_empty() {
            rule_statuses.push(rule_status(
                &rule,
                CleanupRuleAvailability::Unavailable,
                rule.unavailable_reason,
            ));
            continue;
        }
        let mut bytes = 0_u64;
        let mut items = 0_u64;
        let mut metadata_hasher = Sha256::new();
        let mut readable_root = false;
        let mut inaccessible_root = false;
        let existing_roots: Vec<_> = rule.paths.iter().filter(|path| path.exists()).collect();

        for path in &existing_roots {
            match measure_tree(path, &mut scanned_items, &mut skipped_items) {
                Ok(measurement) => {
                    readable_root = true;
                    bytes = bytes
                        .checked_add(measurement.bytes)
                        .ok_or(CleanupScanError::SizeOverflow)?;
                    items = items.saturating_add(measurement.items);
                    metadata_hasher.update(measurement.metadata_digest.as_bytes());
                }
                // Locked and access-denied roots are evidence that the rule was
                // evaluated but unavailable; they never trigger elevation here.
                Err(CleanupScanError::Read { .. }) => {
                    inaccessible_root = true;
                    skipped_items = skipped_items.saturating_add(1);
                }
                Err(error) => return Err(error),
            }
        }

        if bytes > 0 {
            let paths = existing_roots
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            candidates.push(CleanupCandidate {
                id: rule.id.to_owned(),
                rule_id: rule.id.to_owned(),
                rule_version: rule.version.to_owned(),
                title: rule.title.to_owned(),
                description: rule.description.to_owned(),
                path: paths.join("；"),
                execution_roots: paths,
                evidence: vec![
                    rule.evidence.to_owned(),
                    format!("只读统计到 {items} 个项目，共 {bytes} 字节"),
                ],
                bytes,
                item_count: items,
                risk: rule.risk,
                recoverable: rule.recoverable,
                requires_admin: rule.requires_admin,
                recovery_strategy: rule.recovery_strategy,
                quarantine_eligible: rule.quarantine_eligible,
                default_selected: rule.default_selected,
                metadata_digest: format_digest(metadata_hasher.finalize()),
                observed_at_unix_ms: Some(now),
            });
            rule_statuses.push(rule_status(&rule, CleanupRuleAvailability::Available, None));
        } else {
            let (availability, reason) = if inaccessible_root && !readable_root {
                (
                    CleanupRuleAvailability::Unavailable,
                    Some("当前权限无法读取该规则的固定目录，未执行任何修改"),
                )
            } else if existing_roots.is_empty() {
                (
                    CleanupRuleAvailability::Unavailable,
                    Some("固定允许目录不存在，已安全跳过"),
                )
            } else {
                (
                    CleanupRuleAvailability::Empty,
                    Some("固定允许目录中未发现可预览内容"),
                )
            };
            rule_statuses.push(rule_status(&rule, availability, reason));
        }
    }

    let completed_scan = ScanProgress {
        status: ScanStatus::Completed,
        scanned_items,
        skipped_items,
        message: "清理扫描完成（仅预览）".to_owned(),
        ..scan
    };
    CleanupPreview::completed(completed_scan, candidates, rule_statuses)
        .map_err(CleanupScanError::Preview)
}

fn rule_status(
    rule: &CleanupRule,
    availability: CleanupRuleAvailability,
    reason: Option<&str>,
) -> CleanupRuleStatus {
    CleanupRuleStatus {
        rule_id: rule.id.to_owned(),
        title: rule.title.to_owned(),
        availability,
        reason: reason.map(str::to_owned),
        requires_admin: rule.requires_admin,
        risk: rule.risk,
    }
}

#[allow(clippy::too_many_lines)]
fn cleanup_rules() -> Vec<CleanupRule> {
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let system_root = std::env::var_os("SystemRoot").map(PathBuf::from);
    let local_missing = local_app_data.is_none();
    let system_missing = system_root.is_none();
    let local_paths = |builder: fn(&Path) -> Vec<PathBuf>| {
        local_app_data.as_deref().map(builder).unwrap_or_default()
    };
    vec![
        CleanupRule {
            id: BROWSER_CACHE_RULE_ID,
            version: "1",
            title: "浏览器缓存",
            description: "Chrome、Edge 与 Brave 可重新生成的缓存内容",
            evidence: "命中受支持浏览器的固定 Cache 目录",
            paths: local_paths(browser_cache_paths),
            unavailable_reason: local_missing.then_some("LOCALAPPDATA 不可用，规则已安全跳过"),
            risk: SuggestionRisk::Safe,
            recoverable: true,
            requires_admin: false,
            recovery_strategy: RecoveryStrategy::Regenerate,
            quarantine_eligible: true,
            default_selected: true,
        },
        CleanupRule {
            id: THUMBNAIL_CACHE_RULE_ID,
            version: "1",
            title: "缩略图缓存",
            description: "Windows 可重新生成的缩略图数据库",
            evidence: "命中 Windows Explorer 的固定缩略图数据库名称",
            paths: local_paths(thumbnail_cache_paths),
            unavailable_reason: local_missing.then_some("LOCALAPPDATA 不可用，规则已安全跳过"),
            risk: SuggestionRisk::Safe,
            recoverable: true,
            requires_admin: false,
            recovery_strategy: RecoveryStrategy::Regenerate,
            quarantine_eligible: true,
            default_selected: true,
        },
        CleanupRule {
            id: USER_TEMP_RULE_ID,
            version: "1",
            title: "用户临时文件",
            description: "应用产生的临时内容，正在使用的项目会被跳过",
            evidence: "命中当前用户 TEMP/TMP 的操作系统环境目录",
            paths: temp_paths(),
            unavailable_reason: Some("TEMP 与 TMP 均不可用，规则已安全跳过"),
            risk: SuggestionRisk::Review,
            recoverable: true,
            requires_admin: false,
            recovery_strategy: RecoveryStrategy::Quarantine,
            quarantine_eligible: true,
            default_selected: false,
        },
        CleanupRule {
            id: RECYCLE_BIN_RULE_ID,
            version: "1",
            title: "回收站",
            description: "已移入 Windows 回收站的内容，永久清空后不可恢复",
            evidence: "命中系统卷固定 $Recycle.Bin 目录",
            paths: recycle_bin_paths(),
            unavailable_reason: Some("SystemDrive 不可用，规则已安全跳过"),
            risk: SuggestionRisk::Review,
            recoverable: true,
            requires_admin: false,
            recovery_strategy: RecoveryStrategy::WindowsManaged,
            quarantine_eligible: false,
            default_selected: false,
        },
        CleanupRule {
            id: BUILD_CACHE_RULE_ID,
            version: "1",
            title: "应用构建缓存",
            description: "包管理器和开发工具可重新生成的缓存，首次构建可能变慢",
            evidence: "命中受支持开发工具的固定本机缓存目录",
            paths: local_paths(build_cache_paths),
            unavailable_reason: local_missing.then_some("LOCALAPPDATA 不可用，规则已安全跳过"),
            risk: SuggestionRisk::Review,
            recoverable: true,
            requires_admin: false,
            recovery_strategy: RecoveryStrategy::Regenerate,
            quarantine_eligible: true,
            default_selected: false,
        },
        CleanupRule {
            id: WINDOWS_UPDATE_CACHE_RULE_ID,
            version: "1",
            title: "Windows 更新下载缓存",
            description: "Windows Update 已下载的安装缓存；本阶段仅统计，不停止服务、不删除",
            evidence: "仅命中 SystemRoot\\SoftwareDistribution\\Download 固定目录",
            paths: allowed_roots(WINDOWS_UPDATE_CACHE_RULE_ID),
            unavailable_reason: system_missing.then_some("SystemRoot 不可用，规则已安全跳过"),
            risk: SuggestionRisk::ConfirmationRequired,
            recoverable: true,
            requires_admin: true,
            recovery_strategy: RecoveryStrategy::WindowsManaged,
            quarantine_eligible: false,
            default_selected: false,
        },
    ]
}

fn browser_cache_paths(local_app_data: &Path) -> Vec<PathBuf> {
    let _ = local_app_data;
    allowed_roots(BROWSER_CACHE_RULE_ID)
}

fn thumbnail_cache_paths(local_app_data: &Path) -> Vec<PathBuf> {
    let _ = local_app_data;
    allowed_roots(THUMBNAIL_CACHE_RULE_ID)
}

fn temp_paths() -> Vec<PathBuf> {
    allowed_roots(USER_TEMP_RULE_ID)
}

fn recycle_bin_paths() -> Vec<PathBuf> {
    allowed_roots(RECYCLE_BIN_RULE_ID)
}

fn build_cache_paths(local_app_data: &Path) -> Vec<PathBuf> {
    let _ = local_app_data;
    allowed_roots(BUILD_CACHE_RULE_ID)
}

fn measure_tree(
    root: &Path,
    scanned_items: &mut u64,
    skipped_items: &mut u64,
) -> Result<TreeMeasurement, CleanupScanError> {
    let metadata = fs::symlink_metadata(root).map_err(|source| CleanupScanError::Read {
        path: root.to_path_buf(),
        source,
    })?;
    // Reparse-like indirection is never traversed. On supported platforms,
    // symlink metadata exposes the unsafe boundary without opening the target.
    if is_unsafe_indirection(&metadata) {
        *skipped_items = skipped_items.saturating_add(1);
        return Ok(empty_measurement());
    }
    if metadata.is_file() {
        *scanned_items = scanned_items.saturating_add(1);
        let mut hasher = Sha256::new();
        hasher.update(root.to_string_lossy().as_bytes());
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos().to_le_bytes().to_vec())
                .unwrap_or_default(),
        );
        return Ok(TreeMeasurement {
            bytes: metadata.len(),
            items: 1,
            metadata_digest: format_digest(hasher.finalize()),
        });
    }
    if !metadata.is_dir() {
        *skipped_items = skipped_items.saturating_add(1);
        return Ok(empty_measurement());
    }

    let mut total_bytes = 0_u64;
    let mut total_items = 0_u64;
    let mut hasher = Sha256::new();
    let entries = fs::read_dir(root).map_err(|source| CleanupScanError::Read {
        path: root.to_path_buf(),
        source,
    })?;
    let mut entries: Vec<_> = entries.collect();
    // Metadata digests must be stable across fresh validation scans; Windows
    // directory enumeration order is not a contractual ordering.
    entries.sort_by_key(|entry| entry.as_ref().ok().map(std::fs::DirEntry::path));
    for entry in entries {
        let Ok(entry) = entry else {
            *skipped_items = skipped_items.saturating_add(1);
            continue;
        };
        let path = entry.path();
        if !is_allowed_path(root, &path) {
            *skipped_items = skipped_items.saturating_add(1);
            continue;
        }
        match measure_tree(&path, scanned_items, skipped_items) {
            Ok(measurement) => {
                total_bytes = total_bytes
                    .checked_add(measurement.bytes)
                    .ok_or(CleanupScanError::SizeOverflow)?;
                total_items = total_items.saturating_add(measurement.items);
                hasher.update(measurement.metadata_digest.as_bytes());
            }
            Err(CleanupScanError::Read { .. }) => {
                *skipped_items = skipped_items.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(TreeMeasurement {
        bytes: total_bytes,
        items: total_items,
        metadata_digest: format_digest(hasher.finalize()),
    })
}

fn empty_measurement() -> TreeMeasurement {
    TreeMeasurement {
        bytes: 0,
        items: 0,
        metadata_digest: String::new(),
    }
}

fn is_unsafe_indirection(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || is_platform_reparse_point(metadata)
}

#[cfg(windows)]
fn is_platform_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_platform_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn format_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .fold(String::new(), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
}

fn is_allowed_path(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
}

/// Errors produced while measuring allow-listed cache roots.
#[derive(Debug, thiserror::Error)]
pub enum CleanupScanError {
    /// An allow-listed path could not be read; its rule records an unavailable state.
    #[error("could not read cleanup path {path}: {source}")]
    Read {
        /// Path that produced the error.
        path: PathBuf,
        /// Underlying filesystem error.
        source: std::io::Error,
    },
    /// Aggregated file sizes exceeded the representable range.
    #[error("cleanup scan size overflowed")]
    SizeOverflow,
    /// The domain rejected the generated preview.
    #[error("invalid cleanup preview: {0}")]
    Preview(#[from] CleanupError),
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{WINDOWS_UPDATE_CACHE_RULE_ID, cleanup_rules, is_allowed_path, measure_tree};
    use clarity_core::SuggestionRisk;

    fn test_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "clarity-disk-cleanup-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test root should be created");
        root
    }

    #[test]
    fn measures_files_without_mutating_them() {
        let root = test_root("measure");
        fs::write(root.join("one.tmp"), b"1234").expect("file should be written");
        fs::create_dir(root.join("nested")).expect("nested directory should be created");
        fs::write(root.join("nested/two.tmp"), b"12").expect("file should be written");
        let mut scanned = 0;
        let mut skipped = 0;
        let result =
            measure_tree(&root, &mut scanned, &mut skipped).expect("test tree should be readable");
        assert_eq!(result.bytes, 6);
        assert_eq!(result.items, 2);
        assert_eq!(
            fs::read(root.join("one.tmp")).expect("file remains"),
            b"1234"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_paths_outside_allow_listed_root() {
        let root = Path::new("C:\\Users\\demo\\Cache");
        assert!(is_allowed_path(root, &root.join("nested/file")));
        assert!(!is_allowed_path(
            root,
            Path::new("C:\\Users\\demo\\Cache-old/file")
        ));
    }

    #[test]
    fn windows_update_rule_is_privileged_confirmation_only() {
        let rule = cleanup_rules()
            .into_iter()
            .find(|rule| rule.id == WINDOWS_UPDATE_CACHE_RULE_ID)
            .expect("Windows Update rule should exist");
        assert_eq!(rule.risk, SuggestionRisk::ConfirmationRequired);
        assert!(rule.requires_admin);
        assert!(!rule.default_selected);
        assert!(!rule.quarantine_eligible);
        assert!(rule.paths.len() <= 1);
        if let Some(path) = rule.paths.first() {
            assert!(path.ends_with("SoftwareDistribution/Download"));
        }
    }
}
