//! Versioned cleanup adapters shared by discovery and restricted execution.
//!
//! Keeping the allow-listed roots in one module prevents the scanner and
//! executor from drifting apart. UI input is never used to construct these paths.

use std::path::{Path, PathBuf};

/// Execution shape for a fixed cleanup adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdapterTarget {
    /// Enumerate direct children of each configured directory.
    RootContents,
    /// Operate only on the exact files returned by the adapter.
    ExactItems,
    /// Delegate to the Windows Shell Recycle Bin API.
    WindowsRecycleBin,
    /// This rule is intentionally read-only.
    ReadOnly,
}

/// Immutable policy description for one versioned cleanup adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CleanupAdapter {
    /// Stable versioned rule identifier.
    pub(crate) rule_id: &'static str,
    /// Allowed execution shape.
    pub(crate) target: AdapterTarget,
    /// Whether quarantine execution is permitted.
    pub(crate) quarantine: bool,
}

pub(crate) const USER_TEMP: CleanupAdapter = CleanupAdapter {
    rule_id: "user-temp.v1",
    target: AdapterTarget::RootContents,
    quarantine: true,
};
pub(crate) const BROWSER_CACHE: CleanupAdapter = CleanupAdapter {
    rule_id: "browser-cache.v1",
    target: AdapterTarget::RootContents,
    quarantine: true,
};
pub(crate) const THUMBNAIL_CACHE: CleanupAdapter = CleanupAdapter {
    rule_id: "thumbnail-cache.v1",
    target: AdapterTarget::ExactItems,
    quarantine: true,
};
pub(crate) const BUILD_CACHE: CleanupAdapter = CleanupAdapter {
    rule_id: "build-cache.v1",
    target: AdapterTarget::RootContents,
    quarantine: true,
};
pub(crate) const RECYCLE_BIN: CleanupAdapter = CleanupAdapter {
    rule_id: "recycle-bin.v1",
    target: AdapterTarget::WindowsRecycleBin,
    quarantine: false,
};
pub(crate) const WINDOWS_UPDATE: CleanupAdapter = CleanupAdapter {
    rule_id: "windows-update-download-cache.v1",
    target: AdapterTarget::ReadOnly,
    quarantine: false,
};

/// Returns the reviewed adapter for a versioned rule ID.
pub(crate) fn adapter_for(rule_id: &str) -> Option<CleanupAdapter> {
    [
        USER_TEMP,
        BROWSER_CACHE,
        THUMBNAIL_CACHE,
        BUILD_CACHE,
        RECYCLE_BIN,
        WINDOWS_UPDATE,
    ]
    .into_iter()
    .find(|adapter| adapter.rule_id == rule_id)
}

/// Resolves dynamic allow-listed roots for a rule from trusted process
/// environment variables. The caller must still validate every existing path.
pub(crate) fn allowed_roots(rule_id: &str) -> Vec<PathBuf> {
    match rule_id {
        "user-temp.v1" => temp_paths(),
        "browser-cache.v1" => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map_or_else(Vec::new, |root| {
                vec![
                    root.join("Google/Chrome/User Data/Default/Cache"),
                    root.join("Microsoft/Edge/User Data/Default/Cache"),
                    root.join("BraveSoftware/Brave-Browser/User Data/Default/Cache"),
                ]
            }),
        "thumbnail-cache.v1" => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map_or_else(Vec::new, |root| {
                let root = root.join("Microsoft/Windows/Explorer");
                [32_u32, 96, 256, 768, 1280, 1600, 1920, 2560]
                    .into_iter()
                    .map(|size| root.join(format!("thumbcache_{size}.db")))
                    .collect()
            }),
        "build-cache.v1" => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map_or_else(Vec::new, |root| {
                vec![
                    root.join("npm-cache"),
                    root.join("Yarn/Cache"),
                    root.join("pnpm/store"),
                    root.join("NuGet/Cache"),
                ]
            }),
        "recycle-bin.v1" => std::env::var_os("SystemDrive")
            .map(PathBuf::from)
            .map_or_else(Vec::new, |drive| vec![drive.join("$Recycle.Bin")]),
        "windows-update-download-cache.v1" => std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .map_or_else(Vec::new, |root| {
                vec![root.join("SoftwareDistribution/Download")]
            }),
        _ => Vec::new(),
    }
}

/// Compares backend execution roots with the current adapter roots exactly.
pub(crate) fn roots_match(rule_id: &str, candidate_roots: &[String]) -> bool {
    let mut expected: Vec<_> = allowed_roots(rule_id)
        .into_iter()
        // Discovery includes only roots that existed in the immutable scan;
        // execution resolves the same dynamic adapter immediately beforehand.
        .filter(|path| path.exists())
        .filter_map(|path| canonical_text(&path))
        .collect();
    let mut actual: Vec<_> = candidate_roots
        .iter()
        .filter_map(|path| canonical_text(Path::new(path)))
        .collect();
    expected.sort_unstable();
    actual.sort_unstable();
    expected == actual
}

fn canonical_text(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    Some(path.to_string_lossy().to_ascii_lowercase())
}

fn temp_paths() -> Vec<PathBuf> {
    std::env::var_os("TEMP")
        .or_else(|| std::env::var_os("TMP"))
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{AdapterTarget, adapter_for};

    #[test]
    fn adapters_have_fixed_execution_shapes() {
        assert_eq!(
            adapter_for("thumbnail-cache.v1").unwrap().target,
            AdapterTarget::ExactItems
        );
        assert_eq!(
            adapter_for("windows-update-download-cache.v1")
                .unwrap()
                .target,
            AdapterTarget::ReadOnly
        );
        assert!(!adapter_for("recycle-bin.v1").unwrap().quarantine);
    }
}
