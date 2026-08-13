//! Windows-neutral, read-only directory scanner with cooperative cancellation.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_core::{
    SpaceScanEntry, SpaceScanError, SpaceScanProgress, SpaceScanRequest, SpaceScanSnapshot,
    SpaceScanStart, SpaceScanStatus, SpaceScanTypeStat,
};

const MAX_DEPTH: u8 = 64;
const MAX_ENTRIES: u64 = 1_000_000;
const MAX_LARGEST_ENTRIES: usize = 20;

#[derive(Clone, Default)]
pub struct SpaceScanManager {
    tasks: Arc<Mutex<HashMap<String, Task>>>,
}

struct Task {
    progress: SpaceScanSnapshot,
    cancel: Arc<AtomicBool>,
}

impl SpaceScanManager {
    /// Starts a bounded read-only scan in a background thread.
    pub fn start(&self, request: SpaceScanRequest) -> Result<SpaceScanStart, SpaceScanError> {
        validate_request(&request)?;
        let scan_id = format!("space-{}-{}", std::process::id(), unix_ms());
        let cancel = Arc::new(AtomicBool::new(false));
        let initial = SpaceScanSnapshot {
            progress: SpaceScanProgress {
                scan_id: scan_id.clone(),
                status: SpaceScanStatus::Scanning,
                scanned_items: 0,
                skipped_items: 0,
                bytes_scanned: 0,
                current_path: Some(request.root_path.clone()),
                message: "正在分析目录空间".to_owned(),
            },
            largest_entries: Vec::new(),
            file_types: Vec::new(),
        };
        self.tasks
            .lock()
            .expect("space scan state poisoned")
            .insert(
                scan_id.clone(),
                Task {
                    progress: initial,
                    cancel: Arc::clone(&cancel),
                },
            );

        let tasks = Arc::clone(&self.tasks);
        let thread_scan_id = scan_id.clone();
        thread::spawn(move || {
            let result = run_scan(&thread_scan_id, &request, &cancel);
            let mut guard = tasks.lock().expect("space scan state poisoned");
            if let Some(task) = guard.get_mut(&thread_scan_id) {
                match result {
                    Ok(snapshot) => task.progress = snapshot,
                    Err(error) => {
                        task.progress.progress.status = SpaceScanStatus::Failed;
                        task.progress.progress.message = error.to_string();
                    }
                }
            }
        });
        Ok(SpaceScanStart { scan_id })
    }

    /// Returns the latest progress snapshot for a task.
    pub fn snapshot(&self, scan_id: &str) -> Result<SpaceScanSnapshot, SpaceScanError> {
        self.tasks
            .lock()
            .expect("space scan state poisoned")
            .get(scan_id)
            .map(|task| task.progress.clone())
            .ok_or_else(|| SpaceScanError::TaskNotFound(scan_id.to_owned()))
    }

    /// Requests cooperative cancellation; the scanner stops at its next safe boundary.
    pub fn cancel(&self, scan_id: &str) -> Result<(), SpaceScanError> {
        let guard = self.tasks.lock().expect("space scan state poisoned");
        let task = guard
            .get(scan_id)
            .ok_or_else(|| SpaceScanError::TaskNotFound(scan_id.to_owned()))?;
        task.cancel.store(true, Ordering::Release);
        Ok(())
    }
}

fn validate_request(request: &SpaceScanRequest) -> Result<(), SpaceScanError> {
    let root = Path::new(&request.root_path);
    if !root.is_dir() {
        return Err(SpaceScanError::InvalidRoot(request.root_path.clone()));
    }
    if request.max_depth > MAX_DEPTH
        || request.max_entries == 0
        || request.max_entries > MAX_ENTRIES
    {
        return Err(SpaceScanError::InvalidLimits);
    }
    Ok(())
}

fn run_scan(
    scan_id: &str,
    request: &SpaceScanRequest,
    cancel: &AtomicBool,
) -> Result<SpaceScanSnapshot, SpaceScanError> {
    let mut context = ScanContext::new(scan_id, request, cancel);
    walk_dir(Path::new(&request.root_path), 0, &mut context)?;
    let status = if cancel.load(Ordering::Acquire) {
        SpaceScanStatus::Cancelled
    } else {
        SpaceScanStatus::Completed
    };
    Ok(context.finish(status))
}

struct ScanContext<'a> {
    scan_id: &'a str,
    request: &'a SpaceScanRequest,
    cancel: &'a AtomicBool,
    scanned_items: u64,
    skipped_items: u64,
    bytes_scanned: u64,
    current_path: Option<String>,
    largest: BinaryHeap<Reverse<(u64, String, String, u64)>>,
    file_types: HashMap<String, (u64, u64)>,
}

impl<'a> ScanContext<'a> {
    fn new(scan_id: &'a str, request: &'a SpaceScanRequest, cancel: &'a AtomicBool) -> Self {
        Self {
            scan_id,
            request,
            cancel,
            scanned_items: 0,
            skipped_items: 0,
            bytes_scanned: 0,
            current_path: None,
            largest: BinaryHeap::new(),
            file_types: HashMap::new(),
        }
    }

    fn finish(self, status: SpaceScanStatus) -> SpaceScanSnapshot {
        let mut largest: Vec<_> = self.largest.into_iter().map(|Reverse(item)| item).collect();
        largest.sort_by(|left, right| right.0.cmp(&left.0));
        let file_types = self
            .file_types
            .into_iter()
            .map(|(file_type, (bytes, item_count))| SpaceScanTypeStat {
                file_type,
                bytes,
                item_count,
            })
            .collect();
        SpaceScanSnapshot {
            progress: SpaceScanProgress {
                scan_id: self.scan_id.to_owned(),
                status,
                scanned_items: self.scanned_items,
                skipped_items: self.skipped_items,
                bytes_scanned: self.bytes_scanned,
                current_path: self.current_path,
                message: match status {
                    SpaceScanStatus::Completed => "空间扫描完成（仅读取）".to_owned(),
                    SpaceScanStatus::Cancelled => "扫描已取消".to_owned(),
                    _ => "扫描未完成".to_owned(),
                },
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

fn walk_dir(path: &Path, depth: u8, context: &mut ScanContext<'_>) -> Result<(), SpaceScanError> {
    if context.cancel.load(Ordering::Acquire)
        || context.scanned_items >= context.request.max_entries
    {
        return Ok(());
    }
    context.current_path = Some(path.to_string_lossy().into_owned());
    let entries = fs::read_dir(path).map_err(|error| SpaceScanError::Read {
        path: path.to_string_lossy().into_owned(),
        message: error.to_string(),
    })?;
    for entry in entries {
        if context.cancel.load(Ordering::Acquire)
            || context.scanned_items >= context.request.max_entries
        {
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                context.skipped_items = context.skipped_items.saturating_add(1);
                continue;
            }
        };
        let child = entry.path();
        let metadata = match fs::symlink_metadata(&child) {
            Ok(metadata) => metadata,
            Err(_) => {
                context.skipped_items = context.skipped_items.saturating_add(1);
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            context.skipped_items = context.skipped_items.saturating_add(1);
            continue;
        }
        context.scanned_items = context.scanned_items.saturating_add(1);
        if metadata.is_file() {
            let bytes = metadata.len();
            context.bytes_scanned = context.bytes_scanned.saturating_add(bytes);
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
                walk_dir(&child, depth.saturating_add(1), context)?;
            } else {
                context.skipped_items = context.skipped_items.saturating_add(1);
            }
        }
    }
    Ok(())
}

fn push_largest(
    context: &mut ScanContext<'_>,
    bytes: u64,
    path: &Path,
    kind: &str,
    item_count: u64,
) {
    let item = Reverse((
        bytes,
        path.to_string_lossy().into_owned(),
        kind.to_owned(),
        item_count,
    ));
    context.largest.push(item);
    if context.largest.len() > MAX_LARGEST_ENTRIES {
        context.largest.pop();
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
