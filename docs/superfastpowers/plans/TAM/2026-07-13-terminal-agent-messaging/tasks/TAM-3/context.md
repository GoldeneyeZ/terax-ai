# Context for TAM-3

**Plan:** `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
**Task:** `TAM-3`
**Commit SHA:** `9d49509`
**Implementation range:** `cdc329b..9d49509`

## Starting Context

- `src-tauri/Cargo.toml`: TAM-3 existing integration point.
- `src-tauri/Cargo.lock`: TAM-3 existing integration point.
- `src-tauri/src/modules/terminal_control/framing.rs`: TAM-3 planned creation target.
- `src-tauri/src/modules/terminal_control/transport/mod.rs`: TAM-3 planned creation target.
- `src-tauri/src/modules/terminal_control/transport/windows.rs`: TAM-3 planned creation target.
- `src-tauri/tests/terminal_control_pipe_windows.rs`: TAM-3 planned creation target.

## Open Context Rule

The files above are starting points only. Inspect any additional files needed to complete the task correctly.

## Completion Updates

### Files created

- `src-tauri/src/modules/terminal_control/framing.rs`
- `src-tauri/src/modules/terminal_control/transport/mod.rs`
- `src-tauri/src/modules/terminal_control/transport/windows.rs`
- `src-tauri/tests/terminal_control_pipe_windows.rs`

### Files modified

- `src-tauri/Cargo.toml`
- `src-tauri/src/modules/terminal_control/mod.rs`

`src-tauri/Cargo.lock` was inspected and remained unchanged because
`windows-sys 0.61.2` was already locked and only its enabled feature set changed.

### Inspected dependencies

- `windows-sys 0.61.2` installed crate source for the exact signatures and
  constants used by token lookup, SDDL conversion, named-pipe creation,
  connection, flushing, disconnection, client opening, and `LocalFree` cleanup.
- `src-tauri/src/modules/terminal_control/protocol.rs` for the shared 64 KiB
  frame bound and stable transport-facing error codes.
- TAM design sections 10, 15, 16, 17, and 18 for transport, shutdown, error,
  security, and test constraints.

### RED-GREEN evidence

- RED: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_pipe_windows --test terminal_control_pipe_windows`
  exited 101 because `terminal_control::framing` and
  `terminal_control::transport` did not exist.
- Initial GREEN investigation: 7/9 passed; normal response reads intermittently
  failed with Win32 error 233 because the server disconnected before an OS pipe
  flush. Adding the verified `FlushFileBuffers` call before
  `DisconnectNamedPipe` fixed the response lifecycle.
- GREEN: `cargo test --locked --manifest-path src-tauri/Cargo.toml --test terminal_control_pipe_windows`
  passed 9/9. Five consecutive locked reruns also passed 5/5 iterations.
- The task document's exact command passed with 9 tests filtered out because its
  positional `terminal_control_pipe_windows` argument is a test-name filter; the
  unfiltered integration command above executed all nine cases.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml terminal_control --lib`
  passed 23/23.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets`
  passed. It emitted only the pre-existing out-of-scope `src/lib.rs:84` unused
  `window` warning.
- `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --test terminal_control_pipe_windows -- -D warnings -A unused-variables`
  passed.
- `rustfmt --edition 2021 --check` passed for all TAM-3 Rust files, and
  `git diff --cached --check` passed before the implementation commit.

### Implementation notes

- Frames are little-endian length-prefixed and reject oversized lengths before
  allocation or body I/O.
- Every pipe instance uses a protected DACL granting generic access only to the
  current process user SID and sets `PIPE_REJECT_REMOTE_CLIENTS`.
- The production server admits at most 32 active handlers, reads and writes one
  frame per connection, reports client wait saturation as `SERVER_BUSY`, and
  wakes blocking accept with a local connection during shutdown.
- Win32 kernel/token handles use `OwnedHandle`; SDDL and SID allocations use a
  dedicated RAII owner that calls `LocalFree`. The security descriptor outlives
  every `CreateNamedPipeW` call that references it.
- Repository-wide `cargo fmt` was run as required. Formatter-only changes to
  out-of-scope pre-existing files were removed before commit; TAM-3 files remain
  rustfmt-clean.
