### Task 8: Prove security, concurrency, restart, and full request flow

<TASK-ID>TAM-8</TASK-ID>

**Files:**
- Create: `src-tauri/tests/terminal_control_windows.rs`
- Create: `src/modules/terminal/lib/terminalControl.integration.test.ts`
- Modify: `src-tauri/src/modules/terminal_control/*`
- Modify: `src/modules/terminal/lib/terminalControl.ts`
- Modify: `src/modules/terminal/lib/useTerminalControlBridge.ts`

- [ ] **Step 1: Write complete backend integration matrix**

Use real Windows named pipe + real `teraxctl` request encoding + fake awaited PTY sink. Required cases:

```rust
#[test]
fn authenticated_send_returns_after_exact_writer_completion() {}

#[test]
fn forged_expired_and_cross_instance_tokens_fail() {}

#[test]
fn concurrent_name_claims_have_one_winner() {}

#[test]
fn concurrent_target_writes_do_not_interleave() {}

#[test]
fn send_close_race_has_only_documented_outcomes() {}

#[test]
fn private_source_can_send_but_private_target_is_masked() {}

#[test]
fn capacity_and_rate_limits_reject_without_offline_queue() {}
```

Use barriers/channels instead of sleeps for write-completion and close-race assertions.

- [ ] **Step 2: Write frontend restart/persistence integration tests**

Test this full pure-data cycle:

1. Hydrate legacy leaf.
2. Persist generated `terminalId`.
3. Apply backend name event.
4. Serialize space.
5. Rehydrate serialized space.
6. Collect catalog.
7. Assert same terminal ID/name/private status.

Also prove duplicate persisted names remain conflicted in backend sync and are never silently suffixed.

- [ ] **Step 3: Run new matrix and capture failures**

Run on Windows: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_windows --test terminal_control_windows -- --test-threads=1`

Expected: the new test target compiles and runs. Record the exact failing assertion if the matrix exposes a backend gap; PASS is valid when prior tasks already satisfy the matrix.

Run: `pnpm test -- src/modules/terminal/lib/terminalControl.integration.test.ts`

Expected: the new test target compiles and runs. Record the exact failing assertion if the matrix exposes a frontend gap; PASS is valid when prior tasks already satisfy the matrix.

- [ ] **Step 4: Fix only matrix-exposed gaps, if any**

Keep corrections inside existing boundaries:

- Directory/name reservation locking.
- Pending persistence acknowledgement cleanup.
- Credential revocation order.
- Pipe handler capacity accounting.
- Per-target writer serialization.
- Catalog conflict reconciliation.
- Sanitized diagnostics.

Do not add delivery history, retry, receiver acknowledgement, multi-line input, broadcast, TCP, WSL shim, or Unix socket.
If both new suites passed in Step 3, make no production change in this step and continue to the repeatable green run.

- [ ] **Step 5: Run integration matrix**

Run on Windows: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_windows --test terminal_control_windows -- --test-threads=1`

Expected: PASS.

Run: `pnpm test -- src/modules/terminal/lib/terminalControl.integration.test.ts`

Expected: PASS.

- [ ] **Step 6: Perform manual two-pane Windows smoke test**

Run Terax dev build, open two Windows-native PowerShell panes, then execute:

```powershell
teraxctl name agent-a
teraxctl list
```

In second pane:

```powershell
teraxctl name agent-b
```

Back in first pane:

```powershell
teraxctl send agent-b "reply with received"
```

Expected in second pane input: `[terax from agent-a] reply with received` submitted with Enter. Repeat with private target, closed target, app restart, and stopped app; observe `TARGET_NOT_FOUND`, `TARGET_NOT_LIVE`, restored names, and `TERAX_UNAVAILABLE` respectively.

- [ ] **Step 7: Commit integration hardening**

```bash
git add src-tauri/tests/terminal_control_windows.rs src-tauri/src/modules/terminal_control src/modules/terminal/lib/terminalControl.integration.test.ts src/modules/terminal/lib/terminalControl.ts src/modules/terminal/lib/useTerminalControlBridge.ts
git commit -m "test(control): cover terminal messaging flow"
```
