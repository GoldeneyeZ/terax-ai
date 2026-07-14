# Plan-wide Spec Review

Result: checked

Reviewed implementation range: `9a967322^..de4ecf3`

Working-tree scope: no tracked implementation changes after `de4ecf3`; the user-owned untracked `docs/architecture/terax-architecture-report.md` was excluded and untouched.

## Repair verification

1. Duplicate persisted names now produce the required visible repair warning.

   The approved design requires every canonical name collision to remain unaddressable and the frontend to surface a repair warning (`docs/superfastpowers/specs/2026-07-13-terminal-agent-messaging-design.md:92`, `:140`). `findDuplicateCatalogNames` canonicalizes, counts, and deterministically sorts all collisions (`src/modules/terminal/lib/terminalControl.ts:51`). The bridge derives that collision set from the complete catalog and emits a deduplicated Sonner warning that identifies the names and tells the user to rename the affected panes (`src/modules/terminal/lib/useTerminalControlBridge.ts:53`). The regression test covers case-insensitive collisions, private records, multiple duplicate groups, and unnamed records (`src/modules/terminal/lib/terminalControl.test.ts:92`). Existing Rust directory behavior continues to mark every colliding record conflicted and mask it from discovery and targeting.

2. Rust now rejects non-UUID catalog terminal IDs before directory mutation.

   The boot contract requires Rust to validate UUIDs and names before accepting CLI traffic (`docs/superfastpowers/specs/2026-07-13-terminal-agent-messaging-design.md:90`). `validate_catalog_ids` parses every incoming ID with `uuid::Uuid::parse_str`, and `terminal_control_sync_catalog` invokes it before calling state synchronization (`src-tauri/src/modules/terminal_control/mod.rs:23`, `:35`). The focused regression test proves a UUID is accepted and `pane-a` is rejected with `INVALID_REQUEST` (`src-tauri/src/modules/terminal_control/mod.rs:58`). Because validation precedes `state.sync_catalog`, invalid input cannot partially replace the directory.

## Coverage and evidence

- Re-inspected the approved design, master plan, all TAM task/context packages, the complete implementation range, repair diff, and post-repair working tree. No remaining missing, extra, misunderstood, or regressed blocking requirement was found.
- `pnpm test -- src/modules/terminal/lib/terminalControl.test.ts`: PASS, 1 file and 7 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked catalog_ids_must_be_uuids --lib`: PASS, 1 executed test.
- Task contexts record the broader frontend, Rust, packaging, and production-build gates; this re-review independently inspected the repaired source and reran the two focused regressions rather than treating context files as proof.

## Decision

The plan-wide specification phase is checked. The implementation satisfies the approved Terminal Agent Messaging design and may proceed to the policy-declared plan-wide code-quality phase.
