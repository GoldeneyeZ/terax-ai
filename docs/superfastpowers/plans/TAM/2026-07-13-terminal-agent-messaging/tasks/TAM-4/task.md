### Task 4: Route requests through Rust control service

<TASK-ID>TAM-4</TASK-ID>

**Files:**
- Create: `src-tauri/src/modules/terminal_control/service.rs`
- Modify: `src-tauri/src/modules/terminal_control/mod.rs`
- Modify: `src-tauri/src/modules/pty/mod.rs:18-37,98-134`
- Modify: `src-tauri/src/lib.rs:114-285`
- Create: `src-tauri/tests/terminal_control_service.rs`

- [ ] **Step 1: Write failing service tests with fake PTY writer and frontend persistence**

```rust
#[derive(Default)]
struct FakePty {
    writes: Mutex<Vec<(u32, Vec<u8>)>>,
    fail: AtomicBool,
}

impl PtySink for FakePty {
    fn write(&self, pty_id: u32, bytes: &[u8]) -> Result<(), String> {
        if self.fail.load(Ordering::Acquire) {
            return Err("broken pipe".into());
        }
        self.writes.lock().unwrap().push((pty_id, bytes.to_vec()));
        Ok(())
    }
}
```

Test list masking, authenticated send, source unnamed, write failure, rate limit, name commit, name failure rollback, and five-second timeout using an injectable clock and persistence adapter.

- [ ] **Step 2: Run service tests and confirm routing is absent**

Run: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_service --test terminal_control_service`

Expected: FAIL because `ControlService`, `PtySink`, and persistence adapter do not exist.

- [ ] **Step 3: Add shared PTY write primitive**

Refactor existing `pty_write` through:

```rust
impl PtyState {
    pub fn write_bytes(&self, id: u32, bytes: &[u8]) -> Result<(), String> {
        let session = self
            .sessions
            .read()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| "no session".to_string())?;
        let result = session.writer.lock().unwrap().write_all(bytes).map_err(|error| {
            log::debug!("pty write id={id} failed: {error}");
            error.to_string()
        });
        result
    }
}
```

`pty_write` keeps its raw-body/header contract and delegates to `write_bytes`. Implement `PtySink` for `PtyState` so control sends and keyboard writes share the same awaited writer mutex.

- [ ] **Step 4: Implement service state and typed request routing**

```rust
pub struct TerminalControlState {
    directory: Mutex<TerminalDirectory>,
    credentials: Mutex<Credentials>,
    rate_limits: Mutex<HashMap<String, TokenBucket>>,
    pending_names: Mutex<HashMap<String, PendingName>>,
    hydrated: AtomicBool,
    shutdown: AtomicBool,
    endpoint: String,
    pipe: Mutex<Option<PipeServer>>,
}
```

Request order is fixed:

1. Reject unsupported protocol version.
2. Reject before first complete catalog sync.
3. Authenticate token and derive source ID.
4. Apply source rate limit for `send`.
5. Route `name`, `list`, or `send`.
6. Serialize one typed response.

`send` resolves source name and target under directory lock, releases that lock, builds envelope, then calls `PtySink::write`. It logs request ID, operation, source/target terminal IDs, error code, and duration; it never logs token, digest, or message.

- [ ] **Step 5: Implement name persistence transaction without lock-held waits**

Use event name:

```rust
pub const PERSIST_NAME_EVENT: &str = "terminal-control://persist-name";
```

Event payload:

```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistNameRequest {
    pub request_id: String,
    pub terminal_id: String,
    pub old_name: Option<String>,
    pub new_name: String,
}
```

Reserve name, release directory mutex, emit to main webview, wait on a per-request `Condvar` for at most five seconds, then commit or roll back. The acknowledgement Tauri command only signals pending state; the waiting request reacquires directory lock for commit/rollback.

- [ ] **Step 6: Add Tauri commands and server startup**

```rust
#[tauri::command]
pub fn terminal_control_sync_catalog(
    state: tauri::State<'_, TerminalControlState>,
    records: Vec<CatalogRecord>,
) -> Result<(), String> {
    state.sync_catalog(records).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn terminal_control_ack_name(
    state: tauri::State<'_, TerminalControlState>,
    request_id: String,
    error: Option<String>,
) -> Result<(), String> {
    state.ack_name(&request_id, error)
}
```

Manage `TerminalControlState`, register both commands, and start the pipe server in Tauri `setup`. Generate a fresh 16-byte endpoint nonce with `getrandom`, encode it as lowercase hex, and use `\\.\pipe\terax-control-<pid>-<nonce>` for this app instance; never persist or reuse the endpoint. On `RunEvent::Exit`, stop accepting pipe clients before PTY/LSP teardown and revoke every credential.

- [ ] **Step 7: Run service and existing PTY tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_service --test terminal_control_service`

Expected: PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml pty --lib`

Expected: PASS; raw frontend PTY writes still use the same writer and error behavior.

- [ ] **Step 8: Commit service routing**

```bash
git add src-tauri/src/modules/terminal_control src-tauri/src/modules/pty/mod.rs src-tauri/src/lib.rs src-tauri/tests/terminal_control_service.rs
git commit -m "feat(control): route terminal messages"
```
