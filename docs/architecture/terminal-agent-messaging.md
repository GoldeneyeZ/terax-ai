# Terminal agent messaging

This guide describes the implemented Windows terminal-control path. It lets a process running inside a Terax-managed, Windows-native terminal name its pane, discover live public targets, and submit one bounded line of input to another pane.

The public entry point is `teraxctl.exe`. Rust owns authentication, name resolution, lifecycle checks, and PTY writes. The frontend owns durable pane-tree persistence.

## Identity model

Terminal messaging keeps runtime and persisted identities separate:

| Identity | Lifetime | Purpose |
| --- | --- | --- |
| Numeric leaf `id` | Frontend tree instance | Pane-tree operations; it may change during hydration. |
| `terminalId` | Persisted UUID | Stable terminal-leaf identity across hydration, restart, and space restore. |
| PTY ID | One backend PTY session | Runtime lookup for reads, writes, resize, and close; a respawn gets a new ID. |
| Renderer slot ID | Renderer-pool allocation | Runtime xterm/WebGL reuse; it never participates in messaging. |
| `addressName` | Persisted until renamed | Optional app-wide human address owned by one `terminalId`. |

Every serialized terminal leaf carries its `terminalId`. Legacy leaves receive a UUID during hydration and the migrated state is saved before the control service becomes ready. At startup, the frontend sends Rust the catalog from every saved space, including inactive panes.

Names are lowercased ASCII matching `^[a-z][a-z0-9-]{0,62}$`. They are unique across the complete persisted catalog, not just the active space. Duplicate names in corrupt persisted data are all marked conflicted and unaddressable; Terax never chooses a winner or silently adds a suffix.

## CLI contract

The sidecar is added to the `PATH` of each eligible native Windows pane. It reads the instance endpoint and source capability from `TERAX_IPC_ENDPOINT` and `TERAX_IPC_TOKEN`; callers cannot supply a sender identity.

```text
teraxctl name <name> [--json]
teraxctl list [--json]
teraxctl send <target> <message> [--json]
```

`--json` must be the final argument. `send` accepts exactly one message argument, so shell quoting is required for spaces:

```powershell
teraxctl name agent-a
teraxctl list
teraxctl send agent-b "review the current diff"
teraxctl send agent-b "review the current diff" --json
```

Human-readable output is the default. JSON mode emits the protocol response object with `version`, `requestId`, `ok`, and either `data` or `error`. It never includes the source token. `list` is sorted and includes only live, public, named, non-conflicted targets.

## Naming transaction

Rust and the frontend coordinate a rename as a reservation transaction:

1. Rust authenticates the source, validates the requested name, and acquires the app-wide directory lock.
2. Renaming to the current committed name succeeds idempotently.
3. Rust reserves a free name while retaining the source's old committed name.
4. Rust emits a persistence request containing the request ID, `terminalId`, old name, and new name.
5. The frontend updates the matching terminal leaf and awaits `saveState`.
6. The frontend acknowledges success or failure with the same request ID.
7. Rust commits the reserved name only after a success acknowledgement.

An explicit persistence failure or the five-second acknowledgement timeout releases the reservation and retains the old name. If persistence succeeds but acknowledgement is lost during a process failure, the persisted catalog reconciles the name on the next startup.

## Protocol and delivery

Protocol v1 uses one request and one response per named-pipe connection. Each frame is a four-byte little-endian payload length followed by UTF-8 JSON. Frames are limited to 64 KiB.

Requests are tagged by `operation` (`name`, `list`, or `send`) and contain `version`, `requestId`, `sourceToken`, and an operation-specific `payload`. Responses echo the request ID and use typed error codes.

`send` validates a message as UTF-8 bytes with a maximum size of 16 KiB. Tabs are allowed. Newlines, carriage returns, terminal escapes, C0 controls other than tab, DEL, and C1 controls are rejected. A valid message becomes exactly:

```text
[terax from <source-name>] <message>\r
```

The final carriage return submits one line in PowerShell. A per-target mutex prevents concurrent messages from interleaving. Success means the backend PTY writer completed; it does not mean the receiving shell or process acted on the line. There is no offline queue, retry, delivery history, receiver acknowledgement, broadcast, or output capture.

Close first marks the target closing and revokes its source capability. New and queued sends then fail with `TARGET_NOT_LIVE`; a send already holding the target writer may finish through normal PTY teardown. No send reports success after a writer failure.

## Privacy and platform scope

A private pane may own a name and send outbound. Its chosen receiver sees that source name in the envelope. Private panes are excluded from `list` and inbound resolution, and lookup returns `TARGET_NOT_FOUND` just as it does for an unknown name.

The MVP is available only to Windows-native Terax terminals. WSL panes receive no terminal-control endpoint, token, or sidecar path. Unix sockets, WSL shims, TCP listeners, and macOS/Linux transport adapters are not implemented.

## Security and bounds

Each app instance creates a pipe named like `\\.\pipe\terax-control-<pid>-<nonce>`, where the nonce is 16 random bytes encoded as hex. The pipe:

- grants access only to the current Windows user SID;
- rejects remote clients;
- caps simultaneous connections at 32;
- rejects oversized frames before allocation or write.

Each eligible PTY receives a random 32-byte, URL-safe capability token through its environment. Rust stores only the SHA-256 digest, compares candidate digests through a constant-time path, derives source identity from the matching entry, and revokes the entry when the PTY closes. Tokens are never persisted and cannot be passed as CLI flags.

Sends use a per-source token bucket with a burst of 40 and a refill rate of 20 per second. Capacity and rate failures are immediate and never create an offline queue.

These controls prevent accidental cross-pane identity claims, unbound callers, remote clients, and predictable cross-instance attachment. They are not isolation from malware, debuggers, or hostile processes already running as the same OS user, which may be able to inspect another process or inherited environment.

## Errors and process exits

| Exit | Protocol codes |
| ---: | --- |
| 0 | Success |
| 1 | `INTERNAL` or an unclassified local failure |
| 2 | CLI usage, `INVALID_REQUEST`, `UNSUPPORTED_VERSION`, `INVALID_NAME`, `MESSAGE_INVALID`, `MESSAGE_TOO_LARGE` |
| 3 | `TERAX_UNAVAILABLE`, `AUTH_FAILED`, `SOURCE_UNNAMED` |
| 4 | `NAME_IN_USE`, `PERSIST_FAILED`, `PERSIST_TIMEOUT` |
| 5 | `TARGET_NOT_FOUND`, `TARGET_NOT_LIVE` |
| 6 | `RATE_LIMITED`, `SERVER_BUSY` |
| 7 | `WRITE_FAILED` |

Errors and diagnostics may include operation, request ID, source `terminalId`, target `terminalId`, typed code, and elapsed time. Raw tokens, token digests, and message payloads must not be logged or added to JSON diagnostics.

## Troubleshooting

### `TERAX_UNAVAILABLE`

Confirm Terax is running and that the command is executing inside a current Windows-native Terax pane. This error is expected from WSL, Unix, an external shell, a stopped app, a stale endpoint, or a failed pipe connection. Open a fresh native pane after restarting Terax so it receives current environment credentials.

### `SOURCE_UNNAMED`

The authenticated source terminal has no committed name. Run `teraxctl name <name>` in that pane and wait for success before sending. A persistence failure or timeout leaves the old name unchanged.

### `TARGET_NOT_LIVE`

The name exists in the persisted catalog, but its terminal is inactive, closing, or exited. Open or restart that terminal leaf, then retry. Messages are not queued while it is offline.

### `WRITE_FAILED`

The target was resolved as live, but its PTY writer rejected the input or failed during teardown. Check whether the target shell exited or the pane closed. The failed message is not retried automatically.

## Implementation map

- Frontend catalog and persistence bridge: `src/modules/terminal/lib/terminalControl.ts` and `useTerminalControlBridge.ts`
- Persisted terminal identity: `src/modules/spaces/lib/serialize.ts`
- Rust directory, credentials, protocol, service, and limits: `src-tauri/src/modules/terminal_control/`
- PTY lifecycle binding: `src-tauri/src/modules/pty/mod.rs`
- CLI and sidecar: `src-tauri/src/bin/teraxctl.rs`, `src-tauri/src/modules/terminal_control/cli.rs`, and `scripts/prepare-teraxctl-sidecar.mjs`

## See also

- [`TERAX.md`](../../TERAX.md) - architecture source of truth
- [PTY shell integration](pty-shell-integration.md) - PTY session lifecycle and Windows input details
- [Security model](security-model.md) - Terax trust boundaries and security invariants
