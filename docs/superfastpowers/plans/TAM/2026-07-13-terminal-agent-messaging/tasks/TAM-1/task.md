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
