/**
 * The usage dashboard, rendered on the server.
 *
 * Plain HTML with its CSS inline and its one chart drawn as SVG. No scripts and no CDN,
 * because a page behind an admin key that pulls a charting library from someone else's host
 * is a page that stops working the day that host does — and this is the only view of the
 * numbers there is.
 */

import { adminAllowed, collectStats, windowDays, type Bucket, type EventRow, type Stats } from "./usage";

/** Windows the header offers. Anything else still works via `?days=`. */
const RANGES = [7, 30, 90, 365];

export async function usageDashboard(request: Request, url: URL, env: Env): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return page(503, "No admin key is configured on this deployment.");
  if (allowed === "denied") return page(401, "Unauthorized.");

  const days = windowDays(url);
  const stats = await collectStats(env, days);
  return new Response(render(stats, url), {
    status: 200,
    headers: {
      "content-type": "text/html; charset=utf-8",
      // Numbers about people, behind a key: never cached by anything in between.
      "cache-control": "no-store",
    },
  });
}

function render(s: Stats, url: URL): string {
  const key = url.searchParams.get("key");
  const href = (d: number) => `?days=${d}${key ? `&key=${encodeURIComponent(key)}` : ""}`;
  const pages = s.events.filter((e) => e.name.startsWith("view."));
  const features = s.events.filter((e) => !e.name.startsWith("view.") && !e.name.startsWith("app."));
  const lifecycle = s.events.filter((e) => e.name.startsWith("app."));

  return `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>MXB App usage</title>
<style>${CSS}</style>
</head><body>
<header>
  <h1>MXB App usage</h1>
  <nav>${RANGES.map(
    (d) => `<a class="${d === s.days ? "on" : ""}" href="${esc(href(d))}">${d}d</a>`,
  ).join("")}</nav>
</header>

<section class="tiles">
  ${tile("Active today", s.active.day, "installs that reported in on today's UTC day")}
  ${tile("Active this week", s.active.week, "distinct installs over 7 days")}
  ${tile("Active this month", s.active.month, "distinct installs over 30 days")}
  ${tile("Installs ever", s.installsEver, "distinct installs, all time")}
  ${tile("New in window", s.newInstalls, `first seen within the last ${s.days} days`)}
  ${tile("Sessions", s.sessions, "app launches in the window")}
  ${tile("Hours open", Math.round(s.minutes / 60), "time the app was running")}
</section>

<section class="panel">
  <h2>Active installs per day</h2>
  ${chart(s)}
</section>

<div class="cols">
  ${buckets("Version now", s.currentVersions, "what each install last reported — one vote each")}
  ${buckets("Versions seen", s.versions, "ran it at any point; an install that updated is in both")}
  ${buckets("Platform", s.platforms)}
  ${buckets("Title", s.games)}
</div>

<section class="panel">
  <h2>Pages</h2>
  ${table(pages, s.active.month)}
</section>

<section class="panel">
  <h2>Features</h2>
  ${table(features, s.active.month)}
</section>

${lifecycle.length ? `<section class="panel"><h2>Lifecycle</h2>${table(lifecycle, s.active.month)}</section>` : ""}

<section class="panel">
  <h2>Never touched <span class="muted">— in this window</span></h2>
  ${
    s.unused.length
      ? `<ul class="unused">${s.unused.map((n) => `<li><code>${esc(n)}</code></li>`).join("")}</ul>`
      : `<p class="muted">Everything the app can report has been reported at least once.</p>`
  }
</section>

<footer class="muted">
  Anonymous counts keyed on a random install id. No rider names, no paths, no addresses.
  Generated ${esc(new Date(s.generatedAt).toISOString().replace("T", " ").slice(0, 16))} UTC.
</footer>
</body></html>`;
}

function tile(label: string, value: number, hint: string): string {
  return `<div class="tile"><span class="n">${value.toLocaleString("en-GB")}</span>
    <span class="l">${esc(label)}</span><span class="h">${esc(hint)}</span></div>`;
}

/**
 * The daily line.
 *
 * Drawn against a zero baseline on purpose: the shape of "are more people using this than
 * last month" is the whole point, and an axis that starts at the minimum makes noise look
 * like a trend.
 */
function chart(s: Stats): string {
  if (s.daily.length < 2) {
    return `<p class="muted">Not enough days yet — the line needs two.</p>`;
  }
  const w = 960;
  const h = 180;
  const pad = 24;
  const max = Math.max(1, ...s.daily.map((d) => d.installs));
  const step = (w - pad * 2) / (s.daily.length - 1);
  const points = s.daily.map((d, i) => {
    const x = pad + i * step;
    const y = h - pad - (d.installs / max) * (h - pad * 2);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  const area = `M${pad},${h - pad} L${points.join(" L")} L${(pad + (s.daily.length - 1) * step).toFixed(1)},${h - pad} Z`;
  const first = s.daily[0];
  const last = s.daily[s.daily.length - 1];

  return `<svg viewBox="0 0 ${w} ${h}" role="img" aria-label="active installs per day" preserveAspectRatio="none">
  <path d="${area}" class="area"/>
  <polyline points="${points.join(" ")}" class="line"/>
  <text x="${pad}" y="14" class="tick">${max}</text>
  <text x="${pad}" y="${h - 6}" class="tick">${esc(first.day.slice(5))}</text>
  <text x="${w - pad}" y="${h - 6}" class="tick end">${esc(last.day.slice(5))}</text>
</svg>`;
}

function buckets(title: string, rows: Bucket[], hint?: string): string {
  const head = `<h2>${esc(title)}</h2>${hint ? `<p class="muted hint">${esc(hint)}</p>` : ""}`;
  if (!rows.length) return `<section class="panel">${head}<p class="muted">Nothing yet.</p></section>`;
  const max = Math.max(...rows.map((r) => r.installs));
  return `<section class="panel">${head}<ul class="bars">${rows
    .map(
      (r) => `<li><span class="k">${esc(r.label)}</span>
        <span class="bar"><i style="width:${((r.installs / max) * 100).toFixed(1)}%"></i></span>
        <span class="v">${r.installs}</span></li>`,
    )
    .join("")}</ul></section>`;
}

/**
 * Reach beside volume, ranked by reach.
 *
 * `share` is reach as a fraction of the month's active installs — "one in five people opened
 * this" reads as a decision where a bare count does not.
 */
function table(rows: EventRow[], activeMonth: number): string {
  if (!rows.length) return `<p class="muted">Nothing reported yet.</p>`;
  const max = Math.max(...rows.map((r) => r.reach));
  return `<table>
  <thead><tr><th>Name</th><th class="num">Installs</th><th class="num">Share</th>
    <th class="num">Times</th><th class="num">Per install</th></tr></thead>
  <tbody>${rows
    .map((r) => {
      const share = activeMonth ? Math.round((r.reach / activeMonth) * 100) : 0;
      const per = r.reach ? (r.volume / r.reach).toFixed(1) : "0";
      return `<tr>
      <td><span class="rowbar" style="width:${((r.reach / max) * 100).toFixed(1)}%"></span>
        <code>${esc(r.name)}</code></td>
      <td class="num">${r.reach}</td><td class="num">${share}%</td>
      <td class="num">${r.volume.toLocaleString("en-GB")}</td><td class="num">${per}</td>
    </tr>`;
    })
    .join("")}</tbody></table>`;
}

function page(status: number, message: string): Response {
  return new Response(
    `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>MXB App usage</title>
<style>${CSS}</style></head><body><header><h1>MXB App usage</h1></header>
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
:root{--bg:#f6f7f9;--panel:#fff;--ink:#16181d;--muted:#6b7280;--line:#e3e6ea;--accent:#e2492b}
@media (prefers-color-scheme:dark){
  :root{--bg:#0f1114;--panel:#171a1f;--ink:#e8eaed;--muted:#9aa3ae;--line:#262b32;--accent:#ff6a42}
}
*{box-sizing:border-box}
body{margin:0;padding:24px;background:var(--bg);color:var(--ink);
  font:14px/1.5 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
header{display:flex;align-items:baseline;justify-content:space-between;gap:16px;margin-bottom:20px}
h1{font-size:18px;margin:0;letter-spacing:-.01em}
h2{font-size:13px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);margin:0 0 12px}
.hint{margin:-8px 0 12px}
nav a{color:var(--muted);text-decoration:none;padding:4px 8px;border-radius:6px;margin-left:4px}
nav a.on{background:var(--accent);color:#fff}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin-bottom:16px}
.tile{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:14px;
  display:flex;flex-direction:column;gap:2px}
.tile .n{font-size:26px;font-weight:600;letter-spacing:-.02em}
.tile .l{font-size:13px}
.tile .h,.muted{color:var(--muted);font-size:12px}
.panel{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:16px;margin-bottom:16px}
.cols{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:16px}
svg{width:100%;height:180px;display:block}
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
table{width:100%;border-collapse:collapse}
th{text-align:left;font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:var(--muted);
  font-weight:500;padding:0 8px 8px;border-bottom:1px solid var(--line)}
td{padding:7px 8px;border-bottom:1px solid var(--line);position:relative}
tr:last-child td{border-bottom:0}
.num{text-align:right;font-variant-numeric:tabular-nums}
.rowbar{position:absolute;left:0;top:2px;bottom:2px;background:var(--accent);opacity:.10;border-radius:4px}
code{font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;position:relative}
.unused{list-style:none;margin:0;padding:0;display:flex;flex-wrap:wrap;gap:8px}
.unused li{border:1px solid var(--line);border-radius:6px;padding:3px 8px}
footer{color:var(--muted);font-size:12px;margin-top:8px}
`;
