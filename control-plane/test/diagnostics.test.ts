import { describe, expect, it } from "vitest";
import {
  classify,
  isUnaccounted,
  MAX_MODULES,
  parseModules,
  stateRank,
  type ModuleRule,
  type Origin,
  type ReportedModule,
} from "../src/diagnostics";

/** What a module with nothing read off it looks like — the shape an older client sends. */
const BLANK = {
  size: 0,
  mtime: 0,
  trust: "unchecked" as const,
  publisher: "",
  company: "",
  product: "",
  description: "",
};

function mod(
  name: string,
  origin: Origin,
  sha256 = "",
  extra: Partial<ReportedModule> = {},
): ReportedModule {
  return { name, origin, sha256, ...BLANK, ...extra };
}

function rule(kind: "deny" | "allow", by: { pattern?: string; sha256?: string }): ModuleRule {
  return { id: 1, kind, pattern: by.pattern ?? "", sha256: by.sha256 ?? "", label: "named" };
}

const HASH = "a".repeat(64);
const OTHER_HASH = "b".repeat(64);

describe("reading a reported module list", () => {
  it("takes an ordinary one", () => {
    const list = parseModules([
      { name: "MXBikes.exe", origin: "game", sha256: HASH },
      { name: "kernel32.dll", origin: "system" },
    ]);
    expect(list).toEqual([
      mod("mxbikes.exe", "game", HASH),
      mod("kernel32.dll", "system"),
    ]);
  });

  it("takes what a file says about itself, and bounds every word of it", () => {
    const list = parseModules([
      {
        name: "overlay.dll",
        origin: "other",
        sha256: HASH,
        size: 2_400_000,
        mtime: 1_750_000_000,
        trust: "signed",
        publisher: "NVIDIA Corporation",
        company: "NVIDIA Corporation",
        product: "NVIDIA Share",
        description: "NVIDIA Share overlay\u0000\n",
      },
    ]);
    expect(list?.[0]).toEqual(
      mod("overlay.dll", "other", HASH, {
        size: 2_400_000,
        mtime: 1_750_000_000,
        trust: "signed",
        publisher: "NVIDIA Corporation",
        company: "NVIDIA Corporation",
        product: "NVIDIA Share",
        // The control characters are gone: this is text off a file, rendered on a page.
        description: "NVIDIA Share overlay",
      }),
    );
  });

  it("keeps a report a client sent nonsense detail in, and drops the nonsense", () => {
    // A report thrown away says nothing at all, which is worse than one that says less. Only
    // the fields that failed are dropped, and each falls back to "we do not know".
    const list = parseModules([
      {
        name: "x.dll",
        origin: "other",
        size: -5,
        mtime: 10,
        trust: "definitely-fine",
        company: 42,
      },
    ]);
    expect(list?.[0]).toEqual(mod("x.dll", "other"));
  });

  it("reads an older client's report, which carries none of this", () => {
    const list = parseModules([{ name: "x.dll", origin: "other", sha256: HASH }]);
    expect(list?.[0]).toEqual(mod("x.dll", "other", HASH));
    expect(list?.[0].trust).toBe("unchecked");
  });

  it("refuses a path where a file name belongs", () => {
    // The name is rendered on the admin page and stored as a column. A path would carry the
    // player's user folder into both, which is exactly what this must never collect.
    for (const bad of [
      "c:/users/rider/cheats/x.dll",
      "..\\x.dll",
      "sub/dir.dll",
      "my file.dll",
    ]) {
      expect(parseModules([{ name: bad, origin: "other" }]), bad).toBeNull();
    }
  });

  it("refuses anything that is not a module list", () => {
    expect(parseModules("nope")).toBeNull();
    expect(parseModules([{ name: "x.dll" }])).toBeNull();
    expect(parseModules([{ name: "x.dll", origin: "everywhere" }])).toBeNull();
    expect(parseModules([{ name: "x.dll", origin: "other", sha256: "short" }])).toBeNull();
    expect(parseModules([{ name: "x".repeat(200), origin: "other" }])).toBeNull();
  });

  it("caps the list, so one client cannot write a thousand rows", () => {
    const many = Array.from({ length: MAX_MODULES + 1 }, (_, i) => ({
      name: `m${i}.dll`,
      origin: "system" as const,
    }));
    expect(parseModules(many)).toBeNull();
    expect(parseModules(many.slice(0, MAX_MODULES))).toHaveLength(MAX_MODULES);
  });

  it("drops a repeat of the same file rather than storing it twice", () => {
    const list = parseModules([
      { name: "x.dll", origin: "other", sha256: HASH },
      { name: "X.DLL", origin: "other", sha256: HASH.toUpperCase() },
    ]);
    expect(list).toHaveLength(1);
  });
});

describe("what the rules make of a list", () => {
  it("accounts for a plain machine once the game's own files are cleared", () => {
    // The system's libraries and our own install folder account for themselves. The game's
    // do not any more — they are cleared once, by hash, and then a plain machine is quiet.
    const machine = [
      mod("mxbikes.exe", "game", HASH),
      mod("kernel32.dll", "system"),
      mod("frostmod.dll", "app"),
    ];
    const v = classify(machine, [rule("allow", { sha256: HASH })]);
    expect(v.state).toBe("ok");
    expect(v.unknown).toHaveLength(0);
  });

  it("does not need a rule to notice something loaded from outside", () => {
    // The heuristic is what works on day one, before anything has been named. A deployment
    // with an empty rule list still sees the shape.
    const v = classify([mod("something.dll", "other", HASH)], []);
    expect(v.state).toBe("warn");
    expect(v.unknown.map((m) => m.name)).toEqual(["something.dll"]);
    expect(v.matched).toHaveLength(0);
  });

  it("names a file by hash wherever it loaded from", () => {
    // The point of hashing: copying itself into the game's own folder must not buy trust.
    const v = classify([mod("innocent.dll", "game", HASH)], [rule("deny", { sha256: HASH })]);
    expect(v.state).toBe("alert");
    expect(v.matched).toEqual([{ name: "innocent.dll", sha256: HASH, label: "named" }]);
  });

  it("names a file by a substring of its name", () => {
    const v = classify([mod("trainer_v3.dll", "other")], [rule("deny", { pattern: "trainer" })]);
    expect(v.state).toBe("alert");
  });

  it("lets an allow rule silence a false positive", () => {
    const overlay = [mod("someoverlay.dll", "other", HASH)];
    expect(classify(overlay, []).state).toBe("warn");
    expect(classify(overlay, [rule("allow", { sha256: HASH })]).state).toBe("ok");
  });

  it("will not let an allow rule clear something a deny rule named", () => {
    // Order is the policy: allow exists to quiet a heuristic, never to un-name a match.
    const v = classify(
      [mod("x.dll", "other", HASH)],
      [rule("deny", { sha256: HASH }), rule("allow", { pattern: "x.dll" })],
    );
    expect(v.state).toBe("alert");
  });

  it("treats a loader name in the game's own folder as unaccounted for", () => {
    // Nothing has to inject a d3d9.dll sitting beside the exe — the loader does it. Being in
    // the game folder is the observation here, not the defence.
    const v = classify([mod("d3d9.dll", "game", HASH)], []);
    expect(v.state).toBe("warn");
    expect(v.unknown.map((m) => m.name)).toEqual(["d3d9.dll"]);
  });

  it("asks about the game's folder whatever the file is called", () => {
    // The hole the first version left: the loader-name list was the only thing that looked
    // inside the game's folder, so anything injected under an ordinary name read as
    // accounted for purely because of where it sat.
    const v = classify([mod("physx_helper.dll", "game", HASH)], []);
    expect(v.state).toBe("warn");
    expect(v.unknown.map((m) => m.name)).toEqual(["physx_helper.dll"]);
  });

  it("still accounts for the system's own libraries by where they are", () => {
    // A Windows system folder is not writable without already owning the machine, and there
    // are hundreds of them. Asking about those would bury everything worth reading.
    expect(classify([mod("kernel32.dll", "system")], []).state).toBe("ok");
    expect(isUnaccounted(mod("kernel32.dll", "system"))).toBe(false);
  });

  it("accounts for what the app installed, unless it took a loader's name", () => {
    // We put every file in that folder there ourselves — except that a loader name sitting
    // in it is worth the same question it is worth anywhere else.
    expect(isUnaccounted(mod("frostmod.dll", "app"))).toBe(false);
    expect(isUnaccounted(mod("dxgi.dll", "app"))).toBe(true);
  });

  it("clears a stock game library once its hash is allowed", () => {
    // What the change costs: the game's own libraries arrive unaccounted for on day one and
    // are cleared once, by hash, from the page. After that they never come back.
    const stock = [mod("mxbikes.exe", "game", HASH)];
    expect(classify(stock, []).state).toBe("warn");
    expect(classify(stock, [rule("allow", { sha256: HASH })]).state).toBe("ok");
  });

  it("clears a loader name once its exact file is allowed", () => {
    // ReShade installs as opengl32.dll in an OpenGL title, which MX Bikes is. Allowing the
    // hash clears that build everywhere without clearing the name for anyone else.
    const reshade = [mod("opengl32.dll", "game", HASH)];
    expect(classify(reshade, []).state).toBe("warn");
    expect(classify(reshade, [rule("allow", { sha256: HASH })]).state).toBe("ok");
    expect(classify([mod("opengl32.dll", "game", OTHER_HASH)], [rule("allow", { sha256: HASH })]).state)
      .toBe("warn");
  });

  it("reports the worst thing it found, not the last", () => {
    const v = classify(
      [mod("unknown.dll", "other"), mod("bad.dll", "other", HASH), mod("kernel32.dll", "system")],
      [rule("deny", { sha256: HASH })],
    );
    expect(v.state).toBe("alert");
    expect(v.matched).toHaveLength(1);
    expect(v.unknown).toHaveLength(1);
  });
});

describe("state ordering", () => {
  it("puts 'could not look' below 'looked and it was fine'", () => {
    // The one that matters: a client that could not read the module list must never sort or
    // carry forward as though it had, and must never overwrite a worse answer.
    expect(stateRank("unknown")).toBeLessThan(stateRank("ok"));
    expect(stateRank("ok")).toBeLessThan(stateRank("warn"));
    expect(stateRank("warn")).toBeLessThan(stateRank("alert"));
    expect(stateRank("nonsense")).toBe(0);
  });
});
