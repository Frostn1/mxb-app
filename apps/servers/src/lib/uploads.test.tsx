// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ServerView } from "@/lib/api";
import type { Upload } from "@/lib/uploads";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The Tauri side: `invoke` is the command list, `listen` hands us the event emitter.
type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Handler>();
const invoke = vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (c: string, a?: Record<string, unknown>) => invoke(c, a) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: Handler) => {
    handlers.set(name, handler);
    return () => void handlers.delete(name);
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const sent = (name: string, payload: unknown) => act(async () => handlers.get(name)?.({ payload }));

const upload = (over: Partial<Upload> = {}): Upload => ({
  id: "u1",
  serverId: "s1",
  serverName: "Prod",
  path: "C:/tracks/755-Compound.pkz",
  fileName: "755-Compound.pkz",
  bytes: 1000,
  sent: 0,
  speed: 0,
  attempt: 1,
  status: "uploading",
  error: null,
  ...over,
});

const server = { id: "s1", name: "Prod", kind: "native", local: false } as ServerView;

let root: Root;
let host: HTMLElement;

beforeEach(async () => {
  handlers.clear();
  invoke.mockReset();
  invoke.mockImplementation(async (command) => {
    if (command === "upload_list") return [];
    if (command === "server_tracks") return { library: [], installed: [], current: null, rotation: [] };
    return undefined;
  });
  const { resetUploads } = await import("@/lib/uploads");
  resetUploads();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("track uploads outlive the Tracks view", () => {
  it("keeps updating after the Tracks tab unmounts, and the tab picks it up again", async () => {
    const { TracksTab } = await import("@/components/TracksTab");
    const { UploadsIndicator } = await import("@/components/UploadsIndicator");
    const { getUploads } = await import("@/lib/uploads");

    await act(async () => root.render(<><UploadsIndicator /><TracksTab server={server} /></>));
    await sent("upload-update", upload({ sent: 100 }));
    expect(host.textContent).toContain("755-Compound.pkz");
    expect(host.textContent).toContain("10%");

    // Leave the tab (the header, which is on every page, stays).
    await act(async () => root.render(<UploadsIndicator />));
    expect(host.querySelector("[data-testid=upload-row]")).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("upload_cancel", expect.anything());

    // The backend keeps reporting while no tab is mounted, and the header follows.
    await sent("upload-update", upload({ sent: 600, speed: 2048 }));
    expect(getUploads()[0].sent).toBe(600);
    expect(host.textContent).toContain("60%");
    expect(host.textContent).toContain("2 KiB/s");

    // Come back: the same upload is there, mid-way, not lost.
    await act(async () => root.render(<><UploadsIndicator /><TracksTab server={server} /></>));
    expect(host.textContent).toContain("60%");
    await sent("upload-update", upload({ sent: 1000, status: "done" }));
    expect(host.textContent).toContain("Installed");
    expect(invoke).not.toHaveBeenCalledWith("upload_cancel", expect.anything());
  });

  it("finds uploads already running when the app view is created", async () => {
    invoke.mockImplementation(async (command) => (command === "upload_list" ? [upload({ sent: 300 })] : undefined));
    const { initUploads, getUploads } = await import("@/lib/uploads");
    await initUploads();
    expect(getUploads().map((u) => u.sent)).toEqual([300]);
  });

  it("does not let an older list overwrite a newer event", async () => {
    let release: (v: Upload[]) => void = () => undefined;
    invoke.mockImplementation((command) => (command === "upload_list" ? new Promise((r) => (release = r as typeof release)) : Promise.resolve(undefined)));
    const { initUploads, getUploads } = await import("@/lib/uploads");
    const ready = initUploads();
    await vi.waitFor(() => expect(handlers.has("upload-update")).toBe(true));
    await sent("upload-update", upload({ sent: 700 }));
    release([upload({ sent: 300 })]);
    await ready;
    expect(getUploads()[0].sent).toBe(700);
  });

  it("cancel goes to the backend, and only on an explicit click", async () => {
    const { UploadsIndicator } = await import("@/components/UploadsIndicator");
    await act(async () => root.render(<UploadsIndicator />));
    await sent("upload-update", upload({ sent: 200 }));
    await act(async () => host.querySelector<HTMLButtonElement>("button[aria-label=Uploads]")?.click());
    const cancel = [...host.querySelectorAll("button")].find((b) => b.textContent === "Cancel");
    expect(cancel).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith("upload_cancel", expect.anything());
    await act(async () => cancel?.click());
    expect(invoke).toHaveBeenCalledWith("upload_cancel", { id: "u1" });

    await sent("upload-update", upload({ status: "cancelled" }));
    expect(host.textContent).toContain("Cancelled");
    expect(host.textContent).toContain("Retry");
  });

  it("shows a failure clearly with Retry, which starts the same file again", async () => {
    const { UploadsIndicator } = await import("@/components/UploadsIndicator");
    await act(async () => root.render(<UploadsIndicator />));
    await sent("upload-update", upload({ status: "error", error: "Permission denied (publickey)." }));
    await act(async () => host.querySelector<HTMLButtonElement>("button[aria-label=Uploads]")?.click());
    expect(host.textContent).toContain("Permission denied (publickey).");
    invoke.mockImplementation(async (command) => (command === "upload_start" ? upload({ id: "u2" }) : command === "upload_list" ? [] : undefined));
    await act(async () => [...host.querySelectorAll("button")].find((b) => b.textContent === "Retry")?.click());
    expect(invoke).toHaveBeenCalledWith("upload_dismiss", { id: "u1" });
    expect(invoke).toHaveBeenCalledWith("upload_start", { id: "s1", path: "C:/tracks/755-Compound.pkz" });
  });
});

describe("quitting", () => {
  it("asks first when an upload is running, and quits only when confirmed", async () => {
    const { initUploads, handleQuitRequested } = await import("@/lib/uploads");
    await initUploads();
    await sent("upload-update", upload());
    const no = vi.fn(() => false);
    await handleQuitRequested(no);
    expect(no).toHaveBeenCalledWith(expect.stringContaining("1 upload is still running"));
    expect(invoke).not.toHaveBeenCalledWith("quit_app", undefined);
    await handleQuitRequested(() => true);
    expect(invoke).toHaveBeenCalledWith("quit_app", undefined);
  });
});
