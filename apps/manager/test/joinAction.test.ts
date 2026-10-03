import { describe, expect, test } from "bun:test";
import type { CatalogTrack } from "@frost/shared/api/mods";
import { joinAction, type JoinInputs } from "../src/Components/Servers/joinAction";

const base: JoinInputs = {
  missing: false,
  installing: false,
  queued: false,
  joinable: true,
  full: false,
};

const free = { source: "mods", exact: true, name: "Fakey Mx – FMX – Somewhere", slug: "fakey" } as CatalogTrack;
const sold = { source: "shop", exact: false, name: "2026 FAKE SX ROUND 17", url: "https://x" } as CatalogTrack;

describe("joinAction", () => {
  test("an installed track (exact id match) is a plain join", () => {
    expect(joinAction(base).kind).toBe("join");
    // A catalogue product for an installed track never turns it into a download.
    expect(joinAction({ ...base, product: free }).kind).toBe("join");
  });

  test("a missing track with nothing to install is not a plain join", () => {
    // The RD01 server whose catalogue guess was a different round: missing, no product.
    expect(joinAction({ ...base, missing: true }).kind).toBe("missing");
  });

  test("a missing track offers its install or its shop page", () => {
    expect(joinAction({ ...base, missing: true, product: free }).kind).toBe("install");
    expect(joinAction({ ...base, missing: true, product: sold }).kind).toBe("buy");
  });

  test("a parked track is switched on, never downloaded", () => {
    const parked = "mods/tracks/supercross/Fakey Mx - FMX - Somewhere.pkz";
    expect(joinAction({ ...base, missing: true, product: free, inactive: parked })).toEqual({
      kind: "activate",
      rel: parked,
    });
  });

  test("queue, install and full keep their precedence", () => {
    expect(joinAction({ ...base, queued: true, missing: true }).kind).toBe("queued");
    expect(joinAction({ ...base, installing: true, missing: true }).kind).toBe("installing");
    expect(joinAction({ ...base, full: true }).kind).toBe("wait");
    expect(joinAction({ ...base, full: true, joinable: false }).kind).toBe("join");
  });
});
