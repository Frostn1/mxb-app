/**
 * Minting and revoking plugin keys, on a page.
 *
 * The two things that happen to a paid plugin — someone is given access, and someone has it
 * taken away — both used to be a hand-written SQL file and a `wrangler d1 execute --remote`.
 * That is fine for a batch of a hundred keys sold at once; it is far too much ceremony for
 * the case that actually comes up, which is getting the plugin running on a tester's machine
 * this afternoon and taking it back afterwards.
 *
 * Two views over the three plugin tables:
 *
 *   * **Keys** — mint a batch, and see every code with what became of it.
 *   * **Licenses** — who can run what right now, and the button that ends it.
 *
 * A section of the one admin dashboard, drawn in the shell every other page uses. Plain HTML
 * behind `ADMIN_KEY` and no scripts, for the reason the others give: a page that pulls
 * anything from someone else's host stops working the day that host does.
 */

import {
  adminPlugins,
  grantLicense,
  keyState,
  licenseState,
  MAX_MINT,
  MAX_MONTHS,
  mintKeys,
  normaliseCode,
  searchKeys,
  searchLicenses,
  setKeyRevoked,
  setLicenseRevoked,
  type KeyQuery,
  type KeyRow,
  type KeyState,
  type LicenseAdminRow,
  type LicenseQuery,
  type LicenseState,
  type PluginRow,
} from "./plugins";
import { adminAllowed } from "./usage";
import {
  ago,
  count,
  ctx,
  errorPage,
  esc,
  href,
  pager,
  parsePage,
  shell,
  stamp,
  wrap,
  type Ctx,
  type Paged,
  type Params,
  type SubTab,
} from "./adminui";

const ROOT = "/admin/plugins";

const TITLE = "MXB App plugins";

/** Seconds here, milliseconds in `adminui`: these tables were written in `unixepoch()`. */
const ms = (seconds: number | null): number => (seconds ? seconds * 1000 : 0);

const KEY_STATES = ["any", "unused", "redeemed", "revoked"] as const;
const LICENSE_STATES = ["any", "live", "expired", "revoked"] as const;

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

function gate(request: Request, url: URL, env: Env): Response | null {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") {
    return errorPage(TITLE, 503, "No admin key is configured on this deployment.");
  }
  if (allowed === "denied") return errorPage(TITLE, 401, "Unauthorized.");
  return null;
}

function html(body: string, status = 200): Response {
  return new Response(body, {
    status,
    headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" },
  });
}

function redirect(to: string): Response {
  return new Response(null, { status: 303, headers: { location: to, "cache-control": "no-store" } });
}

function oneOf<T extends string>(value: string | null, allowed: readonly T[], fallback: T): T {
  return (allowed as readonly string[]).includes(value ?? "") ? (value as T) : fallback;
}

function keyQuery(url: URL): KeyQuery {
  return {
    q: url.searchParams.get("q") ?? "",
    plugin: url.searchParams.get("plugin") ?? "",
    state: oneOf(url.searchParams.get("state"), KEY_STATES, "any") as KeyQuery["state"],
    page: parsePage(url.searchParams.get("page")),
  };
}

function licenseQuery(url: URL): LicenseQuery {
  return {
    q: url.searchParams.get("q") ?? "",
    plugin: url.searchParams.get("plugin") ?? "",
    state: oneOf(url.searchParams.get("state"), LICENSE_STATES, "any") as LicenseQuery["state"],
    page: parsePage(url.searchParams.get("page")),
  };
}

export async function pluginKeysPage(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  const c = ctx(url);
  const query = keyQuery(url);
  // A batch is identified by the second it was minted in. It survives the redirect, a
  // refresh and a link — which the codes themselves could not do without being in the URL.
  const minted = Number(url.searchParams.get("minted") ?? "");
  const [plugins, found, batch] = await Promise.all([
    adminPlugins(env),
    searchKeys(env, query),
    Number.isInteger(minted) && minted > 0 ? batchCodes(env, minted) : Promise.resolve([]),
  ]);
  return html(keysView(plugins, found, query, batch, url.searchParams.get("said") ?? "", c));
}

export async function pluginLicensesPage(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  const c = ctx(url);
  const query = licenseQuery(url);
  const [plugins, found] = await Promise.all([adminPlugins(env), searchLicenses(env, query)]);
  return html(licensesView(plugins, found, query, url.searchParams.get("said") ?? "", c));
}

/** The codes from one mint, read back after the redirect. */
async function batchCodes(env: Env, createdAt: number): Promise<string[]> {
  const { results } = await env.DB.prepare(
    `SELECT code FROM plugin_keys WHERE created_at = ? ORDER BY code LIMIT ${MAX_MINT}`,
  )
    .bind(createdAt)
    .all<{ code: string }>();
  return (results ?? []).map((r) => r.code);
}

/**
 * Every button on both pages.
 *
 * One handler rather than six routes: they are all the same shape — a form post, a change,
 * and back to where the button was, filters and page intact.
 */
export async function pluginsAction(request: Request, url: URL, env: Env): Promise<Response> {
  const denied = gate(request, url, env);
  if (denied) return denied;

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return errorPage(TITLE, 400, "That was not a form.");
  }
  const field = (name: string) => String(form.get(name) ?? "");

  const asked = field("back");
  // Back where the button was pressed. Anything not one of our own paths is an open redirect. Falling back to the keys page
  // rather than to nothing, and keeping the key that got this far, so a mangled `back` is
  // a wrong landing rather than a 401 with the work already done.
  const home = href(ROOT, {}, url.searchParams.get("key") ?? "");
  const back = asked.startsWith(ROOT) && !asked.startsWith("//") ? asked : home;
  const said = (message: string) => {
    // Replace rather than append: two actions in a row would otherwise stack `said=` and the
    // page would read back the first one, which is the message that is no longer true.
    const to = new URL(back, url);
    to.searchParams.set("said", message);
    return redirect(`${to.pathname}${to.search}`);
  };

  switch (field("action")) {
    case "mint": {
      const result = await mintKeys(
        env,
        field("plugin"),
        Number(field("months")),
        Number(field("count")),
        field("note"),
      );
      if (!result.ok) return errorPage(TITLE, 400, result.error ?? "Those keys were not minted.");
      // Straight to the batch, unfiltered — the codes are the point of having pressed it,
      // and a filter left over from a search would hide half of them.
      return redirect(href(ROOT, { minted: result.at }, url.searchParams.get("key") ?? ""));
    }
    case "key-revoke":
      await setKeyRevoked(env, field("code"), true);
      return said(`Key ${normaliseCode(field("code"))} revoked.`);
    case "key-restore":
      await setKeyRevoked(env, field("code"), false);
      return said(`Key ${normaliseCode(field("code"))} is redeemable again.`);
    case "license-revoke":
      await setLicenseRevoked(env, field("account"), field("plugin"), true);
      return said("License revoked. The app stops running it within the grace window.");
    case "license-restore":
      await setLicenseRevoked(env, field("account"), field("plugin"), false);
      return said("License restored, with the months it had left.");
    case "grant": {
      const result = await grantLicense(
        env,
        field("who"),
        field("plugin"),
        Number(field("months")),
      );
      if (!result.ok) return errorPage(TITLE, 400, result.error ?? "Nothing was granted.");
      return said(
        `${field("plugin")} granted to ${result.account} until ${stamp(ms(result.expires ?? 0))}.`,
      );
    }
    default:
      return errorPage(TITLE, 400, "No such action.");
  }
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

type Tab = "keys" | "licenses";

const TABS: SubTab[] = [
  { id: "keys", text: "Keys", path: ROOT },
  { id: "licenses", text: "Licenses", path: `${ROOT}/licenses` },
];

/** This section's views, in the chrome every admin page shares. */
function view(title: string, tab: Tab, body: string, c: Ctx): string {
  return shell({
    title,
    section: "plugins",
    tabs: TABS,
    current: tab,
    body,
    footer: `A license is checked offline for up to seven days. Revoking one stops the app
      running the plugin at its next check — within the week, not within the minute.`,
    c,
  });
}

/** What the last button did, if it did something worth a sentence. */
function notice(said: string): string {
  if (!said) return "";
  return `<section class="panel said"><p>${esc(said.slice(0, 200))}</p></section>`;
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

function keysView(
  plugins: PluginRow[],
  found: Paged<KeyRow>,
  query: KeyQuery,
  batch: string[],
  said: string,
  c: Ctx,
): string {
  const params: Params = { q: query.q, plugin: query.plugin, state: query.state };
  const filtered = Boolean(query.q || query.plugin || query.state !== "any");

  const body = `
${notice(said)}
${batch.length ? mintedPanel(batch) : ""}
<section class="panel">
  <h2>Mint keys</h2>
  <form class="search add" method="post" action="${esc(href(ROOT, {}, c.key))}">
    ${hidden("action", "mint")}
    ${pluginSelect(plugins, query.plugin, false)}
    <input name="months" type="number" min="1" max="${MAX_MONTHS}" value="1" aria-label="months">
    <input name="count" type="number" min="1" max="${MAX_MINT}" value="1" aria-label="count">
    <input name="note" placeholder="what this batch is for" maxlength="200">
    <button type="submit">Mint</button>
  </form>
  <p class="muted">Months each, then how many. A key is one-shot: redeeming it adds its months
    to that account's license and spends the code. Up to ${MAX_MINT} at a time.</p>
</section>

<section class="tiles">${plugins.map(pluginTile).join("")}</section>

<section class="panel">
  <h2>Keys</h2>
  <form class="search" method="get" action="${ROOT}">
    ${hidden("key", c.key)}
    <input name="q" value="${esc(query.q)}" maxlength="96" placeholder="code, note, or who spent it">
    ${select("state", KEY_STATES, query.state, {
      any: "any state",
      unused: "unused",
      redeemed: "redeemed",
      revoked: "revoked",
    })}
    ${pluginSelect(plugins, query.plugin)}
    <button type="submit">Search</button>
    ${filtered ? `<a class="clear" href="${esc(href(ROOT, {}, c.key))}">Clear</a>` : ""}
  </form>
  ${count(found, "key")}
  ${found.rows.length ? keyTable(found.rows, c) : `<p class="muted">Nothing matches.</p>`}
  ${pager(ROOT, params, found, c)}
</section>`;

  return view("Plugin keys", "keys", body, c);
}

/**
 * The codes from the mint that just happened.
 *
 * In a textarea because that is the one control a browser will let someone select all of and
 * copy without a line of script, and these are going into a Discord message.
 */
function mintedPanel(codes: string[]): string {
  return `<section class="panel minted">
  <h2>${codes.length} key${codes.length === 1 ? "" : "s"} minted</h2>
  <textarea readonly rows="${Math.min(codes.length, 12)}" class="mono">${esc(
    codes.join("\n"),
  )}</textarea>
  <p class="muted">Copy them now if you are sending them on. They stay listed below either
    way — an unused key is never hidden.</p>
</section>`;
}

function pluginTile(p: PluginRow): string {
  return `<div class="tile">
    <span class="n">${p.live.toLocaleString("en-GB")}</span>
    <span class="l">${esc(p.name)}</span>
    <span class="h">live licenses · ${p.keys.toLocaleString("en-GB")} key${
      p.keys === 1 ? "" : "s"
    } · ${p.bundle_sha256 ? `v${esc(p.version ?? "?")}` : "no build published"}</span>
  </div>`;
}

function keyTable(rows: KeyRow[], c: Ctx): string {
  return wrap(`<table>
  <thead><tr><th>Code</th><th>Plugin</th><th class="num">Months</th><th>State</th>
    <th>Spent by</th><th>Minted</th><th>Note</th><th class="act">Action</th></tr></thead>
  <tbody>${rows
    .map((r) => {
      const state = keyState(r);
      return `<tr>
      <td class="mono">${esc(r.code)}</td>
      <td>${esc(r.plugin_id)}</td>
      <td class="num">${r.months}</td>
      <td>${keyBadge(state)}</td>
      <td>${
        r.redeemed_by
          ? `<a href="${esc(
              href("/admin/diagnostics/rider", { id: r.redeemed_by }, c.key),
            )}">${esc(r.rider_name ?? r.redeemed_by)}</a>
             <span class="muted">${esc(ago(ms(r.redeemed_at)))}</span>`
          : `<span class="muted">—</span>`
      }</td>
      <td class="muted" title="${esc(stamp(ms(r.created_at)))}">${esc(ago(ms(r.created_at)))}</td>
      <td class="muted">${esc(r.note ?? "")}</td>
      <td class="act">${keyActions(r, state, c)}</td>
    </tr>`;
    })
    .join("")}</tbody></table>`);
}

/**
 * What can still be done to a key.
 *
 * A spent key has nothing left to take away — the months are on the license now, and that is
 * where the row that ends them lives. So it offers the way there rather than a button that
 * would look like it did something.
 */
function keyActions(r: KeyRow, state: KeyState, c: Ctx): string {
  const post = (action: string, text: string, cls = "") =>
    `<form method="post" action="${esc(href(ROOT, {}, c.key))}">
      ${hidden("back", c.back)}${hidden("action", action)}${hidden("code", r.code)}
      <button type="submit" class="${cls}">${text}</button></form>`;

  if (state === "revoked") return post("key-restore", "Restore", "allow");
  if (state === "redeemed") {
    return `<a href="${esc(
      href(`${ROOT}/licenses`, { q: r.redeemed_by ?? "" }, c.key),
    )}">Its license</a>`;
  }
  return post("key-revoke", "Revoke", "deny");
}

function keyBadge(state: KeyState): string {
  const tone = state === "revoked" ? "alert" : state === "redeemed" ? "" : "ok";
  return `<span class="pill ${tone}">${state}</span>`;
}

// ---------------------------------------------------------------------------
// Licenses
// ---------------------------------------------------------------------------

function licensesView(
  plugins: PluginRow[],
  found: Paged<LicenseAdminRow>,
  query: LicenseQuery,
  said: string,
  c: Ctx,
): string {
  const path = `${ROOT}/licenses`;
  const params: Params = { q: query.q, plugin: query.plugin, state: query.state };
  const filtered = Boolean(query.q || query.plugin || query.state !== "any");

  const body = `
${notice(said)}
<section class="panel">
  <h2>Grant months</h2>
  <form class="search add" method="post" action="${esc(href(ROOT, {}, c.key))}">
    ${hidden("back", c.back)}${hidden("action", "grant")}
    <input name="who" placeholder="rider name, account id, or Steam id" maxlength="96" required>
    ${pluginSelect(plugins, query.plugin, false)}
    <input name="months" type="number" min="1" max="${MAX_MONTHS}" value="1" aria-label="months">
    <button type="submit">Grant</button>
  </form>
  <p class="muted">No key in between — for a tester, or for putting someone right. The months
    are added the way a key adds them: to what is left, if anything is.</p>
</section>

<section class="panel">
  <h2>Licenses</h2>
  <form class="search" method="get" action="${path}">
    ${hidden("key", c.key)}
    <input name="q" value="${esc(query.q)}" maxlength="96" placeholder="rider name, account id, Steam id">
    ${select("state", LICENSE_STATES, query.state, {
      any: "any state",
      live: "live",
      expired: "expired",
      revoked: "revoked",
    })}
    ${pluginSelect(plugins, query.plugin)}
    <button type="submit">Search</button>
    ${filtered ? `<a class="clear" href="${esc(href(path, {}, c.key))}">Clear</a>` : ""}
  </form>
  ${count(found, "license")}
  ${found.rows.length ? licenseTable(found.rows, c) : `<p class="muted">Nothing matches.</p>`}
  ${pager(path, params, found, c)}
</section>`;

  return view("Plugin licenses", "licenses", body, c);
}

function licenseTable(rows: LicenseAdminRow[], c: Ctx): string {
  return wrap(`<table>
  <thead><tr><th>Rider</th><th>Plugin</th><th>State</th><th>Expires</th><th>Granted</th>
    <th class="act">Action</th></tr></thead>
  <tbody>${rows
    .map((r) => {
      const state = licenseState(r);
      const post = (action: string, text: string, cls: string) =>
        `<form method="post" action="${esc(href(ROOT, {}, c.key))}">
          ${hidden("back", c.back)}${hidden("action", action)}
          ${hidden("account", r.account_id)}${hidden("plugin", r.plugin_id)}
          <button type="submit" class="${cls}">${text}</button></form>`;
      return `<tr>
      <td><a href="${esc(href("/admin/diagnostics/rider", { id: r.account_id }, c.key))}">${esc(
        r.rider_name,
      )}</a><br><span class="muted mono">${esc(r.account_id)}</span></td>
      <td>${esc(r.plugin_id)}</td>
      <td>${licenseBadge(state)}</td>
      <td title="${esc(stamp(ms(r.expires_at)))}">${esc(stamp(ms(r.expires_at)))}</td>
      <td class="muted">${esc(ago(ms(r.granted_at)))}</td>
      <td class="act">${
        state === "revoked"
          ? post("license-restore", "Restore", "allow")
          : post("license-revoke", "Revoke", "deny")
      }</td>
    </tr>`;
    })
    .join("")}</tbody></table>`);
}

function licenseBadge(state: LicenseState): string {
  const tone = state === "revoked" ? "alert" : state === "live" ? "ok" : "";
  return `<span class="pill ${tone}">${state}</span>`;
}

// ---------------------------------------------------------------------------
// Bits both views use
// ---------------------------------------------------------------------------

/**
 * The plugin picker.
 *
 * `any` is a filter's answer and not an action's: minting for "every plugin" is not a thing
 * anyone means, so the forms that do something get a list they must pick from, and the first
 * one is picked already.
 */
function pluginSelect(plugins: PluginRow[], selected: string, any = true): string {
  const chosen = any || plugins.some((p) => p.id === selected) ? selected : (plugins[0]?.id ?? "");
  const opts = (any ? [`<option value=""${selected ? "" : " selected"}>every plugin</option>`] : [])
    .concat(
      plugins.map(
        (p) =>
          `<option value="${esc(p.id)}"${p.id === chosen ? " selected" : ""}>${esc(p.name)}</option>`,
      ),
    )
    .join("");
  return `<select name="plugin" aria-label="plugin">${opts}</select>`;
}

function hidden(name: string, value: string): string {
  return `<input type="hidden" name="${esc(name)}" value="${esc(value)}">`;
}

function select<T extends string>(
  name: string,
  options: readonly T[],
  selected: T,
  labels: Partial<Record<string, string>>,
): string {
  const opts = options
    .map(
      (o) =>
        `<option value="${esc(o)}"${o === selected ? " selected" : ""}>${esc(
          labels[o] ?? o,
        )}</option>`,
    )
    .join("");
  return `<select name="${esc(name)}" aria-label="${esc(name)}">${opts}</select>`;
}
