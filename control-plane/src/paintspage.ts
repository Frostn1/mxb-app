/**
 * What paint sync is actually holding, and who put it there.
 *
 * Everything else in the control plane treats a paint as a digest — which is right for
 * moving bytes between riders and leaves nobody able to answer the two questions worth
 * asking about it: who has published a look, and what does what we are shipping to a grid
 * actually look like. Both are answered here, behind the same `ADMIN_KEY` as the usage and
 * diagnostics pages and for the same reason: rider names, GUIDs and Steam ids are on it.
 *
 * The pictures come from `pntthumb.ts`, which reads the `.pnt` in R2. Nothing on the page
 * depends on one appearing — a paint that is locked content, or whose blob was never
 * uploaded, still lists with everything else and shows a tile saying which.
 */

import {
  ago,
  bytes,
  count,
  ctx,
  errorPage,
  esc,
  href,
  likeTerm,
  MAX_COUNT,
  PAGE_SIZE,
  pager,
  parsePage,
  stamp,
  wrap,
  CSS,
  type Ctx,
  type Paged,
  type Params,
} from "./adminui";
import { adminAllowed } from "./usage";
import { imageTable, paintThumb, pickImage, type PntImage } from "./pntthumb";
import { PRESENCE_TTL_MS } from "./validate";

const ROOT = "/admin/paints";

/** How many riders one paint's page lists. Past this it is a popular paint, not a list. */
const MAX_WEARERS = 200;

type Tab = "riders" | "paints";

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

interface RiderRow {
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
}

interface PaintRow {
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

interface SlotRow {
  bike_id: string;
  slot: string;
  file_name: string;
  sha256: string;
  size: number;
  rel_dest: string;
}

interface Totals {
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
// Routes
// ---------------------------------------------------------------------------

/** The gate every view goes through, so no handler can forget it. */
function gate(request: Request, url: URL, env: Env): Response | null {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") {
    return errorPage("Paint sync", 503, "No admin key is configured on this deployment.");
  }
  if (allowed === "denied") return errorPage("Paint sync", 401, "Unauthorized.");
  return null;
}

function html(body: string, status = 200): Response {
  return new Response(body, {
    status,
    headers: {
      "content-type": "text/html; charset=utf-8",
      // Named people, behind a key: never cached by anything in between.
      "cache-control": "no-store",
    },
  });
}

/** `GET /admin/paints` — who has published a look. */
export async function paintRiders(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  const c = ctx(url);
  const q = (url.searchParams.get("q") ?? "").slice(0, 96);
  const order = parseOrder(url, RIDER_COLUMNS, "published");
  const page = parsePage(url.searchParams.get("page"));
  const [sums, found] = await Promise.all([totals(env), searchRiders(env, q, order, page)]);
  return html(shell("Paint sync", "riders", ridersView(sums, found, q, order, c), c));
}

/** `GET /admin/paints/rider?id=…` — one rider's bikes, slot by slot. */
export async function paintRider(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  const c = ctx(url);
  const id = url.searchParams.get("id") ?? "";
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
  if (!account) return html(shell("Paint sync", "riders", empty("No such account."), c), 404);

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

  const when = new Map((published.results ?? []).map((r) => [r.bike_id, r.updated_at]));
  return html(
    shell(
      account.rider_name,
      "riders",
      riderView(account, slots.results ?? [], when, presence, c),
      c,
    ),
  );
}

/** `GET /admin/paints/files` — every paint we hold, once per digest. */
export async function paintFiles(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  const c = ctx(url);
  const q = (url.searchParams.get("q") ?? "").slice(0, 96);
  const order = parseOrder(url, PAINT_COLUMNS, "riders");
  const page = parsePage(url.searchParams.get("page"));
  const [sums, found] = await Promise.all([totals(env), searchPaints(env, q, order, page)]);
  return html(shell("Paints", "paints", paintsView(sums, found, q, order, c), c));
}

/** `GET /admin/paints/paint?sha=…` — one paint: its sheets, and who is wearing it. */
export async function paintOne(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  const c = ctx(url);
  const sha = (url.searchParams.get("sha") ?? "").toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(sha)) {
    return html(shell("Paints", "paints", empty("That is not a paint digest."), c), 400);
  }

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
  if (rows.length === 0 && !stored) {
    return html(shell("Paints", "paints", empty("Nothing here has that digest."), c), 404);
  }
  const title = rows[0]?.file_name ?? sha.slice(0, 12);
  return html(shell(title, "paints", oneView(sha, rows, stored?.size ?? null, sheets, c), c));
}

/** `GET /admin/paints/thumb?sha=…` — the picture itself. */
export async function paintThumbnail(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;
  return paintThumb((url.searchParams.get("sha") ?? "").toLowerCase(), env);
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
      "   AS at_server" +
      " FROM accounts a JOIN loadout_paints p ON p.account_id = a.id" +
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

async function searchPaints(
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

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

function shell(title: string, tab: Tab, body: string, c: Ctx): string {
  const nav = (
    [
      ["riders", "Riders", ROOT],
      ["paints", "Paints", `${ROOT}/files`],
    ] as const
  )
    .map(
      ([id, text, path]) =>
        `<a class="${id === tab ? "on" : ""}" href="${esc(href(path, {}, c.key))}">${text}</a>`,
    )
    .join("");

  return `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>${esc(title)} — MXB App paint sync</title>
<style>${CSS}</style>
</head><body>
<header><h1>${esc(title)}</h1><nav>${nav}
  <a class="out" href="${esc(href("/admin/diagnostics", {}, c.key))}">Diagnostics</a></nav></header>
${body}
</body></html>`;
}

function empty(message: string): string {
  return `<section class="panel"><p class="muted">${esc(message)}</p></section>`;
}

function tile(label: string, value: string, hint: string): string {
  return `<div class="tile"><div class="n">${esc(value)}</div><div class="l">${esc(label)}</div>
  <div class="h">${esc(hint)}</div></div>`;
}

function tiles(s: Totals): string {
  return `<section class="tiles">
  ${tile("Riders publishing", s.riders.toLocaleString("en-GB"), "accounts with a loadout")}
  ${tile("Published today", s.today.toLocaleString("en-GB"), "in the last 24 hours")}
  ${tile("On a server", s.present.toLocaleString("en-GB"), "reported in the last 10 minutes")}
  ${tile("Paints", s.paints.toLocaleString("en-GB"), "distinct files, de-duplicated")}
  ${tile("Equipped slots", s.slots.toLocaleString("en-GB"), "rider × bike × slot")}
  ${tile("Stored", bytes(s.stored), "one copy per digest")}
</section>`;
}

/** The picture, linking to the paint it belongs to. */
function thumb(sha: string, name: string, c: Ctx, big = false): string {
  return `<a class="thumblink" href="${esc(href(`${ROOT}/paint`, { sha }, c.key))}"
  ><img class="thumb${big ? " big" : ""}" loading="lazy" decoding="async"
    src="${esc(href(`${ROOT}/thumb`, { sha }, c.key))}" alt="${esc(name)}"></a>`;
}

/**
 * A header row whose columns are links.
 *
 * Clicking the column already being read turns it around; clicking a new one starts it the
 * way that column is worth reading. `page` is dropped rather than carried: page 7 of a
 * different ordering is a different set of rows, and landing there reads as a bug.
 */
function sortable(
  path: string,
  params: Params,
  columns: Record<string, Column>,
  order: Order,
  c: Ctx,
  lead = "",
  trail = "",
): string {
  const heads = Object.entries(columns)
    .map(([key, col]) => {
      const on = key === order.sort;
      const dir: Dir = on ? (order.dir === "asc" ? "desc" : "asc") : col.first;
      const arrow = on ? `<span class="dir">${order.dir === "asc" ? "\u2191" : "\u2193"}</span>` : "";
      const to = href(path, { ...params, sort: key, dir, page: undefined }, c.key);
      return `<th class="${col.num ? "num" : ""}${on ? " on" : ""}"><a href="${esc(to)}"
    title="Sort by ${esc(col.label)}">${esc(col.label)}${arrow}</a></th>`;
    })
    .join("");
  return `<thead><tr>${lead}${heads}${trail}</tr></thead>`;
}

/** The current ordering, as hidden fields, so searching does not reset the table. */
function keep(order: Order): string {
  return `<input type="hidden" name="sort" value="${esc(order.sort)}">
  <input type="hidden" name="dir" value="${esc(order.dir)}">`;
}

function searchBox(path: string, q: string, extra: string, c: Ctx, hint: string): string {
  return `<form class="search" method="get" action="${esc(path)}">
  ${c.key ? `<input type="hidden" name="key" value="${esc(c.key)}">` : ""}
  <input name="q" value="${esc(q)}" placeholder="${esc(hint)}" autocomplete="off">
  ${extra}
  <button type="submit">Search</button>
  ${q ? `<a class="clear" href="${esc(href(path, {}, c.key))}">clear</a>` : ""}
</form>`;
}

// ---------------------------------------------------------------------------
// Riders
// ---------------------------------------------------------------------------

function ridersView(
  s: Totals,
  found: Paged<RiderRow>,
  q: string,
  order: Order,
  c: Ctx,
): string {
  const params: Params = { q, sort: order.sort, dir: order.dir };
  return `${tiles(s)}
<section class="panel">
  <h2>Riders</h2>
  ${searchBox(ROOT, q, keep(order), c, "rider name, GUID or Steam id")}
  ${count(found, "rider")}
  ${
    found.rows.length
      ? wrap(riderTable(found.rows, params, order, c))
      : `<p class="muted">Nobody matches.</p>`
  }
  ${pager(ROOT, params, found, c)}
</section>`;
}

function riderTable(rows: RiderRow[], params: Params, order: Order, c: Ctx): string {
  return `<table>
${sortable(ROOT, params, RIDER_COLUMNS, order, c)}
<tbody>
${rows
  .map(
    (r) => `<tr>
  <td><a href="${esc(href(`${ROOT}/rider`, { id: r.id }, c.key))}">${esc(r.rider_name)}</a>
    ${r.kind === "device" ? ` <span class="tag">device</span>` : ""}</td>
  <td class="mono">${r.guid ? esc(r.guid) : `<span class="muted">—</span>`}</td>
  <td class="mono">${r.steam_id ? esc(r.steam_id) : `<span class="muted">—</span>`}</td>
  <td class="num">${r.bikes}</td>
  <td class="num">${r.slots}</td>
  <td class="num">${r.files}</td>
  <td class="num">${esc(bytes(r.bytes))}</td>
  <td title="${esc(stamp(r.published_at ?? 0))}">${esc(ago(r.published_at ?? 0))}</td>
  <td>${r.at_server ? `<span class="dot ok"></span> ${esc(r.at_server)}` : `<span class="muted">—</span>`}</td>
</tr>`,
  )
  .join("")}
</tbody></table>`;
}

// ---------------------------------------------------------------------------
// One rider
// ---------------------------------------------------------------------------

function riderView(
  account: {
    id: string;
    rider_name: string;
    guid: string | null;
    steam_id: string | null;
    kind: string;
    created_at: number;
  },
  slots: SlotRow[],
  when: Map<string, number>,
  presence: { server_id: string; updated_at: number } | null,
  c: Ctx,
): string {
  const live = presence && presence.updated_at > Date.now() - PRESENCE_TTL_MS;
  // Both dashboards key on the same account id, so one rider is one link away from what
  // their game has loaded — the two halves of the same person, previously unconnected.
  const diagnostics = href("/admin/diagnostics/rider", { id: account.id }, c.key);
  const facts = `<section class="panel">
  <div class="who"><span class="name">${esc(account.rider_name)}</span>
    <span class="tag">${esc(account.kind)}</span>
    ${live ? `<span class="pill ok">on ${esc(presence!.server_id)}</span>` : ""}
    <a class="aside" href="${esc(diagnostics)}">Diagnostics for this rider →</a></div>
  <dl class="facts">
    ${fact("GUID", account.guid ?? "—", true)}
    ${fact("Steam id", account.steam_id ?? "—", true)}
    ${fact("Account id", account.id, true)}
    ${fact("Enrolled", stamp(account.created_at))}
    ${fact("Bikes published", String(when.size))}
    ${fact("Last publish", when.size ? ago(Math.max(...when.values())) : "—")}
    ${fact(
      "Last seen",
      presence ? `${ago(presence.updated_at)} on ${presence.server_id}` : "never reported",
    )}
  </dl>
</section>`;

  if (slots.length === 0) return `${facts}${empty("This account has no paints published.")}`;

  const byBike = new Map<string, SlotRow[]>();
  for (const row of slots) {
    const at = byBike.get(row.bike_id);
    if (at) at.push(row);
    else byBike.set(row.bike_id, [row]);
  }

  const bikes = [...byBike.entries()]
    .map(
      ([bike, rows]) => `<section class="panel">
  <h2>${esc(bike || "(no bike named)")}
    <span class="more muted">${rows.length} slot${rows.length === 1 ? "" : "s"} ·
      published ${esc(ago(when.get(bike) ?? 0))}</span></h2>
  ${wrap(slotTable(rows, c))}
</section>`,
    )
    .join("");

  return `${facts}${bikes}`;
}

function fact(label: string, value: string, mono = false): string {
  return `<dt>${esc(label)}</dt><dd${mono ? ' class="mono"' : ""}>${esc(value)}</dd>`;
}

function slotTable(rows: SlotRow[], c: Ctx): string {
  return `<table><thead><tr>
  <th></th><th>Slot</th><th>File</th><th class="num">Size</th><th>Installs at</th><th>Digest</th>
</tr></thead><tbody>
${rows
  .map(
    (r) => `<tr>
  <td>${thumb(r.sha256, r.file_name, c)}</td>
  <td>${esc(r.slot)}</td>
  <td>${esc(r.file_name)}</td>
  <td class="num">${esc(bytes(r.size))}</td>
  <td class="mono">${esc(r.rel_dest)}</td>
  <td class="mono"><a href="${esc(href(`${ROOT}/paint`, { sha: r.sha256 }, c.key))}">${esc(
    r.sha256.slice(0, 12),
  )}</a></td>
</tr>`,
  )
  .join("")}
</tbody></table>`;
}

// ---------------------------------------------------------------------------
// Paints
// ---------------------------------------------------------------------------

function paintsView(
  s: Totals,
  found: Paged<PaintRow>,
  q: string,
  order: Order,
  c: Ctx,
): string {
  const path = `${ROOT}/files`;
  const params: Params = { q, sort: order.sort, dir: order.dir };

  return `${tiles(s)}
<section class="panel">
  <h2>Paints</h2>
  ${searchBox(path, q, keep(order), c, "file name or digest")}
  ${count(found, "paint")}
  ${
    found.rows.length
      ? wrap(paintTable(found.rows, params, order, c))
      : `<p class="muted">Nothing matches.</p>`
  }
  ${pager(path, params, found, c)}
</section>`;
}

function paintTable(rows: PaintRow[], params: Params, order: Order, c: Ctx): string {
  // The picture leads and `Blob` trails, neither of them a column the database can order by:
  // one is the paint itself, and the other is what the bucket answered about this page's
  // rows after the query had already run.
  return `<table>
${sortable(`${ROOT}/files`, params, PAINT_COLUMNS, order, c, "<th></th>", "<th>Blob</th>")}
<tbody>
${rows
  .map(
    (r) => `<tr>
  <td>${thumb(r.sha256, r.file_name, c)}</td>
  <td><a href="${esc(href(`${ROOT}/paint`, { sha: r.sha256 }, c.key))}">${esc(r.file_name)}</a>
    ${r.names > 1 ? ` <span class="tag">${r.names} names</span>` : ""}</td>
  <td class="muted">${esc((r.slots ?? "").split(",").join(", "))}</td>
  <td class="num">${r.riders}</td>
  <td class="num">${r.uses}</td>
  <td class="num">${esc(bytes(r.size))}</td>
  <td class="mono">${esc(r.sha256.slice(0, 12))}</td>
  <td>${
    r.stored === null
      ? `<span class="pill alert">missing</span>`
      : r.stored === r.size
        ? `<span class="dot ok"></span>`
        : `<span class="pill warn">${esc(bytes(r.stored))}</span>`
  }</td>
</tr>`,
  )
  .join("")}
</tbody></table>`;
}

// ---------------------------------------------------------------------------
// One paint
// ---------------------------------------------------------------------------

function oneView(
  sha: string,
  wearers: {
    id: string;
    rider_name: string;
    guid: string | null;
    bike_id: string;
    slot: string;
    file_name: string;
    size: number;
    rel_dest: string;
  }[],
  stored: number | null,
  sheets: { images: PntImage[]; chosen: number } | null,
  c: Ctx,
): string {
  const names = [...new Set(wearers.map((w) => w.file_name))];
  const riders = new Set(wearers.map((w) => w.id)).size;

  const head = `<section class="panel">
  <div class="who">${thumb(sha, names[0] ?? sha, c, true)}
    <div>
      <div class="name">${esc(names[0] ?? "unknown file")}</div>
      <div class="muted mono">${esc(sha)}</div>
    </div></div>
  <dl class="facts">
    ${fact("Stored", stored === null ? "not in the bucket" : bytes(stored))}
    ${fact("Riders", String(riders))}
    ${fact("Equipped", `${wearers.length} slot${wearers.length === 1 ? "" : "s"}`)}
    ${fact("Names in use", names.length > 1 ? names.join(", ") : (names[0] ?? "—"))}
    ${fact("Sheets", sheets ? String(sheets.images.length) : "unreadable — sealed or absent")}
  </dl>
</section>`;

  const table = sheets
    ? `<section class="panel">
  <h2>Sheets <span class="more muted">the largest is the one drawn</span></h2>
  ${wrap(`<table><thead><tr><th>Texture</th><th class="num">Size</th><th></th></tr></thead><tbody>
  ${sheets.images
    .map(
      (i, at) => `<tr><td class="mono">${esc(i.name)}</td>
    <td class="num">${i.width}×${i.height}</td>
    <td>${at === sheets.chosen ? `<span class="tag">drawn</span>` : ""}</td></tr>`,
    )
    .join("")}
  </tbody></table>`)}
</section>`
    : "";

  const worn = wearers.length
    ? `<section class="panel">
  <h2>Worn by${wearers.length >= MAX_WEARERS ? ` <span class="more muted">first ${MAX_WEARERS}</span>` : ""}</h2>
  ${wrap(`<table><thead><tr>
    <th>Rider</th><th>GUID</th><th>Bike</th><th>Slot</th><th>File</th><th>Installs at</th>
  </tr></thead><tbody>
  ${wearers
    .map(
      (w) => `<tr>
    <td><a href="${esc(href(`${ROOT}/rider`, { id: w.id }, c.key))}">${esc(w.rider_name)}</a></td>
    <td class="mono">${w.guid ? esc(w.guid) : `<span class="muted">—</span>`}</td>
    <td>${esc(w.bike_id || "—")}</td>
    <td>${esc(w.slot)}</td>
    <td>${esc(w.file_name)}</td>
    <td class="mono">${esc(w.rel_dest)}</td>
  </tr>`,
    )
    .join("")}
  </tbody></table>`)}
</section>`
    : empty("Nobody has this paint equipped — the blob is stored but unreferenced.");

  return `${head}${table}${worn}`;
}
