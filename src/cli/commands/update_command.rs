//! Update command feature.
//!
//! This module owns and handles the "wole update" command behavior.

use crate::output::OutputMode;

pub(crate) fn handle_update(
    yes: bool,
    check: bool,
    output_mode: OutputMode,
) -> anyhow::Result<()> {
    crate::update::check_and_update(yes, check, output_mode)?;
    Ok(())
}
