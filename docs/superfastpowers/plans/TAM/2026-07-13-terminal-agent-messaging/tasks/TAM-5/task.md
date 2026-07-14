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
