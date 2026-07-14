import type { Tab } from "@/modules/tabs";
import { describe, expect, it } from "vitest";
import {
  applyPersistedName,
  collectTerminalCatalog,
  persistAndSyncTerminalCatalog,
  persistTerminalName,
  TerminalControlError,
} from "./terminalControl";

const FIRST_TERMINAL_ID = "00000000-0000-4000-8000-000000000001";
const SECOND_TERMINAL_ID = "00000000-0000-4000-8000-000000000002";

const tabs: Tab[] = [
  {
    id: 1,
    kind: "terminal",
    title: "Public",
    spaceId: "space-a",
    activeLeafId: 1,
    paneTree: {
      kind: "leaf",
      id: 1,
      terminalId: FIRST_TERMINAL_ID,
      addressName: "agent-a",
    },
  },
  {
    id: 2,
    kind: "terminal",
    title: "Private",
    spaceId: "space-b",
    activeLeafId: 2,
    private: true,
    paneTree: {
      kind: "split",
      id: 4,
      dir: "row",
      children: [
        {
          kind: "leaf",
          id: 2,
          terminalId: SECOND_TERMINAL_ID,
        },
        {
          kind: "leaf",
          id: 3,
          terminalId: "00000000-0000-4000-8000-000000000003",
          addressName: "agent-c",
        },
      ],
    },
  },
  {
    id: 3,
    kind: "editor",
    title: "Ignored",
    spaceId: "space-a",
    path: "ignored.ts",
    dirty: false,
    preview: false,
  },
];

function findTerminalLeaf(current: Tab[], terminalId: string) {
  const visit = (
    node: Extract<Tab, { kind: "terminal" }>["paneTree"],
  ): Extract<
    Extract<Tab, { kind: "terminal" }>["paneTree"],
    { kind: "leaf" }
  > | null => {
    if (node.kind === "leaf") {
      return node.terminalId === terminalId ? node : null;
    }
    for (const child of node.children) {
      const found = visit(child);
      if (found) return found;
    }
    return null;
  };

  for (const tab of current) {
    if (tab.kind !== "terminal") continue;
    const found = visit(tab.paneTree);
    if (found) return found;
  }
  return null;
}

describe("terminal control catalog", () => {
  it("projects every saved terminal leaf into one canonical catalog", () => {
    expect(collectTerminalCatalog(tabs)).toEqual([
      {
        terminalId: FIRST_TERMINAL_ID,
        addressName: "agent-a",
        private: false,
      },
      {
        terminalId: SECOND_TERMINAL_ID,
        addressName: undefined,
        private: true,
      },
      {
        terminalId: "00000000-0000-4000-8000-000000000003",
        addressName: "agent-c",
        private: true,
      },
    ]);
  });

  it("updates one address by terminal UUID", () => {
    const next = applyPersistedName(tabs, {
      requestId: "req-1",
      terminalId: SECOND_TERMINAL_ID,
      oldName: undefined,
      newName: "agent-b",
    });

    expect(findTerminalLeaf(next, SECOND_TERMINAL_ID)?.addressName).toBe(
      "agent-b",
    );
    expect(findTerminalLeaf(tabs, SECOND_TERMINAL_ID)?.addressName).toBe(
      undefined,
    );
  });

  it("returns a typed failure for a missing terminal UUID", () => {
    const terminalId = "00000000-0000-4000-8000-000000000099";
    const apply = () =>
      applyPersistedName(tabs, {
        requestId: "req-missing",
        terminalId,
        oldName: undefined,
        newName: "agent-z",
      });

    expect(apply).toThrowError(TerminalControlError);
    try {
      apply();
    } catch (error) {
      expect(error).toMatchObject({ code: "TERMINAL_NOT_FOUND", terminalId });
    }
  });

  it("makes the initial catalog durable before synchronizing it", async () => {
    const events: string[] = [];

    await persistAndSyncTerminalCatalog(
      tabs,
      1,
      "space-a",
      true,
      async () => {
        events.push("persist");
      },
      async (records) => {
        events.push(`sync:${records.length}`);
      },
    );

    expect(events).toEqual(["persist", "sync:3"]);
  });

  it("persists a name before acknowledging the backend request", async () => {
    const events: string[] = [];

    await persistTerminalName(
      tabs,
      {
        requestId: "req-2",
        terminalId: SECOND_TERMINAL_ID,
        oldName: undefined,
        newName: "agent-b",
      },
      2,
      "space-b",
      {
        replaceTabs: (next, activeId) => {
          expect(findTerminalLeaf(next, SECOND_TERMINAL_ID)?.addressName).toBe(
            "agent-b",
          );
          events.push(`replace:${activeId}`);
        },
        persistNow: async () => {
          events.push("persist");
        },
        ackName: async (requestId, error) => {
          events.push(`ack:${requestId}:${String(error)}`);
        },
      },
    );

    expect(events).toEqual(["replace:2", "persist", "ack:req-2:undefined"]);
  });

  it("acknowledges a persistence failure without reporting success", async () => {
    const acknowledgements: Array<[string, string | undefined]> = [];

    await persistTerminalName(
      tabs,
      {
        requestId: "req-failed",
        terminalId: SECOND_TERMINAL_ID,
        oldName: undefined,
        newName: "agent-b",
      },
      2,
      "space-b",
      {
        replaceTabs: () => {},
        persistNow: async () => {
          throw new Error("save failed");
        },
        ackName: async (requestId, error) => {
          acknowledgements.push([requestId, error]);
        },
      },
    );

    expect(acknowledgements).toEqual([["req-failed", "Error: save failed"]]);
  });
});
