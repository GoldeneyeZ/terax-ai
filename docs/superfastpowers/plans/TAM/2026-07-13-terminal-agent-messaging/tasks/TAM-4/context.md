# Context for TAM-4

**Plan:** `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
**Task:** `TAM-4`
**Commit SHA:** `116dde1` (`feat(control): route terminal messages`)
**Implementation range:** `ce4fb1c..116dde1`

## Starting Context

- `src-tauri/src/modules/terminal_control/service.rs`: TAM-4 planned creation target.
- `src-tauri/src/modules/terminal_control/mod.rs`: TAM-4 existing integration point.
- `src-tauri/src/modules/pty/mod.rs`: TAM-4 existing integration point.
- `src-tauri/src/lib.rs`: TAM-4 existing integration point.
- `src-tauri/tests/terminal_control_service.rs`: TAM-4 planned creation target.

## Open Context Rule

The files above are starting points only. Inspect any additional files needed to complete the task correctly.

## Completion Updates

### Files created

- `src-tauri/src/modules/terminal_control/service.rs`
- `src-tauri/tests/terminal_control_service.rs`

### Files modified

- `src-tauri/src/modules/terminal_control/mod.rs`
- `src-tauri/src/modules/pty/mod.rs`
- `src-tauri/src/lib.rs`

### Additional files inspected

- `AGENTS.md`
- `TERAX.md`
- `docs/superfastpowers/specs/2026-07-13-terminal-agent-messaging-design.md`
- `docs/contributing/testing.md`
- `docs/architecture/pty-shell-integration.md`
- `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
- `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging/plan-progression.md`
- `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging/tasks/TAM-2/context.md`
- `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging/tasks/TAM-3/context.md`
- `src-tauri/src/modules/terminal_control/{protocol,directory,credentials,rate_limit,framing}.rs`
- `src-tauri/src/modules/terminal_control/transport/windows.rs`

### TDD evidence

- RED: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_service --test terminal_control_service` exited 101 because `CatalogRecord`, `Clock`, `ControlService`, `NamePersistence`, `PersistNameRequest`, and `PtySink` did not exist.
- GREEN: the task's literal focused command exited 0 but filtered all nine integration tests because its positional argument is a test-name filter.
- GREEN: `cargo test --locked --manifest-path src-tauri/Cargo.toml --test terminal_control_service` passed 9/9. The suite covers list masking, exact authenticated delivery, unnamed sources, writer failure, per-source rate limiting, name commit, name rollback, five-second timeout, and version/hydration ordering.

### Final verification

- `cargo test --locked --manifest-path src-tauri/Cargo.toml terminal_control_service --test terminal_control_service`: PASS with 9 filtered out, matching the task's literal command behavior.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test terminal_control_service`: PASS, 9 passed, 0 failed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml pty --lib`: PASS, 55 passed, 0 failed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml terminal_control --lib`: PASS, 23 passed, 0 failed.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets`: PASS with only the existing Windows unused `window` warning at `src-tauri/src/lib.rs:84`.
- `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`: blocked only by the existing `src-tauri/src/lib.rs:84` unused `window` warning.
- `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings -A unused-variables`: PASS, confirming no TAM-4 Clippy findings after allowing the single documented baseline lint.
- `rustfmt --edition 2021 --check src-tauri/src/modules/terminal_control/service.rs src-tauri/src/modules/terminal_control/mod.rs src-tauri/tests/terminal_control_service.rs`: PASS.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`: existing repository formatting drift remains in unrelated baseline files; no TAM-4-created file was reported, and the focused TAM-4 rustfmt check passed.
- `git diff --cached --check`: PASS before the implementation commit.

### Notes

- Requests reject unsupported versions and pre-hydration access before authentication, then apply the per-source send bucket before operation routing.
- Name persistence reserves under the directory mutex, emits after releasing it, waits on a per-request condition variable, and commits or rolls back without a lock-held external wait.
- `PtyState::write_bytes` is now the single writer path used by raw frontend input and authenticated control sends, preserving the existing awaited writer mutex and error contract.
- The Windows endpoint is generated per app instance from the process ID and a fresh 16-byte random nonce. Shutdown stops pipe acceptance before credential revocation and LSP teardown.
- `docs/architecture/terax-architecture-report.md` remained untouched and untracked.
