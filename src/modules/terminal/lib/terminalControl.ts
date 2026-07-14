import type { Tab } from "@/modules/tabs";
import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { PaneNode } from "./panes";
import { updateAddressName } from "./terminalIdentity";

export const PERSIST_NAME_EVENT = "terminal-control://persist-name";

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

export class TerminalControlError extends Error {
  readonly code = "TERMINAL_NOT_FOUND";

  constructor(readonly terminalId: string) {
    super(`Terminal not found: ${terminalId}`);
    this.name = "TerminalControlError";
  }
}

function terminalLeaves(node: PaneNode): Extract<PaneNode, { kind: "leaf" }>[] {
  return node.kind === "leaf" ? [node] : node.children.flatMap(terminalLeaves);
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

export function applyPersistedName(
  tabs: Tab[],
  request: PersistNameRequest,
): Tab[] {
  let updated = false;
  const next = tabs.map((tab) => {
    if (tab.kind !== "terminal") return tab;
    const result = updateAddressName(
      tab.paneTree,
      request.terminalId,
      request.newName,
    );
    updated ||= result.updated;
    return result.updated ? { ...tab, paneTree: result.node } : tab;
  });
  if (!updated) throw new TerminalControlError(request.terminalId);
  return next;
}

export type PersistNow = (
  tabs: Tab[],
  activeId: number,
  activeSpaceId: string,
) => Promise<void>;

export async function persistAndSyncTerminalCatalog(
  tabs: Tab[],
  activeId: number,
  activeSpaceId: string,
  persistFirst: boolean,
  persistNow: PersistNow,
  syncCatalog: (records: CatalogRecord[]) => Promise<void>,
  onPersisted: () => void = () => {},
): Promise<void> {
  if (persistFirst) {
    await persistNow(tabs, activeId, activeSpaceId);
    onPersisted();
  }
  await syncCatalog(collectTerminalCatalog(tabs));
}

type PersistNameDependencies = {
  replaceTabs: (tabs: Tab[], activeId: number) => void;
  persistNow: PersistNow;
  ackName: (requestId: string, error?: string) => Promise<void>;
};

export async function persistTerminalName(
  tabs: Tab[],
  request: PersistNameRequest,
  activeId: number,
  activeSpaceId: string,
  dependencies: PersistNameDependencies,
): Promise<void> {
  try {
    const next = applyPersistedName(tabs, request);
    dependencies.replaceTabs(next, activeId);
    await dependencies.persistNow(next, activeId, activeSpaceId);
  } catch (error) {
    await dependencies.ackName(request.requestId, String(error));
    return;
  }
  await dependencies.ackName(request.requestId);
}

export function syncCatalog(records: CatalogRecord[]): Promise<void> {
  return invoke("terminal_control_sync_catalog", { records });
}

export function ackName(requestId: string, error?: string): Promise<void> {
  return invoke("terminal_control_ack_name", {
    requestId,
    error: error ?? null,
  });
}

export function listenPersistName(
  handler: (request: PersistNameRequest) => void | Promise<void>,
): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<PersistNameRequest>(
    PERSIST_NAME_EVENT,
    (event) => {
      void handler(event.payload);
    },
  );
}
