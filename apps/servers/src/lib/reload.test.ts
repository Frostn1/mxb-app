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

  it("keeps sockets, content and the bots themselves restart-only", () => {
    for (const key of ["server.listen", "admin.listen", "track.package", "world.ruts", "ghost.count", "ghost.bikes", "native.track_bounds", "event.weather"]) {
      expect(reloadClass(key)).toBe("restart");
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
