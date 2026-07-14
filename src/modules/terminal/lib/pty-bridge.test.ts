import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  Channel: class<T> {
    onmessage: (message: T) => void = () => {};
  },
}));

vi.mock("@/modules/workspace", () => ({
  currentWorkspaceEnv: () => ({ kind: "local" }),
}));

import { openPty } from "./pty-bridge";

describe("openPty", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue(7);
  });

  it("binds stable pane metadata to the PTY open request", async () => {
    await openPty(
      80,
      24,
      { onData: () => {} },
      {
        terminalId: "00000000-0000-4000-8000-000000000007",
        addressName: "agent-a",
        private: true,
      },
    );

    expect(invoke).toHaveBeenCalledWith(
      "pty_open",
      expect.objectContaining({
        terminalId: "00000000-0000-4000-8000-000000000007",
        addressName: "agent-a",
        private: true,
      }),
    );
  });

  it("sends null for an unnamed public pane", async () => {
    await openPty(
      80,
      24,
      { onData: () => {} },
      {
        terminalId: "00000000-0000-4000-8000-000000000008",
        private: false,
      },
    );

    expect(invoke).toHaveBeenCalledWith(
      "pty_open",
      expect.objectContaining({ addressName: null, private: false }),
    );
  });
});
