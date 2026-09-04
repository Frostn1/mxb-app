/**
 * The chrome every admin page is drawn in.
 *
 * The diagnostics dashboard grew its own stylesheet and its own set of formatters, and the
 * paint view needs exactly the same ones — a second copy would mean the two pages drift
 * apart the first time either is touched. Nothing here knows what it is rendering; it is the
 * shell, the palette and the four ways a number is written for a person.
 */

/** Query values a link carries. `undefined` and `""` are dropped rather than sent empty. */
export type Params = Record<string, string | number | undefined | null>;

/** What every view needs to draw a link back to itself: the key, and where it is. */
export interface Ctx {
  key: string;
  /** The path and query of the page being drawn, for a form to come back to. */
  back: string;
}

export function ctx(url: URL): Ctx {
  return { key: url.searchParams.get("key") ?? "", back: `${url.pathname}${url.search}` };
}

/** Every link carries the admin key: a browser typing a URL cannot send a header. */
export function href(path: string, params: Params, key: string): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "") continue;
    q.set(k, String(v));
  }
  if (key) q.set("key", key);
  const query = q.toString();
  return query ? `${path}?${query}` : path;
}

export function esc(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** A size a person reads, not a byte count. */
export function bytes(size: number): string {
  if (!size) return "—";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`;
  if (size < 1024 * 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)} MB`;
  return `${(size / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** How long ago, in the coarsest unit that still says something. */
export function ago(at: number): string {
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
export function stamp(at: number): string {
  if (!at) return "—";
  return new Date(at).toISOString().replace("T", " ").slice(0, 16) + "Z";
}

/** Wide tables scroll inside their panel rather than pushing the page sideways. */
export function wrap(table: string): string {
  return `<div class="scroll">${table}</div>`;
}

/** The page a gate returns: no key configured, or the wrong one presented. */
export function errorPage(title: string, status: number, message: string): Response {
  return new Response(
    `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>${esc(title)} — MXB control plane</title>
<style>${CSS}</style></head><body>
<header class="top"><span class="mark">MXB <span>Control</span></span></header>
<h1>${esc(title)}</h1>
<section class="panel"><p>${esc(message)}</p></section></body></html>`,
    { status, headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" } },
  );
}

// ---------------------------------------------------------------------------
// The shell
// ---------------------------------------------------------------------------

/** The three dashboards. One tab each, on every page. */
export type Section = "usage" | "diagnostics" | "paints";

// Usage is `/admin` rather than `/admin/usage`: the front door and the first tab are the same
// page, so the URL to bookmark is the one every link uses. `/admin/usage` still answers, for
// the bookmarks that predate this.
const SECTIONS: readonly (readonly [Section, string, string])[] = [
  ["usage", "Usage", "/admin"],
  ["diagnostics", "Diagnostics", "/admin/diagnostics"],
  ["paints", "Paints", "/admin/paints"],
];

/** A view inside a section — the second row of the header. */
export interface SubTab {
  id: string;
  text: string;
  path: string;
  params?: Params;
}

export interface Page {
  title: string;
  /** Which top tab is lit. `null` on search, which belongs to none of them. */
  section: Section | null;
  tabs?: SubTab[];
  /** Which of `tabs` is lit. */
  current?: string;
  /** The right-hand end of the second row — the day-window switcher, where there is one. */
  aside?: string;
  body: string;
  footer?: string;
  /** What the search box holds, so a search page keeps its own query in it. */
  q?: string;
  c: Ctx;
}

/**
 * The one document every admin page is drawn into.
 *
 * These were three pages with three headers and, in the usage page's case, a second copy of
 * the stylesheet — so they drifted, and answering one question meant three browser tabs and
 * three search boxes. They are one tool: the same key, the same accounts, the same
 * questions. The tabs and the search box are therefore on every page, which is what makes
 * one browser tab enough.
 */
export function shell(p: Page): string {
  const sections = SECTIONS.map(
    ([id, text, path]) =>
      `<a class="${id === p.section ? "on" : ""}" href="${esc(href(path, {}, p.c.key))}">${text}</a>`,
  ).join("");

  const sub = (p.tabs ?? [])
    .map(
      (t) =>
        `<a class="${t.id === p.current ? "on" : ""}" href="${esc(
          href(t.path, t.params ?? {}, p.c.key),
        )}">${esc(t.text)}</a>`,
    )
    .join("");

  const bar =
    sub || p.aside
      ? `<div class="subbar"><nav class="sub">${sub}</nav>` +
        `<div class="aside">${p.aside ?? ""}</div></div>`
      : "";

  return `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>${esc(p.title)} — MXB control plane</title>
<style>${CSS}</style>
</head><body>
<header class="top">
  <a class="mark" href="${esc(href("/admin", {}, p.c.key))}">MXB <span>Control</span></a>
  <nav class="sections">${sections}</nav>
  ${findBox(p.q ?? "", p.c)}
</header>
${bar}
<h1>${esc(p.title)}</h1>
${p.body}
${p.footer ? `<footer class="muted">${p.footer}</footer>` : ""}
</body></html>`;
}

/** The search box that sits in every header. */
function findBox(q: string, c: Ctx): string {
  return `<form class="find" method="get" action="/admin/search" role="search">
  ${c.key ? `<input type="hidden" name="key" value="${esc(c.key)}">` : ""}
  <input name="q" type="search" value="${esc(q)}" maxlength="96" autocomplete="off"
    aria-label="Search" placeholder="Search riders, mod files, paints…">
</form>`;
}

/** The day-window switcher, for the pages that have one. */
export function ranges(
  days: number,
  options: readonly number[],
  path: string,
  params: Params,
  c: Ctx,
): string {
  return options
    .map(
      (d) =>
        `<a class="${d === days ? "on" : ""}" href="${esc(
          href(path, { ...params, days: d, page: undefined }, c.key),
        )}">${d}d</a>`,
    )
    .join("");
}

// ---------------------------------------------------------------------------
// Paging
// ---------------------------------------------------------------------------

/** How many rows a listing shows at once. */
export const PAGE_SIZE = 50;

/** The most rows a count query will count exactly. Past it the page says "10,000+". */
export const MAX_COUNT = 10_000;

export interface Paged<T> {
  rows: T[];
  /** Matching rows, counted up to `MAX_COUNT`. */
  total: number;
  /** 1-based. */
  page: number;
  size: number;
}

/** `?page=` as a 1-based page number. Anything unusable is page 1. */
export function parsePage(value: string | null): number {
  const asked = Number(value ?? "1");
  if (!Number.isFinite(asked)) return 1;
  return Math.min(10_000, Math.max(1, Math.trunc(asked)));
}

/**
 * What was typed in a search box, as a LIKE pattern.
 *
 * The wildcards are escaped, so a typed underscore matches an underscore rather than
 * anything at all — which matters when the thing being searched for is a file name.
 */
export function likeTerm(query: string): string {
  const trimmed = query.trim().slice(0, 96);
  if (!trimmed) return "";
  return `%${trimmed.replace(/[\\%_]/g, (c) => `\\${c}`)}%`;
}

/** The row range and total above a table. */
export function count(found: Paged<unknown>, noun: string): string {
  const label = `${found.total.toLocaleString("en-GB")}${found.total >= MAX_COUNT ? "+" : ""} ${noun}${
    found.total === 1 ? "" : "s"
  }`;
  const shown = found.rows.length;
  // A hand-edited `?page=` past the end. Say so, rather than showing an empty table that
  // reads as "nothing matched".
  if (!shown) {
    return found.total ? `<p class="count">Page ${found.page} is past the end — ${label}.</p>` : "";
  }
  const from = (found.page - 1) * found.size + 1;
  return `<p class="count">${from.toLocaleString("en-GB")}–${(from + shown - 1).toLocaleString(
    "en-GB",
  )} of ${label}</p>`;
}

/** The page links under a table. Nothing at all when everything fits on one. */
export function pager(path: string, params: Params, found: Paged<unknown>, c: Ctx): string {
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

export const CSS = `
:root{--bg:#f6f7f9;--panel:#fff;--ink:#16181d;--muted:#6b7280;--line:#e3e6ea;--accent:#e2492b;
  --ok:#3f9e5a;--warn:#c98a1b;--alert:#d33b2c;--check:#dfe3e8;--check2:#bcc3cc}
@media (prefers-color-scheme:dark){
  :root{--bg:#0f1114;--panel:#171a1f;--ink:#e8eaed;--muted:#9aa3ae;--line:#262b32;--accent:#ff6a42;
    --ok:#4fb872;--warn:#e0a53a;--alert:#ff5c4d;--check:#20242a;--check2:#2b3038}
}
*{box-sizing:border-box}
body{margin:0;padding:20px;background:var(--bg);color:var(--ink);
  font:13px/1.5 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
a{color:var(--accent);text-decoration:none}
a:hover{text-decoration:underline}
h1{font-size:19px;margin:16px 0 12px;letter-spacing:-.015em;overflow-wrap:anywhere}
h2{font-size:11px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);
  margin:0 0 10px;display:flex;align-items:baseline;gap:8px}
h2 .more{margin-left:auto;text-transform:none;letter-spacing:0;font-size:12px}
/* A line under a heading saying what the panel counts, pulled up against it. */
.hint{margin:-8px 0 12px}

/* Chrome. Two rows: the three dashboards and the search box, then the section's own views.
   The first row is sticky, so the way out of a page is still there four hundred rows down. */
.top{position:sticky;top:0;z-index:5;display:flex;align-items:center;gap:6px 14px;
  flex-wrap:wrap;padding:9px 20px;margin:-20px -20px 0;
  background:var(--panel);border-bottom:1px solid var(--line)}
.mark{color:var(--ink);font-weight:600;letter-spacing:-.01em;white-space:nowrap}
.mark span{color:var(--accent)}
.mark:hover{text-decoration:none}
.sections{display:flex;gap:2px}
.sections a{color:var(--muted);padding:5px 11px;border-radius:7px;font-weight:500}
.sections a:hover{background:var(--bg);text-decoration:none}
.sections a.on{background:var(--accent);color:#fff}
.sections a.on:hover{background:var(--accent)}
.find{margin-left:auto;flex:0 1 300px;min-width:170px;display:flex}
.find input{width:100%;font:inherit;font-size:13px;padding:6px 10px;border-radius:7px;
  border:1px solid var(--line);background:var(--bg);color:var(--ink)}
.find input:focus{outline:none;border-color:var(--accent)}
.subbar{display:flex;align-items:center;gap:8px;flex-wrap:wrap;padding:7px 20px;margin:0 -20px;
  border-bottom:1px solid var(--line)}
.sub{display:flex;flex-wrap:wrap;gap:2px}
.sub a,.subbar .aside a{color:var(--muted);padding:3px 9px;border-radius:6px;font-size:12px}
.sub a:hover,.subbar .aside a:hover{color:var(--ink);text-decoration:none}
.sub a.on{color:var(--ink);background:var(--panel);box-shadow:inset 0 0 0 1px var(--line)}
.subbar .aside{margin-left:auto;display:flex;gap:2px}
.subbar .aside a.on{background:var(--accent);color:#fff}
.subbar .aside a.on:hover{color:#fff}
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
td{padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top;position:relative}
tr:last-child td{border-bottom:0}
.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
/* A sortable header is a link that still reads as a header until it is the one in use. */
th a{color:inherit;display:inline-block}
th a:hover{color:var(--accent);text-decoration:none}
th.on a{color:var(--accent)}
th .dir{margin-left:3px;font-size:10px;vertical-align:1px}
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
/* The same rider on the other dashboard. Pushed right, so it reads as a way out of this
   page rather than as another fact about them. */
.who .aside{margin-left:auto;font-size:12px}
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
/* Paint sheets carry alpha, so they sit on a checkerboard rather than on the panel — an
   all-white plastics sheet and a transparent one are otherwise the same picture. */
.thumb{display:block;width:64px;height:64px;border-radius:6px;border:1px solid var(--line);
  object-fit:contain;background-color:var(--check);
  background-image:linear-gradient(45deg,var(--check2) 25%,transparent 25%,transparent 75%,var(--check2) 75%),
    linear-gradient(45deg,var(--check2) 25%,transparent 25%,transparent 75%,var(--check2) 75%);
  background-size:12px 12px;background-position:0 0,6px 6px}
.thumb.big{width:132px;height:132px}
a.thumblink{display:inline-block}

/* The usage view's chart furniture, in the shared sheet rather than in a second stylesheet
   of its own — that second copy is how the three pages stopped looking like one tool. */
.cols{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:14px}
svg.chart{width:100%;height:180px;display:block}
.area{fill:var(--accent);opacity:.12}
.line{fill:none;stroke:var(--accent);stroke-width:2;stroke-linejoin:round}
.tick{fill:var(--muted);font-size:11px}
.tick.end{text-anchor:end}
.bars{list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:8px}
.bars li{display:grid;grid-template-columns:1fr 2fr auto;gap:10px;align-items:center}
.bars .k{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.bars .bar{background:var(--line);border-radius:4px;height:8px;overflow:hidden}
.bars .bar i{display:block;height:100%;background:var(--accent)}
.bars .v{color:var(--muted);font-variant-numeric:tabular-nums}
/* Reach drawn behind the row it belongs to, so the ranking reads without a second column. */
.rowbar{position:absolute;left:0;top:2px;bottom:2px;background:var(--accent);opacity:.10;
  border-radius:4px}
.rowbar+code{position:relative}
.unused{list-style:none;margin:0;padding:0;display:flex;flex-wrap:wrap;gap:8px}
.unused li{border:1px solid var(--line);border-radius:6px;padding:3px 8px}

/* Search hits: a row that is a link with a line of detail under it. */
.hits{list-style:none;margin:0;padding:0;display:flex;flex-direction:column}
.hits li{padding:7px 0;border-bottom:1px solid var(--line);display:flex;gap:10px;
  align-items:center}
.hits li:last-child{border-bottom:0}
.hits .name{font-weight:500;overflow-wrap:anywhere}
.hits .meta{color:var(--muted);font-size:12px;overflow-wrap:anywhere}
.hits .to{margin-left:auto;font-size:12px;white-space:nowrap;padding-left:8px}
.hits .thumb{width:40px;height:40px;flex:0 0 auto}
.tips{color:var(--muted);margin:0}
.tips code{color:var(--ink)}
`;
