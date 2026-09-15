/**
 * The usage dashboard, rendered on the server.
 *
 * Plain HTML with its one chart drawn as SVG. No scripts and no CDN, because a page behind an
 * admin key that pulls a charting library from someone else's host is a page that stops
 * working the day that host does — and this is the only view of the numbers there is.
 *
 * The chrome, the palette and the formatters come from `adminui.ts`. This page used to carry
 * its own copy of all three, which is how it ended up a size and a shade away from the other
 * two dashboards it is a tab of.
 */

import { ctx, errorPage, esc, ranges, shell, wrap } from "./adminui";
import { adminAllowed, collectStats, windowApp, windowDays, type Bucket, type EventRow, type Stats } from "./usage";

/** Windows the header offers. Anything else still works via `?days=`. */
const RANGES = [7, 30, 90, 365];

const TITLE = "MXB App usage";

export async function usageDashboard(request: Request, url: URL, env: Env): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") {
    return errorPage(TITLE, 503, "No admin key is configured on this deployment.");
  }
  if (allowed === "denied") return errorPage(TITLE, 401, "Unauthorized.");

  const days = windowDays(url);
  const stats = await collectStats(env, days, Date.now(), windowApp(url));
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
  const c = ctx(url);
  const pages = s.events.filter((e) => e.name.startsWith("view."));
  const features = s.events.filter((e) => !e.name.startsWith("view.") && !e.name.startsWith("app."));
  const lifecycle = s.events.filter((e) => e.name.startsWith("app."));

  const body = `<section class="tiles">
  ${tile("Active today", s.active.day, "installs that reported in on today's UTC day")}
  ${tile("Active this week", s.active.week, "distinct installs over 7 days")}
  ${tile("Active this month", s.active.month, "distinct installs over 30 days")}
  ${tile("Installs seen", s.installsEver, `distinct installs over ${s.retentionDays} days; a reinstall is a new one`)}
  ${tile("New in window", s.newInstalls, `first seen within the last ${s.days} days`)}
  ${tile("Sessions per install", per(s.sessions, s.active.month), "how often an active install is opened")}
  ${tile("Minutes per session", per(s.minutes, s.sessions), "how long it stays open")}
</section>

${retention(s)}

<section class="panel">
  <h2>Active installs per day</h2>
  ${chart(s)}
</section>

<div class="cols">
  ${buckets("Version now", s.currentVersions, "what each install last reported — one vote each")}
  ${buckets("Platform", s.platforms)}
</div>

<section class="panel">
  <h2>Title</h2>
  <p class="muted">${
    s.games.length
      ? s.games.map((g) => `${esc(g.label)} ${g.installs.toLocaleString("en-GB")}`).join(" · ")
      : "Nothing yet."
  }</p>
</section>

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
</section>`;

  return shell({
    title: TITLE,
    section: "usage",
    // Back to the path that was asked for. This page answers on both `/admin` and
    // `/admin/usage`, and switching the window should not move you off the one you typed.
    aside: ranges(s.days, RANGES, url.pathname === "/admin/usage" ? "/admin/usage" : "/admin", {}, c),
    body,
    footer: `Anonymous counts keyed on a random install id. No rider names, no paths, no
      addresses. Generated ${esc(
        new Date(s.generatedAt).toISOString().replace("T", " ").slice(0, 16),
      )} UTC.`,
    c,
  });
}

function tile(label: string, value: number | string, hint: string): string {
  const n = typeof value === "number" ? value.toLocaleString("en-GB") : value;
  return `<div class="tile"><span class="n">${esc(n)}</span>
    <span class="l">${esc(label)}</span><span class="h">${esc(hint)}</span></div>`;
}

/**
 * A rate rather than a total.
 *
 * A window total scales with the window: change the range and the number changes with it, so
 * "21,540 sessions" is not a fact anyone can act on. Per active install it holds still.
 */
function per(total: number, over: number): string {
  return over ? (total / over).toFixed(1) : "—";
}

/** This window against the one before it, or why there isn't one. */
function retention(s: Stats): string {
  if (!s.retention) {
    return `<section class="panel"><h2>Returning</h2><p class="muted">Not over this window —
      it would have to look back ${s.days * 2} days, and counters are kept for ${s.retentionDays}.
      Pick a shorter one.</p></section>`;
  }
  const { recent, prior, returning } = s.retention;
  const share = (n: number, of: number) => (of ? `${Math.round((n / of) * 100)}%` : "—");
  return `<section class="panel">
  <h2>Returning <span class="muted">— against the ${s.days} days before this window</span></h2>
  <div class="tiles">
    ${tile("Came back", share(returning, prior), `of the ${prior.toLocaleString("en-GB")} active before this window`)}
    ${tile("Already knew it", share(returning, recent), `of this window's ${recent.toLocaleString("en-GB")} were here before`)}
    ${tile("New to it", share(recent - returning, recent), "first seen in this window")}
  </div>
</section>`;
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

  return `<svg class="chart" viewBox="0 0 ${w} ${h}" role="img"
  aria-label="active installs per day" preserveAspectRatio="none">
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
  // Wrapped, like every other table on these pages: five numeric columns is wider than a
  // narrow window, and the one that gives should be the table rather than the page.
  return wrap(`<table>
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
    .join("")}</tbody></table>`);
}
