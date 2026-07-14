### Task 6: Bind pane identity, credentials, and PTY lifecycle

<TASK-ID>TAM-6</TASK-ID>

**Files:**
- Modify: `src/modules/terminal/TerminalStack.tsx:27-107`
- Modify: `src/modules/terminal/PaneTreeView.tsx:19-80`
- Modify: `src/modules/terminal/TerminalPane.tsx:25-82`
- Modify: `src/modules/terminal/lib/useTerminalSession.ts:426-561,800-855`
- Modify: `src/modules/terminal/lib/pty-bridge.ts:18-74`
- Modify: `src-tauri/src/modules/pty/mod.rs:42-93,171-197,283-302`
- Modify: `src-tauri/src/modules/pty/session.rs:102-315`
- Modify: `src-tauri/src/modules/pty/shell_init.rs:52-68,144-176,507-636`
- Modify: `src-tauri/src/modules/terminal_control/service.rs`
- Create: `src-tauri/tests/terminal_control_pty.rs`

- [ ] **Step 1: Write failing PTY lifecycle tests**

Cover:

```rust
#[test]
fn native_spawn_env_contains_pane_capability_and_cli_path() {
    let env = SpawnCredential::test_value();
    let vars = env.variables();
    assert_eq!(vars["TERAX_PANE_ID"], "pane-a");
    assert_eq!(vars["TERAX_IPC_ENDPOINT"], r"\\.\pipe\terax-control-test");
    assert_eq!(vars["TERAX_IPC_TOKEN"], "token-a");
    assert!(vars["PATH"].starts_with(r"C:\Program Files\Terax;"));
}

#[test]
fn wsl_spawn_gets_no_control_environment_and_never_becomes_live() {
    let result = prepare_spawn(WorkspaceEnv::Wsl { distro: "Ubuntu".into() });
    assert!(result.credential.is_none());
    assert_eq!(result.record_state, RecordState::Persisted);
}
```

Add send/close race tests proving either one complete write or `TARGET_NOT_LIVE`/`WRITE_FAILED`, never stale writer access or false success.

- [ ] **Step 2: Run PTY integration tests and confirm metadata is absent**

Run: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_pty --test terminal_control_pty`

Expected: FAIL because PTY open does not accept stable terminal metadata or credentials.

- [ ] **Step 3: Carry metadata through frontend stack**

Add these props from terminal tab/leaf to `TerminalPane` and `useTerminalSession`:

```ts
terminalId: string;
addressName?: string;
private: boolean;
```

`TerminalStack` passes `t.private === true`; `PaneTreeView` passes `node.terminalId` and `node.addressName`; `useTerminalSession` captures metadata for the first PTY spawn but does not rebind renderer slots when the name changes.

Extend `openPty` invoke payload:

```ts
terminalId,
addressName: addressName ?? null,
private: privateTerminal,
```

- [ ] **Step 4: Prepare credential before native child spawn**

Extend `pty_open` with `terminal_id`, `address_name`, and `private`. Preserve `workspace` before moving it into spawn logic and classify `WorkspaceEnv::Wsl` as unsupported for MVP.

For native Windows:

1. Ensure/upsert unnamed new record against current catalog.
2. Generate capability and endpoint env before `session::spawn`.
3. Pass `Option<SpawnCredential>` through `session::spawn` to `shell_init::build_command`.
4. Apply variables only in `shell_init::windows::build` after the WSL early return.
5. Spawn PTY.
6. Insert session.
7. Activate `terminalId → pty_id` and credential digest.
8. On any error, abort pending credential and leave record non-live.

Resolve CLI directory from `std::env::current_exe()?.parent()`; do not inspect HOME or hard-code installation paths.

- [ ] **Step 5: Update close, early-exit, close-all, and waiter paths**

Before session removal, call `begin_close_by_pty(id)` so new target resolution fails. After removal/drop scheduling, call `finish_close_by_pty(id)` to revoke credential and retain persisted identity as exited. Apply this to:

- Explicit `pty_close`.
- `pty_close_all`.
- Shell waiter calling `PtyState::take`.
- Immediate-exit recheck inside `pty_open`.
- App shutdown.

Existing send requests already holding the session `Arc` and writer mutex may finish; all later requests fail typed.

- [ ] **Step 6: Run PTY and shell-init tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_pty --test terminal_control_pty`

Expected: PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml shell_init pty --lib`

Expected: PASS; existing ConPTY lifecycle, WSL launch, and shell initialization behavior remains unchanged outside native env injection.

- [ ] **Step 7: Run frontend terminal tests**

Run: `pnpm test -- src/modules/terminal`

Expected: PASS.

Run: `pnpm check-types`

Expected: exit 0.

- [ ] **Step 8: Commit PTY integration**

```bash
git add src/modules/terminal src-tauri/src/modules/pty src-tauri/src/modules/terminal_control/service.rs src-tauri/tests/terminal_control_pty.rs
git commit -m "feat(terminal): bind control identities"
```
