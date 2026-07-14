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
  return updateAddressName(node, terminalId, addressName).node;
}

export function updateAddressName(
  node: PaneNode,
  terminalId: TerminalId,
  addressName: string | undefined,
): { node: PaneNode; updated: boolean } {
  if (node.kind === "leaf") {
    return node.terminalId === terminalId
      ? { node: { ...node, addressName }, updated: true }
      : { node, updated: false };
  }
  let updated = false;
  const children = node.children.map((child) => {
    const result = updateAddressName(child, terminalId, addressName);
    updated ||= result.updated;
    return result.node;
  });
  return {
    node: updated ? { ...node, children } : node,
    updated,
  };
}
