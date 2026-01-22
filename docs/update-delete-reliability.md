# Update + Delete Reliability Spec

## Goals
- Reduce intermittent failures for `wole update` by isolating temp workspaces and cleaning up robustly.
- Improve delete diagnostics by distinguishing permission failures from lock conflicts.
- Preserve existing safety checks (system-path blocks, lock prechecks).

## Observed Edge Cases
### Update
- **Temp dir reuse collisions:** Updates reuse a fixed `wole-update` temp path. Partial artifacts can conflict with new downloads or extractions.
- **Extraction collisions:** Download and extraction share a folder, increasing collision risk when prior files persist.
- **Cleanup gaps:** Deferred updates leave temp artifacts; subsequent runs can reuse or conflict with those leftovers.

### Delete
- **Permission vs. lock ambiguity:** Access denied errors are reported as generic failures or “locked,” making it hard to understand root cause.
- **Race conditions:** A path can become locked between precheck and deletion.

## Proposed Behavior
### Update
- Create a **unique per-run temp directory** (PID + timestamp + retry suffix).
- Download the zip **inside the unique temp directory**.
- Extract into a **dedicated subfolder** (e.g., `<temp>/extract`) to isolate artifacts.
- Clean up the **entire temp directory** on successful immediate install.
- When update is deferred, let the deferred script remove the entire temp directory after it finishes.

### Delete
- Introduce a **permission-denied classification** distinct from “locked” and “missing.”
- When delete fails, map IO error codes to:
  - `SkippedLocked` (sharing/lock violations).
  - `SkippedPermission` (permission denied).
  - `SkippedMissing` (path disappeared during delete).
- When Windows reports access denied, re-check the lock state to avoid mislabeling locked files as permission errors.
- Log and surface **permission-denied** paths with a clearer failure reason.

## Non-Goals
- Changing deletion rules for system path protection.
- Altering which categories are eligible for deletion.

## Validation Checklist
- `wole update` uses a unique temp dir and doesn’t reuse global temp paths.
- If update is deferred, the background script cleans up the temp directory.
- Delete flows report locked vs. permission denied separately in history logs.
