//! Read-only cleanup rules for allow-listed browser cache locations.

use std::fs;
use std::path::{Path, PathBuf};

use clarity_core::{
    CleanupCandidate, CleanupError, CleanupPreview, ScanProgress, ScanStatus, SuggestionRisk,
};

const RULE_ID: &str = "browser-cache.v1";

/// Scans browser cache roots without deleting, opening, or modifying files.
pub fn scan_browser_caches() -> Result<CleanupPreview, CleanupScanError> {
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

    for (browser, root) in browser_cache_roots() {
        if !root.exists() {
            continue;
        }

        let (bytes, items) = match measure_tree(&root, &mut scanned_items, &mut skipped_items) {
            Ok(result) => result,
            // A locked or concurrently removed cache must not make the whole
            // read-only preview unusable; retain the skip count as provenance.
            Err(CleanupScanError::Read { .. }) => {
                skipped_items = skipped_items.saturating_add(1);
                continue;
            }
            Err(error) => return Err(error),
        };
        if bytes == 0 {
            continue;
        }

        candidates.push(CleanupCandidate {
            id: format!("browser-cache:{browser}"),
            rule_id: RULE_ID.to_owned(),
            title: format!("{browser} 缓存"),
            description: "可由浏览器重新生成的缓存内容，不会直接删除文件".to_owned(),
            path: root.to_string_lossy().into_owned(),
            bytes,
            item_count: items,
            risk: SuggestionRisk::Safe,
            recoverable: true,
            default_selected: true,
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

fn browser_cache_roots() -> Vec<(&'static str, PathBuf)> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let local_app_data = PathBuf::from(local_app_data);
    vec![
        (
            "Chrome",
            local_app_data.join("Google/Chrome/User Data/Default/Cache"),
        ),
        (
            "Edge",
            local_app_data.join("Microsoft/Edge/User Data/Default/Cache"),
        ),
        (
            "Brave",
            local_app_data.join("BraveSoftware/Brave-Browser/User Data/Default/Cache"),
        ),
    ]
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
        if !path.starts_with(root) {
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
