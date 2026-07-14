### Task 3: Add bounded Windows named-pipe transport

<TASK-ID>TAM-3</TASK-ID>

**Files:**
- Modify: `src-tauri/Cargo.toml:70-84`
- Modify: `src-tauri/Cargo.lock`
- Create: `src-tauri/src/modules/terminal_control/framing.rs`
- Create: `src-tauri/src/modules/terminal_control/transport/mod.rs`
- Create: `src-tauri/src/modules/terminal_control/transport/windows.rs`
- Create: `src-tauri/tests/terminal_control_pipe_windows.rs`

- [ ] **Step 1: Write failing frame tests**

```rust
#[test]
fn frame_round_trip() {
    let payload = br#"{"version":1}"#;
    let mut bytes = Vec::new();
    write_frame(&mut bytes, payload).unwrap();
    assert_eq!(read_frame(&mut bytes.as_slice()).unwrap(), payload);
}

#[test]
fn oversized_frame_is_rejected_before_body_read() {
    let mut bytes = ((MAX_FRAME_BYTES as u32) + 1).to_le_bytes().to_vec();
    bytes.extend_from_slice(b"ignored");
    assert_eq!(read_frame(&mut bytes.as_slice()).unwrap_err().kind(), ErrorKind::InvalidData);
}
```

- [ ] **Step 2: Write failing Windows transport tests**

Test a real random pipe endpoint:

```rust
#[test]
fn current_process_can_round_trip_one_frame() {
    let endpoint = unique_test_endpoint();
    let server = TestServer::spawn(endpoint.clone(), |request| request);
    let response = call(&endpoint, br#"{"operation":"list"}"#, Duration::from_secs(2)).unwrap();
    assert_eq!(response, br#"{"operation":"list"}"#);
    server.shutdown();
}
```

Also assert remote-client rejection flag, maximum-frame enforcement, busy timeout, and clean shutdown wake-up.

- [ ] **Step 3: Run tests and confirm transport is absent**

Run on Windows: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_pipe_windows --test terminal_control_pipe_windows`

Expected: FAIL because framing and pipe transport are missing.

- [ ] **Step 4: Enable required Windows APIs**

Extend `windows-sys` features with:

```toml
"Win32_Security_Authorization",
"Win32_Storage_FileSystem",
"Win32_System_Memory",
"Win32_System_Pipes",
```

- [ ] **Step 5: Implement bounded framing**

```rust
pub fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0_u8; 4];
    reader.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload)?;
    Ok(payload)
}

pub fn write_frame(writer: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
    }
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}
```

- [ ] **Step 6: Implement current-user security descriptor**

Retrieve current process token with `OpenProcessToken` + `GetTokenInformation(TokenUser)`, convert SID using `ConvertSidToStringSidW`, then create protected SDDL:

```text
D:P(A;;GA;;;<current-user-sid>)
```

Convert using `ConvertStringSecurityDescriptorToSecurityDescriptorW`. Own the returned descriptor with a small RAII type calling `LocalFree`. Pass its `SECURITY_ATTRIBUTES` to every `CreateNamedPipeW` call.

- [ ] **Step 7: Implement server and client adapters**

Server pipe flags:

```rust
PIPE_ACCESS_DUPLEX
PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS
```

The accept loop creates one pipe instance, connects, rejects above 32 active handlers, and spawns one bounded handler thread. Each connection reads exactly one request frame and writes exactly one response frame. Shutdown sets an atomic flag and connects a local client to wake `ConnectNamedPipe`.

The client uses `WaitNamedPipeW` with caller timeout, then `CreateFileW` for `GENERIC_READ | GENERIC_WRITE`. Convert Win32 errors into `TERAX_UNAVAILABLE` or `SERVER_BUSY` at the client boundary.

- [ ] **Step 8: Run Windows transport tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control_pipe_windows --test terminal_control_pipe_windows`

Expected: PASS; pipe disappears after shutdown and oversize input is rejected.

- [ ] **Step 9: Run cross-platform library checks**

Run: `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`

Expected: exit 0; Windows module is `#[cfg(windows)]`, non-Windows builds keep protocol/domain code but expose no pipe server.

- [ ] **Step 10: Commit transport**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/modules/terminal_control src-tauri/tests/terminal_control_pipe_windows.rs
git commit -m "feat(control): add Windows pipe transport"
```
