//! Shared safe replacement helpers for small local JSON state files.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

pub(crate) fn state_path(file_name: &str) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("ClarityDisk/state")
        .join(file_name)
}

pub(crate) fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub(crate) fn write_json<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> Result<(), StateStoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(StateStoreError::CreateDirectory)?;
    }
    let encoded = serde_json::to_vec_pretty(value).map_err(StateStoreError::Serialize)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, encoded).map_err(StateStoreError::WriteTemporary)?;

    // Windows does not replace an existing destination atomically. Keep the
    // last valid JSON file as a backup until the new file is in place.
    let backup = path.with_extension("json.backup");
    let _ = fs::remove_file(&backup);
    let had_previous = path.exists() && fs::rename(path, &backup).is_ok();
    if let Err(source) = fs::rename(&temporary, path) {
        if had_previous {
            let _ = fs::rename(&backup, path);
        }
        return Err(StateStoreError::Replace(source));
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum StateStoreError {
    #[error("could not create local state directory: {0}")]
    CreateDirectory(std::io::Error),
    #[error("could not serialize local state: {0}")]
    Serialize(serde_json::Error),
    #[error("could not write temporary local state: {0}")]
    WriteTemporary(std::io::Error),
    #[error("could not replace local state: {0}")]
    Replace(std::io::Error),
}
