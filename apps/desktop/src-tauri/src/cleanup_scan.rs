//! Read-only cleanup rules for allow-listed browser cache locations.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_core::{
    CleanupCandidate, CleanupError, CleanupPreview, ScanProgress, ScanStatus, SuggestionRisk,
};
use sha2::{Digest, Sha256};

const BROWSER_CACHE_RULE_ID: &str = "browser-cache.v1";
const THUMBNAIL_CACHE_RULE_ID: &str = "thumbnail-cache.v1";
const USER_TEMP_RULE_ID: &str = "user-temp.v1";
const RECYCLE_BIN_RULE_ID: &str = "recycle-bin.v1";
const BUILD_CACHE_RULE_ID: &str = "build-cache.v1";

struct CleanupRule {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    paths: Vec<PathBuf>,
    risk: SuggestionRisk,
    recoverable: bool,
    default_selected: bool,
}

struct TreeMeasurement {
    bytes: u64,
    items: u64,
    metadata_digest: String,
}

/// Scans all currently enabled low-risk cleanup roots without deleting,
/// opening, or modifying files.
pub fn scan_cleanup_preview() -> Result<CleanupPreview, CleanupScanError> {
    let scan = ScanProgress {
        scan_id: format!("scan-{}", std::process::id()),
        status: ScanStatus::Scanning,
        scanned_items: 0,
        skipped_items: 0,
        message: "正在读取允许的清理目录".to_owned(),
        source_volume_id: std::env::var("SystemDrive").ok(),
    };
    let mut candidates = Vec::new();
    let mut scanned_items = 0_u64;
    let mut skipped_items = 0_u64;

    for rule in cleanup_rules() {
        let mut bytes = 0_u64;
        let mut items = 0_u64;
        let mut metadata_hasher = Sha256::new();
        let mut observed_at_unix_ms = None;
        for path in &rule.paths {
            if !path.exists() {
                continue;
            }
            let measurement = match measure_tree(path, &mut scanned_items, &mut skipped_items) {
                Ok(result) => result,
                // A locked or concurrently removed cache must not make the whole
                // read-only preview unusable; retain the skip count as provenance.
                Err(CleanupScanError::Read { .. }) => {
                    skipped_items = skipped_items.saturating_add(1);
                    continue;
                }
                Err(error) => return Err(error),
            };
            bytes = bytes
                .checked_add(measurement.bytes)
                .ok_or(CleanupScanError::SizeOverflow)?;
            items = items.saturating_add(measurement.items);
            metadata_hasher.update(measurement.metadata_digest.as_bytes());
            observed_at_unix_ms = Some(current_unix_ms());
        }
        if bytes == 0 {
            continue;
        }

        candidates.push(CleanupCandidate {
            id: rule.id.to_owned(),
            rule_id: rule.id.to_owned(),
            title: rule.title.to_owned(),
            description: rule.description.to_owned(),
            path: rule
                .paths
                .first()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes,
            item_count: items,
            risk: rule.risk,
            recoverable: rule.recoverable,
            default_selected: rule.default_selected,
            metadata_digest: format_digest(metadata_hasher.finalize()),
            observed_at_unix_ms,
        });
    }

    let completed_scan = ScanProgress {
        status: ScanStatus::Completed,
        scanned_items,
        skipped_items,
        message: "清理扫描完成（仅预览）".to_owned(),
        ..scan
    };

    CleanupPreview::completed(completed_scan, candidates).map_err(CleanupScanError::Preview)
}

fn cleanup_rules() -> Vec<CleanupRule> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let local_app_data = PathBuf::from(local_app_data);
    vec![
        CleanupRule {
            id: BROWSER_CACHE_RULE_ID,
            title: "浏览器缓存",
            description: "Chrome、Edge 与 Brave 可重新生成的缓存内容，不会直接删除文件",
            paths: vec![
                local_app_data.join("Google/Chrome/User Data/Default/Cache"),
                local_app_data.join("Microsoft/Edge/User Data/Default/Cache"),
                local_app_data.join("BraveSoftware/Brave-Browser/User Data/Default/Cache"),
            ],
            risk: SuggestionRisk::Safe,
            recoverable: true,
            default_selected: true,
        },
        CleanupRule {
            id: THUMBNAIL_CACHE_RULE_ID,
            title: "缩略图缓存",
            description: "Windows 可重新生成的缩略图数据库，不会直接删除文件",
            paths: thumbnail_cache_paths(&local_app_data),
            risk: SuggestionRisk::Safe,
            recoverable: true,
            default_selected: true,
        },
        CleanupRule {
            id: USER_TEMP_RULE_ID,
            title: "用户临时文件",
            description: "应用运行产生的临时内容，正在使用的项目可能会被跳过",
            paths: temp_paths(),
            risk: SuggestionRisk::Review,
            recoverable: true,
            default_selected: false,
        },
        CleanupRule {
            id: RECYCLE_BIN_RULE_ID,
            title: "回收站",
            description: "已移入 Windows 回收站的项目，永久清空前仍可恢复",
            paths: recycle_bin_paths(),
            risk: SuggestionRisk::Review,
            recoverable: true,
            default_selected: false,
        },
        CleanupRule {
            id: BUILD_CACHE_RULE_ID,
            title: "应用构建缓存",
            description: "包管理器和开发工具可重新生成的缓存，首次构建可能变慢",
            paths: build_cache_paths(&local_app_data),
            risk: SuggestionRisk::Review,
            recoverable: true,
            default_selected: false,
        },
    ]
}

fn thumbnail_cache_paths(local_app_data: &Path) -> Vec<PathBuf> {
    let root = local_app_data.join("Microsoft/Windows/Explorer");
    [32_u32, 96, 256, 768, 1280, 1600, 1920, 2560]
        .into_iter()
        .map(|size| root.join(format!("thumbcache_{size}.db")))
        .collect()
}

fn temp_paths() -> Vec<PathBuf> {
    std::env::var_os("TEMP")
        .or_else(|| std::env::var_os("TMP"))
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

fn recycle_bin_paths() -> Vec<PathBuf> {
    let Some(system_drive) = std::env::var_os("SystemDrive") else {
        return Vec::new();
    };
    vec![PathBuf::from(system_drive).join("$Recycle.Bin")]
}

fn build_cache_paths(local_app_data: &Path) -> Vec<PathBuf> {
    vec![
        local_app_data.join("npm-cache"),
        local_app_data.join("Yarn/Cache"),
        local_app_data.join("pnpm/store"),
        local_app_data.join("NuGet/Cache"),
    ]
}

fn measure_tree(
    root: &Path,
    scanned_items: &mut u64,
    skipped_items: &mut u64,
) -> Result<TreeMeasurement, CleanupScanError> {
    let metadata = fs::symlink_metadata(root).map_err(|error| CleanupScanError::Read {
        path: root.to_path_buf(),
        source: error,
    })?;
    if metadata.file_type().is_symlink() {
        *skipped_items = skipped_items.saturating_add(1);
        return Ok(TreeMeasurement {
            bytes: 0,
            items: 0,
            metadata_digest: String::new(),
        });
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
        return Ok(TreeMeasurement {
            bytes: 0,
            items: 0,
            metadata_digest: String::new(),
        });
    }

    let mut total_bytes = 0_u64;
    let mut total_items = 0_u64;
    let mut hasher = Sha256::new();
    let entries = fs::read_dir(root).map_err(|error| CleanupScanError::Read {
        path: root.to_path_buf(),
        source: error,
    })?;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                *skipped_items = skipped_items.saturating_add(1);
                continue;
            }
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

fn format_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn is_allowed_path(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
}

/// Errors produced while measuring allow-listed cache roots.
#[derive(Debug, thiserror::Error)]
pub enum CleanupScanError {
    /// An allow-listed path could not be read; the candidate is omitted.
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

    use super::{is_allowed_path, measure_tree};

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
        assert_eq!(scanned, 2);
        assert_eq!(skipped, 0);
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

    #[cfg(unix)]
    #[test]
    fn skips_symbolic_links() {
        use std::os::unix::fs::symlink;

        let root = test_root("symlink");
        let target = root.join("target.tmp");
        fs::write(&target, b"target").expect("target should be written");
        symlink(&target, root.join("link.tmp")).expect("symlink should be created");

        let mut scanned = 0;
        let mut skipped = 0;
        let result = measure_tree(&root, &mut scanned, &mut skipped)
            .expect("symlink should be skipped safely");

        assert_eq!(result.bytes, 6);
        assert_eq!(result.items, 1);
        assert_eq!(scanned, 1);
        assert_eq!(skipped, 1);
        let _ = fs::remove_dir_all(root);
    }
}
