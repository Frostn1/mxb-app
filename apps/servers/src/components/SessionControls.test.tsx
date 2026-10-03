// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { ServerView } from "@/lib/api";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const invoke = vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (c: string, a?: Record<string, unknown>) => invoke(c, a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

import { SessionControls } from "./ServerDetail";

const mount = async (current: string) => {
  invoke.mockReset();
  invoke.mockImplementation(async (c) => (c === "config_load" ? { values: {} } : {}));
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  await act(async () => root.render(<SessionControls server={{ id: "s1", name: "Prod", local: false } as ServerView} current={current} remaining={null} refresh={() => {}} />));
  const practice = () => [...host.querySelectorAll("button")].find((b) => b.textContent?.startsWith("1practice")) as HTMLButtonElement;
  return { host, practice };
};

describe("SessionControls Practice", () => {
  it.each(["running(race)", "countdown(race)", "countdown { next: race }", "running(warmup)", "running(qualifying)"])("is reachable from %s", async (state) => {
    const { practice } = await mount(state);
    expect(practice().disabled).toBe(false);
  });

  it("is the current stage in practice (disabled)", async () => {
    const { practice } = await mount("running(practice)");
    expect(practice().disabled).toBe(true);
  });

  it("confirms, then jumps to practice mid-race", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const { practice } = await mount("running(race)");
    await act(async () => practice().click());
    expect(confirm.mock.calls[0][0]).toContain("race in progress");
    expect(invoke).toHaveBeenCalledWith("server_session", { id: "s1", action: "jump", to: "practice" });
  });

  it("shows the API error", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const { host, practice } = await mount("running(race)");
    invoke.mockImplementation(async (c) => { if (c === "server_session") throw "boom: refused"; return { values: {} }; });
    await act(async () => practice().click());
    expect(host.textContent).toContain("boom: refused");
  });
});
