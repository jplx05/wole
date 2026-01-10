//! Progress events emitted during scanning (used by TUI)

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Real-time progress updates during scanning.
#[derive(Debug, Clone)]
pub enum ScanProgressEvent {
    /// A category scan has started.
    CategoryStarted {
        category: String,
        total_units: Option<u64>,
        current_path: Option<PathBuf>,
    },

    /// Incremental progress within a category scan.
    CategoryProgress {
        category: String,
        completed_units: u64,
        total_units: Option<u64>,
        current_path: Option<PathBuf>,
    },

    /// A category scan has finished.
    CategoryFinished {
        category: String,
        items: usize,
        size_bytes: u64,
    },
}

/// Throttled emitter for current-path updates during scanning.
#[derive(Debug)]
pub struct ScanPathReporter {
    category: String,
    tx: Mutex<std::sync::mpsc::Sender<ScanProgressEvent>>,
    min_interval_ms: u64,
    last_emit_ms: AtomicU64,
}

impl ScanPathReporter {
    pub fn new(
        category: &str,
        tx: std::sync::mpsc::Sender<ScanProgressEvent>,
        min_interval_ms: u64,
    ) -> Self {
        Self {
            category: category.to_string(),
            tx: Mutex::new(tx),
            min_interval_ms,
            last_emit_ms: AtomicU64::new(0),
        }
    }

    pub fn emit_path(&self, path: &std::path::Path) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        if last == 0 {
            self.last_emit_ms.store(now_ms, Ordering::Relaxed);
        } else if now_ms.saturating_sub(last) < self.min_interval_ms {
            return;
        } else if self
            .last_emit_ms
            .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let event = ScanProgressEvent::CategoryProgress {
            category: self.category.clone(),
            completed_units: 0,
            total_units: None,
            current_path: Some(path.to_path_buf()),
        };
        if let Ok(lock) = self.tx.lock() {
            let _ = lock.send(event);
        }
    }
}
