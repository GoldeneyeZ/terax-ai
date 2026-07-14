# Context for TAM-2

**Plan:** `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
**Task:** `TAM-2`
**Commit SHA:** `181d394` (`feat(control): add terminal messaging domain`)
**Reviewed range:** `6a610c6..181d394`

## Starting Context

- `src-tauri/Cargo.toml`: TAM-2 existing integration point.
- `src-tauri/Cargo.lock`: TAM-2 existing integration point.
- `src-tauri/src/modules/mod.rs`: TAM-2 existing integration point.
- `src-tauri/src/modules/terminal_control/mod.rs`: TAM-2 planned creation target.
- `src-tauri/src/modules/terminal_control/protocol.rs`: TAM-2 planned creation target.
- `src-tauri/src/modules/terminal_control/directory.rs`: TAM-2 planned creation target.
- `src-tauri/src/modules/terminal_control/credentials.rs`: TAM-2 planned creation target.
- `src-tauri/src/modules/terminal_control/rate_limit.rs`: TAM-2 planned creation target.

## Open Context Rule

The files above are starting points only. Inspect any additional files needed to complete the task correctly.

## Completion Updates

### Files created

- `src-tauri/src/modules/terminal_control/mod.rs`
- `src-tauri/src/modules/terminal_control/protocol.rs`
- `src-tauri/src/modules/terminal_control/directory.rs`
- `src-tauri/src/modules/terminal_control/credentials.rs`
- `src-tauri/src/modules/terminal_control/rate_limit.rs`

### Files modified

- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/src/modules/mod.rs`

### Additional files inspected

- `AGENTS.md`
- `TERAX.md`
- `src-tauri/src/lib.rs`
- `docs/superfastpowers/specs/2026-07-13-terminal-agent-messaging-design.md`
- `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
- `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging/plan-progression.md`
- `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging/tasks/TAM-2/task.md`

### TDD evidence

- RED: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control --lib` failed to compile because `ControlRequest`, `TerminalDirectory`, `Credentials`, `TokenBucket`, protocol validators, and constants did not exist.
- GREEN: the same focused command passed 23 tests after the minimal protocol, directory, credential, and rate-limit implementation.
- RED: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control::directory::tests::private_and_conflicted_targets_are_masked --lib` failed with `left: Ok("conflict-a")`, `right: Err(SourceUnnamed)`.
- GREEN: the focused 23-test domain suite passed after conflicted source names were masked.

### Final verification

- `rustfmt --edition 2021 --check src-tauri/src/modules/terminal_control/mod.rs src-tauri/src/modules/terminal_control/protocol.rs src-tauri/src/modules/terminal_control/directory.rs src-tauri/src/modules/terminal_control/credentials.rs src-tauri/src/modules/terminal_control/rate_limit.rs`: PASS, no output.
- `cargo test --manifest-path src-tauri/Cargo.toml terminal_control --lib --locked`: PASS, 23 passed, 0 failed.
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets --locked`: PASS. It reports the existing Windows-only unused `window` warning at `src-tauri/src/lib.rs:84`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib --locked`: 205 passed and 1 environment-only failure at `modules::workspace::auth_tests::authorize_spawn_cwd_blocks_symlink_escape` because this non-elevated Windows shell lacks symlink privilege. This is the documented baseline failure and was not changed.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings`: blocked only by the existing `src-tauri/src/lib.rs:84` unused `window` warning.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings -A unused-variables`: PASS, confirming no TAM-2 Clippy findings after allowing the single baseline lint.
- `git diff --cached --check`: PASS before implementation commit.

### Notes

- Protocol v1 has exact request/response wire shapes, stable error codes, byte-bounded single-line messages, canonical names, and one envelope builder that appends `\r`.
- The directory keeps authoritative persisted membership, committed names, pending reservations, privacy/conflict masking, sorted live targets, and explicit live/closing/exited transitions in one domain value intended for a single service mutex.
- Credentials retain only SHA-256 token digests and terminal IDs, authenticate every entry with constant-time comparisons, and revoke all tokens for a pane.
- TAM-3 transport, TAM-4 service routing, frontend synchronization, and PTY lifecycle integration were not implemented.
