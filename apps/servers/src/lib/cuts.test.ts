import { describe, expect, it } from "vitest";
import { parseCuts, parseRecentCuts, pointAt, projectToLine, segment, zonesFrom, type CutPoint } from "./cuts";

// A straight 100 m line along x, a point every 10 m, 6 m half width.
const line: CutPoint[] = Array.from({ length: 11 }, (_, i) => [i * 10, 0, 6, i * 10]);

describe("cut geometry", () => {
  it("projects a clicked point onto the nearest track distance", () => {
    expect(projectToLine(line, 34, 3)).toEqual({ s: 34, dist: 3 });
    expect(projectToLine(line, -5, 0)?.s).toBe(0);
    expect(projectToLine(line, 500, 0)?.s).toBe(100);
    expect(projectToLine([], 0, 0)).toBeNull();
  });

  it("interpolates positions and segments", () => {
    expect(pointAt(line, 25)).toEqual([25, 0]);
    expect(segment(line, 80, 15).map((p) => p[0])).toEqual([15, 20, 30, 40, 50, 60, 70, 80]);
  });
});

describe("cut answers", () => {
  it("treats a missing endpoint as not supported", () => {
    expect(parseCuts({ supported: false })).toBeNull();
    expect(parseCuts(null)).toBeNull();
  });

  it("reads tracks and fills in zone defaults", () => {
    const info = parseCuts({
      enabled: true,
      mode: "auto",
      current_track: "Fake",
      tracks: [{ track: "Fake", source: "auto", closed: true, length_m: 100, points: [...line, ["x"]], zones: [{ name: "Z", from_m: 10, to_m: 20, area: null }] }],
    });
    expect(info?.tracks[0].points).toHaveLength(11);
    expect(info?.tracks[0].zones[0]).toEqual({ track: "Fake", name: "Z", from_m: 10, to_m: 20, area: null, seconds: 10, enable: true });
    expect(parseCuts({ enabled: false, mode: "off", tracks: [] })?.tracks).toEqual([]);
  });

  it("reads cuts.recent and zones from settings", () => {
    expect(parseRecentCuts({ cuts: { recent: [{ track: "T", race: 1, name: "A" }] } })).toHaveLength(1);
    expect(parseRecentCuts({})).toEqual([]);
    expect(zonesFrom([{ track: "T", name: "Z", area: [[1, 2], [3, 4], ["x", 1]], seconds: 0, enable: false }])[0]).toMatchObject({ area: [[1, 2], [3, 4]], seconds: 0, enable: false });
  });
});
