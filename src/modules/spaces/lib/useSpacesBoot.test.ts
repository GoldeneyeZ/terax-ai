import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SerializedTab } from "./serialize";

const mocks = vi.hoisted(() => ({
  adoptWorkspaceEnv: vi.fn(async () => null as string | null),
  hydrateSpaces: vi.fn(),
  loadAll: vi.fn(),
  markBooted: vi.fn(),
  replaceTabs: vi.fn(),
  saveActiveId: vi.fn(async () => {}),
  saveSpacesList: vi.fn(async () => {}),
  saveState: vi.fn(
    async (
      _id: string,
      _state: { tabs: SerializedTab[]; activeTabIndex: number },
    ) => {},
  ),
  setActiveSpaceForNewTabs: vi.fn(),
  setControlCatalogEligible: vi.fn(),
  workspaceAuthorize: vi.fn(async () => {}),
}));

vi.mock("react", () => ({
  useEffect: (effect: () => void) => effect(),
  useRef: () => ({ current: false }),
  useState: () => [false, mocks.setControlCatalogEligible],
}));

vi.mock("@/modules/ai/lib/native", () => ({
  native: { workspaceAuthorize: mocks.workspaceAuthorize },
}));

vi.mock("@/modules/settings/preferences", () => ({
  usePreferencesStore: {
    getState: () => ({ defaultWorkspaceEnv: "system", init: async () => {} }),
  },
}));

vi.mock("./activeSpace", () => ({
  activeSpaceEnv: () => "system",
  freshTabCwd: () => null,
}));

vi.mock("./store", () => ({
  loadAll: mocks.loadAll,
  saveActiveId: mocks.saveActiveId,
  saveSpacesList: mocks.saveSpacesList,
  saveState: mocks.saveState,
}));

vi.mock("./useSpaces", () => ({
  useSpaces: { getState: () => ({ hydrate: mocks.hydrateSpaces }) },
}));

import { useSpacesBoot } from "./useSpacesBoot";

const space = {
  id: "space-a",
  name: "Space A",
  root: null,
  env: "system" as const,
  createdAt: 1,
  updatedAt: 1,
};

function loadedWith(tree: SerializedTab) {
  return {
    spaces: [space],
    activeId: space.id,
    states: new Map([[space.id, { tabs: [tree], activeTabIndex: 0 }]]),
  };
}

function boot() {
  return useSpacesBoot({
    ready: true,
    launchCwd: null,
    home: null,
    allocId: (() => {
      let id = 1;
      return () => id++;
    })(),
    replaceTabs: mocks.replaceTabs,
    markBooted: mocks.markBooted,
    setActiveSpaceForNewTabs: mocks.setActiveSpaceForNewTabs,
    adoptWorkspaceEnv: mocks.adoptWorkspaceEnv,
  });
}

describe("useSpacesBoot", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.saveState.mockResolvedValue(undefined);
  });

  it("persists a migrated identity before enabling the control catalog", async () => {
    let resolveSave!: () => void;
    mocks.saveState.mockReturnValue(
      new Promise<void>((resolve) => {
        resolveSave = resolve;
      }),
    );
    mocks.loadAll.mockResolvedValue(
      loadedWith({
        kind: "terminal",
        tree: { kind: "leaf", active: true },
      }),
    );

    expect(boot()).toBe(false);
    await vi.waitFor(() => expect(mocks.saveState).toHaveBeenCalledOnce());
    expect(mocks.setControlCatalogEligible).not.toHaveBeenCalled();
    expect(mocks.hydrateSpaces).not.toHaveBeenCalled();

    const saved = mocks.saveState.mock.calls[0][1];
    expect(saved.tabs[0]).toMatchObject({
      tree: { kind: "leaf", terminalId: expect.any(String) },
    });

    resolveSave();
    await vi.waitFor(() =>
      expect(mocks.setControlCatalogEligible).toHaveBeenCalledWith(true),
    );
    expect(mocks.hydrateSpaces).toHaveBeenCalledOnce();
    expect(mocks.markBooted).toHaveBeenCalledOnce();
  });

  it("keeps the control catalog disabled when migration persistence fails", async () => {
    const error = new Error("secret storage detail");
    mocks.saveState.mockRejectedValue(error);
    mocks.loadAll.mockResolvedValue(
      loadedWith({
        kind: "terminal",
        tree: { kind: "leaf", active: true },
      }),
    );
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});

    expect(boot()).toBe(false);
    await vi.waitFor(() => expect(mocks.markBooted).toHaveBeenCalledOnce());

    expect(mocks.setControlCatalogEligible).not.toHaveBeenCalled();
    expect(mocks.hydrateSpaces).not.toHaveBeenCalled();
    expect(consoleError).toHaveBeenCalledWith(
      "[terax] terminal control unavailable during spaces boot",
    );
    expect(consoleError).not.toHaveBeenCalledWith(expect.anything(), error);
    consoleError.mockRestore();
  });

  it("does not rewrite an already versioned leaf", async () => {
    mocks.loadAll.mockResolvedValue(
      loadedWith({
        kind: "terminal",
        tree: {
          kind: "leaf",
          terminalId: "00000000-0000-4000-8000-000000000001",
          active: true,
        },
      }),
    );

    expect(boot()).toBe(false);
    await vi.waitFor(() => expect(mocks.markBooted).toHaveBeenCalledOnce());

    expect(mocks.saveState).not.toHaveBeenCalled();
    expect(mocks.setControlCatalogEligible).toHaveBeenCalledWith(true);
  });
});
