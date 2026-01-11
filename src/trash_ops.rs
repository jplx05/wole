//! Thin wrapper around the `trash` crate.
//!
//! Why this exists:
//! - On Windows, `trash` uses COM under the hood (Shell APIs).
//! - If COM is already initialized on the current thread with a different
//!   concurrency model, `trash` can panic (e.g. `CoInitializeEx failed` with
//!   HRESULT `0x80010106` / `RPC_E_CHANGED_MODE`).
//! - We treat panics from dependencies as errors so the CLI/TUI can continue
//!   and report a useful message instead of crashing.

use anyhow::{anyhow, Result};
use std::any::Any;
use std::path::{Path, PathBuf};

fn panic_payload_to_string(panic_payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic_payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn catch_trash_panic<R>(f: impl FnOnce() -> Result<R>) -> Result<R> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(panic_payload) => {
            let msg = panic_payload_to_string(panic_payload);
            Err(anyhow!(
                "Recycle Bin operation panicked (dependency bug): {msg}"
            ))
        }
    }
}

pub fn delete(path: &Path) -> Result<()> {
    catch_trash_panic(|| Ok(trash::delete(path)?))
}

pub fn delete_all(paths: &[PathBuf]) -> Result<()> {
    catch_trash_panic(|| Ok(trash::delete_all(paths)?))
}

pub fn list() -> Result<Vec<trash::TrashItem>> {
    catch_trash_panic(|| Ok(trash::os_limited::list()?))
}

pub fn purge_all(items: &[trash::TrashItem]) -> Result<()> {
    catch_trash_panic(|| Ok(trash::os_limited::purge_all(items)?))
}

pub fn restore_all<I>(items: I) -> Result<()>
where
    I: IntoIterator<Item = trash::TrashItem>,
{
    catch_trash_panic(|| Ok(trash::os_limited::restore_all(items)?))
}
