/**
 * The client diagnostics dashboard, rendered on the server.
 *
 * Plain HTML with its CSS inline and no scripts, for the reason the usage page gives: a page
 * behind an admin key that pulls anything from someone else's host stops working the day
 * that host does, and this is the only view of this data there is.
 *
 * Five views over the same two tables:
 *
 *   * **Overview** — who is reporting right now, and what turned up lately.
 *   * **Riders** — search every account by name, GUID, Steam id or account id.
 *   * **Rider** — one person: identity, last report, and every file their game has loaded.
 *   * **Files** — search every file by name, hash, signer, or anything it claims to be.
 *   * **File** — one file, and everybody who has it. The lookup run backwards.
 *
 * It is a working surface rather than a report: the rule buttons sit on the rows they are
 * about, and a rule takes effect on the next report from every install. Every list is paged
 * — an offset and a total, never a scroll that loads as it goes — so a link to page four is
 * a link to page four tomorrow as well.
 */

import {
  addRule,
  collectAdminView,
  deleteRule,
  stateRank,
  type AdminView,
  type LiveRow,
  type Matched,
  type ModuleRule,
  type State,
  type Trust,
} from "./diagnostics";
import {
  clampDays,
  claimText,
  fileDetail,
  FILE_ORIGINS,
  FILE_SORTS,
  FILE_STATES,
  FILE_TRUSTS,
  MAX_COUNT,
  parseFileQuery,
  parsePage,
  parseRiderQuery,
  parseSightingQuery,
  riderDetail,
  RIDER_SORTS,
  RIDER_STATES,
  searchFiles,
  searchRiders,
  SIGHTING_STATES,
  totals,
  type FileDetail,
  type FileGroup,
  type FileQuery,
  type FileVariant,
  type Paged,
  type RiderDetail,
  type RiderQuery,
  type RiderRow,
  type Sighting,
  type SightingQuery,
} from "./diagnosticssearch";
import { adminAllowed } from "./usage";

/** Windows the header offers. Anything else still works via `?days=`. */
const RANGES = [1, 7, 30, 90];

const ROOT = "/admin/diagnostics";

/** Query values a link carries. `undefined` and `""` are dropped rather than sent empty. */
type Params = Record<string, string | number | undefined | null>;

/** What every view needs to draw a link back to itself: the key, and where it is. */
interface Ctx {
  key: string;
  /** The path and query of the page being drawn, for a rule form to come back to. */
  back: string;
}

function ctx(url: URL): Ctx {
  return { key: url.searchParams.get("key") ?? "", back: `${url.pathname}${url.search}` };
}

/** Every link carries the admin key: a browser typing a URL cannot send a header. */
function href(path: string, params: Params, key: string): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "") continue;
    q.set(k, String(v));
  }
  if (key) q.set("key", key);
  const query = q.toString();
  return query ? `${path}?${query}` : path;
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

/** The gate every view goes through, so no handler can forget it. */
function gate(request: Request, url: URL, env: Env): Response | null {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return page(503, "No admin key is configured on this deployment.");
  if (allowed === "denied") return page(401, "Unauthorized.");
  return null;
}

function html(body: string, status = 200): Response {
  return new Response(body, {
    status,
    headers: {
      "content-type": "text/html; charset=utf-8",
      // About named people, behind a key: never cached by anything in between.
      "cache-control": "no-store",
    },
  });
}

export async function diagnosticsDashboard(
  request: Request,
  url: URL,
  env: Env,
): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  const c = ctx(url);
  const days = clampDays(url.searchParams.get("days"));
  const [view, sums, recent] = await Promise.all([
    collectAdminView(env),
    totals(env, days),
    searchFiles(env, {
      q: "",
      state: "flagged",
      trust: "any",
      origin: "any",
      sort: "state",
      days,
      page: 1,
    }),
  ]);
  return html(overview(view, sums, recent, days, c));
}

export async function diagnosticsRiders(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;
  const query = parseRiderQuery(url);
  return html(ridersView(await searchRiders(env, query), query, ctx(url)));
}

export async function diagnosticsRider(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;
  const who = (url.searchParams.get("id") ?? "").trim().slice(0, 128);
  if (!who) return html(shell("Rider", "riders", empty("No rider asked for."), ctx(url)), 400);
  const query = parseSightingQuery(url);
  const detail = await riderDetail(env, who, query);
  if (!detail) {
    return html(shell("Rider", "riders", empty("No account matches that."), ctx(url)), 404);
  }
  return html(riderView(detail, query, ctx(url)));
}

export async function diagnosticsFiles(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;
  const query = parseFileQuery(url);
  return html(filesView(await searchFiles(env, query), query, ctx(url)));
}

export async function diagnosticsFile(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;
  const name = (url.searchParams.get("name") ?? "").trim().slice(0, 96);
  const sha256 = (url.searchParams.get("sha256") ?? "").trim().slice(0, 64);
  if (!name) return html(shell("File", "files", empty("No file asked for."), ctx(url)), 400);
  const detail = await fileDetail(env, name, sha256, parsePage(url.searchParams.get("page")));
  if (!detail) {
    return html(shell("File", "files", empty("Nothing has been seen under that name."), ctx(url)), 404);
  }
  return html(fileView(detail, ctx(url)));
}

export async function diagnosticsRulesPage(
  request: Request,
  url: URL,
  env: Env,
): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;
  const view = await collectAdminView(env);
  return html(rulesView(view, ctx(url)));
}

/**
 * The rule buttons post here and come back to the page they were pressed on.
 *
 * A 303 rather than a rendered result, so a refresh does not re-submit — this is the one
 * surface where a double-click would silently add a rule twice.
 */
export async function diagnosticsRules(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return page(400, "That was not a form.");
  }
  const field = (name: string) => String(form.get(name) ?? "");
  // Back where the button was pressed, filters and page intact. Anything not one of our own
  // paths is an open redirect, so it becomes the overview.
  const asked = field("back");
  const back = asked.startsWith(`${ROOT}`) && !asked.startsWith("//") ? asked : ROOT;

  if (field("action") === "delete") {
    const id = Number(field("id"));
    if (Number.isInteger(id) && id > 0) await deleteRule(env, id);
    return redirect(back);
  }

  const result = await addRule(
    env,
    field("kind"),
    field("pattern"),
    field("sha256"),
    field("label"),
    field("note"),
  );
  if (!result.ok) return page(400, result.error ?? "That rule was not usable.");
  return redirect(back);
}

function redirect(to: string): Response {
  return new Response(null, { status: 303, headers: { location: to, "cache-control": "no-store" } });
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

type Tab = "overview" | "riders" | "files" | "rules";

function shell(title: string, tab: Tab, body: string, c: Ctx): string {
  const nav = (
    [
      ["overview", "Overview", ROOT],
      ["riders", "Riders", `${ROOT}/riders`],
      ["files", "Files", `${ROOT}/files`],
      ["rules", "Rules", `${ROOT}/rules`],
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
<title>${esc(title)} — MXB App diagnostics</title>
<style>${CSS}</style>
</head><body>
<header><h1>${esc(title)}</h1><nav>${nav}</nav></header>
${body}
<footer class="muted">Only clients running the app report. A missing row means nobody told us,
  not that nothing happened.</footer>
</body></html>`;
}

function empty(message: string): string {
  return `<section class="panel"><p class="muted">${esc(message)}</p></section>`;
}

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

function overview(
  v: AdminView,
  sums: Awaited<ReturnType<typeof totals>>,
  recent: Paged<FileGroup>,
  days: number,
  c: Ctx,
): string {
  const alerts = v.live.filter((r) => worst(r) === "alert");
  const warns = v.live.filter((r) => worst(r) === "warn");
  const blind = v.live.filter((r) => r.state === "unknown");
  const top = recent.rows.slice(0, 12);

  const body = `
<section class="tiles">
  ${tile("Reporting", v.reporting, "last 10 minutes")}
  ${tile("Alerts", alerts.length, "a rule named a file", alerts.length ? "alert" : "")}
  ${tile("Unaccounted", warns.length, "no rule covers it", warns.length ? "warn" : "")}
  ${tile("Blind", blind.length, "could not read the list")}
  ${tile("Riders", sums.accounts, `${sums.reported.toLocaleString("en-GB")} have reported`)}
  ${tile("Files", sums.files, `${sums.builds.toLocaleString("en-GB")} builds, ${days}d`)}
  ${tile("Flagged", sums.flagged, `file names, ${days}d`, sums.flagged ? "warn" : "")}
  ${tile("Rules", v.rules.length, `version ${v.rulesVersion}`)}
</section>

<section class="panel">
  <h2>Reporting now <span class="muted">${v.live.length} client${v.live.length === 1 ? "" : "s"}</span></h2>
  ${liveTable(v.live, c)}
</section>

<section class="panel">
  <h2>Flagged files <span class="muted">worst first, last ${days}d</span>
    <a class="more" href="${esc(href(`${ROOT}/files`, { days }, c.key))}">Search all files →</a></h2>
  ${top.length ? fileTable(top, c, false) : `<p class="muted">Nothing unaccounted for in this window.</p>`}
</section>`;

  return shell("Diagnostics", "overview", withRanges(body, ROOT, { days }, days, c), c);
}

/** The day-window switcher, above whichever view uses one. */
function withRanges(body: string, path: string, params: Params, days: number, c: Ctx): string {
  const links = RANGES.map(
    (d) =>
      `<a class="${d === days ? "on" : ""}" href="${esc(
        href(path, { ...params, days: d, page: undefined }, c.key),
      )}">${d}d</a>`,
  ).join("");
  return `<div class="ranges">${links}</div>${body}`;
}

function tile(label: string, value: number, hint: string, tone = ""): string {
  return `<div class="tile ${tone}"><span class="n">${value.toLocaleString("en-GB")}</span>
    <span class="l">${esc(label)}</span><span class="h">${esc(hint)}</span></div>`;
}

/** The worse of what a client says now and the worst it said inside the window. */
function worst(row: LiveRow): State {
  return stateRank(row.worstState) > stateRank(row.state) ? row.worstState : row.state;
}

function liveTable(rows: LiveRow[], c: Ctx): string {
  if (!rows.length) return `<p class="muted">Nobody is reporting right now.</p>`;
  return wrap(`<table>
  <thead><tr><th>Rider</th><th>State</th><th>Server</th><th>Named</th>
    <th class="num">Unaccounted</th><th class="num">Modules</th><th>App</th><th>Seen</th></tr></thead>
  <tbody>${rows
    .map((r) => {
      const peak = worst(r);
      const drift = peak !== r.state ? ` <span class="muted">was ${esc(peak)}</span>` : "";
      return `<tr>
      <td>${riderLink(r.accountId, r.riderName, c)}</td>
      <td><span class="pill ${esc(peak)}">${esc(r.state)}</span>${drift}</td>
      <td class="muted">${esc(r.serverId || "—")}</td>
      <td>${named(r.matched, c)}</td>
      <td class="num">${r.unknownCount}</td>
      <td class="num">${r.moduleCount}</td>
      <td class="muted">${esc(r.appVersion || "—")}</td>
      <td class="muted">${esc(ago(r.updatedAt))}</td>
    </tr>`;
    })
    .join("")}</tbody></table>`);
}

function named(matched: Matched[], c: Ctx): string {
  if (!matched.length) return `<span class="muted">—</span>`;
  return matched
    .map(
      (m) =>
        `<a class="tag" href="${esc(
          href(`${ROOT}/file`, { name: m.name }, c.key),
        )}">${esc(m.label || m.name)}</a>`,
    )
    .join(" ");
}

// ---------------------------------------------------------------------------
// Riders
// ---------------------------------------------------------------------------

function ridersView(found: Paged<RiderRow>, query: RiderQuery, c: Ctx): string {
  const params: Params = {
    q: query.q,
    state: query.state,
    sort: query.sort,
    days: query.days,
  };
  const body = `
<section class="panel">
  <form class="search" method="get" action="${ROOT}/riders">
    ${hidden("key", c.key)}
    <input name="q" value="${esc(query.q)}" maxlength="96" autofocus
      placeholder="rider name, GUID, Steam id, account id">
    ${select("state", RIDER_STATES, query.state, {
      any: "any state",
      quiet: "not reporting",
    })}
    ${select("sort", RIDER_SORTS, query.sort, {
      seen: "last report",
      state: "worst state",
      name: "name",
      unaccounted: "most unaccounted",
    })}
    ${select("days", [1, 7, 30, 90, 365], query.days, {}, (d) => `${d}d`)}
    <button type="submit">Search</button>
    ${query.q || query.state !== "any" ? `<a class="clear" href="${esc(href(`${ROOT}/riders`, {}, c.key))}">Clear</a>` : ""}
  </form>
  ${count(found, "rider")}
  ${riderTable(found.rows, c)}
  ${pager(`${ROOT}/riders`, params, found, c)}
</section>`;
  return shell("Riders", "riders", body, c);
}

function riderTable(rows: RiderRow[], c: Ctx): string {
  if (!rows.length) return `<p class="muted">No rider matches.</p>`;
  return wrap(`<table>
  <thead><tr><th>Rider</th><th>GUID</th><th>State</th><th class="num">Files</th>
    <th class="num">Flagged</th><th>Server</th><th>App</th><th>Last report</th></tr></thead>
  <tbody>${rows
    .map((r) => {
      const peak = r.worstState && stateRank(r.worstState) > stateRank(r.state) ? r.worstState : r.state;
      const state = r.state
        ? `<span class="pill ${esc(peak)}">${esc(r.state)}</span>${
            peak !== r.state ? ` <span class="muted">was ${esc(peak)}</span>` : ""
          }`
        : `<span class="muted">never reported</span>`;
      return `<tr>
      <td>${riderLink(r.accountId, r.riderName, c)}</td>
      <td><code class="muted">${esc(r.guid || "—")}</code></td>
      <td>${state}</td>
      <td class="num">${r.files.toLocaleString("en-GB")}</td>
      <td class="num ${r.flagged ? "hot" : "muted"}">${r.flagged.toLocaleString("en-GB")}</td>
      <td class="muted">${esc(r.serverId || "—")}</td>
      <td class="muted">${esc(r.appVersion || "—")}</td>
      <td class="muted">${esc(r.updatedAt ? ago(r.updatedAt) : "—")}</td>
    </tr>`;
    })
    .join("")}</tbody></table>`);
}

function riderLink(accountId: string, name: string, c: Ctx): string {
  const shown = name || accountId.slice(0, 8);
  return `<a href="${esc(href(`${ROOT}/rider`, { id: accountId }, c.key))}">${esc(shown)}</a>`;
}

// ---------------------------------------------------------------------------
// One rider
// ---------------------------------------------------------------------------

function riderView(d: RiderDetail, query: SightingQuery, c: Ctx): string {
  const r = d.rider;
  const peak = r.worstState && stateRank(r.worstState) > stateRank(r.state) ? r.worstState : r.state;
  const params: Params = { id: r.accountId, f: query.q, fstate: query.state };

  const body = `
<section class="panel">
  <div class="who">
    <span class="name">${esc(r.riderName || "(no name)")}</span>
    ${r.state ? `<span class="pill ${esc(peak)}">${esc(r.state)}</span>` : `<span class="muted">never reported</span>`}
    ${peak !== r.state ? `<span class="muted">peaked at ${esc(peak)} ${esc(ago(r.worstAt))}</span>` : ""}
  </div>
  <dl class="facts">
    ${fact("GUID", r.guid || "—", true)}
    ${fact("Account", r.accountId, true)}
    ${fact("Steam", r.steamId || "—", true)}
    ${fact("Enrolled", r.createdAt ? stamp(r.createdAt) : "—")}
    ${fact("Last report", r.updatedAt ? `${ago(r.updatedAt)} · ${stamp(r.updatedAt)}` : "never")}
    ${fact("App", r.appVersion || "—")}
    ${fact("Modules", `${r.moduleCount} loaded, ${r.unknownCount} unaccounted`)}
    ${fact("Rules version", r.rulesVersion ? String(r.rulesVersion) : "—")}
    ${fact("Server now", r.serverId || "—")}
    ${fact("Files on record", `${r.files.toLocaleString("en-GB")}, ${r.flagged.toLocaleString("en-GB")} flagged`)}
  </dl>
  ${
    d.matched.length
      ? `<p class="named">Named by rules in the last report: ${named(d.matched, c)}</p>`
      : ""
  }
  ${
    d.servers.length
      ? `<p class="muted">Seen on: ${d.servers
          .map((s) => `<code>${esc(s.serverId)}</code> <span class="muted">${esc(ago(s.lastAt))}</span>`)
          .join(" · ")}</p>`
      : ""
  }
</section>

<section class="panel">
  <h2>Files this game has loaded</h2>
  <form class="search" method="get" action="${ROOT}/rider">
    ${hidden("key", c.key)}${hidden("id", r.accountId)}
    <input name="f" value="${esc(query.q)}" maxlength="96" placeholder="file name, hash, signer, claim">
    ${select("fstate", SIGHTING_STATES, query.state, { any: "any state" })}
    <button type="submit">Filter</button>
    ${query.q || query.state !== "any" ? `<a class="clear" href="${esc(href(`${ROOT}/rider`, { id: r.accountId }, c.key))}">Clear</a>` : ""}
  </form>
  ${count(d.files, "file")}
  ${sightingTable(d.files.rows, c)}
  ${pager(`${ROOT}/rider`, params, d.files, c)}
</section>`;

  return shell(r.riderName || "Rider", "riders", body, c);
}

function fact(label: string, value: string, mono = false): string {
  return `<div><dt>${esc(label)}</dt><dd${mono ? ' class="mono"' : ""}>${esc(value)}</dd></div>`;
}

/** One rider's files. Each row links out to who else has the same file. */
function sightingTable(rows: Sighting[], c: Ctx): string {
  if (!rows.length) return `<p class="muted">Nothing recorded.</p>`;
  return wrap(`<table>
  <thead><tr><th>File</th><th>Where</th><th>Signed</th><th>Claims</th><th class="num">Size</th>
    <th>Built</th><th>First</th><th>Last</th><th class="num">Seen</th><th></th></tr></thead>
  <tbody>${rows
    .map(
      (r) => `<tr>
      <td><span class="dot ${esc(r.state)}"></span>
        <a href="${esc(href(`${ROOT}/file`, { name: r.name }, c.key))}"><code>${esc(r.name)}</code></a>
        ${r.label ? `<span class="muted">${esc(r.label)}</span>` : ""}
        <div><code class="muted">${esc(r.sha256 ? r.sha256.slice(0, 16) : "not read")}</code></div></td>
      <td class="muted">${esc(r.origin)}</td>
      <td>${sig(r.trust, r.publisher)}</td>
      <td class="muted">${esc(claimText(r.description, r.product, r.company) || "—")}</td>
      <td class="num muted">${esc(bytes(r.size))}</td>
      <td class="muted">${esc(r.mtime ? ago(r.mtime * 1000) : "—")}</td>
      <td class="muted">${esc(ago(r.firstAt))}</td>
      <td class="muted">${esc(ago(r.lastAt))}</td>
      <td class="num">${r.hits}</td>
      <td class="act">${ruleForms(
        r.sha256
          ? hidden("sha256", r.sha256)
          : hidden("pattern", r.name),
        r.label || r.name,
        c,
      )}</td>
    </tr>`,
    )
    .join("")}</tbody></table>`);
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

function filesView(found: Paged<FileGroup>, query: FileQuery, c: Ctx): string {
  const params: Params = {
    q: query.q,
    state: query.state,
    trust: query.trust,
    origin: query.origin,
    sort: query.sort,
    days: query.days,
  };
  const filtered =
    query.q || query.state !== "flagged" || query.trust !== "any" || query.origin !== "any";

  const body = `
<section class="panel">
  <form class="search" method="get" action="${ROOT}/files">
    ${hidden("key", c.key)}
    <input name="q" value="${esc(query.q)}" maxlength="96" autofocus
      placeholder="file name, hash, signer, company, product, description">
    ${select("state", FILE_STATES, query.state, { flagged: "flagged", any: "any state" })}
    ${select("trust", FILE_TRUSTS, query.trust, { any: "any signature" })}
    ${select("origin", FILE_ORIGINS, query.origin, { any: "anywhere" })}
    ${select("sort", FILE_SORTS, query.sort, {
      state: "worst first",
      last: "most recent",
      accounts: "most accounts",
      hits: "most sightings",
      name: "name",
    })}
    ${select("days", [1, 7, 30, 90, 365], query.days, {}, (d) => `${d}d`)}
    <button type="submit">Search</button>
    ${filtered ? `<a class="clear" href="${esc(href(`${ROOT}/files`, {}, c.key))}">Clear</a>` : ""}
  </form>
  ${count(found, "file name")}
  ${fileTable(found.rows, c, true)}
  ${pager(`${ROOT}/files`, params, found, c)}
</section>`;
  return shell("Files", "files", body, c);
}

function fileTable(rows: FileGroup[], c: Ctx, actions: boolean): string {
  if (!rows.length) return `<p class="muted">No file matches.</p>`;
  return wrap(`<table>
  <thead><tr><th>File</th><th>Signed</th><th>Claims</th><th class="num">Builds</th>
    <th class="num">Accounts</th><th class="num">Seen</th><th>First</th><th>Last</th>
    ${actions ? "<th></th>" : ""}</tr></thead>
  <tbody>${rows
    .map(
      (g) => `<tr>
      <td><span class="dot ${esc(g.state)}"></span>
        <a href="${esc(href(`${ROOT}/file`, { name: g.name }, c.key))}"><code>${esc(g.name)}</code></a>
        ${g.label ? `<span class="muted">${esc(g.label)}</span>` : ""}
        ${g.riderName ? `<span class="muted">— ${esc(g.riderName)}</span>` : ""}</td>
      <td>${sig(g.trust, g.publisher)}</td>
      <td class="muted">${esc(g.claims || "—")}</td>
      <td class="num">${g.variantCount}</td>
      <td class="num">${g.accounts}</td>
      <td class="num">${g.hits.toLocaleString("en-GB")}</td>
      <td class="muted">${esc(ago(g.firstAt))}</td>
      <td class="muted">${esc(ago(g.lastAt))}</td>
      ${actions ? `<td class="act">${ruleForms(hidden("pattern", g.name), g.label || g.name, c)}</td>` : ""}
    </tr>`,
    )
    .join("")}</tbody></table>`);
}

// ---------------------------------------------------------------------------
// One file, backwards
// ---------------------------------------------------------------------------

function fileView(d: FileDetail, c: Ctx): string {
  const params: Params = { name: d.name, sha256: d.sha256 };
  const body = `
<section class="panel">
  <div class="who">
    <span class="dot ${esc(d.state)}"></span>
    <span class="name mono">${esc(d.name)}</span>
    ${d.label ? `<span class="muted">${esc(d.label)}</span>` : ""}
    ${d.sha256 ? `<span class="muted">one build only</span>
      <a class="clear" href="${esc(href(`${ROOT}/file`, { name: d.name }, c.key))}">all builds</a>` : ""}
  </div>
  <dl class="facts">
    ${fact("Accounts", d.accounts.toLocaleString("en-GB"))}
    ${fact("Builds", String(d.variants.length))}
    ${fact("Sightings", d.hits.toLocaleString("en-GB"))}
    ${fact("First", `${ago(d.firstAt)} · ${stamp(d.firstAt)}`)}
    ${fact("Last", `${ago(d.lastAt)} · ${stamp(d.lastAt)}`)}
  </dl>
  <div class="act left">${ruleForms(hidden("pattern", d.name), d.label || d.name, c)}
    <span class="muted">a name rule catches every build, including ones not seen yet</span></div>
</section>

<section class="panel">
  <h2>Builds</h2>
  ${variantTable(d, c)}
</section>

<section class="panel">
  <h2>Who has it</h2>
  ${count(d.holders, "account")}
  ${holderTable(d.holders.rows, c)}
  ${pager(`${ROOT}/file`, params, d.holders, c)}
</section>`;
  return shell(d.name, "files", body, c);
}

function variantTable(d: FileDetail, c: Ctx): string {
  if (!d.variants.length) return `<p class="muted">Nothing recorded.</p>`;
  return wrap(`<table>
  <thead><tr><th>Hash</th><th>Where</th><th>Signed</th><th>Claims</th><th class="num">Size</th>
    <th>Built</th><th class="num">Accounts</th><th class="num">Seen</th><th>Last</th><th></th></tr></thead>
  <tbody>${d.variants
    .map((v: FileVariant) => {
      const only = v.sha256
        ? `<a href="${esc(href(`${ROOT}/file`, { name: d.name, sha256: v.sha256 }, c.key))}">
            <code>${esc(v.sha256.slice(0, 16))}</code></a>`
        : `<code class="muted">not read</code>`;
      return `<tr>
      <td><span class="dot ${esc(v.state)}"></span> ${only}</td>
      <td class="muted">${esc(v.origin)}</td>
      <td>${sig(v.trust, v.publisher)}</td>
      <td class="muted">${esc(claimText(v.description, v.product, v.company) || "—")}</td>
      <td class="num muted">${esc(bytes(v.size))}</td>
      <td class="muted">${esc(v.mtime ? ago(v.mtime * 1000) : "—")}</td>
      <td class="num">${v.accounts}</td>
      <td class="num">${v.hits.toLocaleString("en-GB")}</td>
      <td class="muted">${esc(ago(v.lastAt))}</td>
      <td class="act">${ruleForms(
        v.sha256 ? hidden("sha256", v.sha256) : hidden("pattern", v.name),
        v.label || v.name,
        c,
      )}</td>
    </tr>`;
    })
    .join("")}</tbody></table>`);
}

/** The reverse lookup: everyone whose game has loaded this file. */
function holderTable(rows: Sighting[], c: Ctx): string {
  if (!rows.length) return `<p class="muted">Nobody.</p>`;
  return wrap(`<table>
  <thead><tr><th>Rider</th><th>GUID</th><th>Build</th><th>State</th><th>Where</th><th>Server</th>
    <th>First</th><th>Last</th><th class="num">Seen</th></tr></thead>
  <tbody>${rows
    .map(
      (r) => `<tr>
      <td>${riderLink(r.accountId, r.riderName, c)}</td>
      <td><code class="muted">${esc(r.guid || "—")}</code></td>
      <td><code class="muted">${esc(r.sha256 ? r.sha256.slice(0, 16) : "not read")}</code></td>
      <td><span class="pill ${esc(r.state)}">${esc(r.state)}</span></td>
      <td class="muted">${esc(r.origin)}</td>
      <td class="muted">${esc(r.serverId || "—")}</td>
      <td class="muted">${esc(ago(r.firstAt))}</td>
      <td class="muted">${esc(ago(r.lastAt))}</td>
      <td class="num">${r.hits}</td>
    </tr>`,
    )
    .join("")}</tbody></table>`);
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

function rulesView(v: AdminView, c: Ctx): string {
  const body = `
<section class="panel">
  <h2>Rules <span class="muted">version ${v.rulesVersion}</span></h2>
  ${ruleTable(v.rules, c)}
  <form class="search add" method="post" action="${esc(href(`${ROOT}/rules`, {}, c.key))}">
    ${hidden("back", `${ROOT}/rules${c.key ? `?key=${encodeURIComponent(c.key)}` : ""}`)}
    <select name="kind" aria-label="kind">
      <option value="deny">deny</option>
      <option value="allow">allow</option>
    </select>
    <input name="pattern" placeholder="name contains" maxlength="96">
    <input name="sha256" placeholder="or sha256" maxlength="64">
    <input name="label" placeholder="label" maxlength="96">
    <input name="note" placeholder="note" maxlength="256">
    <button type="submit">Add</button>
  </form>
  <p class="muted">A name or a hash, not both. A name matches as a substring; a hash is exact
    and survives a rename. A deny rule says the person running that file is cheating.</p>
</section>`;
  return shell("Rules", "rules", body, c);
}

function ruleTable(rows: ModuleRule[], c: Ctx): string {
  if (!rows.length) {
    return `<p class="muted">No rules yet — everything from outside the game reads as
      unaccounted for until one says otherwise.</p>`;
  }
  return wrap(`<table>
  <thead><tr><th class="num">#</th><th>Kind</th><th>Matches</th><th>Label</th><th>Note</th>
    <th></th></tr></thead>
  <tbody>${rows
    .map(
      (r) => `<tr>
      <td class="num muted">${r.id}</td>
      <td><span class="pill ${r.kind === "deny" ? "alert" : "ok"}">${esc(r.kind)}</span></td>
      <td><a href="${esc(
        href(`${ROOT}/files`, { q: r.pattern || r.sha256, state: "any" }, c.key),
      )}"><code>${esc(r.pattern || r.sha256)}</code></a></td>
      <td>${esc(r.label)}</td>
      <td class="muted">${esc(r.note ?? "")}</td>
      <td class="act"><form method="post" action="${esc(href(`${ROOT}/rules`, {}, c.key))}">
        ${hidden("back", c.back)}${hidden("action", "delete")}${hidden("id", String(r.id))}
        <button type="submit">Remove</button>
      </form></td>
    </tr>`,
    )
    .join("")}</tbody></table>`);
}

/** The two buttons that turn a row into a rule, wherever the row is drawn. */
function ruleForms(by: string, label: string, c: Ctx): string {
  const one = (kind: string, text: string) => `<form method="post" action="${esc(
    href(`${ROOT}/rules`, {}, c.key),
  )}">${by}${hidden("back", c.back)}${hidden("kind", kind)}${hidden("label", label)}
      <button type="submit" class="${kind}">${text}</button></form>`;
  return `${one("deny", "Flag")}${one("allow", "Clear")}`;
}

// ---------------------------------------------------------------------------
// Bits every view uses
// ---------------------------------------------------------------------------

function hidden(name: string, value: string): string {
  return `<input type="hidden" name="${esc(name)}" value="${esc(value)}">`;
}

function select<T extends string | number>(
  name: string,
  options: readonly T[],
  selected: T,
  labels: Partial<Record<string, string>>,
  format: (value: T) => string = (v) => String(v),
): string {
  const opts = options
    .map(
      (o) =>
        `<option value="${esc(String(o))}"${o === selected ? " selected" : ""}>${esc(
          labels[String(o)] ?? format(o),
        )}</option>`,
    )
    .join("");
  return `<select name="${esc(name)}" aria-label="${esc(name)}">${opts}</select>`;
}

/** "51–100 of 312 riders" — or "10,000+" where the count stopped counting. */
function count(found: Paged<unknown>, noun: string): string {
  const label = `${found.total.toLocaleString("en-GB")}${found.total >= MAX_COUNT ? "+" : ""} ${noun}${
    found.total === 1 ? "" : "s"
  }`;
  const shown = found.rows.length;
  // A hand-edited `?page=` past the end. Say so, rather than showing an empty table that
  // reads as "nothing matched".
  if (!shown) {
    return found.total
      ? `<p class="count">Page ${found.page} is past the end — ${label}.</p>`
      : "";
  }
  const from = (found.page - 1) * found.size + 1;
  return `<p class="count">${from.toLocaleString("en-GB")}–${(from + shown - 1).toLocaleString(
    "en-GB",
  )} of ${label}</p>`;
}

/**
 * Numbered pages, not a scroll that loads as it goes.
 *
 * A link to page four has to still be page four tomorrow, and a page of evidence you cannot
 * link to is not much use to anybody.
 */
function pager(path: string, params: Params, found: Paged<unknown>, c: Ctx): string {
  const pages = Math.max(1, Math.ceil(Math.min(found.total, MAX_COUNT) / found.size));
  if (pages <= 1) return "";

  // Clamped for the arrows, so a `?page=` past the end still has a way back. The highlight
  // is not clamped: nothing is marked current when the page asked for does not exist.
  const at = Math.min(found.page, pages);
  const here = found.page === at;
  const first = Math.max(1, Math.min(at - 3, pages - 6));
  const last = Math.min(pages, first + 6);

  const step = (to: number, text: string, disabled: boolean) =>
    disabled
      ? `<span class="off">${text}</span>`
      : `<a href="${esc(href(path, { ...params, page: to }, c.key))}">${text}</a>`;

  const numbers: string[] = [];
  for (let p = first; p <= last; p++) {
    numbers.push(
      p === at && here
        ? `<span class="on">${p}</span>`
        : `<a href="${esc(href(path, { ...params, page: p }, c.key))}">${p}</a>`,
    );
  }

  return `<nav class="pages">
    ${step(1, "First", at === 1 && here)}
    ${step(at - 1, "‹ Prev", at === 1 && here)}
    ${numbers.join("")}
    ${step(at + 1, "Next ›", at === pages)}
    ${step(pages, "Last", at === pages && here)}
  </nav>`;
}

/** A signature, which is Windows' answer, and who it names. */
function sig(trust: Trust, publisher: string): string {
  const tone = trust === "signed" ? "" : trust === "unchecked" ? "muted" : "warn";
  const who = trust === "signed" && publisher ? ` <span class="muted">${esc(publisher)}</span>` : "";
  return `<span class="sig ${tone}">${esc(trust)}</span>${who}`;
}

/** Wide tables scroll inside their panel rather than pushing the page sideways. */
function wrap(table: string): string {
  return `<div class="scroll">${table}</div>`;
}

/** A size a person reads, not a byte count. */
function bytes(size: number): string {
  if (!size) return "—";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

/** How long ago, in the coarsest unit that still says something. */
function ago(at: number): string {
  if (!at) return "—";
  const secs = Math.max(0, Math.round((Date.now() - at) / 1000));
  if (secs < 90) return `${secs}s ago`;
  const mins = Math.round(secs / 60);
  if (mins < 90) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 48) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/** The absolute time, where "3d ago" is not precise enough to act on. */
function stamp(at: number): string {
  if (!at) return "—";
  return new Date(at).toISOString().replace("T", " ").slice(0, 16) + "Z";
}

function page(status: number, message: string): Response {
  return new Response(
    `<!doctype html><html lang="en"><head><meta charset="utf-8">
<title>MXB App diagnostics</title>
<style>${CSS}</style></head><body><header><h1>Diagnostics</h1></header>
<section class="panel"><p>${esc(message)}</p></section></body></html>`,
    { status, headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" } },
  );
}

function esc(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

const CSS = `
:root{--bg:#f6f7f9;--panel:#fff;--ink:#16181d;--muted:#6b7280;--line:#e3e6ea;--accent:#e2492b;
  --ok:#3f9e5a;--warn:#c98a1b;--alert:#d33b2c}
@media (prefers-color-scheme:dark){
  :root{--bg:#0f1114;--panel:#171a1f;--ink:#e8eaed;--muted:#9aa3ae;--line:#262b32;--accent:#ff6a42;
    --ok:#4fb872;--warn:#e0a53a;--alert:#ff5c4d}
}
*{box-sizing:border-box}
body{margin:0;padding:20px;background:var(--bg);color:var(--ink);
  font:13px/1.5 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
a{color:var(--accent);text-decoration:none}
a:hover{text-decoration:underline}
header{display:flex;align-items:baseline;justify-content:space-between;gap:16px;margin-bottom:14px}
h1{font-size:17px;margin:0;letter-spacing:-.01em;overflow-wrap:anywhere}
h2{font-size:11px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);
  margin:0 0 10px;display:flex;align-items:baseline;gap:8px}
h2 .more{margin-left:auto;text-transform:none;letter-spacing:0;font-size:12px}
header nav a{color:var(--muted);padding:4px 9px;border-radius:6px;margin-left:2px}
header nav a.on{background:var(--accent);color:#fff}
header nav a.on:hover{text-decoration:none}
.ranges{margin:0 0 12px}
.ranges a{color:var(--muted);padding:3px 8px;border-radius:6px;margin-right:2px;font-size:12px}
.ranges a.on{background:var(--accent);color:#fff}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(130px,1fr));gap:10px;margin-bottom:14px}
.tile{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:12px;
  display:flex;flex-direction:column;gap:1px}
.tile.alert{border-color:var(--alert)}
.tile.warn{border-color:var(--warn)}
.tile .n{font-size:24px;font-weight:600;letter-spacing:-.02em}
.tile .l{font-size:12px}
.tile .h,.muted{color:var(--muted);font-size:12px}
.panel{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:14px;
  margin-bottom:14px}
.scroll{overflow-x:auto}
table{width:100%;border-collapse:collapse}
th{text-align:left;font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:var(--muted);
  font-weight:500;padding:0 8px 7px;border-bottom:1px solid var(--line);white-space:nowrap}
td{padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top}
tr:last-child td{border-bottom:0}
.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
.num.hot{color:var(--warn);font-weight:600}
.act{text-align:right;white-space:nowrap}
.act.left{text-align:left;margin-top:10px;display:flex;align-items:center;gap:8px}
.act form{display:inline}
code,.mono{font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;overflow-wrap:anywhere}
.pill{display:inline-block;min-width:9px;padding:1px 7px;border-radius:99px;font-size:11px;
  color:#fff;background:var(--muted)}
.pill.ok{background:var(--ok)}
.pill.warn{background:var(--warn)}
.pill.alert{background:var(--alert)}
.dot{display:inline-block;width:8px;height:8px;border-radius:99px;background:var(--muted);
  vertical-align:baseline}
.dot.ok{background:var(--ok)}
.dot.warn{background:var(--warn)}
.dot.alert{background:var(--alert)}
.tag{display:inline-block;padding:1px 7px;border-radius:99px;font-size:11px;
  border:1px solid var(--line)}
.sig{font-size:12px}
.sig.warn{color:var(--warn);font-weight:600}
button{font:inherit;font-size:12px;padding:3px 10px;border-radius:6px;border:1px solid var(--line);
  background:var(--panel);color:var(--ink);cursor:pointer;margin-left:4px}
button.deny{border-color:var(--alert);color:var(--alert)}
button.allow{border-color:var(--ok);color:var(--ok)}
.search{display:flex;flex-wrap:wrap;gap:8px;align-items:center;margin:0 0 12px}
.search input,.search select{font:inherit;font-size:13px;padding:5px 8px;border-radius:6px;
  border:1px solid var(--line);background:var(--bg);color:var(--ink)}
.search input[name="q"],.search input[name="f"]{flex:1 1 280px;min-width:200px}
.search .clear{font-size:12px;color:var(--muted)}
.add input[name="sha256"]{min-width:240px}
.count{color:var(--muted);font-size:12px;margin:0 0 8px}
.who{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-bottom:12px}
.who .name{font-size:16px;font-weight:600;overflow-wrap:anywhere}
.facts{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:10px 16px;margin:0}
.facts dt{font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:var(--muted)}
.facts dd{margin:1px 0 0;overflow-wrap:anywhere}
.named{margin:12px 0 0}
.pages{display:flex;flex-wrap:wrap;gap:4px;align-items:center;margin-top:12px;font-size:12px}
.pages a,.pages span{padding:3px 9px;border-radius:6px;border:1px solid var(--line);color:var(--muted)}
.pages a{color:var(--ink)}
.pages .on{background:var(--accent);color:#fff;border-color:var(--accent)}
.pages .off{opacity:.4}
footer{color:var(--muted);font-size:12px;margin-top:4px;max-width:70ch}
`;
