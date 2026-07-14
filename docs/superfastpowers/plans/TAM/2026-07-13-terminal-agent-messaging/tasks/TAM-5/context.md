# Context for TAM-5

**Plan:** `docs/superfastpowers/plans/TAM/2026-07-13-terminal-agent-messaging.md`
**Task:** `TAM-5`
**Commit SHA:** `58bdde1` (`feat(control): sync terminal catalog`)
**Implementation range:** `de07437..58bdde1`

## Starting Context

- `src/modules/terminal/lib/terminalControl.ts`: TAM-5 planned creation target.
- `src/modules/terminal/lib/terminalControl.test.ts`: TAM-5 planned creation target.
- `src/modules/terminal/lib/useTerminalControlBridge.ts`: TAM-5 planned creation target.
- `src/modules/terminal/lib/terminalIdentity.ts`: TAM-5 existing integration point.
- `src/modules/spaces/lib/useSpacePersistence.ts`: TAM-5 existing integration point.
- `src/app/App.tsx`: TAM-5 existing integration point.

## Open Context Rule

The files above are starting points only. Inspect any additional files needed to complete the task correctly.

## Completion Updates

### Files created

- `src/modules/terminal/lib/terminalControl.ts`
- `src/modules/terminal/lib/terminalControl.test.ts`
- `src/modules/terminal/lib/useTerminalControlBridge.ts`

### Files modified

- `src/modules/terminal/lib/terminalIdentity.ts`
- `src/modules/spaces/lib/useSpacePersistence.ts`
- `src/app/App.tsx`

### Additional files inspected

- `AGENTS.md`, `TERAX.md`, and `package.json`
- TAM plan, progression policy, TAM-1 context, TAM-4 context, TAM-5 task, and phase-worker prompts
- `docs/superfastpowers/specs/2026-07-13-terminal-agent-messaging-design.md`
- `src/modules/terminal/lib/panes.ts`
- `src/modules/terminal/lib/terminalIdentity.test.ts`
- `src/modules/tabs/lib/useTabs.ts`
- `src/modules/spaces/lib/useSpacesBoot.ts`
- `src/modules/spaces/lib/store.ts`
- `src/modules/ai/lib/native.ts`
- `src-tauri/src/modules/terminal_control/service.rs`
- `src-tauri/src/modules/terminal_control/mod.rs`

### TDD evidence

All Node commands used Node `v22.15.0` by prepending
`C:\Users\Zacha\AppData\Local\nvm\v22.15.0` to `PATH`.

- RED: `pnpm test -- src/modules/terminal/lib/terminalControl.test.ts` failed as expected with `Cannot find module './terminalControl'`; 1 test file failed before tests could load.
- GREEN: the same command passed 1 test file and 3 tests for canonical catalog projection, immutable UUID-based name updates, and typed missing-terminal failure.
- RED: the same command failed 3 new durability-ordering tests while the first 3 tests remained green because `persistAndSyncTerminalCatalog` and `persistTerminalName` did not exist.
- GREEN: the same command passed 1 test file and 6 tests after implementing the persistence barriers and acknowledgement flow.

### Final verification

- `pnpm test -- src/modules/terminal/lib/terminalControl.test.ts src/modules/spaces/lib/serialize.test.ts`: PASS, 2 test files and 15 tests.
- `pnpm check-types`: PASS, exit 0.
- `pnpm test`: PASS, 48 test files and 367 tests.
- `pnpm lint`: PASS, exit 0 with the existing repository baseline of 105 warnings and 1 info diagnostic.
- `pnpm exec biome lint src/modules/terminal/lib/useTerminalControlBridge.ts src/modules/terminal/lib/terminalControl.ts src/modules/terminal/lib/terminalControl.test.ts src/modules/terminal/lib/terminalIdentity.ts src/modules/spaces/lib/useSpacePersistence.ts`: PASS, no diagnostics.
- `pnpm exec biome format src/app/App.tsx src/modules/spaces/lib/useSpacePersistence.ts src/modules/terminal/lib/terminalIdentity.ts src/modules/terminal/lib/terminalControl.ts src/modules/terminal/lib/terminalControl.test.ts src/modules/terminal/lib/useTerminalControlBridge.ts`: PASS, 6 files checked with no fixes required.
- `git diff --cached --check`: PASS before the implementation commit.

### Notes

- Catalog records are sorted by stable terminal UUID and inherit the terminal tab's private flag for every leaf across every saved space.
- Missing UUID name events fail with typed `TERMINAL_NOT_FOUND` data instead of silently persisting no change.
- Space persistence now returns an awaitable flush, reuses an identical pending write, serializes superseding writes per space, and leaves failed snapshots retryable.
- Catalog synchronization is gated on both spaces hydration and migration eligibility. The first catalog is persisted before Rust can hydrate, successful canonical strings are deduplicated, and sync failures emit only a sanitized diagnostic.
- Name events are serialized, read current tab and active-space refs, update the ref before React replacement, persist the new durable name, and acknowledge only after persistence completes.
- `docs/architecture/terax-architecture-report.md` remained untouched and untracked.
