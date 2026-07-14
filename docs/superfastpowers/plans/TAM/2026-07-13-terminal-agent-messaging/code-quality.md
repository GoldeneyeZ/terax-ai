# Plan-wide Code-quality Review

Result: checked-with-minor-notes

Reviewed implementation range: `9a967322^..12e9659`

Working-tree scope: no tracked implementation changes after `12e9659`; the user-owned untracked `docs/architecture/terax-architecture-report.md` was excluded and untouched.

## Findings

### Minor

1. The visible duplicate-name warning is not exercised at the hook boundary.

   The repair has good pure coverage for canonical duplicate detection (`src/modules/terminal/lib/terminalControl.test.ts:92`), but no frontend test renders `useTerminalControlBridge` or asserts the Sonner warning, its one-warning-per-collision-set behavior, or reset after repair. Those behaviors live in the effect and ref state at `src/modules/terminal/lib/useTerminalControlBridge.ts:43` and `:57`. A later effect dependency or toast integration regression could therefore pass the current helper tests. This is a test-depth note only; source inspection confirms the current behavior is correct.

2. Per-target serialization locks are retained for the lifetime of the control service.

   `TerminalControlState` owns a strong `HashMap<String, Arc<Mutex<()>>>` (`src-tauri/src/modules/terminal_control/service.rs:126`), and every first send to a terminal inserts an entry (`src-tauri/src/modules/terminal_control/service.rs:509`). Close cleanup removes credentials and rate-limit state but not the writer entry (`src-tauri/src/modules/terminal_control/service.rs:294`). A long-running process that creates and targets many terminal UUIDs can accumulate small stale lock entries. Cleanup requires care because queued sends may still hold an `Arc`; a weak-reference map or opportunistic removal after the last waiter would avoid reuse races. This is minor and does not affect current correctness.

No Critical or Important findings were found. No fix is required before integration review.

## Review evidence

- Inspected the complete range, repair commits, post-range working tree, all task contexts, and the focused frontend/Rust control paths rather than relying on context summaries as proof.
- Confirmed the spec-review prerequisite is `checked` and the repair boundary validates UUIDs before state mutation.
- Fresh controller verification reports targeted Biome checks, strict all-target Clippy, and the complete frontend suite passing 372/372; task contexts accurately distinguish focused passes from known repository-wide formatting baselines.
- Scope-only support changes (`scripts/eager-graph.mjs`, `vite.config.ts`, and the settings-window warning cleanup) are tied to required test/build gates; no unrelated production feature was introduced.

## Decision

The plan-wide code-quality phase is checked with two non-blocking minor notes. Proceed to the policy-declared integration review.
