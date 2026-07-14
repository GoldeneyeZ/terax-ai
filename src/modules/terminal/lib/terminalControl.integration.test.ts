import {
  hydrateTabs,
  hydrateTabsWithMigration,
  type SerializedTab,
  serializeTabs,
} from "@/modules/spaces/lib/serialize";
import { describe, expect, it } from "vitest";
import { applyPersistedName, collectTerminalCatalog } from "./terminalControl";

function counter(start = 1): () => number {
  let next = start;
  return () => next++;
}

function serializedTerminal(
  terminalId?: string,
  addressName?: string,
): SerializedTab {
  return {
    kind: "terminal",
    tree: {
      kind: "leaf",
      cwd: "C:\\repo",
      active: true,
      ...(terminalId !== undefined && { terminalId }),
      ...(addressName !== undefined && { addressName }),
    },
  };
}

describe("terminal control restart integration", () => {
  it("preserves migrated identity, persisted name, and privacy through restart", () => {
    const terminalId = "00000000-0000-4000-8000-000000000701";
    const migrated = hydrateTabsWithMigration(
      [serializedTerminal()],
      "space-a",
      counter(),
      () => terminalId,
    );
    expect(migrated.migrated).toBe(true);

    const [persistedIdentity] = serializeTabs(migrated.tabs);
    expect(persistedIdentity).toMatchObject({
      kind: "terminal",
      tree: { terminalId },
    });

    const named = applyPersistedName(migrated.tabs, {
      requestId: "persist-name-1",
      terminalId,
      newName: "agent-a",
    });
    const serialized = serializeTabs(named);
    const restarted = hydrateTabs(serialized, "space-a", counter(100));

    expect(collectTerminalCatalog(restarted)).toEqual([
      {
        terminalId,
        addressName: "agent-a",
        private: false,
      },
    ]);
  });

  it("keeps duplicate persisted names conflicted instead of suffixing them", () => {
    const tabs = hydrateTabs(
      [
        serializedTerminal("00000000-0000-4000-8000-000000000711", "duplicate"),
        serializedTerminal("00000000-0000-4000-8000-000000000712", "duplicate"),
      ],
      "space-a",
      counter(),
    );

    expect(collectTerminalCatalog(tabs)).toEqual([
      {
        terminalId: "00000000-0000-4000-8000-000000000711",
        addressName: "duplicate",
        private: false,
      },
      {
        terminalId: "00000000-0000-4000-8000-000000000712",
        addressName: "duplicate",
        private: false,
      },
    ]);
  });
});
