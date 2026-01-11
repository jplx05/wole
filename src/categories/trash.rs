use crate::output::CategoryResult;
use crate::trash_ops;
use anyhow::{Context, Result};

/// Scan the Recycle Bin for items
///
/// Note: Size calculation is skipped as it would require reading each file,
/// which is expensive. Only item count is tracked.
pub fn scan() -> Result<CategoryResult> {
    let mut result = CategoryResult::default();

    match trash_ops::list() {
        Ok(items) => {
            result.items = items.len();
            // TrashItem doesn't expose size, so we just count items
            // Size would require reading each file which is expensive
            result.size_bytes = 0;
            result.paths = items
                .iter()
                .map(|i| i.original_parent.join(&i.name))
                .collect();
        }
        Err(e) => {
            eprintln!("Warning: Could not read Recycle Bin: {}", e);
        }
    }

    Ok(result)
}

/// Empty the Recycle Bin by purging all items
pub fn clean() -> Result<()> {
    let items = trash_ops::list().context("Failed to list Recycle Bin items")?;

    if !items.is_empty() {
        trash_ops::purge_all(&items).context("Failed to empty Recycle Bin")?;
    }

    Ok(())
}
