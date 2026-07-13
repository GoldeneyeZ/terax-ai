# Terminal Agent Messaging Design

- Date: 2026-07-13
- Status: approved design
- Baseline: `460657a` (`main`)
- Source analysis: [`docs/architecture/terax-architecture-report.md`](../../architecture/terax-architecture-report.md)

## 1. Purpose

Terax agents running in separate terminal panes need a small local messaging mechanism. A process inside one Terax-managed Windows terminal must be able to name its pane, discover other public named panes, and submit one text message to another pane through a bundled CLI.

Delivery means the Terax backend completed the target PTY write. It does not mean the receiving shell or agent parsed, accepted, or acted on the message.

## 2. Approved decisions

| Concern | Decision |
| --- | --- |
| Caller scope | Processes inside Terax-managed panes only |
| CLI binary | `teraxctl.exe` |
| MVP commands | `name`, `list`, `send` |
| Target address | App-wide unique pane name |
| Name persistence | Persist with pane tree across restart and space restore |
| Default delivery | UTF-8 envelope followed by PTY Enter (`\r`) |
| Success | Backend PTY write completed |
| Private panes | Hidden and untargetable; outbound send allowed |
| Sender identity | Derived from source-pane credential; never caller-supplied |
| Receiver format | `[terax from <source>] <message>\r` |
| Duplicate names | Reject; existing owner keeps name |
| Unavailable target | Immediate typed failure; no queue or retry |
| Transport | Rust-owned Windows named pipe |
| Platform scope | Windows-native shells only for MVP; WSL and Unix deferred |

## 3. Goals

- Provide stable pane identity independent of renderer slots, PTY sessions, tabs, and current numeric leaf IDs.
- Provide app-wide unique, human-readable pane names.
- Let authenticated pane processes use `teraxctl.exe` without caller-supplied identity.
- Keep terminal lookup, authorization, lifecycle checks, and PTY writes inside Rust.
- Await the actual backend writer result.
- Preserve private-pane target confidentiality.
- Keep protocol versioned, bounded, testable, and transport-independent above the Windows IPC adapter.

## 4. Non-goals

- Message queues, retries, delivery history, broadcasts, fan-out, or offline delivery.
- Receiver processing acknowledgements.
- Terminal-output capture or inferred replies.
- Remote, network, loopback HTTP, or cross-user access.
- Arbitrary external callers outside Terax-managed panes.
- WSL, Linux, or macOS CLI support in MVP.
- Plugin APIs or general automation APIs.
- Multi-line input, binary payloads, shell command execution, or raw key-sequence injection.

## 5. Identity model

Four identities remain distinct:

- `PaneNode.id`: existing frontend-local numeric ID used by pane-tree UI logic. It may change after hydration.
- `terminalId`: new persisted UUID attached to each terminal leaf. It is stable across hydration, restart, and space restore.
- PTY session ID: runtime backend identity for one spawned PTY. It changes when a pane starts a new PTY.
- `addressName`: optional persisted human-readable address owned by one `terminalId` app-wide.

Renderer slot identity never participates in messaging.

`addressName` uses this canonical form:

```text
^[a-z][a-z0-9-]{0,62}$
```

Input is ASCII-lowercased before validation. Names are unique across every saved space, including inactive panes. Inactive names remain reserved but are absent from `list` and cannot receive messages.

Unnamed panes may run normally. They cannot send because the receiver envelope requires a stable source name. A private pane may own a name and send outbound; doing so intentionally reveals that name to its chosen receiver. The name remains unavailable through discovery or inbound lookup.

## 6. Persisted model and migration

Terminal leaf state gains two optional-compatible fields:

```ts
type TerminalLeaf = {
  id: number;
  terminalId: string;
  addressName?: string;
  cwd?: string;
};
```

Serialized terminal leaves persist `terminalId` and `addressName`. Existing saved leaves without `terminalId` receive a UUID during hydration. All migrated space states are saved before the control server becomes ready. If migration persistence fails, normal terminal use may continue, but control IPC remains unavailable and the app surfaces a diagnostic; unstable identities must never enter the directory.

At boot, the frontend loads all saved spaces, migrates them, and sends the complete persisted terminal catalog to Rust. Rust validates UUIDs and names before accepting CLI traffic.

Corrupt saved data containing duplicate names does not choose a winner. Every colliding record is marked conflicted and unaddressable, and the frontend surfaces a repair warning. Normal runtime name claims remain first-owner-wins and never remove an existing owner.

## 7. Rust architecture

Rust owns runtime messaging and OS access through five components:

### `TerminalDirectory`

Stores persisted identity and runtime status:

```text
TerminalRecord {
  terminal_id
  address_name?
  private
  state: persisted | live | closing | exited | conflicted
  pty_session_id?
}
```

It maintains atomic indexes from `terminalId` and canonical `addressName`. The name index includes inactive saved panes so uniqueness is app-wide. Target resolution returns only live, non-private, non-conflicted records.

### `PtyTargetRegistry`

Maps live `terminalId` values to PTY session writers. It integrates with existing `pty_open`, `pty_write`, and PTY teardown paths. Each target owns a write mutex so concurrent messages cannot interleave at byte level.

### `PaneCredentialRegistry`

Maps hashed per-PTY capability tokens to source `terminalId` values. Tokens expire when their PTY closes or the app exits. Raw tokens never enter logs or persistent storage.

### `LocalControlServer`

Accepts versioned requests over a Windows named pipe. The server starts only after directory hydration succeeds and stops accepting requests before app shutdown teardown.

### `ControlProtocol`

Defines transport-neutral request, response, error, validation, and operation types. Windows named-pipe code is an adapter around this protocol. Future Unix-domain-socket support may implement the same adapter interface without changing operations.

## 8. Frontend responsibilities

Frontend remains authority for saved pane-tree state. It:

- Creates and migrates `terminalId` values.
- Serializes `terminalId` and `addressName` with each terminal leaf.
- Hydrates Rust with the complete terminal catalog before IPC readiness.
- Passes `terminalId`, `addressName`, and privacy status into PTY open.
- Applies backend-originated name persistence requests idempotently.
- Awaits `saveState` before acknowledging a name change.
- Displays duplicate-catalog, migration, and persistence failures.

Frontend/webview never brokers `list` or `send`. Message delivery continues if the pane is hidden or lacks a renderer slot because Rust targets the PTY writer directly.

Any new frontend-to-Rust commands must be registered in `src-tauri/src/lib.rs` and explicitly granted through the relevant Tauri capability file.

## 9. PTY open and credential injection

PTY open receives stable identity metadata before child spawn:

```text
terminalId
addressName?
private
```

Rust performs this sequence:

1. Validate `terminalId` against the hydrated directory.
2. Generate a cryptographically random 256-bit pane token.
3. Add endpoint and raw token to the Windows-native child environment before spawn.
4. Create the PTY session using that environment.
5. Bind `terminalId` to its session writer.
6. Store only the token hash in `PaneCredentialRegistry`.
7. Mark the record `live` only after spawn and binding succeed; discard the token on any earlier failure.

Injected variables:

```text
TERAX_PANE_ID=<terminalId>
TERAX_IPC_ENDPOINT=<instance-specific named-pipe endpoint>
TERAX_IPC_TOKEN=<raw per-PTY token>
```

The Terax bundle directory is prepended to `PATH` for Windows-native child shells so `teraxctl.exe` resolves normally. MVP does not inject a WSL shim or promise access from WSL sessions.

Child processes inherit the pane credential. This is intentional: every descendant inside that PTY acts with the source pane's messaging authority.

## 10. IPC transport and protocol

The Windows server uses an instance-specific named pipe under `\\.\pipe\` with a random nonce. Its ACL grants access only to the current user SID. It never binds TCP or exposes a remote pipe.

Frames use a four-byte little-endian unsigned length followed by UTF-8 JSON. The maximum frame length is 64 KiB and is checked before allocation or JSON parsing.

Protocol version 1 request:

```json
{
  "version": 1,
  "requestId": "uuid",
  "operation": "name | list | send",
  "sourceToken": "base64url-token",
  "payload": {}
}
```

Success response:

```json
{
  "version": 1,
  "requestId": "uuid",
  "ok": true,
  "data": {}
}
```

Failure response:

```json
{
  "version": 1,
  "requestId": "uuid",
  "ok": false,
  "error": {
    "code": "TARGET_NOT_LIVE",
    "message": "Target is not live"
  }
}
```

Authentication runs before operation-specific lookup. Token comparison is constant-time. Unknown, private, and conflicted targets do not expose private record details.

Server bounds:

- Maximum 32 simultaneous pipe connections.
- Maximum one active request per connection.
- Per-source token bucket: 20 sends/second, burst 40.
- Requests rejected immediately when capacity or rate limits are exceeded.
- No durable or target-availability queue exists. Per-target writer-lock contention may wait only within the bounded lifetime of an active request.

## 11. CLI contract

`teraxctl.exe` is a separate bundled binary because the desktop executable already owns the `terax` name.

```text
teraxctl name <name> [--json]
teraxctl list [--json]
teraxctl send <target> <message> [--json]
```

`<message>` is one shell argument. Callers quote spaces using their shell's normal syntax.

### `name`

- Authenticates the current source pane.
- Canonicalizes and validates the requested name.
- Renaming to the source pane's existing name succeeds idempotently.
- Reserving another pane's name returns `NAME_IN_USE` without changing either record.
- Success is returned only after frontend persistence acknowledgement.

### `list`

- Authenticates the current source pane.
- Returns live, named, non-private, non-conflicted targets only.
- Sorts names lexicographically for deterministic output.
- May include the caller if the caller is public and named.

### `send`

- Authenticates and resolves the source pane from its token.
- Requires the source pane to have a name.
- Resolves a live public target by canonical name.
- Validates the message.
- Builds one envelope.
- Awaits target PTY writer completion.
- Returns success only after that writer completes successfully.

Human-readable output is default. `--json` emits the stable protocol result shape without the source token. Tokens and message payloads never appear in diagnostics.

Stable process exit codes:

| Exit | Meaning |
| ---: | --- |
| 0 | Success |
| 1 | Internal/unclassified failure |
| 2 | CLI usage, invalid request, invalid name, or invalid message |
| 3 | Terax unavailable or authentication failure |
| 4 | Name conflict or persistence failure |
| 5 | Target missing or not live |
| 6 | Rate limited or server busy |
| 7 | PTY write failure |

## 12. Message validation and envelope

Message constraints:

- Valid UTF-8.
- Maximum 16 KiB after UTF-8 encoding.
- One logical line.
- Reject CR, LF, NUL, ESC, DEL, and other C0/C1 control characters except horizontal tab.
- No implicit shell escaping, interpolation, JSON encoding, or command parsing.

The backend produces exactly:

```text
[terax from <canonical-source-name>] <message>\r
```

The trailing carriage return models Enter for PTY input. Because controls and newlines are rejected, one request creates one submitted input line.

## 13. Name transaction

Name changes cross Rust runtime state and frontend-owned persistence. They use an idempotent request ID:

1. Authenticate source and validate name.
2. Acquire the directory name lock.
3. Return success immediately if the same pane already owns that name.
4. Reserve the name without removing the source's current committed name.
5. Emit a frontend persistence request containing request ID, `terminalId`, old name, and new name.
6. Frontend updates the matching pane leaf and awaits `saveState`.
7. Frontend invokes commit or failure acknowledgement using the same request ID.
8. On commit, Rust atomically swaps the committed name and releases the reservation.
9. On explicit failure or five-second timeout, Rust releases the reservation and retains the old name.

Frontend processing is idempotent. If persistence completes but acknowledgement is lost during process failure, the next boot hydrates the persisted value. A retry of the same effective rename succeeds idempotently.

## 14. Send data flow

1. `teraxctl.exe` reads `TERAX_IPC_ENDPOINT` and `TERAX_IPC_TOKEN`.
2. Missing variables return `AUTH_FAILED` locally without probing default endpoints.
3. CLI connects to the instance pipe and sends one versioned request.
4. Server authenticates token and derives source `terminalId`.
5. Directory requires a committed source name.
6. Directory resolves target name without exposing private/conflicted records.
7. Registry requires target state `live` and acquires its writer.
8. Protocol layer validates and constructs envelope bytes.
9. Per-target write mutex serializes the write with other messaging writes.
10. Backend awaits PTY writer completion.
11. Server returns success or a typed error.
12. CLI prints human or JSON output and exits with its stable code.

Success covers steps through writer completion only. Receiver processing remains outside the contract.

## 15. Close and shutdown lifecycle

PTY close proceeds in this order:

1. Mark directory record `closing`.
2. Reject new sends immediately.
3. Revoke the source credential.
4. Wait for any write already holding the per-target writer lock to finish or fail through normal PTY teardown.
5. Remove the writer binding.
6. Close the PTY session.
7. Mark the record `exited` while retaining persisted identity/name.

App shutdown stops accepting new IPC connections before PTY teardown, revokes all credentials, and removes the named-pipe endpoint.

The send/close race has only two valid outcomes: completed PTY write or typed failure. Stale writer access and reported-success-after-write-failure are invalid.

## 16. Error model

Protocol errors:

```text
TERAX_UNAVAILABLE
INVALID_REQUEST
UNSUPPORTED_VERSION
AUTH_FAILED
SOURCE_UNNAMED
INVALID_NAME
NAME_IN_USE
TARGET_NOT_FOUND
TARGET_NOT_LIVE
MESSAGE_INVALID
MESSAGE_TOO_LARGE
PERSIST_FAILED
PERSIST_TIMEOUT
RATE_LIMITED
SERVER_BUSY
WRITE_FAILED
INTERNAL
```

Rules:

- Private and conflicted target lookup returns `TARGET_NOT_FOUND`.
- Persisted but inactive public target returns `TARGET_NOT_LIVE`.
- Backend writer error returns `WRITE_FAILED`; it is never converted to success.
- Internal messages are sanitized for CLI output.
- Logs may include request ID, operation, source `terminalId`, target `terminalId`, error code, and timing.
- Logs must not include raw token, token hash, or message payload.

## 17. Security model

Security controls:

- Rust owns all OS IPC and PTY access.
- Named-pipe ACL restricts connections to current user SID.
- Random endpoint nonce prevents predictable cross-instance attachment.
- Per-PTY 256-bit token proves source-pane membership.
- Only token hashes remain in backend state.
- Tokens expire with PTY lifetime and are never persistent.
- Caller cannot supply or override sender identity.
- Private targets are excluded from discovery and indistinguishable from unknown targets.
- Payload/frame bounds limit memory and terminal-input abuse.
- Control-character rejection prevents raw key-sequence injection through this API.
- No remote listener exists.

This mechanism is not a security boundary against malware, debuggers, or hostile processes already running as the same OS user. Such processes may be able to inspect another process or inherited environment through OS-level privileges. The design prevents unbound callers and accidental cross-pane identity claims; it does not claim same-user hostile-process isolation.

## 18. Testing strategy

### Frontend unit tests

- Serialize and hydrate `terminalId` and `addressName`.
- Migrate legacy leaves exactly once and persist generated UUIDs.
- Detect duplicate catalog names without silently selecting an owner.
- Apply name persistence request idempotently.
- Report save failure and timeout acknowledgement paths.

### Rust unit tests

- Name canonicalization, syntax, uniqueness, reservation, commit, and rollback.
- Stable directory state transitions.
- Target privacy/conflict masking.
- Credential authentication, constant-time comparison path, and revocation.
- Frame truncation, oversize, invalid UTF-8/JSON, and unsupported version.
- Message control-character and size validation.
- Exact envelope bytes, including final `\r`.
- Error-to-exit-code mapping and JSON response schema.

### Rust integration tests

- Mock writer success returns only after completion.
- Mock writer error returns `WRITE_FAILED`.
- Concurrent identical name claims produce exactly one owner.
- Concurrent sends to one target never interleave bytes.
- Send/close races produce write completion or typed failure.
- Expired, forged, and cross-instance tokens fail.
- Private pane can send outbound but cannot be listed or targeted.
- Capacity and rate limits reject without hidden delivery queue.

### Windows end-to-end tests

1. Launch Terax with two public panes.
2. Name them `agent-a` and `agent-b` through `teraxctl.exe`.
3. Confirm `list` returns deterministic names.
4. Send from A to B.
5. Confirm B receives exact `[terax from agent-a] <message>\r`.
6. Confirm a private pane is hidden and untargetable but can send outbound.
7. Restart Terax and confirm names restore.
8. Close B and confirm immediate `TARGET_NOT_LIVE`.
9. Stop Terax and confirm `TERAX_UNAVAILABLE`.

Existing PTY open/write/resize/close, split-pane, renderer-pool, private-terminal, space serialization, and app-shutdown tests remain green.

## 19. Acceptance criteria

- Two named Windows-native Terax panes exchange a submitted plain-text envelope through `teraxctl.exe`.
- Source identity is derived only from pane credential.
- Names remain stable across restart and are unique across all saved spaces.
- `list` exposes only live public named targets.
- Private panes can send outbound but cannot be discovered or targeted.
- Send success is returned only after backend PTY writer completion.
- Write errors, close races, unavailable targets, and stopped app produce stable typed failures.
- Messages cannot inject additional lines or raw terminal control sequences.
- No queues, retries, history, broadcast, output capture, TCP listener, WSL shim, or Unix transport are introduced.

## 20. Deferred extensions

Deferred work requires separate design approval:

- WSL shim invoking the Windows CLI through interop.
- Unix-domain-socket adapters and native Linux/macOS CLI packaging.
- Multi-line delivery using explicit bracketed-paste semantics.
- Explicit receiver acknowledgements.
- Broadcast, history, queueing, or general plugin/control APIs.
