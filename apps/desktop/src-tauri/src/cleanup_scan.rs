//! Read-only cleanup rules for allow-listed browser cache locations.

use std::fs;
use std::path::{Path, PathBuf};

use clarity_core::{
    CleanupCandidate, CleanupError, CleanupPreview, ScanProgress, ScanStatus, SuggestionRisk,
};

const BROWSER_CACHE_RULE_ID: &str = "browser-cache.v1";
const THUMBNAIL_CACHE_RULE_ID: &str = "thumbnail-cache.v1";
const USER_TEMP_RULE_ID: &str = "user-temp.v1";

struct CleanupRule {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    paths: Vec<PathBuf>,
    risk: SuggestionRisk,
    recoverable: bool,
    default_selected: bool,
}

/// Scans all currently enabled low-risk cleanup roots without deleting,
/// opening, or modifying files.
pub fn scan_cleanup_preview() -> Result<CleanupPreview, CleanupScanError> {
    let scan = ScanProgress {
        scan_id: format!("scan-{}", std::process::id()),
        status: ScanStatus::Scanning,
        scanned_items: 0,
        skipped_items: 0,
        message: "正在读取浏览器缓存目录".to_owned(),
    };
    let mut candidates = Vec::new();
    let mut scanned_items = 0_u64;
    let mut skipped_items = 0_u64;

    for rule in cleanup_rules() {
        let mut bytes = 0_u64;
        let mut items = 0_u64;
        for path in &rule.paths {
            if !path.exists() {
                continue;
            }
            let (path_bytes, path_items) =
                match measure_tree(path, &mut scanned_items, &mut skipped_items) {
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
                .checked_add(path_bytes)
                .ok_or(CleanupScanError::SizeOverflow)?;
            items = items.saturating_add(path_items);
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
        });
    }

    let completed_scan = ScanProgress {
        status: ScanStatus::Completed,
        scanned_items,
        skipped_items,
        message: "浏览器缓存扫描完成（仅预览）".to_owned(),
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

fn measure_tree(
    root: &Path,
    scanned_items: &mut u64,
    skipped_items: &mut u64,
) -> Result<(u64, u64), CleanupScanError> {
    let metadata = fs::symlink_metadata(root).map_err(|error| CleanupScanError::Read {
        path: root.to_path_buf(),
        source: error,
    })?;
    if metadata.file_type().is_symlink() {
        *skipped_items = skipped_items.saturating_add(1);
        return Ok((0, 0));
    }
    if metadata.is_file() {
        *scanned_items = scanned_items.saturating_add(1);
        return Ok((metadata.len(), 1));
    }
    if !metadata.is_dir() {
        *skipped_items = skipped_items.saturating_add(1);
        return Ok((0, 0));
    }

    let mut total_bytes = 0_u64;
    let mut total_items = 0_u64;
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
            Ok((bytes, items)) => {
                total_bytes = total_bytes
                    .checked_add(bytes)
                    .ok_or(CleanupScanError::SizeOverflow)?;
                total_items = total_items.saturating_add(items);
            }
            Err(CleanupScanError::Read { .. }) => {
                *skipped_items = skipped_items.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Ok((total_bytes, total_items))
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

        assert_eq!(result, (6, 2));
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

        assert_eq!(result, (6, 1));
        assert_eq!(scanned, 1);
        assert_eq!(skipped, 1);
        let _ = fs::remove_dir_all(root);
    }
}
