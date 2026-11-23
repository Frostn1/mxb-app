import { describe, expect, it } from "vitest";
import { BADGES, badgeLabel, changesTracks, classesFor, reloadClass, waitsForTrackLoad } from "./reload";

describe("badges", () => {
  it("say plain words for each class", () => {
    expect(badgeLabel("hot")).toBe("Applies now");
    expect(badgeLabel("next_session")).toBe("Next session");
    expect(badgeLabel("next_event")).toBe("Next track load");
    expect(badgeLabel("restart")).toBe("Needs restart");
    for (const badge of Object.values(BADGES)) expect(badge.title.length).toBeGreaterThan(20);
    expect(BADGES.next_event.title).toContain("Apply now");
  });

  it("offer Apply now only when something waits for the next track load", () => {
    expect(waitsForTrackLoad(["hot", "next_session"])).toBe(false);
    expect(waitsForTrackLoad(["hot", "next_event"])).toBe(true);
    expect(waitsForTrackLoad([])).toBe(false);
  });

  it("run the slow server check only for track and rotation changes", () => {
    expect(changesTracks(["server.name", "world.deformation"])).toBe(false);
    expect(changesTracks(["track.package"])).toBe(true);
    expect(changesTracks(["rotation.tracks"])).toBe(true);
    expect(changesTracks(["server.name", "rotation.tracks"])).toBe(true);
  });
});

describe("reloadClass", () => {
  it("matches the server's classes for the settings MSM edits", () => {
    expect(reloadClass("server.name")).toBe("hot");
    expect(reloadClass("admission.max_ping_ms")).toBe("hot");
    expect(reloadClass("native.late_join_register")).toBe("hot");
    expect(reloadClass("master.enable")).toBe("hot");
    expect(reloadClass("events.collisions")).toBe("hot");
    expect(reloadClass("ghost.fill_to")).toBe("hot");
    expect(reloadClass("penalties.cut_time_seconds")).toBe("next_session");
    expect(reloadClass("cuts.zones")).toBe("next_session");
    expect(reloadClass("sessions.race_extra_laps")).toBe("next_session");
    expect(reloadClass("ghost.skill_pct")).toBe("next_session");
    expect(reloadClass("ghost.personality.seed")).toBe("next_session");
    expect(reloadClass("sessions.race_minutes")).toBe("next_event");
    expect(reloadClass("event.overjump_crash")).toBe("next_event");
    expect(reloadClass("event.weather_conditions")).toBe("next_event");
  });

  it("keeps only the game port and development tools restart-only", () => {
    for (const key of ["server.listen", "world.probe", "world.record", "world.inject_test_rut"]) {
      expect(reloadClass(key)).toBe("restart");
    }
  });

  it("applies the bots, exporters and admin plane without a restart", () => {
    for (const key of ["ghost.count", "ghost.bikes", "ghost.name", "ghost.replay", "ghost.record", "track.roster", "native.track_bounds", "results.directory", "points.enable", "rating.push_url", "paints.enforce_locks"]) {
      expect(reloadClass(key)).toBe("next_session");
    }
    for (const key of ["admin.listen", "admin.tokens_file", "admin.audit_file", "server.observe", "recording.directory", "ghost.fill_to"]) {
      expect(reloadClass(key)).toBe("hot");
    }
  });

  it("takes a deformation level at the next session and the ruts policy at once", () => {
    expect(reloadClass("world.deformation")).toBe("next_session");
    expect(reloadClass("world.ruts_persist")).toBe("hot");
  });

  it("prefers the server's own answer", () => {
    const server = [{ field: "admission.allowed_bikes", from: null, to: ["X"], class: "restart" as const, note: "" }];
    expect(classesFor(["admission.allowed_bikes", "server.name"], server)).toEqual({
      "admission.allowed_bikes": "restart",
      "server.name": "hot",
    });
  });
});
