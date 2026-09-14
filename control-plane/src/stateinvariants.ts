/**
 * What the game's own memory says about itself.
 *
 * The rest of diagnostics identifies foreign code by what it *is* — a file hash, a region
 * fingerprint, a thread that started nowhere. This is the one part that describes what that
 * code *changed*, and it is the only signal in the pipeline that fires on a trainer nobody
 * has ever hashed: a build with no corpus entry that writes one physics coefficient and
 * unloads is otherwise a clean report.
 *
 * The shape is deliberately the same as everything else here:
 *
 *   * The server says which bytes to hash (`state_regions`, keyed on which build of the game
 *     is running). The client hashes them and reports digests. It is told nothing about what
 *     any region should contain — the baseline lives in the table, so the shipped binary
 *     still looks for nothing.
 *   * A digest equal to the baseline produces **no observation at all**. That is the
 *     overwhelmingly common case and it must not write a row per player per report.
 *   * A digest that differs becomes a `ReportedModule` with origin `state`, so `classify`,
 *     `module_rules`, the prevalence column and the search all read it without knowing it
 *     was ever a different kind of thing. Migration 0020 did this for memory regions; this
 *     is the same trick applied to a number instead of a run of bytes.
 *
 * That last point is what makes a deviation `warn` rather than `alert`: `isUnaccounted` sends
 * it to `unknown`, and only a rule promotes it. Which is the right answer, because MX Bikes is
 * a modding game and a mod that legitimately rewrites a physics table looks exactly like a
 * trainer that does. Prevalence is what separates them — a digest three hundred accounts
 * share is a popular mod and earns one `allow` row; a digest one account has is the one worth
 * reading.
 */

import type { ReportedModule } from "./diagnostics";

/** A region the client was asked to hash, as the manifest hands it over. */
export interface StateRegion {
  name: string;
  rva: number;
  length: number;
}

/** A region plus the answer a clean install gives, as the comparison needs it. */
export interface StateBaseline extends StateRegion {
  baseline: string;
}

/** One region's digest, as a client reports it. */
export interface ReportedDigest {
  name: string;
  digest: string;
}

/**
 * Bigger than any plausible manifest. A client is told what to hash, so this bounds our own
 * table rather than a stranger's input — but the report is bounded by it too, and that is
 * the direction that matters.
 */
export const MAX_STATE_REGIONS = 32;

/** Hex, and long enough to be a digest rather than a number someone typed. */
const DIGEST_SHAPE = /^[a-f0-9]{16,128}$/;

/** A region name as it may be written: the same shape a module name has to satisfy. */
const NAME_SHAPE = /^[a-z0-9._-]{1,48}$/;

/**
 * Read the digests a client reported, or `null` if that is not what they are.
 *
 * Absent is not malformed: an app too old to have been asked sends nothing, and that is the
 * ordinary case for as long as it takes a release to go out. Present and wrong is refused the
 * same way a bad module list is — this becomes rows in a table and text on an admin's page.
 */
export function parseDigests(value: unknown): ReportedDigest[] | null {
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value)) return null;
  if (value.length > MAX_STATE_REGIONS) return null;

  const out: ReportedDigest[] = [];
  const seen = new Set<string>();
  for (const entry of value) {
    if (!entry || typeof entry !== "object") return null;
    const { name, digest } = entry as Record<string, unknown>;
    if (typeof name !== "string" || typeof digest !== "string") return null;

    const lowerName = name.toLowerCase();
    const lowerDigest = digest.toLowerCase();
    if (!NAME_SHAPE.test(lowerName)) return null;
    if (!DIGEST_SHAPE.test(lowerDigest)) return null;
    // A report naming the same region twice is a client bug, and taking either answer would
    // hide which one. Refuse it rather than pick.
    if (seen.has(lowerName)) return null;
    seen.add(lowerName);

    out.push({ name: lowerName, digest: lowerDigest });
  }
  return out;
}

/** What a row carries when it describes a number rather than a file. */
const NOT_A_FILE = {
  size: 0,
  mtime: 0,
  trust: "unchecked" as const,
  publisher: "",
  company: "",
  product: "",
  description: "",
};

/**
 * Turn the digests that differ from their baseline into rows the rest of the pipeline reads.
 *
 * Only differences come back. A region that matches is the whole point of having a baseline:
 * it means there is nothing to say, and saying it anyway would write a row for every player
 * on every report and bury the handful that matter.
 *
 * A digest for a region we did not ask about is dropped rather than stored. It can only come
 * from a client running a manifest we have since changed, and inventing a row for a region
 * whose baseline we no longer hold would be an observation nobody could act on.
 */
export function asDeviations(
  digests: ReportedDigest[],
  regions: StateBaseline[],
): ReportedModule[] {
  const byName = new Map(regions.map((r) => [r.name, r]));
  const out: ReportedModule[] = [];

  for (const reported of digests) {
    const region = byName.get(reported.name);
    if (!region) continue;
    if (region.baseline.toLowerCase() === reported.digest) continue;

    out.push({
      // Prefixed so it reads on the page as what it is, and so it cannot collide with a file
      // name — `state.` is not a shape a DLL name takes.
      name: `state.${region.name}`,
      // Never a value a client can send: `parseModules` will not accept this origin, so a
      // row with it can only have been constructed here, from a region we asked about.
      origin: "state",
      // The digest goes in the hash column because that is the column rules match on and
      // prevalence groups by. A rule naming this digest reads exactly like one naming a file.
      sha256: reported.digest,
      ...NOT_A_FILE,
      detail: `game state differs from baseline (${region.name})`,
    });
  }
  return out;
}

/**
 * The regions to ask a given build of the game for.
 *
 * An unrecognised build has no rows, so this answers empty — the client hashes nothing and
 * the report carries no digests. That is the correct answer to "we have not baselined this
 * version yet", and it is why an MX Bikes patch does not alert on every player the day it
 * lands. Retired regions are excluded for the same reason they are retired rather than
 * deleted: a digest already stored can still be explained, but nobody is asked for it again.
 */
export async function loadStateRegions(
  db: D1Database,
  buildFp: string,
): Promise<StateBaseline[]> {
  if (!buildFp) return [];
  const rows = await db
    .prepare(
      "SELECT name, rva, length, baseline FROM state_regions" +
        " WHERE build_fp = ? AND active = 1 ORDER BY name LIMIT ?",
    )
    .bind(buildFp, MAX_STATE_REGIONS)
    .all<{ name: string; rva: number; length: number; baseline: string }>();

  return (rows.results ?? []).map((r) => ({
    name: r.name,
    rva: r.rva,
    length: r.length,
    baseline: (r.baseline ?? "").toLowerCase(),
  }));
}

/** The manifest as the client receives it: where to read, never what to expect. */
export function asManifest(regions: StateBaseline[]): StateRegion[] {
  return regions.map((r) => ({ name: r.name, rva: r.rva, length: r.length }));
}

/**
 * Serve the manifest for the build a client says it is running.
 *
 * Deliberately carries no baseline. A client that learned what a region is supposed to
 * contain could be made to report that instead of what it read, which would turn the whole
 * check into an honour system — and a `strings` of the binary would hand the same answer to
 * anyone curious. So the wire carries where to read and nothing else.
 *
 * An unknown or missing build answers `[]` rather than an error: the client has nothing to
 * hash, the report carries no digests, and the day MX Bikes patches every player quietly
 * stops being asked instead of every player alerting.
 */
export async function stateRegions(request: Request, env: { DB: D1Database }): Promise<Response> {
  const build = new URL(request.url).searchParams.get("build") ?? "";
  // The fingerprint is a client-supplied string that reaches a query, so it is shaped before
  // it gets there. Anything else is not a build we could hold baselines for anyway.
  const regions = /^[a-z0-9]{1,64}$/.test(build.toLowerCase())
    ? await loadStateRegions(env.DB, build.toLowerCase())
    : [];
  return new Response(JSON.stringify({ regions: asManifest(regions) }), {
    status: 200,
    headers: { "content-type": "application/json; charset=utf-8" },
  });
}
