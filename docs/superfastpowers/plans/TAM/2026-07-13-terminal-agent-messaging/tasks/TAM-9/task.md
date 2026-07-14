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
