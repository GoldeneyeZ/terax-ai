# Context for TAM-9

**Plan:** `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
**Task:** `TAM-9`
**Commit SHA:** `c1ea5ad`

## Starting Context

- `docs/architecture/terminal-agent-messaging.md`: TAM-9 planned creation target.
- `docs/README.md`: TAM-9 existing integration point.
- `docs/architecture/pty-shell-integration.md`: TAM-9 existing integration point.
- `docs/architecture/security-model.md`: TAM-9 existing integration point.
- `TERAX.md`: TAM-9 existing integration point.

## Open Context Rule

The files above are starting points only. Inspect any additional files needed to complete the task correctly.

## Completion Updates

- **Reviewed implementation range:** `851573a..c1ea5ad`
- **Created:** `docs/architecture/terminal-agent-messaging.md`.
- **Modified:** `docs/README.md`, `docs/architecture/pty-shell-integration.md`, `docs/architecture/security-model.md`, `TERAX.md`.
- **Additional gate fixes:** `eb334db` excludes the `node:test` sidecar suite from Vitest discovery while retaining its dedicated runner; `5ab985f` removes the Windows-only unused settings-window warning without changing Linux/macOS handle use.
- **Documentation verification:** all required identity, CLI, transaction, framing, envelope, ACL, nonce, credential, bound, privacy, platform, exit-code, and troubleshooting topics are present. Relative links resolve, no forbidden dash characters were introduced, PTY cardinality is terminal-leaf-to-PTY, and the security guide explicitly disclaims hostile same-user isolation.
- **Frontend gates:** `pnpm test` passed 50 files/371 tests after the runner-boundary fix; `node --test scripts/prepare-teraxctl-sidecar.test.mjs` passed 4/4; `pnpm check-types` passed; `pnpm lint` exited 0 with the existing 104 warnings; targeted TAM-8 Biome checks passed. Repo-wide `pnpm format:check` remains blocked by 345 pre-existing working-tree formatting/line-ending differences across 369 files.
- **Rust gates:** strict locked all-target Clippy passed after the settings-window fix; locked all-target Cargo check passed; all Rust targets passed with the symlink test skipped in the non-admin process (205/205 library tests plus every integration/doc target), and the user separately confirmed `authorize_spawn_cwd_blocks_symlink_escape` passes in Administrator PowerShell. Targeted rustfmt passed; repo-wide `cargo fmt -- --check` remains blocked by pre-existing formatting drift outside TAM files.
- **Production builds:** `pnpm build` passed after transforming 6,551 modules; `pnpm build:teraxctl-sidecar` produced and staged the ignored 259,072-byte release `teraxctl-x86_64-pc-windows-msvc.exe`; release help listed `name`, `list`, and `send`; the artifact remained ignored and unstaged.
- **Sensitive-output audit:** service logs contain request ID, operation, source/target terminal IDs, typed error code, and duration only. CLI JSON/error tests and source inspection confirm raw token, token digest, and message payload are absent from logs and JSON diagnostics.
- **Scope:** `git diff --check` passed. The user-owned untracked `docs/architecture/terax-architecture-report.md` remained untouched and outside all commits.
