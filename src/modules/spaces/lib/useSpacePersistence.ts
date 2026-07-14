import { useCallback, useEffect, useRef } from "react";
import type { Tab } from "@/modules/tabs";
import { isSerializableTab, serializeTabs } from "./serialize";
import { saveState } from "./store";
import { useSpaces } from "./useSpaces";

const DEBOUNCE_MS = 3000;

type Snapshot = { tabs: Tab[]; activeId: number; activeSpaceId: string };

type Params = Snapshot & {
  /** Gate writes until boot hydration finished, so restore never round-trips. */
  enabled: boolean;
};

type LastWrite = {
  json: string;
  activeTabIndex: number;
};

type PendingWrite = LastWrite & { promise: Promise<void> };

export function useSpacePersistence({
  tabs,
  activeId,
  activeSpaceId,
  enabled,
}: Params) {
  const last = useRef<Map<string, LastWrite>>(new Map());
  const pendingWrites = useRef<Map<string, PendingWrite>>(new Map());
  const seeded = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latest = useRef<Snapshot>({ tabs, activeId, activeSpaceId });
  latest.current = { tabs, activeId, activeSpaceId };

  // Seed each space's last-known active index from disk so the first flush
  // preserves it for spaces the user never opens (empty json forces one write
  // with the correct index rather than clobbering it to 0).
  if (enabled && !seeded.current) {
    seeded.current = true;
    for (const [id, idx] of Object.entries(
      useSpaces.getState().initialActiveIndex,
    )) {
      last.current.set(id, { json: "", activeTabIndex: idx });
    }
  }

  const flush = useCallback(async (snap: Snapshot): Promise<void> => {
    const groups = new Map<string, Tab[]>();
    const writes: Promise<void>[] = [];
    for (const t of snap.tabs) {
      const arr = groups.get(t.spaceId);
      if (arr) arr.push(t);
      else groups.set(t.spaceId, [t]);
    }

    for (const [spaceId, group] of groups) {
      const serialized = serializeTabs(group);
      const prev = last.current.get(spaceId);
      let activeTabIndex = prev?.activeTabIndex ?? 0;
      if (spaceId === snap.activeSpaceId) {
        const idx = group
          .filter(isSerializableTab)
          .findIndex((t) => t.id === snap.activeId);
        if (idx >= 0) activeTabIndex = idx;
      }
      const json = JSON.stringify(serialized);
      const pending = pendingWrites.current.get(spaceId);
      if (
        pending &&
        pending.json === json &&
        pending.activeTabIndex === activeTabIndex
      ) {
        writes.push(pending.promise);
        continue;
      }
      if (
        !pending &&
        prev &&
        prev.json === json &&
        prev.activeTabIndex === activeTabIndex
      ) {
        continue;
      }
      let promise: Promise<void>;
      promise = (pending?.promise.catch(() => {}) ?? Promise.resolve())
        .then(() => saveState(spaceId, { tabs: serialized, activeTabIndex }))
        .then(() => {
          if (pendingWrites.current.get(spaceId)?.promise === promise) {
            last.current.set(spaceId, { json, activeTabIndex });
            pendingWrites.current.delete(spaceId);
          }
        })
        .catch((error) => {
          if (pendingWrites.current.get(spaceId)?.promise === promise) {
            pendingWrites.current.delete(spaceId);
          }
          throw error;
        });
      pendingWrites.current.set(spaceId, { json, activeTabIndex, promise });
      writes.push(promise);
    }
    await Promise.all(writes);
  }, []);

  useEffect(() => {
    if (!enabled) return;
    const snap: Snapshot = { tabs, activeId, activeSpaceId };
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      timer.current = null;
      void flush(snap);
    }, DEBOUNCE_MS);
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, [tabs, activeId, activeSpaceId, enabled, flush]);

  useEffect(() => {
    if (!enabled) return;
    const onHidden = () => {
      if (document.visibilityState === "hidden") void flush(latest.current);
    };
    const onLeave = () => void flush(latest.current);
    document.addEventListener("visibilitychange", onHidden);
    window.addEventListener("blur", onLeave);
    window.addEventListener("beforeunload", onLeave);
    return () => {
      document.removeEventListener("visibilitychange", onHidden);
      window.removeEventListener("blur", onLeave);
      window.removeEventListener("beforeunload", onLeave);
      void flush(latest.current);
    };
  }, [enabled, flush]);

  return useCallback(
    (nextTabs: Tab[], nextActiveId: number, nextActiveSpaceId: string) =>
      flush({
        tabs: nextTabs,
        activeId: nextActiveId,
        activeSpaceId: nextActiveSpaceId,
      }),
    [flush],
  );
}
