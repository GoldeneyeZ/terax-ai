### Task 2: Build protocol, directory, credentials, and limits

<TASK-ID>TAM-2</TASK-ID>

**Files:**
- Modify: `src-tauri/Cargo.toml:21-56`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/modules/mod.rs`
- Create: `src-tauri/src/modules/terminal_control/mod.rs`
- Create: `src-tauri/src/modules/terminal_control/protocol.rs`
- Create: `src-tauri/src/modules/terminal_control/directory.rs`
- Create: `src-tauri/src/modules/terminal_control/credentials.rs`
- Create: `src-tauri/src/modules/terminal_control/rate_limit.rs`

- [ ] **Step 1: Add failing protocol and validation tests**

Cover exact wire values:

```rust
#[test]
fn send_builds_one_plain_envelope() {
    let bytes = build_envelope("agent-a", "review commit").unwrap();
    assert_eq!(bytes, b"[terax from agent-a] review commit\r");
}

#[test]
fn rejects_control_characters_and_oversize_messages() {
    assert_eq!(validate_message("a\nb"), Err(ErrorCode::MessageInvalid));
    assert_eq!(validate_message("\u{1b}[31m"), Err(ErrorCode::MessageInvalid));
    assert_eq!(
        validate_message(&"a".repeat(16 * 1024 + 1)),
        Err(ErrorCode::MessageTooLarge),
    );
}
```

Add Serde round-trip assertions for `name`, `list`, `send`, success data, and every error code.

- [ ] **Step 2: Add failing directory tests**

```rust
#[test]
fn duplicate_claim_keeps_existing_owner() {
    let mut directory = hydrated_directory();
    directory.reserve_name("pane-a", "agent-a", "req-1").unwrap();
    directory.commit_name("req-1").unwrap();
    assert_eq!(
        directory.reserve_name("pane-b", "agent-a", "req-2"),
        Err(ErrorCode::NameInUse),
    );
    assert_eq!(directory.owner("agent-a"), Some("pane-a"));
}

#[test]
fn private_and_conflicted_targets_are_masked() {
    let directory = directory_with_private_and_conflicted_records();
    assert_eq!(directory.resolve_target("private-a"), Err(ErrorCode::TargetNotFound));
    assert_eq!(directory.resolve_target("conflict-a"), Err(ErrorCode::TargetNotFound));
}
```

Also prove inactive public names return `TargetNotLive`, catalog deletion releases inactive names, and a live record omitted by sync enters `Closing` before removal.

- [ ] **Step 3: Add failing credential and rate-limit tests**

```rust
#[test]
fn token_authenticates_one_pane_then_revokes() {
    let mut credentials = Credentials::default();
    let token = credentials.issue("pane-a").unwrap();
    assert_eq!(credentials.authenticate(&token), Some("pane-a".to_string()));
    credentials.revoke_pane("pane-a");
    assert_eq!(credentials.authenticate(&token), None);
}

#[test]
fn token_bucket_allows_burst_then_rejects() {
    let mut bucket = TokenBucket::new(20.0, 40.0, Instant::now());
    for _ in 0..40 {
        assert!(bucket.take(Instant::now()));
    }
    assert!(!bucket.take(Instant::now()));
}
```

- [ ] **Step 4: Run Rust tests and confirm modules are absent**

Run: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control --lib`

Expected: FAIL because terminal-control modules and direct crypto dependencies do not exist.

- [ ] **Step 5: Add minimal direct dependencies**

```toml
base64 = "0.22"
getrandom = "0.3"
sha2 = "0.10"
subtle = "2.6"
```

Use `cargo add` or edit `Cargo.toml`, then regenerate `Cargo.lock`. Do not add a CLI parser crate; three commands remain a small manual parser.

- [ ] **Step 6: Implement protocol v1**

Use these stable public shapes:

```rust
pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ControlRequest {
    Name {
        version: u16,
        request_id: String,
        source_token: String,
        payload: NamePayload,
    },
    List {
        version: u16,
        request_id: String,
        source_token: String,
        payload: ListPayload,
    },
    Send {
        version: u16,
        request_id: String,
        source_token: String,
        payload: SendPayload,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamePayload { pub name: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListPayload {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendPayload { pub target: String, pub message: String }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    TeraxUnavailable,
    InvalidRequest,
    UnsupportedVersion,
    AuthFailed,
    SourceUnnamed,
    InvalidName,
    NameInUse,
    TargetNotFound,
    TargetNotLive,
    MessageInvalid,
    MessageTooLarge,
    PersistFailed,
    PersistTimeout,
    RateLimited,
    ServerBusy,
    WriteFailed,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlResponse {
    pub version: u16,
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ControlError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ResponseData {
    Name { name: String },
    List { names: Vec<String> },
    Send { target: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlError { pub code: ErrorCode, pub message: String }
```

Define common-field accessors on `ControlRequest`, plus `ControlResponse`, `ResponseData`, `ControlError`, and `ErrorCode` so serialized codes exactly match the design. `validate_name` ASCII-lowercases then enforces `^[a-z][a-z0-9-]{0,62}$` without a regex dependency. `validate_message` rejects invalid length and C0/C1 controls except horizontal tab. `build_envelope` is the only function that appends `\r`.

- [ ] **Step 7: Implement directory state and transactions**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordState {
    Persisted,
    Live,
    Closing,
    Exited,
    Conflicted,
}

#[derive(Debug, Clone)]
pub struct TerminalRecord {
    pub terminal_id: String,
    pub address_name: Option<String>,
    pub private: bool,
    pub state: RecordState,
    pub pty_id: Option<u32>,
}
```

Maintain `records`, committed `names`, and pending `reservations` under one directory mutex. Catalog sync is complete and authoritative for persisted membership but preserves matching runtime `Live`/`Closing` state. Name reservation, frontend acknowledgement, commit, and rollback are separate calls so no mutex remains held while waiting for the webview.

- [ ] **Step 8: Implement hash-only credentials and rate limiting**

Generate 32 random bytes with `getrandom::fill`, encode URL-safe base64 without padding, hash with SHA-256, and keep only digest + terminal ID. Authentication scans all bounded entries and combines `subtle::ConstantTimeEq` results; it must not return on first mismatch. Revoke by terminal ID on close.

Use a `TokenBucket` with 20 tokens/second and burst 40. The service will keep one bucket per authenticated source pane.

- [ ] **Step 9: Run domain tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml terminal_control --lib`

Expected: PASS for protocol, validation, directory, credentials, and token bucket.

- [ ] **Step 10: Commit backend domain**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/modules/mod.rs src-tauri/src/modules/terminal_control
git commit -m "feat(control): add terminal messaging domain"
```
