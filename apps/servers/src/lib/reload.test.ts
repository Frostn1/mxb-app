import { describe, expect, it } from "vitest";
import { classesFor, reloadClass } from "./reload";

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

  it("prefers the server's own answer", () => {
    const server = [{ field: "admission.allowed_bikes", from: null, to: ["X"], class: "restart" as const, note: "" }];
    expect(classesFor(["admission.allowed_bikes", "server.name"], server)).toEqual({
      "admission.allowed_bikes": "restart",
      "server.name": "hot",
    });
  });
});
