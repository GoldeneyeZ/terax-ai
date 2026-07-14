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
