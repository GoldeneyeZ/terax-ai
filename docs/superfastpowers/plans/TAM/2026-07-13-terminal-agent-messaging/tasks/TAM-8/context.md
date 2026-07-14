# Context for TAM-8

**Plan:** `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
**Task:** `TAM-8`
**Commit SHA:** `3aa1272`

## Starting Context

- `src-tauri/tests/terminal_control_windows.rs`: TAM-8 planned creation target.
- `src/modules/terminal/lib/terminalControl.integration.test.ts`: TAM-8 planned creation target.
- `src-tauri/src/modules/terminal_control/*`: TAM-8 existing integration point.
- `src/modules/terminal/lib/terminalControl.ts`: TAM-8 existing integration point.
- `src/modules/terminal/lib/useTerminalControlBridge.ts`: TAM-8 existing integration point.

## Open Context Rule

The files above are starting points only. Inspect any additional files needed to complete the task correctly.

## Completion Updates

- **Reviewed implementation range:** `1c27547..3aa1272`
- **Created:** `src-tauri/tests/terminal_control_windows.rs`, `src/modules/terminal/lib/terminalControl.integration.test.ts`.
- **Modified:** `src-tauri/src/modules/terminal_control/service.rs`, `src-tauri/src/modules/terminal_control/transport/windows.rs`.
- **TDD evidence:** the frontend restart/persistence suite passed immediately because TAM-1 and TAM-5 already satisfied the pure-data cycle. The first backend matrix run passed 5/7 and exposed two concrete gaps: simultaneous clients could lose the `WaitNamedPipeW`/`CreateFileW` race with `ERROR_PIPE_BUSY`, and same-target sends entered the PTY sink concurrently. Deadline-based client reopen retries and a per-terminal writer mutex with a post-wait liveness check made the matrix pass 7/7.
- **Verification:** corrected locked backend command `cargo test --manifest-path src-tauri/Cargo.toml --locked --test terminal_control_windows -- --test-threads=1` passed 7/7; targeted frontend command `pnpm exec vitest run src/modules/terminal/lib/terminalControl.integration.test.ts` passed 2/2 under Node 22.15; duplicate backend catalog conflict proof passed 1/1; service passed 9/9; Windows pipe passed 9/9; PTY lifecycle passed 5/5; `teraxctl` passed 10/10; locked all-target Cargo check passed; all-target Clippy with the accepted baseline unused-variable lint allowed passed; targeted rustfmt, Biome, and `git diff --check` passed.
- **Decisions:** transport retries only `ERROR_PIPE_BUSY` and consumes the caller's original timeout budget. Sends serialize per stable terminal ID, then re-check that the target is still live before writing; a write already holding the target lock may finish while close rejects queued sends. The frontend preserves duplicate persisted names without suffixing; the existing backend directory conflict test proves both records remain masked/conflicted after sync.
- **Manual smoke:** deferred. Windows computer-control policy prohibits automating commands inside terminal UIs; no manual result was fabricated. The automated matrix covers the specified CLI encoding, delivery completion, privacy, close race, capacity, rate limit, restart, and stopped/unavailable boundaries.
- **Known baselines:** `src-tauri/src/lib.rs:84` unused `window`; repository-wide Rust formatting drift. The user confirmed the Windows symlink-escape authorization test passes in an Administrator PowerShell; non-admin execution can still fail with privilege error 1314. The user-owned untracked architecture report remained untouched.
