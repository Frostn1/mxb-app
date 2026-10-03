import { describe, expect, it } from "vitest";
import type { Rider } from "./api";
import {
  diffRiders,
  diffTiming,
  filterEvents,
  newLogLines,
  parseLogLine,
  prepend,
  stageText,
  withNames,
  type EventKind,
  type ServerEvent,
  type Timing,
  type TimingEntry,
} from "./events";

// Lines exactly as mxbserver prints them (crates/mxbserver/src/{collisions,native,refusal}.rs).
// Addresses are documentation ranges and names are made up.
describe("parseLogLine", () => {
  it("reads a collision with both riders, impact, speed and track position", () => {
    const e = parseLogLine("event collision: #12 Rider One -> #7 Rider Two · rear · 38 km/h closing · s=512 m");
    expect(e?.kind).toBe("collision");
    expect(e?.riders).toEqual(["Rider One", "Rider Two"]);
    expect(e?.fields).toMatchObject({
      "Rider": "Rider One",
      "Race number": 12,
      "Other rider": "Rider Two",
      "Other race number": 7,
      "Impact": "rear",
      "Closing speed (km/h)": 38,
      "Track position (m)": 512,
    });
  });

  it("reads a head-on collision without a track position", () => {
    const e = parseLogLine("event collision: #3 A -> #4 B · head-on · 12 km/h closing");
    expect(e?.fields["Impact"]).toBe("head-on");
    expect(e?.fields["Track position (m)"]).toBeNull();
  });

  it("reads a track cut with its quoted name", () => {
    const e = parseLogLine('event cut: #21 "Test \\"Rider\\"" · Fake Track · route 845.2 m · 6.40 m outside for 1.25 s');
    expect(e?.kind).toBe("cut");
    expect(e?.riders).toEqual(['Test "Rider"']);
    expect(e?.fields).toMatchObject({ "Track": "Fake Track", "Track position (m)": 845.2, "Distance outside (m)": 6.4, "Time outside (s)": 1.25 });
  });

  it("reads holeshot, finish, chequered flag, retirement and race over as race events", () => {
    const lines = [
      "event holeshot: #5 · 3.42 s since the gate drop (H7 layout confirmed; the seconds reading is inferred, H14 #3 unsettled)",
      "  race       chequered flag · leader 5 on lap 12",
      "  race       5 finished · 12 laps",
      "  race       9 retired · left mid-race",
      "  race       over · 18 finished, 2 retired",
    ];
    const events = lines.map(parseLogLine);
    expect(events.map((e) => e?.kind)).toEqual(["race", "race", "race", "race", "race"]);
    expect(events.map((e) => e?.fields["Event"])).toEqual(["Holeshot", "Chequered flag", "Finished", "Retired", "Race over"]);
    expect(events[0]?.fields["Time since gate drop (s)"]).toBe(3.42);
    expect(events[2]?.fields["Laps"]).toBe(12);
  });

  it("reads session transitions, operator ones included", () => {
    expect(parseLogLine("  session    +1200s -> Running(Qualifying)")?.fields).toMatchObject({ "Stage": "Qualifying", "By operator": false, "Event clock (s)": 1200 });
    const op = parseLogLine("  session    operator jump +75.5s -> Countdown { next: Race }");
    expect(op?.kind).toBe("session");
    expect(op?.summary).toBe("Operator: Race countdown");
    expect(parseLogLine("  session    operator restarted Practice at +3s")?.summary).toBe("Operator restarted Practice");
    expect(parseLogLine("  session    rotation · starting the event on fake-track")?.fields["Track"]).toBe("fake-track");
  });

  it("reads joins, disconnects and timeouts", () => {
    const join = parseLogLine("  join       203.0.113.5:51234 change accepted · bike KTM 450 SX-F");
    expect(join?.kind).toBe("join");
    expect(join?.fields).toMatchObject({ "Address": "203.0.113.5:51234", "Bike": "KTM 450 SX-F" });
    expect(parseLogLine("  join       203.0.113.5:51234 ping 42 ms")?.fields["Ping (ms)"]).toBe(42);
    expect(parseLogLine("diag disconnect: 203.0.113.5:51234 (client 3) sent DISCONNECTION (reliable) in Active")?.kind).toBe("leave");
    expect(parseLogLine("  refused    timeout 203.0.113.9:4000: stalled while loading")?.kind).toBe("leave");
    expect(parseLogLine("  refused    password 203.0.113.9:4000: wrong password")?.kind).toBe("refused");
  });

  it("turns a bot crash into a crash event", () => {
    const e = parseLogLine("diag ghost: bot #31 crashes at 44 km/h");
    expect(e?.kind).toBe("crash");
    expect(e?.fields["Outcome"]).toBe("Crash");
  });

  it("drops diagnostics and blank lines, keeps the rest as info", () => {
    expect(parseLogLine("diag relay: 203.0.113.5:1 tick=4 live=1")).toBeNull();
    expect(parseLogLine("native CLIENT_DATA accepted from 203.0.113.5:1: client_id=0")).toBeNull();
    expect(parseLogLine("   ")).toBeNull();
    expect(parseLogLine("\x1b[36mlisten    \x1b[0m 0.0.0.0:54210")?.kind).toBe("info");
  });
});

describe("stageText", () => {
  it("names stages", () => {
    expect(stageText("Running(Race)")).toBe("Race");
    expect(stageText("Countdown { next: Warmup }")).toBe("Warmup countdown");
    expect(stageText("Results")).toBe("Results");
  });
});

describe("newLogLines", () => {
  it("returns only what was appended to a sliding tail", () => {
    expect(newLogLines(["a", "b", "c"], ["b", "c", "d", "e"])).toEqual(["d", "e"]);
    expect(newLogLines(["a", "b", "c"], ["a", "b", "c"])).toEqual([]);
    expect(newLogLines(["a", "b", "c"], ["a", "b", "c", "d"])).toEqual(["d"]);
  });
  it("treats everything as new on the first read or with no overlap", () => {
    expect(newLogLines([], ["a"])).toEqual(["a"]);
    expect(newLogLines(["a", "b"], ["x", "y"])).toEqual(["x", "y"]);
  });
  it("handles repeated lines", () => {
    expect(newLogLines(["x", "x", "y", "x"], ["x", "y", "x", "x"])).toEqual(["x"]);
  });
});

const entry = (over: Partial<TimingEntry> = {}): TimingEntry => ({
  position: 1,
  race: 12,
  name: "Rider One",
  bike: "Fake 450",
  laps: 2,
  last_lap_seconds: 92.5,
  best_lap_seconds: 91.0,
  last_split1_seconds: 30.1,
  last_split2_seconds: 61.2,
  gap_to_leader_seconds: null,
  interval_seconds: null,
  finished: false,
  retired: false,
  penalty_seconds: 0,
  disqualified: false,
  ...over,
});
const timing = (entries: TimingEntry[], session = "race"): Timing => ({ available: true, track: "Fake Track", session, flag: null, entries });

describe("diffTiming", () => {
  it("makes a lap event when the lap count goes up, flagging a personal best", () => {
    const events = diffTiming(timing([entry()]), timing([entry({ laps: 3, last_lap_seconds: 90.25, best_lap_seconds: 90.25, last_split1_seconds: 30.1, last_split2_seconds: 61.2 })]));
    expect(events).toHaveLength(1);
    expect(events[0].kind).toBe("lap");
    expect(events[0].fields).toMatchObject({ "Lap": 3, "Lap time": "1:30.250", "Lap time (s)": 90.25, "Personal best": true, "Position": 1, "Rider": "Rider One" });
  });

  it("makes a time check for each new split", () => {
    const events = diffTiming(timing([entry()]), timing([entry({ last_split1_seconds: 29.8 })]));
    expect(events.map((e) => [e.kind, e.fields["Split"], e.fields["Split time (s)"]])).toEqual([["split", 1, 29.8]]);
  });

  it("makes penalty events for added seconds and a disqualification", () => {
    const events = diffTiming(timing([entry()]), timing([entry({ penalty_seconds: 5, disqualified: true })]));
    expect(events.map((e) => e.kind)).toEqual(["penalty", "penalty"]);
    expect(events[0].fields["Added (s)"]).toBe(5);
  });

  it("only sets a baseline for a new rider, a new session or the first answer", () => {
    expect(diffTiming(null, timing([entry()]))).toEqual([]);
    expect(diffTiming(timing([]), timing([entry({ laps: 9 })]))).toEqual([]);
    expect(diffTiming(timing([entry()], "practice"), timing([entry({ laps: 5 })], "race"))).toEqual([]);
    expect(diffTiming(timing([entry()]), { available: false })).toEqual([]);
  });
});

const rider = (id: number, name: string): Rider => ({
  connection_id: id, entity_id: id, name, bike: "Fake 250", state: "riding", connected_seconds: 60, laps: 4, best_lap_seconds: 88.8, ping_ms: 40,
});

describe("diffRiders", () => {
  it("names joins and leaves", () => {
    const events = diffRiders([rider(1, "Stays"), rider(2, "Goes")], [rider(1, "Stays"), rider(3, "Arrives")]);
    expect(events.map((e) => [e.kind, e.riders[0]])).toEqual([["join", "Arrives"], ["leave", "Goes"]]);
    expect(diffRiders(null, [rider(1, "x")])).toEqual([]);
  });
});

describe("withNames", () => {
  it("adds the rider name to race-number-only events", () => {
    const e = withNames(parseLogLine("  race       5 finished · 12 laps")!, new Map([[5, "Rider Five"]]));
    expect(e.riders).toEqual(["Rider Five"]);
    expect(e.summary).toBe("#5 Rider Five finished · 12 laps");
  });
});

const ev = (id: string, kind: EventKind, riders: string[] = [], summary = id): ServerEvent => ({ id, at: 0, kind, summary, riders, fields: {}, source: "log" });

describe("buffer and filters", () => {
  it("puts new events first and caps the buffer", () => {
    const buffer = prepend([ev("old", "info")], [ev("a", "lap"), ev("b", "lap")], 2);
    expect(buffer.map((e) => e.id)).toEqual(["b", "a"]);
  });

  it("filters by kind and by rider name, case-insensitively", () => {
    const events = [ev("1", "lap", ["Rider One"]), ev("2", "collision", ["Rider One", "Rider Two"]), ev("3", "lap", ["Rider Two"]), ev("4", "info")];
    expect(filterEvents(events, new Set<EventKind>(["lap"]), "").map((e) => e.id)).toEqual(["1", "3"]);
    expect(filterEvents(events, new Set<EventKind>(["lap", "collision"]), "rider two").map((e) => e.id)).toEqual(["2", "3"]);
    expect(filterEvents(events, new Set<EventKind>(), "")).toEqual([]);
  });
});
