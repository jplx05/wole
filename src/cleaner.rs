//! Deletion orchestration for cleaning operations.

mod batch_deletion;
mod category_cleaning;
mod path_precheck;
mod single_deletion;

pub use batch_deletion::{clean_paths_batch, BatchDeleteResult};
pub use category_cleaning::clean_all;
pub use single_deletion::{clean_path, delete_with_precheck, DeleteOutcome};
