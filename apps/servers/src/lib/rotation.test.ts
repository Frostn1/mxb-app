import { describe, expect, it } from "vitest";
import { describeRotationSave, playNextQueue, randomTrack, stateAfterSwitch, addBlockedReason, blockedInQueue } from "./rotation";
import { BADGES, reloadClass } from "./reload";

describe("rotation save", () => {
  it("is applied live at the next event, with no restart", () => {
    const said = describeRotationSave({ live: true, restartRequired: false });
    expect(said.needsRestart).toBe(false);
    expect(said.message).toContain("next track load");
    expect(said.message).toContain("nobody is disconnected");
  });

  it("offers a restart only for a server too old to change tracks live", () => {
    const said = describeRotationSave({ live: false, restartRequired: true });
    expect(said.needsRestart).toBe(true);
    expect(said.message).toContain("Restart");
  });
});

describe("rotation keys", () => {
  it("are next-event changes, badged as such", () => {
    for (const key of ["track.package", "rotation.tracks", "bike_set.manifest", "bike_sets.manifests", "event.weather", "world.ruts", "world.live_ruts"]) {
      expect(reloadClass(key)).toBe("next_event");
    }
    expect(BADGES.next_event.label).toBe("Next track load");
    expect(reloadClass("admission.allowed_bikes")).toBe("hot");
    expect(reloadClass("world.probe")).toBe("restart");
  });
});

describe("play next", () => {
  it("puts the chosen track right after the running one, which goes last", () => {
    expect(playNextQueue("a", ["b", "c", "d"], "c")).toEqual(["c", "b", "d", "a"]);
    expect(playNextQueue("a", ["b"], "b")).toEqual(["b", "a"]);
  });

  it("picks a random track and none from an empty rotation", () => {
    expect(randomTrack(["b", "c", "d"], () => 0.99)).toBe("d");
    expect(randomTrack(["b", "c", "d"], () => 0)).toBe("b");
    expect(randomTrack([])).toBeNull();
  });
});

describe("stateAfterSwitch", () => {
  it("makes the first queued track current and the rest the rotation", () => {
    const state = { current: "A", rotation: ["B", "C"], library: ["A", "B", "C"] };
    const queue = playNextQueue("A", ["B", "C"], "C");
    expect(stateAfterSwitch(state, queue)).toEqual({ current: "C", rotation: ["B", "A"], library: ["A", "B", "C"] });
  });
});
describe("protected packages", () => {
  const protectedTracks = ["WDR_R02.pkz"];
  it("blocks adding a protected package, with the reason", () => {
    expect(addBlockedReason("WDR_R02.pkz", protectedTracks)).toContain("protected package");
    expect(addBlockedReason("Smokey.pkz", protectedTracks)).toBeNull();
    expect(addBlockedReason("Smokey.pkz", undefined)).toBeNull();
  });
  it("finds the protected ones already in a queue", () => {
    expect(blockedInQueue(["A.pkz", "WDR_R02.pkz", "WDR_R02.pkz"], protectedTracks)).toEqual(["WDR_R02.pkz"]);
    expect(blockedInQueue(["A.pkz"], protectedTracks)).toEqual([]);
  });
});