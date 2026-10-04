import { describe, expect, test } from "bun:test";
import type { DownloadOption, ModDetail } from "@frost/shared/types";
import { defaultMirrorIndex, isBlockedDownload, sortMirrors } from "@frost/shared/api/mods";
import {
  GONE_GRACE_MS,
  INSTALL_STALL_MS,
  startWatch,
  stepWatch,
  withTimeout,
} from "../src/Components/Servers/installWatch";

describe("stepWatch", () => {
  test("a job that runs and finishes is done", () => {
    let w = startWatch(0);
    let r = stepWatch(w, { stage: "downloading", received: 10 }, false, 1_000);
    expect(r.verdict).toBe("running");
    w = r.watch;
    r = stepWatch(w, { stage: "done" }, false, 2_000);
    expect(r.verdict).toBe("done");
  });

  test("a job that errors is failed", () => {
    let w = startWatch(0);
    w = stepWatch(w, { stage: "resolving" }, false, 100).watch;
    expect(stepWatch(w, { stage: "error" }, false, 200).verdict).toBe("failed");
  });

  test("a finished card from an earlier attempt is not this attempt's result", () => {
    // The 403 card from the last try is still on screen when Retry is pressed.
    const old = { stage: "error" };
    const w = startWatch(0, old);
    expect(stepWatch(w, old, true, 1_000).verdict).toBe("running");
    // A fresh card for the retry (cards are replaced, never mutated) does count.
    expect(stepWatch(w, { stage: "error" }, false, 2_000).verdict).toBe("failed");
  });

  test("a running job that leaves the panel without finishing is gone", () => {
    let w = startWatch(0);
    w = stepWatch(w, { stage: "downloading", received: 1 }, false, 100).watch;
    expect(stepWatch(w, undefined, false, 200).verdict).toBe("gone");
  });

  test("a job that never reaches the queue or the panel is gone after the grace", () => {
    const w = startWatch(0);
    expect(stepWatch(w, undefined, false, GONE_GRACE_MS - 1).verdict).toBe("running");
    expect(stepWatch(w, undefined, false, GONE_GRACE_MS).verdict).toBe("gone");
  });

  test("a download that stops moving times out; one that moves does not", () => {
    let w = startWatch(0);
    w = stepWatch(w, { stage: "downloading", received: 5 }, false, 1_000).watch;
    // Moving: each new byte count restarts the clock.
    const moved = stepWatch(w, { stage: "downloading", received: 6 }, false, 1_000 + INSTALL_STALL_MS);
    expect(moved.verdict).toBe("running");
    // Stalled on the same count.
    const stuck = stepWatch(w, { stage: "downloading", received: 5 }, false, 1_000 + INSTALL_STALL_MS);
    expect(stuck.verdict).toBe("timeout");
  });

  test("waiting in the queue counts toward the stall, so the tile can't hang there", () => {
    const w = startWatch(0);
    expect(stepWatch(w, undefined, true, INSTALL_STALL_MS - 1).verdict).toBe("running");
    expect(stepWatch(w, undefined, true, INSTALL_STALL_MS).verdict).toBe("timeout");
  });

  test("unpacking and placing are never timed out", () => {
    const w = startWatch(0);
    expect(stepWatch(w, { stage: "extracting" }, false, INSTALL_STALL_MS * 5).verdict).toBe(
      "running",
    );
    expect(stepWatch(w, { stage: "placing" }, false, INSTALL_STALL_MS * 5).verdict).toBe("running");
  });

  test("a pack sent to review ends the watch without a join", () => {
    let w = startWatch(0);
    w = stepWatch(w, { stage: "downloading", received: 1 }, false, 10).watch;
    expect(stepWatch(w, { stage: "review" }, false, 20).verdict).toBe("gone");
  });
});

describe("withTimeout", () => {
  test("passes a prompt answer through", async () => {
    expect(await withTimeout(Promise.resolve(7), 1_000)).toBe(7);
  });

  test("rejects with 'timeout' when nothing answers", async () => {
    const never = new Promise<number>(() => {});
    await expect(withTimeout(never, 10)).rejects.toBe("timeout");
  });
});

describe("one-click track install mirror (LUMBERYARD)", () => {
  const opt = (url: string, host: string, isDefault = false): DownloadOption => ({
    url,
    host,
    isDefault,
    isServer: false,
    label: "",
  });
  // The mxb-mods post as parsed on 2026-10-03: the shop listing first and marked default,
  // the real file on MEGA beside it.
  const shop = opt("https://mxbikes-shop.com/downloads/lumberyard/", "mxbikes-shop.com", true);
  const mega = opt("https://mega.nz/file/abc#key", "MEGA");
  const detail = { downloads: [shop, mega] } as unknown as ModDetail;
  const prefs = { preferServer: false, preferredHost: "" };

  test("a shop product page is not an unattended download", () => {
    expect(isBlockedDownload(shop)).toBe(true);
    // A purchase's signed file link is still a download.
    expect(
      isBlockedDownload(
        opt("https://mxbikes-shop.com/index.php?eddfile=1%3A2%3A0&token=x", "mxbikes-shop.com"),
      ),
    ).toBe(false);
    expect(isBlockedDownload(mega)).toBe(false);
  });

  test("quick install picks the MEGA file over the shop page", () => {
    const mirrors = sortMirrors(detail, prefs);
    expect(mirrors[defaultMirrorIndex(mirrors, prefs)].url).toBe(mega.url);
  });
});
