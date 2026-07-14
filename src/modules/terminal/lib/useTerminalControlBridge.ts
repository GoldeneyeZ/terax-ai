import type { Tab } from "@/modules/tabs";
import { type RefObject, useEffect, useRef } from "react";
import { toast } from "sonner";
import {
  ackName,
  collectTerminalCatalog,
  findDuplicateCatalogNames,
  listenPersistName,
  persistAndSyncTerminalCatalog,
  type PersistNow,
  persistTerminalName,
  syncCatalog,
} from "./terminalControl";

type Params = {
  tabs: Tab[];
  tabsRef: RefObject<Tab[]>;
  activeId: number;
  activeSpaceId: string;
  replaceTabs: (tabs: Tab[], activeId: number) => void;
  persistNow: PersistNow;
  spacesHydrated: boolean;
  controlCatalogEligible: boolean;
};

export function useTerminalControlBridge({
  tabs,
  tabsRef,
  activeId,
  activeSpaceId,
  replaceTabs,
  persistNow,
  spacesHydrated,
  controlCatalogEligible,
}: Params): void {
  const activeIdRef = useRef(activeId);
  const activeSpaceIdRef = useRef(activeSpaceId);
  const replaceTabsRef = useRef(replaceTabs);
  const persistNowRef = useRef(persistNow);
  const catalogEligibleRef = useRef(false);
  const initialCatalogPersistedRef = useRef(false);
  const lastSyncedCatalogRef = useRef<string | null>(null);
  const lastDuplicateWarningRef = useRef<string | null>(null);
  const syncChainRef = useRef<Promise<void>>(Promise.resolve());
  const nameChainRef = useRef<Promise<void>>(Promise.resolve());

  activeIdRef.current = activeId;
  activeSpaceIdRef.current = activeSpaceId;
  replaceTabsRef.current = replaceTabs;
  persistNowRef.current = persistNow;
  catalogEligibleRef.current = spacesHydrated && controlCatalogEligible;

  const catalog = collectTerminalCatalog(tabs);
  const catalogKey = JSON.stringify(catalog);
  const duplicateNamesKey = findDuplicateCatalogNames(catalog).join(",");

  useEffect(() => {
    if (!spacesHydrated || !controlCatalogEligible) return;
    if (!duplicateNamesKey) {
      lastDuplicateWarningRef.current = null;
      return;
    }
    if (lastDuplicateWarningRef.current === duplicateNamesKey) return;

    lastDuplicateWarningRef.current = duplicateNamesKey;
    toast.warning("Terminal names need repair", {
      description: `Duplicate names are unavailable for agent messaging: ${duplicateNamesKey}. Rename the affected terminal panes.`,
    });
  }, [controlCatalogEligible, duplicateNamesKey, spacesHydrated]);

  useEffect(() => {
    if (!spacesHydrated || !controlCatalogEligible) return;
    if (lastSyncedCatalogRef.current === catalogKey) return;

    syncChainRef.current = syncChainRef.current.then(async () => {
      if (!catalogEligibleRef.current) return;
      const currentTabs = tabsRef.current;
      const currentCatalogKey = JSON.stringify(
        collectTerminalCatalog(currentTabs),
      );
      if (lastSyncedCatalogRef.current === currentCatalogKey) return;

      const persistFirst = !initialCatalogPersistedRef.current;
      await persistAndSyncTerminalCatalog(
        currentTabs,
        activeIdRef.current,
        activeSpaceIdRef.current,
        persistFirst,
        persistNowRef.current,
        syncCatalog,
        () => {
          initialCatalogPersistedRef.current = true;
        },
      );
      lastSyncedCatalogRef.current = currentCatalogKey;
    });
    syncChainRef.current = syncChainRef.current.catch(() => {
      console.error("[terax] terminal control catalog synchronization failed");
    });
  }, [catalogKey, controlCatalogEligible, spacesHydrated, tabsRef]);

  useEffect(() => {
    const unlistenPromise = listenPersistName((request) => {
      nameChainRef.current = nameChainRef.current.then(async () => {
        if (!catalogEligibleRef.current) {
          await ackName(request.requestId, "TERMINAL_CONTROL_UNAVAILABLE");
          return;
        }
        await persistTerminalName(
          tabsRef.current,
          request,
          activeIdRef.current,
          activeSpaceIdRef.current,
          {
            replaceTabs: (nextTabs, nextActiveId) => {
              tabsRef.current = nextTabs;
              replaceTabsRef.current(nextTabs, nextActiveId);
            },
            persistNow: (nextTabs, nextActiveId, nextActiveSpaceId) =>
              persistNowRef.current(nextTabs, nextActiveId, nextActiveSpaceId),
            ackName,
          },
        );
      });
      nameChainRef.current = nameChainRef.current.catch(() => {
        console.error("[terax] terminal control name acknowledgement failed");
      });
      return nameChainRef.current;
    });
    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [tabsRef]);
}
