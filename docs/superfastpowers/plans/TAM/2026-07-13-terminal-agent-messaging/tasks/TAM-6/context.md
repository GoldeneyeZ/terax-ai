# Context for TAM-6

**Plan:** `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
**Task:** `TAM-6`
**Commit SHA:** `88e3d66` (`feat(terminal): bind control identities`)
**Implementation range:** `6021ba9..88e3d66`

## Starting Context

- `src/modules/terminal/TerminalStack.tsx`: TAM-6 existing integration point.
- `src/modules/terminal/PaneTreeView.tsx`: TAM-6 existing integration point.
- `src/modules/terminal/TerminalPane.tsx`: TAM-6 existing integration point.
- `src/modules/terminal/lib/useTerminalSession.ts`: TAM-6 existing integration point.
- `src/modules/terminal/lib/pty-bridge.ts`: TAM-6 existing integration point.
- `src-tauri/src/modules/pty/mod.rs`: TAM-6 existing integration point.
- `src-tauri/src/modules/pty/session.rs`: TAM-6 existing integration point.
- `src-tauri/src/modules/pty/shell_init.rs`: TAM-6 existing integration point.
- `src-tauri/src/modules/terminal_control/service.rs`: TAM-6 existing integration point.
- `src-tauri/tests/terminal_control_pty.rs`: TAM-6 planned creation target.

## Open Context Rule

The files above are starting points only. Inspect any additional files needed to complete the task correctly.

## Completion Updates

### Files created

- `src-tauri/tests/terminal_control_pty.rs`
- `src/modules/terminal/lib/pty-bridge.test.ts`

### Files modified

- `src/modules/terminal/TerminalStack.tsx`
- `src/modules/terminal/PaneTreeView.tsx`
- `src/modules/terminal/TerminalPane.tsx`
- `src/modules/terminal/lib/useTerminalSession.ts`
- `src/modules/terminal/lib/pty-bridge.ts`
- `src-tauri/src/lib.rs`
- `src-tauri/src/modules/pty/mod.rs`
- `src-tauri/src/modules/pty/session.rs`
- `src-tauri/src/modules/pty/shell_init.rs`
- `src-tauri/src/modules/terminal_control/credentials.rs`
- `src-tauri/src/modules/terminal_control/directory.rs`
- `src-tauri/src/modules/terminal_control/mod.rs`
- `src-tauri/src/modules/terminal_control/service.rs`

### Additional files inspected

- `AGENTS.md`, `TERAX.md`, `package.json`, and `src-tauri/Cargo.toml`
- TAM plan, progression policy, TAM-1, TAM-2, TAM-4, TAM-5 contexts, TAM-6 task, design specification, and phase-worker prompts
- `src/modules/terminal/lib/panes.ts`
- `src/modules/terminal/lib/terminalIdentity.ts`
- `src-tauri/src/modules/terminal_control/protocol.rs`
- `src-tauri/src/modules/terminal_control/transport.rs`
- `src-tauri/tests/terminal_control_service.rs`

### TDD evidence

All Node commands used Node `v22.15.0` by prepending
`C:\Users\Zacha\AppData\Local\nvm\v22.15.0` to `PATH`.

- RED: `pnpm test -- src/modules/terminal/lib/pty-bridge.test.ts` failed 2/2 tests because `openPty` did not accept or send terminal metadata.
- GREEN: the same command passed 2/2 tests after adding stable UUID, optional address name, and privacy to the invoke payload.
- RED: `cargo test --manifest-path src-tauri/Cargo.toml --locked --test terminal_control_pty` failed to compile because spawn credentials and PTY lifecycle service methods were absent.
- GREEN: the same unfiltered command passed the native environment, WSL exclusion, and send/close lifecycle tests.
- RED: the added activation-boundary test observed `TARGET_NOT_LIVE` before PTY activation, proving that the prepared digest was authenticating too early.
- GREEN: after separating token generation from credential activation, the focused activation-boundary test passed and the complete integration target passed 5/5 tests.

### Final verification

- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test terminal_control_pty`: PASS, 5/5 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib shell_init`: PASS, 8/8 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib pty`: PASS, 55/55 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib terminal_control`: PASS, 23/23 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test terminal_control_service`: PASS, 9/9 tests.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked --all-targets`: PASS with the accepted unused `window` warning at `src-tauri/src/lib.rs:84`.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings`: blocked only by the accepted unused `window` baseline; rerunning with `-A unused-variables` passed without TAM-6 findings.
- `rustfmt --edition 2021 --check --config skip_children=true` over the task Rust entry points: PASS. Repository-wide `cargo fmt --all -- --check` still reports the accepted pre-existing Rust formatting drift.
- `pnpm test -- src/modules/terminal`: PASS, 15 test files and 105 tests.
- `pnpm check-types`: PASS, exit 0.
- `pnpm test`: PASS, 49 test files and 369 tests.
- Focused `biome check` over the six task frontend files: PASS with two pre-existing terminal-view warnings and no errors.
- `git diff --cached --check`: PASS before the implementation commit.
- The task's literal integration command passed while filtering out all 5 tests; the correct unfiltered target above executed them. The task's literal `shell_init pty` command is invalid Cargo syntax, so the two correct filters above were run separately.

### Notes

- Frontend metadata is captured for the first PTY spawn, so durable address-name updates do not rebind renderer slots.
- Native Windows children receive pane ID, endpoint, one-time raw token, and the current executable directory prepended to `PATH`. WSL children receive no control environment and remain persisted but non-live.
- The raw capability exists only in spawn preparation and child environment injection. Its digest becomes authenticatable only after the PTY session is inserted and activated.
- Explicit close, close-all, waiter exit, immediate exit, and app shutdown mark records closing before session removal and revoke credentials after removal/drop scheduling.
- An already admitted send may complete its held write; later resolution fails with typed target-not-live behavior.
- `docs/architecture/terax-architecture-report.md` remained untouched and untracked.
