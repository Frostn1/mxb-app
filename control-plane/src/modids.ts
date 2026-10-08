/**
 * How a mod is named outside D1: a random UUID (v4), never the autoincrement `mod_assets.id`,
 * which anyone could count through, unlisted mods included. Pages read
 * mxbsecure.com/mods/<slug>-<uuid>; the slug is decoration and the UUID alone finds the mod.
 */

export const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/;
const UUID_ONLY = new RegExp(`^${UUID_RE.source}$`);

export function newPublicId(): string {
  return crypto.randomUUID();
}

export function isPublicId(s: string): boolean {
  return UUID_ONLY.test(s);
}

/** "2026 RedBull KTM — Factory!" → "2026-redbull-ktm-factory". At most 60 characters. */
export function modSlug(title: string): string {
  const s = title
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/, "");
  return s || "mod";
}

/** The site's address for a mod. */
export function modPath(title: string, publicId: string): string {
  return `/mods/${modSlug(title)}-${publicId}`;
}

/** The internal id behind a public one, or null. */
export async function internalId(env: Env, publicId: string): Promise<number | null> {
  const id = publicId.toLowerCase();
  if (!isPublicId(id)) return null;
  const row = await env.DB.prepare("SELECT id FROM mod_assets WHERE public_id = ?").bind(id).first<{ id: number }>();
  return row?.id ?? null;
}
