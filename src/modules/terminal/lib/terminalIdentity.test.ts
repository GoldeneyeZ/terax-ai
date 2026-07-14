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
