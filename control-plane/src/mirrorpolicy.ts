/**
 * What the mirror stores: only what is used.
 *
 * The whole mxb-mods catalogue is indexed (metadata, so search covers all of it), but a file is
 * copied into R2 only when one of these says so:
 *
 *  1. **Someone downloads it** through the API (`modapi.ts` `downloadAsset`). The first download
 *     is redirected to the original link and the file is queued; later ones come from the CDN.
 *  2. **It is a track on a live server.** The MXB App's Servers tab asks `GET /v1/tracks` about
 *     the track of every server in the game's master list that the player doesn't have
 *     (`apps/manager/src/Components/Servers/Servers.tsx:493`), and `getTracks` stamps
 *     `track_catalog.requested_at` for each (`trackcatalog.ts`). A row asked about within
 *     `MXB_LIVE_TRACK_DAYS` and matched exactly to an mxb-mods post is a track being hosted now;
 *     its files are pre-mirrored and never evicted while that holds.
 *
 * And not at all when the content is **unsupported**: a livery or bike post filed only under
 * specific bike models (a category name carrying a model year, e.g. "2021 KTM 450 SX-F OEM")
 * none of which any rider has published a paint-sync loadout for within `MXB_BIKE_ACTIVE_DAYS`
 * (`loadouts.bike_id`, `migrations/0006_per_bike_loadouts.sql`, e.g. `MX1OEM_2023_KTM_450_SX-F`).
 * A post with no model term (manufacturer only, or a rider kit, which fits any bike) is
 * supported. `MXB_MIRROR_BIKES_ALLOW` and `MXB_MIRROR_BIKES_DENY` (comma separated names) override
 * the rule either way. An unsupported mod stays searchable; its download goes to the original.
 *
 * Retention: a mirrored blob nobody has downloaded for `MXB_MIRROR_RETAIN_DAYS` is evicted,
 * unless a live-server track uses it. Uploads are never evicted.
 */

const DAY = 24 * 3600_000;
const EVICT_PER_RUN = 200;

function days(v: string | undefined, fallback: number): number {
  const n = Number(v);
  return Number.isFinite(n) && n > 0 ? n : fallback;
}

export const liveTrackDays = (env: Env) => days(env.MXB_LIVE_TRACK_DAYS, 14);
export const retainDays = (env: Env) => days(env.MXB_MIRROR_RETAIN_DAYS, 90);
export const bikeActiveDays = (env: Env) => days(env.MXB_BIKE_ACTIVE_DAYS, 180);

/**
 * A bike's name reduced to what both spellings share: the category "2023 KTM 450 SX-F OEM" and
 * the game folder `MX1OEM_2023_KTM_450_SX-F` both become `2023ktm450sxf`.
 */
export function bikeKey(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/^(mx|en|sm|tr)[0-9e]?oem[_\s-]*/, "")
    .replace(/\boem\b/g, "")
    .replace(/[^a-z0-9]/g, "");
}

/** The model terms among a post's bike terms: the ones naming a model year. */
export function modelTerms(bike: string): string[] {
  return bike
    .split("; ")
    .map((t) => t.trim())
    .filter((t) => /\b(19|20)\d{2}\b/.test(t));
}

function list(v: string | undefined): Set<string> {
  return new Set((v ?? "").split(",").map(bikeKey).filter(Boolean));
}

export interface BikeSet {
  active: Set<string>;
  allow: Set<string>;
  deny: Set<string>;
}

/** The bikes riders actually ride: every loadout published within the window. */
export async function activeBikes(env: Env, now: number): Promise<BikeSet> {
  const { results } = await env.DB.prepare("SELECT DISTINCT bike_id FROM loadouts WHERE bike_id <> '' AND updated_at > ?")
    .bind(now - bikeActiveDays(env) * DAY)
    .all<{ bike_id: string }>();
  return {
    active: new Set(results.map((r) => bikeKey(r.bike_id)).filter(Boolean)),
    allow: list(env.MXB_MIRROR_BIKES_ALLOW),
    deny: list(env.MXB_MIRROR_BIKES_DENY),
  };
}

/** Whether a mirrored post may be copied into R2, and why not. */
export function supported(asset: { type: string; bike: string }, bikes: BikeSet): { ok: boolean; reason: string } {
  const terms = asset.bike ? asset.bike.split("; ").map(bikeKey).filter(Boolean) : [];
  if (terms.some((t) => bikes.deny.has(t))) return { ok: false, reason: "on the deny list" };
  if (terms.some((t) => bikes.allow.has(t))) return { ok: true, reason: "on the allow list" };
  if (asset.type !== "liveries" && asset.type !== "bikes") return { ok: true, reason: "not tied to a bike model" };
  const models = modelTerms(asset.bike).map(bikeKey);
  if (models.length === 0) return { ok: true, reason: "no bike model named" };
  if (models.some((m) => bikes.active.has(m))) return { ok: true, reason: "a rider uses this bike" };
  return { ok: false, reason: "no rider has used this bike recently" };
}

/** Track posts that are on a live server right now, by mxb-mods slug. */
const LIVE_TRACKS = `SELECT a.id FROM mod_assets a JOIN track_catalog t ON t.slug = a.slug
  WHERE a.source = 'mirror' AND a.type = 'tracks' AND t.source = 'mods' AND t.exact = 1 AND t.requested_at > ?`;

/** Queue every not-yet-mirrored file of every live-server track. Returns how many were marked. */
export async function wantLiveTracks(env: Env, now: number): Promise<number> {
  const res = await env.DB.prepare(
    `UPDATE mod_files SET status = 'pending', due_at = 0
     WHERE status = 'idle' AND version_id IN (
       SELECT a.current_version FROM mod_assets a WHERE a.state = 'active' AND a.id IN (${LIVE_TRACKS}))`,
  )
    .bind(now - liveTrackDays(env) * DAY)
    .run();
  return res.meta.changes ?? 0;
}

/**
 * Evict mirrored blobs nobody has downloaded within the retention window. A blob any upload
 * uses, or any live-server track uses, stays. The files that pointed at it go back to `idle`,
 * so the next download mirrors them again.
 */
export async function evictUnused(env: Env, now: number): Promise<number> {
  const cutoff = now - retainDays(env) * DAY;
  const { results } = await env.DB.prepare(
    `SELECT b.sha256, b.bucket, b.r2_key FROM mod_blobs b
     WHERE COALESCE(b.last_used_at, b.first_seen) < ?1
       AND NOT EXISTS (SELECT 1 FROM mod_files f JOIN mod_versions v ON v.id = f.version_id
                       JOIN mod_assets a ON a.id = v.asset_id
                       WHERE f.sha256 = b.sha256 AND a.source = 'upload')
       AND NOT EXISTS (SELECT 1 FROM mod_assets a WHERE a.thumb_sha = b.sha256)
       AND NOT EXISTS (SELECT 1 FROM mod_files f JOIN mod_assets a ON a.current_version = f.version_id
                       WHERE f.sha256 = b.sha256 AND a.id IN (${LIVE_TRACKS.replace("?", "?2")}))
     LIMIT ?3`,
  )
    .bind(cutoff, now - liveTrackDays(env) * DAY, EVICT_PER_RUN)
    .all<{ sha256: string; bucket: string; r2_key: string }>();
  for (const b of results) {
    await (b.bucket === "private" ? env.ASSET_LOCKED : env.ASSET_MIRROR).delete(b.r2_key);
    await env.DB.batch([
      env.DB.prepare(
        "UPDATE mod_files SET status = 'idle', sha256 = NULL, filename = NULL, attempts = 0, error = NULL WHERE sha256 = ?",
      ).bind(b.sha256),
      env.DB.prepare("DELETE FROM mod_blobs WHERE sha256 = ?").bind(b.sha256),
    ]);
  }
  return results.length;
}

/** A download was served from the mirror: keep it. Written at most daily per blob. */
export async function touchBlob(env: Env, sha: string, now: number): Promise<void> {
  await env.DB.prepare("UPDATE mod_blobs SET last_used_at = ? WHERE sha256 = ? AND COALESCE(last_used_at, 0) < ?")
    .bind(now, sha, now - DAY)
    .run();
}
