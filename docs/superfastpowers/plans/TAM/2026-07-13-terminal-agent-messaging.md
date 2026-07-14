# Terminal Agent Messaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superfastpowers:goal-driven-development with `goal-driven-bypass` (recommended), `goal-driven-gated`, superfastpowers:subagent-driven-development, or superfastpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add authenticated Windows-native pane-to-pane agent messaging through `teraxctl.exe name|list|send`, backed by a Rust terminal directory and awaited PTY writes.

**Architecture:** Frontend persists stable pane UUIDs and address names, then synchronizes a complete terminal catalog to a Rust-owned control service. Rust authenticates per-PTY capability tokens, resolves live public targets, and serves a versioned JSON protocol over a current-user-only Windows named pipe; the separate `teraxctl.exe` sidecar is a thin client. Existing PTY writers remain the only delivery primitive, preserving ConPTY lifecycle and renderer independence.
**Plan Acronym:** TAM

**Tech Stack:** React 19, TypeScript 6, Vitest 4, Tauri 2, Rust 2021, `portable-pty` 0.9, `windows-sys` 0.61, Serde, SHA-256, Windows named pipes

---

## Source documents

- Design: `docs/superfastpowers/specs/2026-07-13-terminal-agent-messaging-design.md`
- Architecture evidence: `docs/architecture/terax-architecture-report.md`
- Project constitution: `TERAX.md`
- PTY guide: `docs/architecture/pty-shell-integration.md`
- Security guide: `docs/architecture/security-model.md`
- Two-process guide: `docs/architecture/two-process-model.md`
- Testing contract: `docs/contributing/testing.md`
- Tauri v2 sidecar reference: `https://v2.tauri.app/develop/sidecar/`
- Tauri v2 platform config reference: `https://v2.tauri.app/develop/configuration-files/`

## File structure and ownership

### Frontend

- `src/modules/terminal/lib/terminalIdentity.ts` — UUID creation, leaf identity helpers, catalog projection, immutable address-name update.
- `src/modules/terminal/lib/terminalControl.ts` — typed Tauri command/event wrappers and pure frontend protocol types.
- `src/modules/terminal/lib/useTerminalControlBridge.ts` — catalog synchronization and durable name-change acknowledgement.
- Existing pane, tab, serialization, terminal-stack, and PTY bridge files — carry `terminalId`, `addressName`, and tab privacy to Rust without making renderer slots part of identity.

### Rust

- `src-tauri/src/modules/terminal_control/protocol.rs` — protocol v1 wire models, error codes, validation, envelope construction.
- `src-tauri/src/modules/terminal_control/directory.rs` — persisted/live records, name indexes, reservations, target masking, lifecycle transitions.
- `src-tauri/src/modules/terminal_control/credentials.rs` — random pane capabilities, hash-only storage, constant-time authentication, revocation.
- `src-tauri/src/modules/terminal_control/rate_limit.rs` — per-source token bucket.
- `src-tauri/src/modules/terminal_control/framing.rs` — bounded little-endian length framing.
- `src-tauri/src/modules/terminal_control/transport/windows.rs` — current-user-only named-pipe server/client.
- `src-tauri/src/modules/terminal_control/service.rs` — request routing, name-persistence transaction, PTY adapter, logging, shutdown.
- `src-tauri/src/modules/terminal_control/mod.rs` — state construction, Tauri commands, event constants, public exports.
- `src-tauri/src/bin/teraxctl.rs` — Windows CLI parsing, IPC call, human/JSON output, stable exit codes.

### Build and docs

- `scripts/prepare-teraxctl-sidecar.mjs` — builds and stages target-triple sidecar artifacts.
- `scripts/prepare-teraxctl-sidecar.test.mjs` — pure path/triple/argument tests.
- `src-tauri/tauri.windows.conf.json` — Windows-only `externalBin` and pre-build hooks.
- `docs/architecture/terminal-agent-messaging.md` — implemented architecture, operations, security boundary, troubleshooting.

## Dependency order

`TAM-1 → TAM-2 → TAM-3 → TAM-4 → TAM-5 → TAM-6 → TAM-7 → TAM-8 → TAM-9`

Each task ends with a focused commit. Never stage `docs/architecture/terax-architecture-report.md` unless the user separately requests it.

### Task 1: Persist stable terminal identities

<TASK-ID>TAM-1</TASK-ID>

**Files:**
- Create: `src/modules/terminal/lib/terminalIdentity.ts`
- Create: `src/modules/terminal/lib/terminalIdentity.test.ts`
- Modify: `src/modules/terminal/lib/panes.ts:13-20,72-112`
- Modify: `src/modules/spaces/lib/serialize.ts:14-16,42-55,106-137,202-218`
- Modify: `src/modules/spaces/lib/useSpacesBoot.ts:37-134`
- Create: `src/modules/spaces/lib/useSpacesBoot.test.ts`
- Modify: `src/app/App.tsx:218-239`
- Modify: `src/modules/tabs/lib/useTabs.ts:200-216,306-326,449-496,1045-1070`
- Modify fixtures: `src/modules/terminal/lib/panes.test.ts`
- Modify fixtures: `src/modules/terminal/lib/liveTerminals.test.ts`
- Modify fixtures: `src/modules/spaces/lib/serialize.test.ts`
- Modify fixtures: `src/modules/tabs/lib/pickTabBySpaceIndex.test.ts`
- Modify fixtures: `src/modules/tabs/lib/planSpaceRemoval.test.ts`
- Modify fixtures: `src/modules/tabs/lib/nextActiveInSpace.test.ts`
- Modify fixtures: `src/modules/tabs/lib/reorderTabsByGap.test.ts`
- Modify fixtures: `src/modules/tabs/lib/tabLabel.test.ts`

- [ ] **Step 1: Write failing identity and migration tests**

```ts
import { describe, expect, it } from "vitest";
import {
  canonicalAddressName,
  newTerminalId,
  withAddressName,
} from "./terminalIdentity";

describe("terminal identity", () => {
  it("creates UUID identities", () => {
    expect(newTerminalId()).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });

  it("canonicalizes valid names and rejects invalid names", () => {
    expect(canonicalAddressName("Agent-B")).toBe("agent-b");
    expect(() => canonicalAddressName("agent b")).toThrow("INVALID_NAME");
  });

  it("updates one stable leaf without changing numeric pane ids", () => {
    const tree = {
      kind: "leaf" as const,
      id: 7,
      terminalId: "00000000-0000-4000-8000-000000000007",
    };
    expect(withAddressName(tree, tree.terminalId, "agent-b")).toEqual({
      ...tree,
      addressName: "agent-b",
    });
  });
});
```

Add serialization tests proving:

```ts
expect(serialized.tree).toEqual({
  kind: "leaf",
  terminalId: "00000000-0000-4000-8000-000000000001",
  addressName: "agent-a",
  active: true,
});

expect(migrated.migrated).toBe(true);
expect(migrated.tabs[0].paneTree).toMatchObject({
  kind: "leaf",
  terminalId: "00000000-0000-4000-8000-000000000099",
});
```

- [ ] **Step 2: Run focused tests and confirm missing identity APIs fail**

Run: `pnpm test -- src/modules/terminal/lib/terminalIdentity.test.ts src/modules/spaces/lib/serialize.test.ts`

Expected: FAIL because `terminalIdentity.ts`, persisted `terminalId`, and migration result do not exist.

- [ ] **Step 3: Add identity types and immutable helpers**

```ts
import type { PaneNode } from "./panes";

export type TerminalId = string;

const ADDRESS_NAME = /^[a-z][a-z0-9-]{0,62}$/;

export function newTerminalId(): TerminalId {
  return crypto.randomUUID();
}

export function canonicalAddressName(input: string): string {
  const name = input.toLowerCase();
  if (!ADDRESS_NAME.test(name)) throw new Error("INVALID_NAME");
  return name;
}

export function withAddressName(
  node: PaneNode,
  terminalId: TerminalId,
  addressName: string | undefined,
): PaneNode {
  if (node.kind === "leaf") {
    return node.terminalId === terminalId
      ? { ...node, addressName }
      : node;
  }
  return {
    ...node,
    children: node.children.map((child) =>
      withAddressName(child, terminalId, addressName),
    ),
  };
}
```

Change leaf shape to:

```ts
| {
    kind: "leaf";
    id: PaneId;
    terminalId: TerminalId;
    addressName?: string;
    slotId?: PaneId;
    cwd?: string;
  }
```

- [ ] **Step 4: Persist identity and expose explicit migration result**

Use this serialized leaf shape:

```ts
type SerializedLeaf = {
  kind: "leaf";
  terminalId?: string;
  addressName?: string;
  cwd?: string;
  active?: boolean;
};
```

Add a deterministic migration entry point:

```ts
export type HydratedTabs = { tabs: Tab[]; migrated: boolean };

export function hydrateTabsWithMigration(
  serialized: SerializedTab[],
  spaceId: string,
  allocId: () => number,
  allocTerminalId: () => string = newTerminalId,
): HydratedTabs {
  const migration = { changed: false };
  const tabs = hydrateTabsInternal(
    serialized,
    spaceId,
    allocId,
    allocTerminalId,
    migration,
  );
  return { tabs, migrated: migration.changed };
}
```

When hydrating a leaf, preserve `node.terminalId`; otherwise allocate once and set `migration.changed = true`. Serialize `terminalId` unconditionally and `addressName` only when present.

- [ ] **Step 5: Save migrated identities before spaces become booted**

In `useSpacesBoot`, replace each direct `hydrateTabs` call with `hydrateTabsWithMigration`. Add local `controlCatalogEligible` state, initially false, and return it from the hook. Collect migrated-space writes and await all of them before setting that state true, calling `useSpaces.getState().hydrate`, or calling `replaceTabs`:

```ts
const restored: Tab[] = [];
const migrationWrites: Promise<void>[] = [];
for (const space of spaces) {
  const st = states.get(space.id);
  if (!st) continue;
  const hydrated = hydrateTabsWithMigration(st.tabs, space.id, allocId);
  restored.push(...hydrated.tabs);
  if (hydrated.migrated) {
    migrationWrites.push(
      saveState(space.id, {
        tabs: serializeTabs(hydrated.tabs),
        activeTabIndex: st.activeTabIndex,
      }),
    );
  }
}
await Promise.all(migrationWrites);
setControlCatalogEligible(true);
```

For the empty-spaces branch, set the eligibility state only after `saveSpacesList` and `saveActiveId` succeed. On migration/save rejection, keep it false, preserve the existing `markBooted` terminal-UI behavior, and emit one sanitized control-unavailable diagnostic. In `App`, capture the returned boolean for Task 5.

Add `useSpacesBoot.test.ts` with mocked storage. Assert a legacy leaf is assigned one UUID, `saveState` receives that UUID, and eligibility stays false until the save promise resolves. Assert rejection leaves eligibility false while `markBooted` still runs. Assert an already-versioned leaf causes no migration write.

- [ ] **Step 6: Generate UUIDs in every terminal constructor and split path**

Extend `splitLeaf` with a final parameter:

```ts
newTerminalId: () => string = crypto.randomUUID,
```

Every new leaf becomes:

```ts
const newLeaf: PaneNode = {
  kind: "leaf",
  id: newLeafId,
  terminalId: newTerminalId(),
  cwd: newCwd,
};
```

Apply the same rule to `coldTerminalTab`, `freshTerminalTab`, `newTabInSpace`, `newTab`, `newBlockTab`, `newAgentTab`, `newPrivateTab`, and fresh-workspace reset. Update all test fixtures with fixed UUID strings; never use random UUIDs in equality assertions.

- [ ] **Step 7: Run frontend regression suite**

Run: `pnpm test -- src/modules/terminal/lib src/modules/spaces/lib/serialize.test.ts src/modules/spaces/lib/useSpacesBoot.test.ts src/modules/tabs/lib`

Expected: PASS; all leaf fixtures carry stable identity and legacy serialization migrates once.

- [ ] **Step 8: Check types**

Run: `pnpm check-types`

Expected: exit 0 with no missing `terminalId` errors.

- [ ] **Step 9: Commit stable identity**

```bash
git add src/modules/terminal/lib/terminalIdentity.ts src/modules/terminal/lib/terminalIdentity.test.ts src/modules/terminal/lib/panes.ts src/modules/terminal/lib/panes.test.ts src/modules/terminal/lib/liveTerminals.test.ts src/modules/spaces/lib/serialize.ts src/modules/spaces/lib/serialize.test.ts src/modules/spaces/lib/useSpacesBoot.ts src/modules/spaces/lib/useSpacesBoot.test.ts src/modules/tabs/lib src/app/App.tsx
git commit -m "feat(terminal): persist pane identities"
```

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

### Task 5: Synchronize frontend catalog and durable name changes

<TASK-ID>TAM-5</TASK-ID>

**Files:**
- Create: `src/modules/terminal/lib/terminalControl.ts`
- Create: `src/modules/terminal/lib/terminalControl.test.ts`
- Create: `src/modules/terminal/lib/useTerminalControlBridge.ts`
- Modify: `src/modules/terminal/lib/terminalIdentity.ts`
- Modify: `src/modules/spaces/lib/useSpacePersistence.ts:18-102`
- Modify: `src/app/App.tsx:110-239`

- [ ] **Step 1: Write failing pure catalog and name-change tests**

```ts
it("projects every saved terminal leaf into one canonical catalog", () => {
  expect(collectTerminalCatalog(tabs)).toEqual([
    {
      terminalId: "00000000-0000-4000-8000-000000000001",
      addressName: "agent-a",
      private: false,
    },
  ]);
});

it("updates one address by terminal UUID", () => {
  const next = applyPersistedName(tabs, {
    requestId: "req-1",
    terminalId: "00000000-0000-4000-8000-000000000001",
    oldName: undefined,
    newName: "agent-a",
  });
  expect(findLeaf(next, "00000000-0000-4000-8000-000000000001")?.addressName).toBe("agent-a");
});
```

Also test missing terminal ID returns a typed failure and private tab status propagates to every leaf.

- [ ] **Step 2: Run focused tests and confirm bridge APIs are absent**

Run: `pnpm test -- src/modules/terminal/lib/terminalControl.test.ts`

Expected: FAIL because catalog projection and persistence-event models do not exist.

- [ ] **Step 3: Implement pure frontend control helpers**

```ts
export type CatalogRecord = {
  terminalId: string;
  addressName?: string;
  private: boolean;
};

export type PersistNameRequest = {
  requestId: string;
  terminalId: string;
  oldName?: string;
  newName: string;
};

function terminalLeaves(node: PaneNode): Extract<PaneNode, { kind: "leaf" }>[] {
  return node.kind === "leaf"
    ? [node]
    : node.children.flatMap(terminalLeaves);
}

export function collectTerminalCatalog(tabs: Tab[]): CatalogRecord[] {
  const records: CatalogRecord[] = [];
  for (const tab of tabs) {
    if (tab.kind !== "terminal") continue;
    for (const leaf of terminalLeaves(tab.paneTree)) {
      records.push({
        terminalId: leaf.terminalId,
        addressName: leaf.addressName,
        private: tab.private === true,
      });
    }
  }
  return records.sort((a, b) => a.terminalId.localeCompare(b.terminalId));
}
```

Expose wrappers for `terminal_control_sync_catalog`, `terminal_control_ack_name`, and `getCurrentWebviewWindow().listen(PERSIST_NAME_EVENT, handler)`.

- [ ] **Step 4: Make space persistence awaitable on demand**

Refactor `flush` to collect `saveState` promises and return `Promise<void>`. Keep debounced/background call sites as `void flush(snapshot)`. Return this exact callback from the hook:

```ts
return useCallback(
  (nextTabs: Tab[], nextActiveId: number, nextActiveSpaceId: string) =>
    flush({
      tabs: nextTabs,
      activeId: nextActiveId,
      activeSpaceId: nextActiveSpaceId,
    }),
  [flush],
);
```

Do not acknowledge a control name change until this promise resolves.

- [ ] **Step 5: Implement catalog sync and name event hook**

The hook computes `JSON.stringify(collectTerminalCatalog(tabs))` and synchronizes only when that canonical string changes. Before the first sync, it awaits `persistNow` for the current tabs so a fresh workspace and every generated terminal UUID are durable before Rust marks the directory hydrated. Set `initialCatalogPersistedRef.current = true` only after that save succeeds; a later catalog change retries after failure. It listens once for persistence events. Handler sequence:

```ts
if (!initialCatalogPersistedRef.current) {
  await persistNow(
    tabsRef.current,
    activeIdRef.current,
    activeSpaceIdRef.current,
  );
  initialCatalogPersistedRef.current = true;
}
await syncCatalog(collectTerminalCatalog(tabsRef.current));
```

Name event handler sequence:

```ts
const current = tabsRef.current;
const next = applyPersistedName(current, request);
replaceTabs(next, activeIdRef.current);
try {
  await persistNow(next, activeIdRef.current, activeSpaceIdRef.current);
  await ackName(request.requestId);
} catch (error) {
  await ackName(request.requestId, String(error));
}
```

Keep `tabs`, `activeId`, and active-space values in refs so event callbacks never persist stale state. On catalog-sync failure, log one sanitized diagnostic and leave normal terminal operation intact; service remains unhydrated/unavailable until a later successful sync.

- [ ] **Step 6: Wire bridge in `App`**

Capture return value from `useSpacePersistence`, then invoke `useTerminalControlBridge` after spaces hydration state is available. Pass existing `tabsRef`, `replaceTabs`, active ID, active space ID, `spacesHydrated`, and `controlCatalogEligible` returned by `useSpacesBoot`. The bridge must not persist or synchronize a catalog unless both booleans are true; a migration failure therefore leaves Rust unhydrated while ordinary terminal UI remains usable.

- [ ] **Step 7: Run frontend tests and type checks**

Run: `pnpm test -- src/modules/terminal/lib/terminalControl.test.ts src/modules/spaces/lib/serialize.test.ts`

Expected: PASS.

Run: `pnpm check-types`

Expected: exit 0.

- [ ] **Step 8: Commit frontend bridge**

```bash
git add src/modules/terminal/lib/terminalControl.ts src/modules/terminal/lib/terminalControl.test.ts src/modules/terminal/lib/useTerminalControlBridge.ts src/modules/terminal/lib/terminalIdentity.ts src/modules/spaces/lib/useSpacePersistence.ts src/app/App.tsx
git commit -m "feat(control): sync terminal catalog"
```

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

### Task 7: Add `teraxctl.exe` and Windows sidecar packaging

<TASK-ID>TAM-7</TASK-ID>

**Files:**
- Create: `src-tauri/src/bin/teraxctl.rs`
- Create: `src-tauri/src/modules/terminal_control/cli.rs`
- Modify: `src-tauri/Cargo.toml:1-18`
- Create: `scripts/prepare-teraxctl-sidecar.mjs`
- Create: `scripts/prepare-teraxctl-sidecar.test.mjs`
- Modify: `package.json:scripts`
- Create: `src-tauri/tauri.windows.conf.json`
- Modify: `.gitignore`
- Create: `src-tauri/tests/teraxctl_cli.rs`

- [ ] **Step 1: Write failing CLI parser/output tests**

```rust
#[test]
fn parses_send_and_requires_one_message_argument() {
    assert_eq!(
        parse(["send", "agent-b", "review commit"]),
        Ok(CliCommand::Send {
            target: "agent-b".into(),
            message: "review commit".into(),
            json: false,
        }),
    );
    assert!(parse(["send", "agent-b", "two", "args"]).is_err());
}

#[test]
fn maps_typed_errors_to_stable_exit_codes() {
    assert_eq!(exit_code(ErrorCode::AuthFailed), 3);
    assert_eq!(exit_code(ErrorCode::NameInUse), 4);
    assert_eq!(exit_code(ErrorCode::TargetNotLive), 5);
    assert_eq!(exit_code(ErrorCode::RateLimited), 6);
    assert_eq!(exit_code(ErrorCode::WriteFailed), 7);
}
```

Also test missing env returns local `AUTH_FAILED`, `--json` never prints token, list output is newline-sorted, `--help` prints the three commands and exits 0, and invalid usage exits 2.

Use this complete exit mapping in both tests and implementation:

| Exit | Codes |
| ---: | --- |
| 0 | success |
| 1 | `INTERNAL` and unclassified local failures |
| 2 | CLI usage, `INVALID_REQUEST`, `UNSUPPORTED_VERSION`, `INVALID_NAME`, `MESSAGE_INVALID`, `MESSAGE_TOO_LARGE` |
| 3 | `TERAX_UNAVAILABLE`, `AUTH_FAILED`, `SOURCE_UNNAMED` |
| 4 | `NAME_IN_USE`, `PERSIST_FAILED`, `PERSIST_TIMEOUT` |
| 5 | `TARGET_NOT_FOUND`, `TARGET_NOT_LIVE` |
| 6 | `RATE_LIMITED`, `SERVER_BUSY` |
| 7 | `WRITE_FAILED` |

- [ ] **Step 2: Run CLI tests and confirm binary is absent**

Run: `cargo test --manifest-path src-tauri/Cargo.toml teraxctl_cli --test teraxctl_cli`

Expected: FAIL because `teraxctl` parser/client entry point does not exist.

- [ ] **Step 3: Implement thin CLI**

Manual grammar:

```text
teraxctl name <name> [--json]
teraxctl list [--json]
teraxctl send <target> <message> [--json]
```

Put the manual parser, rendering, and exit-code mapping in cross-platform `terminal_control::cli`. Read only `TERAX_IPC_ENDPOINT` and `TERAX_IPC_TOKEN`; never accept source identity or token as flags. Generate request ID from 16 random bytes encoded as lowercase hex. On Windows, call the shared client once, decode one response, print human or JSON form, then exit with stable code. Keep `cargo check --all-targets` green elsewhere with an explicit stub:

```rust
#[cfg(not(windows))]
fn main() {
    eprintln!("teraxctl is available only in Windows-native Terax terminals");
    std::process::exit(3);
}
```

The Windows `main` delegates to `terminal_control::cli::run`; `--help` is handled before reading environment variables.

Declare explicit binary:

```toml
[[bin]]
name = "teraxctl"
path = "src/bin/teraxctl.rs"
```

- [ ] **Step 4: Write failing sidecar staging tests**

```js
import assert from "node:assert/strict";
import test from "node:test";
import { parseHostTriple, sidecarDestination } from "./prepare-teraxctl-sidecar.mjs";

test("parses rustc host and stages Windows suffix", () => {
  const triple = parseHostTriple("release: 1.88.0\nhost: x86_64-pc-windows-msvc\n");
  assert.equal(triple, "x86_64-pc-windows-msvc");
  assert.match(sidecarDestination(triple), /teraxctl-x86_64-pc-windows-msvc\.exe$/);
});
```

- [ ] **Step 5: Run Node test and confirm staging module is absent**

Run: `node --test scripts/prepare-teraxctl-sidecar.test.mjs`

Expected: FAIL because staging script does not exist.

- [ ] **Step 6: Implement sidecar staging script**

Script behavior:

1. Reject non-Windows hosts with a clear message when invoked directly.
2. Parse `rustc -vV` `host:` line.
3. Accept exactly `--debug` or `--release`.
4. Run `cargo build --manifest-path src-tauri/Cargo.toml --bin teraxctl` with selected profile.
5. Create `src-tauri/binaries`.
6. Copy `target/<profile>/teraxctl.exe` to `src-tauri/binaries/teraxctl-<target-triple>.exe`.
7. Print staged path only; never alter tracked sources.

Add scripts:

```json
"dev:teraxctl-sidecar": "node scripts/prepare-teraxctl-sidecar.mjs --debug",
"build:teraxctl-sidecar": "node scripts/prepare-teraxctl-sidecar.mjs --release"
```

- [ ] **Step 7: Add Windows-only Tauri merge config**

```json
{
  "build": {
    "beforeDevCommand": "pnpm dev:teraxctl-sidecar && pnpm dev",
    "beforeBuildCommand": "pnpm build && pnpm build:teraxctl-sidecar"
  },
  "bundle": {
    "externalBin": ["binaries/teraxctl"]
  }
}
```

Tauri automatically merges `tauri.windows.conf.json` only on Windows. Official sidecar convention requires generated `teraxctl-<target-triple>.exe`; ignore `src-tauri/binaries/teraxctl-*` in Git.

- [ ] **Step 8: Verify CLI and staging**

Run: `cargo test --manifest-path src-tauri/Cargo.toml teraxctl_cli --test teraxctl_cli`

Expected: PASS.

Run: `node --test scripts/prepare-teraxctl-sidecar.test.mjs`

Expected: PASS.

Run on Windows: `pnpm dev:teraxctl-sidecar`

Expected: `src-tauri/binaries/teraxctl-x86_64-pc-windows-msvc.exe` exists and `src-tauri/target/debug/teraxctl.exe --help` prints three commands.

- [ ] **Step 9: Commit CLI and packaging**

```bash
git add src-tauri/src/bin/teraxctl.rs src-tauri/src/modules/terminal_control/cli.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tests/teraxctl_cli.rs scripts/prepare-teraxctl-sidecar.mjs scripts/prepare-teraxctl-sidecar.test.mjs package.json src-tauri/tauri.windows.conf.json .gitignore
git commit -m "feat(cli): add teraxctl sidecar"
```

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

### Task 9: Document implementation and run final gates

<TASK-ID>TAM-9</TASK-ID>

**Files:**
- Create: `docs/architecture/terminal-agent-messaging.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture/pty-shell-integration.md`
- Modify: `docs/architecture/security-model.md`
- Modify: `TERAX.md`

- [ ] **Step 1: Write implemented architecture guide**

Document:

- Stable `terminalId`, runtime PTY ID, numeric leaf ID, renderer slot ID separation.
- `teraxctl name|list|send` grammar and JSON mode.
- App-wide name reservation and persistence transaction.
- Protocol v1 framing and exact envelope.
- Named-pipe ACL, endpoint nonce, token lifecycle, rate/capacity bounds.
- Private outbound-only behavior.
- Windows-native MVP and explicit WSL/Unix exclusions.
- Error/exit-code mapping.
- Troubleshooting `TERAX_UNAVAILABLE`, `SOURCE_UNNAMED`, `TARGET_NOT_LIVE`, and `WRITE_FAILED`.

- [ ] **Step 2: Update existing architecture indexes and invariants**

Add the guide to `docs/README.md` and `TERAX.md` further-reading list. Correct PTY documentation from tab-to-PTY cardinality to terminal-leaf-to-PTY cardinality. Add terminal-control boundary to security model without claiming protection from hostile same-user processes.

- [ ] **Step 3: Run frontend quality gates**

Run: `pnpm test`

Expected: all Vitest tests pass.

Run: `pnpm check-types`

Expected: exit 0.

Run: `pnpm lint`

Expected: exit 0.

Run: `pnpm format:check`

Expected: exit 0.

- [ ] **Step 4: Run Rust quality gates**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Expected: exit 0.

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: exit 0.

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all Rust tests pass.

- [ ] **Step 5: Run production builds**

Run: `pnpm build`

Expected: TypeScript and Vite production build succeed.

Run on Windows: `pnpm build:teraxctl-sidecar`

Expected: release `teraxctl-<target-triple>.exe` is staged under `src-tauri/binaries`.

Run: `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`

Expected: exit 0 across main library and CLI targets.

- [ ] **Step 6: Verify scope and sensitive-output rules**

Run:

```powershell
git diff --check
git status --short
```

Expected: no whitespace errors; no generated sidecar binary staged; `docs/architecture/terax-architecture-report.md` remains outside task commits unless separately authorized.

Inspect tests/log assertions and confirm raw token, digest, and message payload never enter logs or JSON diagnostics.

- [ ] **Step 7: Commit docs and final verification updates**

```bash
git add docs/architecture/terminal-agent-messaging.md docs/README.md docs/architecture/pty-shell-integration.md docs/architecture/security-model.md TERAX.md
git commit -m "docs: document terminal messaging"
```

## Final acceptance checklist

- [ ] Stable pane UUID survives serialization, hydration, restart, split, hide, and renderer rebind.
- [ ] Address names remain unique across every saved space; duplicate runtime claim never steals ownership.
- [ ] `teraxctl.exe` runs only from Windows-native Terax pane credentials in MVP.
- [ ] `list` returns lexicographically sorted live public named targets.
- [ ] Private pane can send outbound but cannot be listed or targeted.
- [ ] Receiver gets exact `[terax from <source>] <message>\r` bytes.
- [ ] Success occurs only after backend writer completion.
- [ ] Unavailable/closing/exited target fails immediately without durable/offline queue or retry.
- [ ] Message controls/newlines and oversize frames/messages are rejected before PTY write.
- [ ] Named pipe rejects remote clients and uses protected current-user SID DACL.
- [ ] Credentials are random, hash-only in memory, constant-time authenticated, and revoked on close.
- [ ] Logs omit token, digest, and message payload.
- [ ] WSL and non-Windows builds do not receive CLI env or Windows sidecar config.
- [ ] Frontend, Rust, CLI, packaging, and manual two-pane gates pass.
