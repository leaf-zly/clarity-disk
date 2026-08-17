//! Bounded read-only directory scanning with cooperative task controls.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clarity_core::{
    SpaceScanEntry, SpaceScanError, SpaceScanHistoryEntry, SpaceScanProgress, SpaceScanRequest,
    SpaceScanSnapshot, SpaceScanStart, SpaceScanStatus, SpaceScanTypeStat,
};

const MAX_DEPTH: u8 = 64;
const MAX_ENTRIES: u64 = 1_000_000;
const MAX_LARGEST: usize = 20;
const POLL_UPDATE_ITEMS: u64 = 128;

/// Builds a conservative scan request for a selected volume.
///
/// Personal folders and the current source tree are excluded only when they
/// exist beneath the selected root. The returned request is normalized by the
/// same boundary checks used for user-authored requests.
pub fn default_request(root_path: String) -> Result<SpaceScanRequest, SpaceScanError> {
    let root =
        fs::canonicalize(&root_path).map_err(|_| SpaceScanError::InvalidRoot(root_path.clone()))?;
    let mut excluded_paths = Vec::new();
    if let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for folder in ["Desktop", "Documents", "Downloads"] {
            push_existing_descendant(&root, profile.join(folder), &mut excluded_paths);
        }
    }
    if let Ok(source_tree) = std::env::current_dir() {
        push_existing_descendant(&root, source_tree, &mut excluded_paths);
    }
    normalize_request(SpaceScanRequest {
        root_path,
        max_depth: 8,
        max_entries: 100_000,
        excluded_paths,
    })
}

fn push_existing_descendant(root: &Path, candidate: PathBuf, output: &mut Vec<String>) {
    if let Ok(normalized) = fs::canonicalize(candidate)
        && normalized.is_dir()
        && normalized.starts_with(root)
        && normalized != root
    {
        output.push(normalized.to_string_lossy().into_owned());
    }
}

/// Coordinates bounded read-only scan tasks and their local terminal history.
#[derive(Clone)]
pub struct SpaceScanManager {
    tasks: Arc<Mutex<HashMap<String, Task>>>,
    history: Arc<Mutex<Vec<SpaceScanHistoryEntry>>>,
    history_path: Arc<PathBuf>,
}

struct Task {
    snapshot: SpaceScanSnapshot,
    root_path: String,
    cancel: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
}

impl Default for SpaceScanManager {
    fn default() -> Self {
        Self::with_history_path(history_path())
    }
}

impl SpaceScanManager {
    fn with_history_path(history_path: PathBuf) -> Self {
        let history = load_history(&history_path);
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            history: Arc::new(Mutex::new(history)),
            history_path: Arc::new(history_path),
        }
    }

    /// Starts a bounded read-only scan in a background thread.
    pub fn start(&self, request: SpaceScanRequest) -> Result<SpaceScanStart, SpaceScanError> {
        let request = normalize_request(request)?;
        let scan_id = format!("space-{}-{}", std::process::id(), unix_ms());
        let cancel = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let started_at_unix_ms = unix_ms_u64();
        let snapshot = empty_snapshot(&scan_id, &request.root_path, started_at_unix_ms);
        self.tasks
            .lock()
            .expect("space scan state poisoned")
            .insert(
                scan_id.clone(),
                Task {
                    snapshot,
                    root_path: request.root_path.clone(),
                    cancel: Arc::clone(&cancel),
                    paused: Arc::clone(&paused),
                },
            );

        let tasks = Arc::clone(&self.tasks);
        let history = Arc::clone(&self.history);
        let history_path = Arc::clone(&self.history_path);
        let worker_id = scan_id.clone();
        thread::spawn(move || {
            let result = run_scan(&worker_id, &request, &cancel, &paused, &tasks);
            let mut guard = tasks.lock().expect("space scan state poisoned");
            if let Some(task) = guard.get_mut(&worker_id) {
                match result {
                    Ok(snapshot) => task.snapshot = snapshot,
                    Err(error) => {
                        task.snapshot.progress.status = SpaceScanStatus::Failed;
                        task.snapshot.progress.finished_at_unix_ms = Some(unix_ms_u64());
                        task.snapshot.progress.message = error.to_string();
                    }
                }
                if let Some(entry) = history_entry(task) {
                    record_history(&history, &history_path, entry);
                }
            }
        });
        Ok(SpaceScanStart { scan_id })
    }

    /// Returns the latest incremental snapshot for a task.
    pub fn snapshot(&self, scan_id: &str) -> Result<SpaceScanSnapshot, SpaceScanError> {
        self.tasks
            .lock()
            .expect("space scan state poisoned")
            .get(scan_id)
            .map(|task| task.snapshot.clone())
            .ok_or_else(|| SpaceScanError::TaskNotFound(scan_id.to_owned()))
    }

    /// Requests cancellation at the next safe traversal boundary.
    pub fn cancel(&self, scan_id: &str) -> Result<(), SpaceScanError> {
        let guard = self.tasks.lock().expect("space scan state poisoned");
        let task = guard
            .get(scan_id)
            .ok_or_else(|| SpaceScanError::TaskNotFound(scan_id.to_owned()))?;
        task.cancel.store(true, Ordering::Release);
        Ok(())
    }

    /// Pauses a running task without terminating its worker thread.
    pub fn pause(&self, scan_id: &str) -> Result<(), SpaceScanError> {
        let mut guard = self.tasks.lock().expect("space scan state poisoned");
        let task = guard
            .get_mut(scan_id)
            .ok_or_else(|| SpaceScanError::TaskNotFound(scan_id.to_owned()))?;
        if task.snapshot.progress.status == SpaceScanStatus::Scanning {
            task.paused.store(true, Ordering::Release);
            task.snapshot.progress.status = SpaceScanStatus::Paused;
            "扫描已暂停".clone_into(&mut task.snapshot.progress.message);
        }
        Ok(())
    }

    /// Resumes a cooperatively paused task.
    pub fn resume(&self, scan_id: &str) -> Result<(), SpaceScanError> {
        let mut guard = self.tasks.lock().expect("space scan state poisoned");
        let task = guard
            .get_mut(scan_id)
            .ok_or_else(|| SpaceScanError::TaskNotFound(scan_id.to_owned()))?;
        if task.snapshot.progress.status == SpaceScanStatus::Paused {
            task.paused.store(false, Ordering::Release);
            task.snapshot.progress.status = SpaceScanStatus::Scanning;
            "正在分析目录空间".clone_into(&mut task.snapshot.progress.message);
        }
        Ok(())
    }

    /// Returns up to twenty terminal summaries, newest first.
    pub fn history(&self) -> Vec<SpaceScanHistoryEntry> {
        let mut history = self
            .history
            .lock()
            .expect("space scan history poisoned")
            .clone();
        history.sort_by_key(|entry| Reverse(entry.finished_at_unix_ms));
        history.truncate(20);
        history
    }

    /// Clears terminal scan summaries while leaving active tasks untouched.
    ///
    /// # Errors
    ///
    /// Returns an error when the empty history document cannot be persisted.
    pub fn clear_history(&self) -> Result<(), String> {
        let mut history = self.history.lock().expect("space scan history poisoned");
        crate::state_store::write_json(&self.history_path, &Vec::<SpaceScanHistoryEntry>::new())
            .map_err(|error| error.to_string())?;
        history.clear();
        Ok(())
    }
}

fn history_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("ClarityDisk/state/space-scan-history.json")
}

fn load_history(path: &Path) -> Vec<SpaceScanHistoryEntry> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn record_history(
    history: &Mutex<Vec<SpaceScanHistoryEntry>>,
    path: &Path,
    entry: SpaceScanHistoryEntry,
) {
    let mut guard = history.lock().expect("space scan history poisoned");
    guard.retain(|existing| existing.scan_id != entry.scan_id);
    guard.push(entry);
    guard.sort_by_key(|item| Reverse(item.finished_at_unix_ms));
    guard.truncate(20);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(encoded) = serde_json::to_vec_pretty(&*guard) {
        let temporary = path.with_extension("json.tmp");
        if fs::write(&temporary, encoded).is_ok() {
            // Windows cannot rename over an existing destination. Preserve the
            // previous valid file until the replacement is fully encoded.
            let backup = path.with_extension("json.backup");
            let _ = fs::remove_file(&backup);
            let had_previous = path.exists() && fs::rename(path, &backup).is_ok();
            if fs::rename(&temporary, path).is_ok() {
                let _ = fs::remove_file(backup);
            } else if had_previous {
                let _ = fs::rename(backup, path);
            }
        }
    }
}

fn empty_snapshot(scan_id: &str, root_path: &str, started_at_unix_ms: u64) -> SpaceScanSnapshot {
    SpaceScanSnapshot {
        progress: SpaceScanProgress {
            scan_id: scan_id.to_owned(),
            status: SpaceScanStatus::Scanning,
            scanned_items: 0,
            skipped_items: 0,
            bytes_scanned: 0,
            current_path: Some(root_path.to_owned()),
            message: "正在分析目录空间".to_owned(),
            percent_complete: 0,
            estimated_seconds_remaining: None,
            started_at_unix_ms,
            finished_at_unix_ms: None,
        },
        largest_entries: Vec::new(),
        file_types: Vec::new(),
    }
}

fn history_entry(task: &Task) -> Option<SpaceScanHistoryEntry> {
    let finished_at_unix_ms = task.snapshot.progress.finished_at_unix_ms?;
    Some(SpaceScanHistoryEntry {
        scan_id: task.snapshot.progress.scan_id.clone(),
        root_path: task.root_path.clone(),
        status: task.snapshot.progress.status,
        scanned_items: task.snapshot.progress.scanned_items,
        bytes_scanned: task.snapshot.progress.bytes_scanned,
        started_at_unix_ms: task.snapshot.progress.started_at_unix_ms,
        finished_at_unix_ms,
    })
}

fn normalize_request(mut request: SpaceScanRequest) -> Result<SpaceScanRequest, SpaceScanError> {
    let root = fs::canonicalize(&request.root_path)
        .map_err(|_| SpaceScanError::InvalidRoot(request.root_path.clone()))?;
    if !root.is_dir() {
        return Err(SpaceScanError::InvalidRoot(request.root_path.clone()));
    }
    if request.max_depth > MAX_DEPTH
        || request.max_entries == 0
        || request.max_entries > MAX_ENTRIES
    {
        return Err(SpaceScanError::InvalidLimits);
    }
    let mut excluded_paths = Vec::with_capacity(request.excluded_paths.len());
    for excluded in &request.excluded_paths {
        let normalized = fs::canonicalize(excluded)
            .map_err(|_| SpaceScanError::InvalidExcludedPath(excluded.clone()))?;
        if !normalized.is_dir() || normalized == root || !normalized.starts_with(&root) {
            return Err(SpaceScanError::InvalidExcludedPath(excluded.clone()));
        }
        excluded_paths.push(normalized.to_string_lossy().into_owned());
    }
    excluded_paths.sort();
    excluded_paths.dedup();
    request.root_path = root.to_string_lossy().into_owned();
    request.excluded_paths = excluded_paths;
    Ok(request)
}

fn run_scan(
    scan_id: &str,
    request: &SpaceScanRequest,
    cancel: &AtomicBool,
    paused: &AtomicBool,
    tasks: &Arc<Mutex<HashMap<String, Task>>>,
) -> Result<SpaceScanSnapshot, SpaceScanError> {
    let started = Instant::now();
    let started_at_unix_ms = unix_ms_u64();
    let excluded: Vec<PathBuf> = request.excluded_paths.iter().map(PathBuf::from).collect();
    let mut context = ScanContext {
        scan_id,
        request,
        cancel,
        paused,
        tasks,
        excluded,
        started,
        started_at_unix_ms,
        scanned_items: 0,
        skipped_items: 0,
        bytes_scanned: 0,
        current_path: None,
        largest: BinaryHeap::new(),
        file_types: HashMap::new(),
    };
    walk_dir(Path::new(&request.root_path), 0, &mut context)?;
    let status = if cancel.load(Ordering::Acquire) {
        SpaceScanStatus::Cancelled
    } else {
        SpaceScanStatus::Completed
    };
    Ok(context.snapshot(status, true))
}

struct ScanContext<'a> {
    scan_id: &'a str,
    request: &'a SpaceScanRequest,
    cancel: &'a AtomicBool,
    paused: &'a AtomicBool,
    tasks: &'a Arc<Mutex<HashMap<String, Task>>>,
    excluded: Vec<PathBuf>,
    started: Instant,
    started_at_unix_ms: u64,
    scanned_items: u64,
    skipped_items: u64,
    bytes_scanned: u64,
    current_path: Option<String>,
    largest: BinaryHeap<Reverse<(u64, String, String, u64)>>,
    file_types: HashMap<String, (u64, u64)>,
}

impl ScanContext<'_> {
    fn cooperative_boundary(&self) -> bool {
        while self.paused.load(Ordering::Acquire) && !self.cancel.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(50));
        }
        self.cancel.load(Ordering::Acquire) || self.scanned_items >= self.request.max_entries
    }

    fn publish(&self) {
        if let Some(task) = self
            .tasks
            .lock()
            .expect("space scan state poisoned")
            .get_mut(self.scan_id)
        {
            let status = if self.paused.load(Ordering::Acquire) {
                SpaceScanStatus::Paused
            } else {
                SpaceScanStatus::Scanning
            };
            task.snapshot = self.snapshot(status, false);
        }
    }

    fn snapshot(&self, status: SpaceScanStatus, terminal: bool) -> SpaceScanSnapshot {
        let mut largest: Vec<_> = self
            .largest
            .iter()
            .map(|Reverse(item)| item.clone())
            .collect();
        largest.sort_by_key(|entry| Reverse(entry.0));
        let mut file_types: Vec<_> = self
            .file_types
            .iter()
            .map(|(file_type, (bytes, item_count))| SpaceScanTypeStat {
                file_type: file_type.clone(),
                bytes: *bytes,
                item_count: *item_count,
            })
            .collect();
        file_types.sort_by_key(|entry| Reverse(entry.bytes));
        let percent = if status == SpaceScanStatus::Completed {
            100
        } else {
            ((self.scanned_items.saturating_mul(100) / self.request.max_entries).min(99)) as u8
        };
        let elapsed = self.started.elapsed().as_secs();
        let estimate = if self.scanned_items > 0 && !terminal {
            Some(
                elapsed.saturating_mul(self.request.max_entries.saturating_sub(self.scanned_items))
                    / self.scanned_items,
            )
        } else if terminal {
            Some(0)
        } else {
            None
        };
        SpaceScanSnapshot {
            progress: SpaceScanProgress {
                scan_id: self.scan_id.to_owned(),
                status,
                scanned_items: self.scanned_items,
                skipped_items: self.skipped_items,
                bytes_scanned: self.bytes_scanned,
                current_path: self.current_path.clone(),
                message: match status {
                    SpaceScanStatus::Completed => "空间扫描完成（仅读取）".to_owned(),
                    SpaceScanStatus::Cancelled => "扫描已取消".to_owned(),
                    SpaceScanStatus::Paused => "扫描已暂停".to_owned(),
                    SpaceScanStatus::Failed => "扫描失败".to_owned(),
                    _ => "正在分析目录空间".to_owned(),
                },
                percent_complete: percent,
                estimated_seconds_remaining: estimate,
                started_at_unix_ms: self.started_at_unix_ms,
                finished_at_unix_ms: terminal.then(unix_ms_u64),
            },
            largest_entries: largest
                .into_iter()
                .map(|(bytes, path, kind, item_count)| SpaceScanEntry {
                    path,
                    bytes,
                    item_count,
                    kind,
                })
                .collect(),
            file_types,
        }
    }
}

fn walk_dir(
    path: &Path,
    depth: u8,
    context: &mut ScanContext<'_>,
) -> Result<(u64, u64), SpaceScanError> {
    if context.cooperative_boundary() {
        return Ok((0, 0));
    }
    if context
        .excluded
        .iter()
        .any(|excluded| path.starts_with(excluded))
    {
        context.skipped_items = context.skipped_items.saturating_add(1);
        return Ok((0, 0));
    }
    context.current_path = Some(path.to_string_lossy().into_owned());
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if depth == 0 => {
            return Err(SpaceScanError::Read {
                path: path.to_string_lossy().into_owned(),
                message: error.to_string(),
            });
        }
        Err(_) => {
            // A single protected or concurrently removed child must not make
            // an otherwise useful scan fail.
            context.skipped_items = context.skipped_items.saturating_add(1);
            return Ok((0, 0));
        }
    };
    let mut directory_bytes = 0_u64;
    let mut directory_items = 0_u64;
    for entry in entries {
        if context.cooperative_boundary() {
            break;
        }
        let Ok(entry) = entry else {
            context.skipped_items = context.skipped_items.saturating_add(1);
            continue;
        };
        let child = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&child) else {
            context.skipped_items = context.skipped_items.saturating_add(1);
            continue;
        };
        if metadata.file_type().is_symlink() {
            context.skipped_items = context.skipped_items.saturating_add(1);
            continue;
        }
        context.scanned_items = context.scanned_items.saturating_add(1);
        if metadata.is_file() {
            let bytes = metadata.len();
            context.bytes_scanned = context.bytes_scanned.saturating_add(bytes);
            directory_bytes = directory_bytes.saturating_add(bytes);
            directory_items = directory_items.saturating_add(1);
            let extension = child
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("[无扩展名]")
                .to_ascii_lowercase();
            let aggregate = context.file_types.entry(extension).or_default();
            aggregate.0 = aggregate.0.saturating_add(bytes);
            aggregate.1 = aggregate.1.saturating_add(1);
            push_largest(context, bytes, &child, "file", 1);
        } else if metadata.is_dir() {
            if depth < context.request.max_depth {
                let (child_bytes, child_items) =
                    walk_dir(&child, depth.saturating_add(1), context)?;
                directory_bytes = directory_bytes.saturating_add(child_bytes);
                directory_items = directory_items.saturating_add(child_items);
                push_largest(context, child_bytes, &child, "directory", child_items);
            } else {
                context.skipped_items = context.skipped_items.saturating_add(1);
            }
        }
        if context.scanned_items.is_multiple_of(POLL_UPDATE_ITEMS) {
            context.publish();
        }
    }
    Ok((directory_bytes, directory_items))
}

fn push_largest(
    context: &mut ScanContext<'_>,
    bytes: u64,
    path: &Path,
    kind: &str,
    item_count: u64,
) {
    context.largest.push(Reverse((
        bytes,
        path.to_string_lossy().into_owned(),
        kind.to_owned(),
        item_count,
    )));
    if context.largest.len() > MAX_LARGEST {
        context.largest.pop();
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn unix_ms_u64() -> u64 {
    unix_ms().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{SpaceScanManager, load_history, record_history};
    use clarity_core::{
        SpaceScanHistoryEntry, SpaceScanRequest, SpaceScanSnapshot, SpaceScanStatus,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::Duration;

    fn test_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("clarity-space-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test root should exist");
        root
    }

    fn manager(root: &Path) -> SpaceScanManager {
        SpaceScanManager::with_history_path(root.join("state/history.json"))
    }

    fn request(root: &Path) -> SpaceScanRequest {
        SpaceScanRequest {
            root_path: root.to_string_lossy().into_owned(),
            max_depth: 4,
            max_entries: 100,
            excluded_paths: vec![],
        }
    }

    fn wait_for_terminal(manager: &SpaceScanManager, scan_id: &str) -> SpaceScanSnapshot {
        for _ in 0..300 {
            let snapshot = manager.snapshot(scan_id).expect("snapshot should exist");
            if matches!(
                snapshot.progress.status,
                SpaceScanStatus::Completed | SpaceScanStatus::Cancelled | SpaceScanStatus::Failed
            ) {
                return snapshot;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("scan did not reach a terminal state");
    }

    #[test]
    fn completes_bounded_read_only_scan() {
        let root = test_root("complete");
        let original = vec![1_u8; 64];
        fs::write(root.join("large.bin"), &original).expect("fixture should be written");
        let manager = manager(&root);
        let start = manager.start(request(&root)).expect("scan should start");
        let snapshot = wait_for_terminal(&manager, &start.scan_id);

        assert_eq!(snapshot.progress.status, SpaceScanStatus::Completed);
        assert_eq!(snapshot.progress.bytes_scanned, 64);
        assert_eq!(snapshot.progress.percent_complete, 100);
        assert_eq!(manager.history().len(), 1);
        assert_eq!(fs::read(root.join("large.bin")).unwrap(), original);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn respects_entry_depth_and_exclusion_limits() {
        let root = test_root("limits");
        let excluded = root.join("excluded");
        let nested = root.join("level-one/level-two");
        fs::create_dir_all(&excluded).unwrap();
        fs::create_dir_all(&nested).unwrap();
        fs::write(excluded.join("private.bin"), vec![1_u8; 80]).unwrap();
        fs::write(nested.join("deep.bin"), vec![1_u8; 40]).unwrap();
        fs::write(root.join("visible.bin"), vec![1_u8; 20]).unwrap();
        let manager = manager(&root);
        let mut scan_request = request(&root);
        scan_request.max_depth = 1;
        scan_request.excluded_paths = vec![excluded.to_string_lossy().into_owned()];
        let start = manager.start(scan_request).unwrap();
        let snapshot = wait_for_terminal(&manager, &start.scan_id);

        assert_eq!(snapshot.progress.bytes_scanned, 20);
        assert!(snapshot.progress.skipped_items >= 2);
        assert!(
            snapshot.largest_entries.iter().all(
                |entry| !entry.path.contains("private.bin") && !entry.path.contains("deep.bin")
            )
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pauses_resumes_and_cancels_cooperatively() {
        let root = test_root("controls");
        for index in 0..2_000 {
            fs::write(root.join(format!("{index}.bin")), [1_u8]).unwrap();
        }
        let manager = manager(&root);
        let mut scan_request = request(&root);
        scan_request.max_entries = 10_000;
        let paused = manager.start(scan_request.clone()).unwrap();
        manager.pause(&paused.scan_id).unwrap();
        assert_eq!(
            manager.snapshot(&paused.scan_id).unwrap().progress.status,
            SpaceScanStatus::Paused
        );
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            manager.snapshot(&paused.scan_id).unwrap().progress.status,
            SpaceScanStatus::Paused
        );
        manager.resume(&paused.scan_id).unwrap();
        assert_eq!(
            wait_for_terminal(&manager, &paused.scan_id).progress.status,
            SpaceScanStatus::Completed
        );

        let cancelled = manager.start(scan_request).unwrap();
        manager.cancel(&cancelled.scan_id).unwrap();
        assert_eq!(
            wait_for_terminal(&manager, &cancelled.scan_id)
                .progress
                .status,
            SpaceScanStatus::Cancelled
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_exclusions_outside_the_scan_root() {
        let root = test_root("scope");
        let outside = test_root("outside");
        let manager = manager(&root);
        let mut scan_request = request(&root);
        scan_request.excluded_paths = vec![outside.to_string_lossy().into_owned()];

        assert!(manager.start(scan_request).is_err());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn history_is_bounded_and_corruption_is_ignored() {
        let root = test_root("history");
        let path = root.join("state/history.json");
        let history = Mutex::new(Vec::new());
        for index in 0..25_u64 {
            record_history(
                &history,
                &path,
                SpaceScanHistoryEntry {
                    scan_id: format!("scan-{index}"),
                    root_path: root.to_string_lossy().into_owned(),
                    status: SpaceScanStatus::Completed,
                    scanned_items: index,
                    bytes_scanned: index,
                    started_at_unix_ms: index,
                    finished_at_unix_ms: index,
                },
            );
        }
        let loaded = load_history(&path);
        assert_eq!(loaded.len(), 20);
        assert_eq!(loaded[0].scan_id, "scan-24");

        fs::write(&path, b"not-json").unwrap();
        assert!(load_history(&path).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn default_request_is_bounded_to_the_selected_root() {
        let root = test_root("defaults");
        let request = super::default_request(root.to_string_lossy().into_owned()).unwrap();

        assert_eq!(request.max_depth, 8);
        assert_eq!(request.max_entries, 100_000);
        assert!(
            request
                .excluded_paths
                .iter()
                .all(|path| Path::new(path).starts_with(&request.root_path))
        );
        let _ = fs::remove_dir_all(root);
    }
}
