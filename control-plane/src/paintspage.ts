/**
 * What paint sync is actually holding, and who put it there.
 *
 * Everything else in the control plane treats a paint as a digest — which is right for
 * moving bytes between riders and leaves nobody able to answer the two questions worth
 * asking about it: who has published a look, and what does what we are shipping to a grid
 * actually look like. The site's paint dashboard asks both through `webadmin.ts`, behind
 * the Steam admin sign-in: rider names, GUIDs and Steam ids are in every answer.
 */

import { likeTerm, MAX_COUNT, PAGE_SIZE, parsePage, type Paged } from "./adminui";
import { imageTable, pickImage, type PntImage } from "./pntthumb";
import { PRESENCE_TTL_MS } from "./validate";

/** How many riders one paint's page lists. Past this it is a popular paint, not a list. */
const MAX_WEARERS = 200;

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

export interface RiderRow {
  id: string;
  rider_name: string;
  guid: string | null;
  steam_id: string | null;
  kind: string;
  bikes: number;
  files: number;
  slots: number;
  bytes: number;
  published_at: number | null;
  at_server: string | null;
  /** The diagnostics half of the same account. Empty when they have never reported. */
  state: string | null;
  reported_at: number | null;
}

export interface PaintRow {
  sha256: string;
  file_name: string;
  names: number;
  riders: number;
  size: number;
  uses: number;
  slots: string;
  /** What R2 has under that digest, or null if the blob was never uploaded. */
  stored: number | null;
}

export interface SlotRow {
  bike_id: string;
  slot: string;
  file_name: string;
  sha256: string;
  size: number;
  rel_dest: string;
}

export interface Totals {
  riders: number;
  paints: number;
  slots: number;
  today: number;
  present: number;
  stored: number;
}

// ---------------------------------------------------------------------------
// Sorting
// ---------------------------------------------------------------------------

/** Which way a column reads. Two words, and the query string may name no others. */
export type Dir = "asc" | "desc";

export function parseDir(value: string | null, fallback: Dir): Dir {
  return value === "asc" || value === "desc" ? value : fallback;
}

interface Column {
  label: string;
  /** Right-aligned, as every number in these tables is. */
  num?: boolean;
  /** Which way this column is worth reading first — names up, quantities and dates down. */
  first: Dir;
  /** The ORDER BY fragment. A fixed expression and one of the two words above; nothing a
   *  caller typed ever reaches it. */
  order(dir: Dir): string;
}

/** Text, with the empties at the bottom whichever way the column is read. */
function byText(expr: string): Column["order"] {
  return (dir) => `(${expr} IS NULL OR ${expr} = ''), ${expr} COLLATE NOCASE ${dir.toUpperCase()}`;
}

/** A count, a size or a timestamp, with the absent ones at the bottom either way. */
function byNumber(expr: string): Column["order"] {
  return (dir) => `${expr} IS NULL, ${expr} ${dir.toUpperCase()}`;
}

/**
 * The columns of the rider table.
 *
 * Aggregates are named by their expression rather than by the alias they are selected under:
 * `size` is also a column of `loadout_paints`, and a bare alias in ORDER BY is one rename
 * away from quietly sorting by the wrong thing. The two correlated subqueries are the
 * exception — nothing else is called `published_at` or `at_server`, and repeating a subquery
 * would mean re-binding its parameter.
 */
export const RIDER_COLUMNS: Record<string, Column> = {
  name: { label: "Rider", first: "asc", order: byText("a.rider_name") },
  guid: { label: "GUID", first: "asc", order: byText("a.guid") },
  steam: { label: "Steam", first: "asc", order: byText("a.steam_id") },
  bikes: { label: "Bikes", num: true, first: "desc", order: byNumber("COUNT(DISTINCT p.bike_id)") },
  slots: { label: "Slots", num: true, first: "desc", order: byNumber("COUNT(*)") },
  paints: {
    label: "Paints",
    num: true,
    first: "desc",
    order: byNumber("COUNT(DISTINCT p.sha256)"),
  },
  size: { label: "Size", num: true, first: "desc", order: byNumber("SUM(p.size)") },
  published: { label: "Published", first: "desc", order: byNumber("published_at") },
  where: { label: "Where", first: "asc", order: byText("at_server") },
  // The other dashboard, sorted by how bad it is rather than alphabetically: the reason to
  // sort by this column is to bring the alerts to the top.
  reported: { label: "Reported", first: "desc", order: byNumber("state_rank") },
};

export const PAINT_COLUMNS: Record<string, Column> = {
  file: { label: "File", first: "asc", order: byText("MIN(p.file_name)") },
  slots: { label: "Slots", first: "asc", order: byText("GROUP_CONCAT(DISTINCT p.slot)") },
  riders: {
    label: "Riders",
    num: true,
    first: "desc",
    order: byNumber("COUNT(DISTINCT p.account_id)"),
  },
  uses: { label: "Uses", num: true, first: "desc", order: byNumber("COUNT(*)") },
  size: { label: "Size", num: true, first: "desc", order: byNumber("MAX(p.size)") },
  digest: { label: "Digest", first: "asc", order: byText("p.sha256") },
};

/** How a table is being read: a column that exists, and a direction. */
export interface Order {
  sort: string;
  dir: Dir;
}

/**
 * The column asked for, or the one the table opens on.
 *
 * The name is looked up in the map rather than trusted — an unknown one is a hand-edited URL
 * or a stale link, and either way the answer is the default rather than an error.
 */
export function parseOrder(url: URL, columns: Record<string, Column>, fallback: string): Order {
  const asked = url.searchParams.get("sort") ?? "";
  const sort = asked in columns ? asked : fallback;
  return { sort, dir: parseDir(url.searchParams.get("dir"), columns[sort].first) };
}

/**
 * The ORDER BY for a table, with a tie-break that never changes.
 *
 * Without the tie-break, rows that match on the sorted column come back in whatever order
 * the query planner felt like — which is invisible on one page and duplicates or drops rows
 * across two.
 */
export function orderBy(columns: Record<string, Column>, order: Order, tiebreak: string): string {
  return `${columns[order.sort].order(order.dir)}, ${tiebreak}`;
}

// ---------------------------------------------------------------------------
// What each view reads
// ---------------------------------------------------------------------------

export interface RidersData {
  totals: Totals;
  found: Paged<RiderRow>;
  q: string;
  order: Order;
}

export async function ridersData(env: Env, url: URL): Promise<RidersData> {
  const q = (url.searchParams.get("q") ?? "").slice(0, 96);
  const order = parseOrder(url, RIDER_COLUMNS, "published");
  const page = parsePage(url.searchParams.get("page"));
  const [sums, found] = await Promise.all([totals(env), searchRiders(env, q, order, page)]);
  return { totals: sums, found, q, order };
}

export interface OneRiderData {
  account: { id: string; rider_name: string; guid: string | null; steam_id: string | null; kind: string; created_at: number };
  slots: SlotRow[];
  /** When each bike's loadout was last published. */
  published: { bike_id: string; updated_at: number }[];
  presence: { server_id: string; updated_at: number } | null;
}

export async function oneRiderData(env: Env, id: string): Promise<OneRiderData | null> {
  const account = await env.DB.prepare(
    "SELECT id, rider_name, guid, steam_id, kind, created_at FROM accounts WHERE id = ?",
  )
    .bind(id)
    .first<{
      id: string;
      rider_name: string;
      guid: string | null;
      steam_id: string | null;
      kind: string;
      created_at: number;
    }>();
  if (!account) return null;

  const [slots, published, presence] = await Promise.all([
    env.DB.prepare(
      "SELECT bike_id, slot, file_name, sha256, size, rel_dest FROM loadout_paints" +
        " WHERE account_id = ? ORDER BY bike_id, slot",
    )
      .bind(id)
      .all<SlotRow>(),
    env.DB.prepare("SELECT bike_id, updated_at FROM loadouts WHERE account_id = ?")
      .bind(id)
      .all<{ bike_id: string; updated_at: number }>(),
    env.DB.prepare("SELECT server_id, updated_at FROM presence WHERE account_id = ?")
      .bind(id)
      .first<{ server_id: string; updated_at: number }>(),
  ]);

  return { account, slots: slots.results ?? [], published: published.results ?? [], presence };
}

export interface FilesData {
  totals: Totals;
  found: Paged<PaintRow>;
  q: string;
  order: Order;
}

export async function filesData(env: Env, url: URL): Promise<FilesData> {
  const q = (url.searchParams.get("q") ?? "").slice(0, 96);
  const order = parseOrder(url, PAINT_COLUMNS, "riders");
  const page = parsePage(url.searchParams.get("page"));
  const [sums, found] = await Promise.all([totals(env), searchPaints(env, q, order, page)]);
  return { totals: sums, found, q, order };
}

export interface Wearer {
  id: string;
  rider_name: string;
  guid: string | null;
  bike_id: string;
  slot: string;
  file_name: string;
  size: number;
  rel_dest: string;
}

export interface OnePaintData {
  sha: string;
  wearers: Wearer[];
  /** What R2 holds under that digest, or null if the blob was never uploaded. */
  stored: number | null;
  sheets: { images: PntImage[]; chosen: number } | null;
}

export async function onePaintData(env: Env, sha: string): Promise<OnePaintData | null> {
  if (!/^[0-9a-f]{64}$/.test(sha)) return null;

  const [wearers, stored, sheets] = await Promise.all([
    env.DB.prepare(
      "SELECT a.id, a.rider_name, a.guid, p.bike_id, p.slot, p.file_name, p.size, p.rel_dest" +
        " FROM loadout_paints p JOIN accounts a ON a.id = p.account_id" +
        " WHERE p.sha256 = ? ORDER BY a.rider_name, p.bike_id, p.slot LIMIT ?",
    )
      .bind(sha, MAX_WEARERS)
      .all<{
        id: string;
        rider_name: string;
        guid: string | null;
        bike_id: string;
        slot: string;
        file_name: string;
        size: number;
        rel_dest: string;
      }>(),
    env.PAINTS.head(sha),
    sheetsOf(sha, env),
  ]);

  const rows = wearers.results ?? [];
  if (rows.length === 0 && !stored) return null;
  return { sha, wearers: rows, stored: stored?.size ?? null, sheets };
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

async function totals(env: Env): Promise<Totals> {
  const now = Date.now();
  const row = await env.DB.prepare(
    "SELECT" +
      " (SELECT COUNT(DISTINCT account_id) FROM loadout_paints) AS riders," +
      " (SELECT COUNT(DISTINCT sha256) FROM loadout_paints) AS paints," +
      " (SELECT COUNT(*) FROM loadout_paints) AS slots," +
      " (SELECT COUNT(DISTINCT account_id) FROM loadouts WHERE updated_at > ?) AS today," +
      " (SELECT COUNT(*) FROM presence WHERE updated_at > ?) AS present," +
      // Per digest, not per row: the same paint on four bikes is stored once.
      " (SELECT COALESCE(SUM(size), 0) FROM (SELECT MAX(size) AS size FROM loadout_paints" +
      "   GROUP BY sha256)) AS stored",
  )
    .bind(now - 86_400_000, now - PRESENCE_TTL_MS)
    .first<Totals>();
  return row ?? { riders: 0, paints: 0, slots: 0, today: 0, present: 0, stored: 0 };
}

/** The filter both the rider list and its count use, so they can never disagree. */
const RIDER_MATCH =
  " (? = '' OR a.rider_name LIKE ? ESCAPE '\\' OR COALESCE(a.guid, '') LIKE ? ESCAPE '\\'" +
  " OR COALESCE(a.steam_id, '') LIKE ? ESCAPE '\\')";

async function searchRiders(
  env: Env,
  q: string,
  order: Order,
  page: number,
): Promise<Paged<RiderRow>> {
  const like = likeTerm(q);
  const fresh = Date.now() - PRESENCE_TTL_MS;

  const rows = await env.DB.prepare(
    "SELECT a.id, a.rider_name, a.guid, a.steam_id, a.kind," +
      " COUNT(DISTINCT p.bike_id) AS bikes, COUNT(DISTINCT p.sha256) AS files," +
      " COUNT(*) AS slots, SUM(p.size) AS bytes," +
      " (SELECT MAX(l.updated_at) FROM loadouts l WHERE l.account_id = a.id) AS published_at," +
      " (SELECT pr.server_id FROM presence pr WHERE pr.account_id = a.id AND pr.updated_at > ?)" +
      "   AS at_server," +
      // The join the two dashboards never had. Both key on the account, so what a rider is
      // wearing and what their game has loaded are one row rather than two searches.
      " m.state, m.updated_at AS reported_at," +
      " CASE m.state WHEN 'alert' THEN 3 WHEN 'warn' THEN 2 WHEN 'ok' THEN 1" +
      "   WHEN 'unknown' THEN 0 ELSE NULL END AS state_rank" +
      " FROM accounts a JOIN loadout_paints p ON p.account_id = a.id" +
      " LEFT JOIN client_modules m ON m.account_id = a.id" +
      " WHERE" +
      RIDER_MATCH +
      ` GROUP BY a.id ORDER BY ${orderBy(RIDER_COLUMNS, order, "a.rider_name COLLATE NOCASE")}` +
      " LIMIT ? OFFSET ?",
  )
    .bind(fresh, like, like, like, like, PAGE_SIZE, (page - 1) * PAGE_SIZE)
    .all<RiderRow>();

  const total = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM (SELECT p.account_id FROM loadout_paints p" +
      " JOIN accounts a ON a.id = p.account_id WHERE" +
      RIDER_MATCH +
      " GROUP BY p.account_id LIMIT ?)",
  )
    .bind(like, like, like, like, MAX_COUNT)
    .first<{ n: number }>();

  return { rows: rows.results ?? [], total: total?.n ?? 0, page, size: PAGE_SIZE };
}

const PAINT_MATCH = " (? = '' OR p.file_name LIKE ? ESCAPE '\\' OR p.sha256 LIKE ? ESCAPE '\\')";

export async function searchPaints(
  env: Env,
  q: string,
  order: Order,
  page: number,
): Promise<Paged<PaintRow>> {
  const like = likeTerm(q);

  const rows = await env.DB.prepare(
    "SELECT p.sha256, MIN(p.file_name) AS file_name, COUNT(DISTINCT p.file_name) AS names," +
      " COUNT(DISTINCT p.account_id) AS riders, MAX(p.size) AS size, COUNT(*) AS uses," +
      " GROUP_CONCAT(DISTINCT p.slot) AS slots" +
      " FROM loadout_paints p WHERE" +
      PAINT_MATCH +
      ` GROUP BY p.sha256 ORDER BY ${orderBy(PAINT_COLUMNS, order, "MIN(p.file_name) COLLATE NOCASE")}` +
      " LIMIT ? OFFSET ?",
  )
    .bind(like, like, like, PAGE_SIZE, (page - 1) * PAGE_SIZE)
    .all<Omit<PaintRow, "stored">>();

  const total = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM (SELECT p.sha256 FROM loadout_paints p WHERE" +
      PAINT_MATCH +
      " GROUP BY p.sha256 LIMIT ?)",
  )
    .bind(like, like, like, MAX_COUNT)
    .first<{ n: number }>();

  // What the database says a rider published and what the bucket holds are two different
  // facts: a loadout row is written before the blob is uploaded, and a publish that was
  // interrupted leaves the first without the second. The page says which rows those are.
  const listed = rows.results ?? [];
  const heads = await Promise.all(listed.map((r) => env.PAINTS.head(r.sha256)));
  return {
    rows: listed.map((r, i) => ({ ...r, stored: heads[i]?.size ?? null })),
    total: total?.n ?? 0,
    page,
    size: PAGE_SIZE,
  };
}

/**
 * The sheets inside one paint, for its own page.
 *
 * Read through the same walk the thumbnail uses, so a paint that cannot be drawn says the
 * same thing in both places rather than being blank in one of them.
 */
async function sheetsOf(
  sha: string,
  env: Env,
): Promise<{ images: PntImage[]; chosen: number } | null> {
  try {
    const head = await env.PAINTS.head(sha);
    if (!head) return null;
    const src = {
      size: head.size,
      async read(offset: number, length: number) {
        const object = await env.PAINTS.get(sha, { range: { offset, length } });
        if (!object) throw new Error("gone");
        return new Uint8Array(await object.arrayBuffer());
      },
      async stream(): Promise<ReadableStream<Uint8Array>> {
        throw new Error("not needed");
      },
    };
    const images = await imageTable(src);
    // By index, not by name: two sheets may share a name, and only one of them is drawn.
    return { images, chosen: images.indexOf(pickImage(images)) };
  } catch {
    // Sealed, absent or unreadable — the thumbnail tile already says which.
    return null;
  }
}
