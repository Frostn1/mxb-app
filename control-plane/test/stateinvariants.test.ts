import { describe, expect, it } from "vitest";
import {
  asDeviations,
  asManifest,
  MAX_STATE_REGIONS,
  parseDigests,
  type StateBaseline,
} from "../src/stateinvariants";
import { classify, isUnaccounted, type ModuleRule } from "../src/diagnostics";

const CLEAN = "a".repeat(64);
const DIRTY = "b".repeat(64);

const region = (extra: Partial<StateBaseline> = {}): StateBaseline => ({
  name: "physics-coefficients",
  rva: 0x1000,
  length: 256,
  baseline: CLEAN,
  ...extra,
});

describe("reading reported digests", () => {
  it("takes a well-formed list", () => {
    const out = parseDigests([{ name: "physics-coefficients", digest: CLEAN }]);
    expect(out).toEqual([{ name: "physics-coefficients", digest: CLEAN }]);
  });

  it("treats absent as nothing to say, not as malformed", () => {
    // An app too old to have been asked sends nothing, and that is the ordinary case for as
    // long as it takes a release to go out. It must not read as a bad report.
    expect(parseDigests(undefined)).toEqual([]);
    expect(parseDigests(null)).toEqual([]);
    expect(parseDigests([])).toEqual([]);
  });

  it("lowercases so a rule written once matches either casing", () => {
    const out = parseDigests([{ name: "Physics-Coefficients", digest: CLEAN.toUpperCase() }]);
    expect(out).toEqual([{ name: "physics-coefficients", digest: CLEAN }]);
  });

  it("refuses anything that is not a digest list", () => {
    expect(parseDigests("no")).toBeNull();
    expect(parseDigests([{ name: "x" }])).toBeNull();
    expect(parseDigests([{ name: "x", digest: 5 }])).toBeNull();
    expect(parseDigests([{ name: "x", digest: "nothex!!" }])).toBeNull();
    expect(parseDigests([{ name: "../etc", digest: CLEAN }])).toBeNull();
  });

  it("refuses a region reported twice rather than picking an answer", () => {
    // Taking either would hide which one was true, and they disagree by construction.
    const dup = [
      { name: "physics-coefficients", digest: CLEAN },
      { name: "physics-coefficients", digest: DIRTY },
    ];
    expect(parseDigests(dup)).toBeNull();
  });

  it("is bounded", () => {
    const many = Array.from({ length: MAX_STATE_REGIONS + 1 }, (_, i) => ({
      name: `r${i}`,
      digest: CLEAN,
    }));
    expect(parseDigests(many)).toBeNull();
  });
});

describe("comparing against the baseline", () => {
  it("says nothing at all when the digest matches", () => {
    // The common case by an enormous margin. A row per player per report would bury the
    // handful worth reading, which is the whole reason a baseline is stored.
    const out = asDeviations([{ name: "physics-coefficients", digest: CLEAN }], [region()]);
    expect(out).toEqual([]);
  });

  it("makes a difference into a row the rest of the pipeline reads", () => {
    const out = asDeviations([{ name: "physics-coefficients", digest: DIRTY }], [region()]);
    expect(out).toHaveLength(1);
    expect(out[0].name).toBe("state.physics-coefficients");
    expect(out[0].origin).toBe("state");
    // The digest goes in the hash column, because that is what rules match and prevalence
    // groups by.
    expect(out[0].sha256).toBe(DIRTY);
    expect(out[0].detail).toContain("physics-coefficients");
  });

  it("drops a digest for a region we did not ask about", () => {
    // Only reachable from a client running a manifest we have since changed. We hold no
    // baseline for it, so a row would be an observation nobody could act on.
    const out = asDeviations([{ name: "retired-region", digest: DIRTY }], [region()]);
    expect(out).toEqual([]);
  });

  it("compares case-insensitively against a baseline stored in either case", () => {
    const out = asDeviations(
      [{ name: "physics-coefficients", digest: CLEAN }],
      [region({ baseline: CLEAN.toUpperCase() })],
    );
    expect(out).toEqual([]);
  });

  it("reports nothing when there are no baselines for this build", () => {
    // An MX Bikes patch retires every baseline at once. That day must be quiet, not an
    // alert on every player.
    expect(asDeviations([{ name: "physics-coefficients", digest: DIRTY }], [])).toEqual([]);
  });
});

describe("what a deviation means", () => {
  const deviation = asDeviations([{ name: "physics-coefficients", digest: DIRTY }], [region()]);

  it("is warn on its own, because only a rule names a thing", () => {
    // MX Bikes is a modding game: a mod that legitimately rewrites a physics table is
    // indistinguishable here from a trainer that does. Prevalence separates them; a bare
    // deviation must not accuse anyone.
    expect(isUnaccounted(deviation[0])).toBe(true);
    expect(classify(deviation, []).state).toBe("warn");
  });

  it("is alert once a rule denies the digest", () => {
    const rules: ModuleRule[] = [
      { id: 1, kind: "deny", pattern: "", sha256: DIRTY, label: "known trainer", note: "" },
    ];
    const verdict = classify(deviation, rules);
    expect(verdict.state).toBe("alert");
    expect(verdict.matched[0].label).toBe("known trainer");
  });

  it("is silenced once a rule allows the digest", () => {
    // How a popular mod stops being asked about: one allow row, keyed on the digest every
    // install of it produces.
    const rules: ModuleRule[] = [
      { id: 1, kind: "allow", pattern: "", sha256: DIRTY, label: "popular mod", note: "" },
    ];
    expect(classify(deviation, rules).state).toBe("ok");
  });
});

describe("the manifest handed to a client", () => {
  it("carries where to read and never what to expect", () => {
    // A client that knew the baseline could be made to report it instead of what it read,
    // and a `strings` of the binary would hand the same answer to anyone curious.
    const manifest = asManifest([region()]);
    expect(manifest).toEqual([{ name: "physics-coefficients", rva: 0x1000, length: 256 }]);
    expect(JSON.stringify(manifest)).not.toContain(CLEAN);
  });
});
