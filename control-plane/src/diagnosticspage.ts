/**
 * The client diagnostics dashboard, rendered on the server.
 *
 * Plain HTML with its CSS inline and no scripts, for the reason the usage page gives: a page
 * behind an admin key that pulls anything from someone else's host stops working the day
 * that host does, and this is the only view of this data there is.
 *
 * It is a working surface rather than a report. The list of files nothing accounts for is
 * next to the buttons that account for them — see something, name it or clear it, and the
 * next report from every install reads it the new way. That loop is the feature; a page that
 * only displayed would leave the rule list permanently empty.
 */

import {
  addRule,
  collectAdminView,
  deleteRule,
  stateRank,
  type AdminView,
  type LiveRow,
  type ModuleRule,
  type SeenGroup,
  type SeenVariant,
  type State,
  type Trust,
} from "./diagnostics";
import { adminAllowed, windowDays } from "./usage";

/** Windows the header offers. Anything else still works via `?days=`. */
const RANGES = [1, 7, 30, 90];

export async function diagnosticsDashboard(
  request: Request,
  url: URL,
  env: Env,
): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return page(503, "No admin key is configured on this deployment.");
  if (allowed === "denied") return page(401, "Unauthorized.");

  const days = windowDays(url);
  const view = await collectAdminView(env, days);
  return new Response(render(view, url, days), {
    status: 200,
    headers: {
      "content-type": "text/html; charset=utf-8",
      // About named people, behind a key: never cached by anything in between.
      "cache-control": "no-store",
    },
  });
}

/**
 * The rule buttons post here and come back to the page.
 *
 * A 303 rather than a rendered result, so a refresh does not re-submit — this is the one
 * surface where a double-click would silently add a rule twice.
 */
export async function diagnosticsRules(request: Request, url: URL, env: Env): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return page(503, "No admin key is configured on this deployment.");
  if (allowed === "denied") return page(401, "Unauthorized.");

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return page(400, "That was not a form.");
  }
  const back = `/admin/diagnostics${url.search}`;
  const field = (name: string) => String(form.get(name) ?? "");

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

function render(v: AdminView, url: URL, days: number): string {
  const key = url.searchParams.get("key");
  const suffix = key ? `&key=${encodeURIComponent(key)}` : "";
  const href = (d: number) => `?days=${d}${suffix}`;
  const action = `/admin/diagnostics/rules?days=${days}${suffix}`;

  const alerts = v.live.filter((r) => worst(r) === "alert");
  const warns = v.live.filter((r) => worst(r) === "warn");
  const blind = v.live.filter((r) => r.state === "unknown");

  return `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>MXB App client diagnostics</title>
<style>${CSS}</style>
</head><body>
<header>
  <h1>Client diagnostics</h1>
  <nav>${RANGES.map(
    (d) => `<a class="${d === days ? "on" : ""}" href="${esc(href(d))}">${d}d</a>`,
  ).join("")}</nav>
</header>

<section class="tiles">
  ${tile("Reporting now", v.reporting, "clients that reported in the last 10 minutes")}
  ${tile("Alerts", alerts.length, "a rule named something in their game", alerts.length ? "alert" : "")}
  ${tile("Unaccounted", warns.length, "something loaded that no rule accounts for", warns.length ? "warn" : "")}
  ${tile("Could not look", blind.length, "the app could not read the module list")}
  ${tile("Rules", v.rules.length, `version ${v.rulesVersion}`)}
</section>

<section class="panel">
  <h2>Reporting now</h2>
  ${live(v.live)}
</section>

<section class="panel">
  <h2>Files nothing accounts for
    <span class="muted">— last ${days}d, by file name, most recent first</span></h2>
  <p class="muted">One row per file name. Accounts is how many distinct people have loaded
    it — one out of many is the interesting shape; many is a driver or an overlay, and worth
    clearing so it stops filling this list. Open a row for each distinct build of that file:
    who signed it, what it claims to be, and its hash.</p>
  <p class="muted"><b>Signed</b> is Windows' answer and means something. <b>Company</b> and
    <b>product</b> are what the file says about itself, which anything can write — good for
    recognising the ordinary, worthless as evidence about the unusual.</p>
  ${trimmed(v)}
  ${sightings(v.seen, action)}
</section>

<section class="panel">
  <h2>Rules</h2>
  ${rules(v.rules, action)}
  <form class="add" method="post" action="${esc(action)}">
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
  <p class="muted">A name or a hash, not both. A name matches as a substring, so
    <code>trainer</code> catches <code>trainer_v3.dll</code>. A hash is exact and survives a
    rename. Adding a deny rule says the person running that file is cheating — be sure.</p>
</section>

<footer class="muted">
  Only clients running the app report, so this says what we have been told and never what is
  true of everybody. A missing row is the ordinary case, not a suspicious one, and someone
  who never installs the app never appears here at all.
  Generated ${esc(new Date().toISOString().replace("T", " ").slice(0, 16))} UTC.
</footer>
</body></html>`;
}

/** The worse of what a client says now and the worst it said inside the window. */
function worst(row: LiveRow): State {
  return stateRank(row.worstState) > stateRank(row.state) ? row.worstState : row.state;
}

function tile(label: string, value: number, hint: string, tone = ""): string {
  return `<div class="tile ${tone}"><span class="n">${value.toLocaleString("en-GB")}</span>
    <span class="l">${esc(label)}</span><span class="h">${esc(hint)}</span></div>`;
}

function live(rows: LiveRow[]): string {
  if (!rows.length) return `<p class="muted">Nobody is reporting right now.</p>`;
  return `<table>
  <thead><tr><th>Rider</th><th>State</th><th>Server</th><th>Found</th>
    <th class="num">Unaccounted</th><th class="num">Modules</th><th>App</th><th>Seen</th></tr></thead>
  <tbody>${rows
    .map((r) => {
      const now = r.state;
      const peak = worst(r);
      const drift = peak !== now ? ` <span class="muted">(was ${esc(peak)})</span>` : "";
      const found = r.matched.length
        ? r.matched.map((m) => `<code>${esc(m.label || m.name)}</code>`).join(" ")
        : `<span class="muted">—</span>`;
      return `<tr>
      <td>${esc(r.riderName)}</td>
      <td><span class="pill ${esc(peak)}">${esc(now)}</span>${drift}</td>
      <td class="muted">${esc(r.serverId || "—")}</td>
      <td>${found}</td>
      <td class="num">${r.unknownCount}</td>
      <td class="num">${r.moduleCount}</td>
      <td class="muted">${esc(r.appVersion || "—")}</td>
      <td class="muted">${esc(ago(r.updatedAt))}</td>
    </tr>`;
    })
    .join("")}</tbody></table>`;
}

/** Says what the page is not showing, when a cap trimmed it. Silence would read as "all". */
function trimmed(v: AdminView): string {
  const shown = v.seen.length;
  if (v.seenNamesTotal <= shown) return "";
  return `<p class="warnline">Showing ${shown} of ${v.seenNamesTotal} file names. Clear the
    ordinary ones and the rest come into view.</p>`;
}

function sightings(groups: SeenGroup[], action: string): string {
  if (!groups.length) return `<p class="muted">Nothing unaccounted for in this window.</p>`;
  return `<table class="seen">
  <thead><tr><th>File</th><th>Signed</th><th>Claims</th><th class="num">Builds</th>
    <th class="num">Accounts</th><th class="num">Seen</th><th>Last</th><th></th></tr></thead>
  <tbody>${groups.map((g) => group(g, action)).join("")}</tbody></table>`;
}

/**
 * One file name, with its builds folded underneath.
 *
 * `<details>` rather than a script: this page has never had one and is not going to start —
 * see the note at the top. The summary row carries the whole group's answer, so the common
 * case is read without opening anything.
 */
function group(g: SeenGroup, action: string): string {
  const label = g.label ? ` <span class="muted">${esc(g.label)}</span>` : "";
  // Named only when one person has it. With several, naming whichever row sorted first
  // would read as an accusation of that one.
  const rider =
    g.accounts === 1 && g.variants[0]?.riderName
      ? ` <span class="muted">— ${esc(g.variants[0].riderName)}</span>`
      : "";
  const detail =
    g.variants.length > 0
      ? `<details><summary class="muted">${g.variants.length} build${
          g.variants.length === 1 ? "" : "s"
        }${
          g.variantCount > g.variants.length ? ` of ${g.variantCount}` : ""
        }</summary>${variants(g.variants, action)}</details>`
      : "";
  return `<tr>
    <td><span class="pill ${esc(g.state)}"></span> <code>${esc(g.name)}</code>${label}${rider}
      ${detail}</td>
    <td>${trust(g.variants)}</td>
    <td class="muted">${esc(claims(g.variants) || "—")}</td>
    <td class="num">${g.variantCount}</td>
    <td class="num">${g.accounts}</td>
    <td class="num">${g.hits}</td>
    <td class="muted">${esc(ago(g.lastAt))}</td>
    <td class="act">${nameButtons(g, action)}</td>
  </tr>`;
}

/**
 * The group's signature answer, which is the worst of its builds.
 *
 * One unsigned build under a name whose other builds are signed is exactly the case worth
 * seeing, so the summary must not average it away.
 */
function trust(variants: SeenVariant[]): string {
  const rank: Record<Trust, number> = { signed: 0, unchecked: 1, unsigned: 2, untrusted: 3 };
  let worst: SeenVariant | undefined;
  for (const v of variants) {
    if (!worst || rank[v.trust] > rank[worst.trust]) worst = v;
  }
  if (!worst) return `<span class="muted">—</span>`;
  const tone = worst.trust === "signed" ? "" : worst.trust === "unchecked" ? "muted" : "warn";
  const who = worst.trust === "signed" && worst.publisher ? esc(worst.publisher) : "";
  return `<span class="sig ${tone}">${esc(worst.trust)}</span>${
    who ? ` <span class="muted">${who}</span>` : ""
  }`;
}

/** What the file says it is: the first description or company any build carries. */
function claims(variants: SeenVariant[]): string {
  for (const v of variants) {
    const said = v.description || v.product || v.company;
    if (said) return v.company && v.company !== said ? `${said} — ${v.company}` : said;
  }
  return "";
}

function variants(rows: SeenVariant[], action: string): string {
  return `<table class="sub">
  <thead><tr><th>Hash</th><th>Where</th><th>Signed</th><th>Claims</th><th class="num">Size</th>
    <th>Built</th><th class="num">Accounts</th><th></th></tr></thead>
  <tbody>${rows
    .map((r) => {
      const said = [r.description, r.product, r.company].filter(Boolean).join(" · ");
      const who = r.trust === "signed" && r.publisher ? ` ${esc(r.publisher)}` : "";
      return `<tr>
      <td><code class="muted">${esc(r.sha256 ? r.sha256.slice(0, 16) : "not read")}</code></td>
      <td class="muted">${esc(r.origin)}</td>
      <td class="muted">${esc(r.trust)}${who}</td>
      <td class="muted">${esc(said || "—")}</td>
      <td class="num muted">${esc(bytes(r.size))}</td>
      <td class="muted">${esc(r.mtime ? ago(r.mtime * 1000) : "—")}</td>
      <td class="num">${r.accounts}</td>
      <td class="act">${hashButtons(r, action)}</td>
    </tr>`;
    })
    .join("")}</tbody></table>`;
}

/** A size a person reads, not a byte count. */
function bytes(size: number): string {
  if (!size) return "—";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * The two buttons that turn a whole name into a rule.
 *
 * By name, because that is the unit this row is: it catches every build under it, including
 * the ones the window has not seen yet. A build that needs its own answer has its own
 * buttons inside the row.
 */
function nameButtons(g: SeenGroup, action: string): string {
  return ruleForms(
    `<input type="hidden" name="pattern" value="${esc(g.name)}">`,
    g.label || g.name,
    action,
  );
}

/**
 * The same two buttons for one build.
 *
 * By hash when there is one, by name when there is not: a hash is what makes a rule survive
 * a rename, and a name is all a file we could not read ever gives us.
 */
function hashButtons(row: SeenVariant, action: string): string {
  const by = row.sha256
    ? `<input type="hidden" name="sha256" value="${esc(row.sha256)}">`
    : `<input type="hidden" name="pattern" value="${esc(row.name)}">`;
  return ruleForms(by, row.label || row.name, action);
}

function ruleForms(by: string, label: string, action: string): string {
  const one = (kind: string, text: string) => `<form method="post" action="${esc(action)}">
      ${by}<input type="hidden" name="kind" value="${kind}">
      <input type="hidden" name="label" value="${esc(label)}">
      <button type="submit" class="${kind}">${text}</button>
    </form>`;
  return `${one("deny", "Flag")}${one("allow", "Clear")}`;
}

function rules(rows: ModuleRule[], action: string): string {
  if (!rows.length) {
    return `<p class="muted">No rules yet — everything from outside the game reads as
      unaccounted for until one says otherwise.</p>`;
  }
  return `<table>
  <thead><tr><th class="num">#</th><th>Kind</th><th>Matches</th><th>Label</th><th></th></tr></thead>
  <tbody>${rows
    .map(
      (r) => `<tr>
      <td class="num muted">${r.id}</td>
      <td><span class="pill ${r.kind === "deny" ? "alert" : "ok"}">${esc(r.kind)}</span></td>
      <td><code>${esc(r.pattern || r.sha256)}</code></td>
      <td>${esc(r.label)}</td>
      <td class="act"><form method="post" action="${esc(action)}">
        <input type="hidden" name="action" value="delete">
        <input type="hidden" name="id" value="${r.id}">
        <button type="submit">Remove</button>
      </form></td>
    </tr>`,
    )
    .join("")}</tbody></table>`;
}

/** How long ago, in the coarsest unit that still says something. */
function ago(at: number): string {
  const secs = Math.max(0, Math.round((Date.now() - at) / 1000));
  if (secs < 90) return `${secs}s ago`;
  const mins = Math.round(secs / 60);
  if (mins < 90) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 48) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

function page(status: number, message: string): Response {
  return new Response(
    `<!doctype html><html lang="en"><head><meta charset="utf-8">
<title>MXB App client diagnostics</title>
<style>${CSS}</style></head><body><header><h1>Client diagnostics</h1></header>
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
body{margin:0;padding:24px;background:var(--bg);color:var(--ink);
  font:14px/1.5 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
header{display:flex;align-items:baseline;justify-content:space-between;gap:16px;margin-bottom:20px}
h1{font-size:18px;margin:0;letter-spacing:-.01em}
h2{font-size:13px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);margin:0 0 12px}
nav a{color:var(--muted);text-decoration:none;padding:4px 8px;border-radius:6px;margin-left:4px}
nav a.on{background:var(--accent);color:#fff}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin-bottom:16px}
.tile{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:14px;
  display:flex;flex-direction:column;gap:2px}
.tile.alert{border-color:var(--alert)}
.tile.warn{border-color:var(--warn)}
.tile .n{font-size:26px;font-weight:600;letter-spacing:-.02em}
.tile .l{font-size:13px}
.tile .h,.muted{color:var(--muted);font-size:12px}
.panel{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:16px;margin-bottom:16px}
table{width:100%;border-collapse:collapse}
th{text-align:left;font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:var(--muted);
  font-weight:500;padding:0 8px 8px;border-bottom:1px solid var(--line)}
td{padding:7px 8px;border-bottom:1px solid var(--line);vertical-align:top}
tr:last-child td{border-bottom:0}
.num{text-align:right;font-variant-numeric:tabular-nums}
.act{text-align:right;white-space:nowrap}
.act form{display:inline}
code{font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace}
.pill{display:inline-block;min-width:10px;padding:1px 7px;border-radius:99px;font-size:11px;
  color:#fff;background:var(--muted)}
.pill.ok{background:var(--ok)}
.pill.warn{background:var(--warn)}
.pill.alert{background:var(--alert)}
button{font:inherit;font-size:12px;padding:3px 10px;border-radius:6px;border:1px solid var(--line);
  background:var(--panel);color:var(--ink);cursor:pointer;margin-left:4px}
button.deny{border-color:var(--alert);color:var(--alert)}
button.allow{border-color:var(--ok);color:var(--ok)}
.add{display:flex;flex-wrap:wrap;gap:8px;margin-top:14px}
.add input,.add select{font:inherit;font-size:13px;padding:4px 8px;border-radius:6px;
  border:1px solid var(--line);background:var(--bg);color:var(--ink);min-width:120px}
.add input[name="sha256"]{min-width:260px}
footer{color:var(--muted);font-size:12px;margin-top:8px;max-width:70ch}

/* One file name, with its builds folded under it. */
.warnline{color:var(--warn);font-size:12px;margin:0 0 12px}
.sig{font-size:12px}
.sig.warn{color:var(--warn);font-weight:600}
details{margin-top:4px}
summary{cursor:pointer;font-size:12px;list-style:none}
summary::-webkit-details-marker{display:none}
summary::before{content:"\\25b8 ";display:inline-block;transition:transform .1s}
details[open] summary::before{content:"\\25be "}
/* The nested table is inside a cell, so it gets its own frame to sit apart from the row. */
table.sub{margin:8px 0 4px;background:var(--bg);border:1px solid var(--line);border-radius:8px}
table.sub th{padding:6px 8px 6px}
table.sub td{padding:5px 8px;font-size:12px}
table.sub tr:last-child td{border-bottom:0}
table.seen>tbody>tr>td{vertical-align:top}
`;
