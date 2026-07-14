# Plan-wide Spec Review

Result: failed

Reviewed implementation range: `9a967322^..d78735c`

## Blocking findings

1. The frontend does not surface the required duplicate-catalog repair warning.

   The approved design requires corrupt persisted layouts with colliding canonical names to leave every collision unaddressable and to display a repair warning (`docs/superfastpowers/specs/2026-07-13-terminal-agent-messaging-design.md:92`, `:140`). The Rust directory correctly marks all collisions conflicted, but `collectTerminalCatalog` forwards the catalog without detecting collisions (`src/modules/terminal/lib/terminalControl.ts:36`) and the bridge only reports synchronization failure to the console (`src/modules/terminal/lib/useTerminalControlBridge.ts:50`). No visible repair warning is produced.

2. Catalog terminal IDs are not validated as UUIDs in Rust.

   The approved boot contract says Rust validates UUIDs and names before accepting CLI traffic (`docs/superfastpowers/specs/2026-07-13-terminal-agent-messaging-design.md:90`). `TerminalDirectory::sync_catalog` rejects only empty or repeated IDs (`src-tauri/src/modules/terminal_control/directory.rs:77`), so arbitrary non-empty strings are accepted.

## Required repair

- Detect duplicated canonical terminal names in the frontend catalog and show a stable, visible warning that tells the user to rename the affected panes.
- Reject non-UUID terminal IDs during Rust catalog synchronization without partially mutating the directory.
- Add focused regression tests for both behaviors, then rerun plan-wide spec review.
