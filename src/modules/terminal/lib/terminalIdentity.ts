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
    return node.terminalId === terminalId ? { ...node, addressName } : node;
  }
  return {
    ...node,
    children: node.children.map((child) =>
      withAddressName(child, terminalId, addressName),
    ),
  };
}
