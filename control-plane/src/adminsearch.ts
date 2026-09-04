/**
 * One question, asked of all three dashboards at once.
 *
 * Usage, diagnostics and paint sync are three views of the same deployment, and until this
 * page existed "who or what is `frost`?" had to be typed into three separate boxes on three
 * separate pages. It is the same string every time — a rider name, a GUID, a Steam id, a file
 * name, a digest — so it is asked once and answered in three groups.
 *
 * Nothing here is a new fact. The mod-file group runs `searchFiles` and the paint group runs
 * `searchPaints`, exactly as their own pages do, so a hit and the page it links to can never
 * disagree. Only the rider group needed its own query, and for a reason worth writing down:
 * the two existing rider searches each start from their own table — diagnostics from
 * `client_modules`, paints from `loadout_paints` — so neither can find an account that only
 * exists on the other side. This one starts from `accounts` and joins both.
 */

import {
  ago,
  bytes,
  ctx,
  errorPage,
  esc,
  href,
  likeTerm,
  shell,
  type Ctx,
} from "./adminui";
import { searchFiles, type FileGroup } from "./diagnosticssearch";
import { searchPaints, thumb, type PaintRow } from "./paintspage";
import { adminAllowed } from "./usage";

const TITLE = "Search";

/** How many of each kind a group shows before it becomes a link to the full list. */
const SHOWN = 6;

/** The window the mod-file group looks back over. The same default the files page opens on. */
const DAYS = 30;

/** What was typed, bounded the way every other search box on these pages bounds it. */
export function parseQuery(url: URL): string {
  return (url.searchParams.get("q") ?? "").trim().slice(0, 96);
}

interface AccountHit {
  id: string;
  rider_name: string;
  guid: string | null;
  steam_id: string | null;
  /** The diagnostics half. Null when this account has never reported. */
  state: string | null;
  reported_at: number | null;
  /** The paint-sync half. Zero when they have never published a look. */
  paints: number;
}

/**
 * Accounts by anything that identifies one.
 *
 * `id` is matched exactly rather than with the wildcards: it is a generated identifier that a
 * person only ever arrives at by copying it, and folding it into the LIKE would mean a short
 * query matching every account whose id happens to contain those characters.
 */
export async function searchAccounts(env: Env, q: string, limit: number): Promise<AccountHit[]> {
  const like = likeTerm(q);
  if (!like) return [];

  const rows = await env.DB.prepare(
    "SELECT a.id, a.rider_name, a.guid, a.steam_id, m.state, m.updated_at AS reported_at," +
      " (SELECT COUNT(DISTINCT p.sha256) FROM loadout_paints p WHERE p.account_id = a.id)" +
      "   AS paints" +
      " FROM accounts a LEFT JOIN client_modules m ON m.account_id = a.id" +
      " WHERE a.id = ? OR a.rider_name LIKE ? ESCAPE '\\'" +
      "   OR COALESCE(a.guid, '') LIKE ? ESCAPE '\\'" +
      "   OR COALESCE(a.steam_id, '') LIKE ? ESCAPE '\\'" +
      // Worst state first, then whoever reported most recently: a search that turns up an
      // alert should not bury it under nine accounts that are fine.
      " ORDER BY CASE m.state WHEN 'alert' THEN 3 WHEN 'warn' THEN 2 WHEN 'ok' THEN 1 ELSE 0 END" +
      "   DESC, m.updated_at DESC, a.rider_name COLLATE NOCASE" +
      " LIMIT ?",
  )
    .bind(q, like, like, like, limit)
    .all<AccountHit>();

  return rows.results ?? [];
}

export async function adminSearch(request: Request, url: URL, env: Env): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") {
    return errorPage(TITLE, 503, "No admin key is configured on this deployment.");
  }
  if (allowed === "denied") return errorPage(TITLE, 401, "Unauthorized.");

  const c = ctx(url);
  const q = parseQuery(url);
  const body = q ? await results(env, q, c) : tips();

  return new Response(
    shell({ title: q ? `Search: ${q}` : TITLE, section: null, body, q, c }),
    {
      status: 200,
      headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" },
    },
  );
}

/**
 * The three groups.
 *
 * Asked together rather than one after another: they are three independent reads of the same
 * database, and a search that waits for each in turn is three round trips slower for no
 * reason. One row over the shown count is enough to know a group has more.
 */
async function results(env: Env, q: string, c: Ctx): Promise<string> {
  const [accounts, files, paints] = await Promise.all([
    searchAccounts(env, q, SHOWN + 1),
    searchFiles(env, {
      q,
      state: "any",
      trust: "any",
      origin: "any",
      sort: "state",
      days: DAYS,
      page: 1,
    }),
    searchPaints(env, q, { sort: "riders", dir: "desc" }, 1),
  ]);

  const found = accounts.length + files.total + paints.total;
  if (!found) {
    return `<section class="panel"><p class="muted">Nothing matches
      <code>${esc(q)}</code> — not a rider, not a mod file, not a paint.</p></section>
      ${tips()}`;
  }

  return (
    group(
      "Riders",
      accounts.length > SHOWN ? `${SHOWN}+` : String(accounts.length),
      // Named for where it goes rather than "See all", because it does not go somewhere with
      // the same rows on it: the diagnostics list is accounts that have reported, and this
      // group also finds the ones that never have.
      "All riders in Diagnostics →",
      href("/admin/diagnostics/riders", { q, state: "any" }, c.key),
      accounts.slice(0, SHOWN).map((a) => riderHit(a, c)),
    ) +
    group(
      "Mod files",
      countOf(files.total),
      "See all →",
      href("/admin/diagnostics/files", { q, state: "any", days: DAYS }, c.key),
      files.rows.slice(0, SHOWN).map((f) => fileHit(f, c)),
    ) +
    group(
      "Paints",
      countOf(paints.total),
      "See all →",
      href("/admin/paints/files", { q }, c.key),
      paints.rows.slice(0, SHOWN).map((p) => paintHit(p, c)),
    )
  );
}

/** A total that was counted only as far as `MAX_COUNT`, written the way the tables write it. */
function countOf(total: number): string {
  return total >= 10_000 ? "10,000+" : total.toLocaleString("en-GB");
}

/** One kind of answer. A group with nothing in it is left out rather than shown empty. */
function group(title: string, total: string, more: string, all: string, hits: string[]): string {
  if (!hits.length) return "";
  return `<section class="panel">
  <h2>${esc(title)} <span class="muted">${esc(total)}</span>
    <a class="more" href="${esc(all)}">${esc(more)}</a></h2>
  <ul class="hits">${hits.join("")}</ul>
</section>`;
}

/**
 * A rider, with both halves of what is known about them on the row.
 *
 * The two links are the point: an account is one person whichever dashboard is looking at
 * them, and the reason to search for a name is usually to see the other side of it.
 */
function riderHit(a: AccountHit, c: Ctx): string {
  const who = a.rider_name || a.id;
  const said = a.state
    ? `<span class="dot ${esc(a.state)}"></span> ${esc(a.state)} · ${esc(ago(a.reported_at ?? 0))}`
    : `<span class="muted">never reported</span>`;
  const wears = a.paints
    ? `${a.paints.toLocaleString("en-GB")} paint${a.paints === 1 ? "" : "s"}`
    : "no paints published";

  return `<li>
  <div>
    <div class="name">${esc(who)}</div>
    <div class="meta">${said} · ${esc(wears)}${a.guid ? ` · <code>${esc(a.guid)}</code>` : ""}</div>
  </div>
  <span class="to">
    <a href="${esc(href("/admin/diagnostics/rider", { id: a.id }, c.key))}">Diagnostics</a> ·
    <a href="${esc(href("/admin/paints/rider", { id: a.id }, c.key))}">Paints</a>
  </span>
</li>`;
}

function fileHit(f: FileGroup, c: Ctx): string {
  return `<li>
  <div>
    <div class="name">
      <a href="${esc(href("/admin/diagnostics/file", { name: f.name }, c.key))}">${esc(f.name)}</a>
    </div>
    <div class="meta"><span class="dot ${esc(f.state)}"></span> ${esc(f.state)} ·
      ${esc(f.trust)} · ${f.accounts.toLocaleString("en-GB")}
      account${f.accounts === 1 ? "" : "s"} ·
      ${f.variantCount} build${f.variantCount === 1 ? "" : "s"} ·
      last seen ${esc(ago(f.lastAt))}${f.publisher ? ` · ${esc(f.publisher)}` : ""}</div>
  </div>
</li>`;
}

function paintHit(p: PaintRow, c: Ctx): string {
  return `<li>
  ${thumb(p.sha256, p.file_name, c)}
  <div>
    <div class="name">
      <a href="${esc(href("/admin/paints/paint", { sha: p.sha256 }, c.key))}">${esc(p.file_name)}</a>
    </div>
    <div class="meta">${p.riders.toLocaleString("en-GB")} rider${p.riders === 1 ? "" : "s"} ·
      ${esc(bytes(p.size))} · <code>${esc(p.sha256.slice(0, 12))}</code></div>
  </div>
</li>`;
}

/** What an empty box can be given, rather than an empty table. */
function tips(): string {
  return `<section class="panel">
  <h2>One box, three dashboards</h2>
  <p class="tips">A rider name, a <code>GUID</code>, a Steam id or an account id finds the
    person on both the diagnostics and the paint side. A file name, a signer, a company or
    anything a build claims about itself finds a mod file. A <code>.pnt</code> name or a
    digest finds a paint.</p>
</section>`;
}
