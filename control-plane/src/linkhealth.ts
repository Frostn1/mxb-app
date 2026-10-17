/**
 * Link health: is a mod's original download link still alive?
 *
 * The mirror copies a file only when someone wants it, so for most files a download goes to the
 * original host, and those links die: a Drive file is removed or unshared, a host drops it. The
 * mxb-mirror cron checks a few links a run, oldest check first, and records `ok` or `broken` on
 * `mod_files` (migration 0077). The public API hands that on, and the site orders working links
 * first, marks dead ones, and falls back past them.
 *
 * Gentle by construction: a handful of links a run, one request at a time, a second apart, and
 * each link again only after `RECHECK_MS`. Only a 404/410 or the host's own "not found" or "no
 * access" page counts as broken; a refusal, a rate limit or a timeout says nothing and leaves
 * the state as it was. Not checked: MEGA (its API says little without the file key and a
 * download), Proton, shop and plain pages, and hosts only the fetcher may reach (mxb-mods.com,
 * MediaFire refuse Workers), which the mirror's own fetch reports on when it tries them.
 */

import { fetcherRouter } from "./fetcherroute";
import { BROWSER_UA, gdriveId, gdrivePageError, hostKind, isGdriveFolder } from "./mirrorhosts";

type Fetch = typeof fetch;
export type LinkState = "ok" | "broken";

const MINUTE = 60_000;
const DAY = 24 * 60 * MINUTE;
/** A link is checked again after this long. A broken one too: hosts restore files. */
export const RECHECK_MS = 7 * DAY;
/** Links checked per cron run (every 10 minutes): ~2,000 a day at most. */
export const CHECKS_PER_RUN = 15;
/** Between two checks in one run. */
export const CHECK_SPACING_MS = 1000;
/** One check's ceiling. A host that doesn't answer in time says nothing. */
const CHECK_TIMEOUT_MS = 10_000;
/** Hosts not checked from here. */
const UNCHECKED = new Set(["mega", "proton", "shop", "page"]);

const HEADERS = { "user-agent": BROWSER_UA, accept: "text/html,application/xhtml+xml,*/*;q=0.8" };

/**
 * Did the mirror's own fetch find the link dead? `mirrorhosts.ts` records a 404/410 as
 * "<host>: the file is gone (404)", and Drive's own pages as "no longer exists" or "not shared
 * publicly", and gives up on the file.
 */
export function errorSaysBroken(status: string, error: string | null): boolean {
  return (
    status === "failed" &&
    !!error &&
    /the file is gone \((404|410)\)|Google Drive: the file no longer exists|Google Drive: not shared publicly/.test(error)
  );
}

/** A file's link is broken: the last check said so, or (never checked) the mirror's fetch did. */
export function linkBroken(row: { status: string; error: string | null; link_state?: string | null }): boolean {
  if (row.link_state === "ok") return false;
  if (row.link_state === "broken") return true;
  return errorSaysBroken(row.status, row.error);
}

/** A body read only as far as a page's title needs. */
async function readHead(res: Response, cap = 64 * 1024): Promise<string> {
  if (!res.body) return "";
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let out = "";
  try {
    while (out.length < cap) {
      const { done, value } = await reader.read();
      if (done || !value) break;
      out += dec.decode(value, { stream: true });
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
  return out;
}

/** Don't keep a body we don't read. */
async function drop(res: Response): Promise<void> {
  await res.body?.cancel().catch(() => {});
}

function gone(status: number): boolean {
  return status === 404 || status === 410;
}

async function timed(f: Fetch, url: string, init: RequestInit): Promise<Response> {
  return await f(url, { ...init, signal: AbortSignal.timeout(CHECK_TIMEOUT_MS) });
}

/**
 * Google Drive: the file's own page. A 404, a "not found" or "access denied" page, or a bounce to
 * Google's sign-in (shared with named people only) is broken; the file's page is ok.
 */
async function checkDrive(url: string, f: Fetch): Promise<LinkState | null> {
  const id = gdriveId(url);
  const page = isGdriveFolder(url) || !id ? url : `https://drive.google.com/file/d/${id}/view`;
  const res = await timed(f, page, { headers: HEADERS, redirect: "follow" });
  if (gone(res.status) || /^https:\/\/accounts\.google\.com\//.test(res.url)) {
    await drop(res);
    return "broken";
  }
  if (!res.ok) {
    await drop(res);
    return null;
  }
  const html = await readHead(res);
  const err = gdrivePageError(html);
  if (err && /no longer exists|not shared/i.test(err.message)) return "broken";
  if (/accounts\.google\.com\/ServiceLogin|You need access|Request access/i.test(html)) return "broken";
  return err ? null : "ok";
}

/** Anything else: HEAD, and a one-byte GET where HEAD isn't answered. */
async function checkPlain(url: string, f: Fetch): Promise<LinkState | null> {
  let res = await timed(f, url, { method: "HEAD", headers: HEADERS, redirect: "follow" });
  if (res.status === 405 || res.status === 501 || res.status === 403) {
    await drop(res);
    res = await timed(f, url, { headers: { ...HEADERS, range: "bytes=0-0" }, redirect: "follow" });
  }
  await drop(res);
  if (gone(res.status)) return "broken";
  if (res.ok) return "ok";
  return null;
}

/** One link's health, or null when the answer says nothing either way (or it isn't checked). */
export async function checkLink(url: string, f: Fetch): Promise<LinkState | null> {
  const kind = hostKind(url);
  if (UNCHECKED.has(kind)) return null;
  try {
    return kind === "gdrive" ? await checkDrive(url, f) : await checkPlain(url, f);
  } catch {
    // A timeout, a reset, a TLS failure: nothing learnt.
    return null;
  }
}

export interface CheckOptions {
  fetch?: Fetch;
  now?: number;
  limit?: number;
  sleep?: (ms: number) => Promise<void>;
}

/**
 * One cron run's checks: the links whose last check is oldest, among files the mirror doesn't
 * hold. A link found alive again on a file the mirror gave up on as gone goes back to idle, so
 * the next download copies it.
 */
export async function checkLinks(env: Env, opts: CheckOptions = {}): Promise<{ checked: number; broken: number }> {
  const f = opts.fetch ?? ((input, init) => fetch(input, init));
  const now = opts.now ?? Date.now();
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const limit = opts.limit ?? CHECKS_PER_RUN;
  const viaFetcher = await fetcherRouter(env);
  const { results } = await env.DB.prepare(
    `SELECT version_id, idx, url, status, error FROM mod_files
     WHERE part = 0 AND url IS NOT NULL AND status NOT IN ('done', 'folder') AND link_checked_at < ?
     ORDER BY link_checked_at LIMIT ?`,
  )
    .bind(now - RECHECK_MS, limit * 4)
    .all<{ version_id: number; idx: number; url: string; status: string; error: string | null }>();
  let checked = 0;
  let broken = 0;
  const touched: D1PreparedStatement[] = [];
  for (const row of results) {
    // Not ours to check from here: touched, so the next run looks further down the list.
    if (viaFetcher(row.url) || UNCHECKED.has(hostKind(row.url))) {
      touched.push(
        env.DB.prepare("UPDATE mod_files SET link_checked_at = ? WHERE version_id = ? AND idx = ? AND part = 0").bind(
          now,
          row.version_id,
          row.idx,
        ),
      );
      continue;
    }
    if (checked >= limit) break;
    if (checked > 0) await sleep(CHECK_SPACING_MS);
    checked++;
    const state = await checkLink(row.url, f);
    if (state === "broken") broken++;
    const revive = state === "ok" && errorSaysBroken(row.status, row.error);
    await env.DB.prepare(
      `UPDATE mod_files SET link_checked_at = ?, link_state = COALESCE(?, link_state)${revive ? ", status = 'idle', attempts = 0, error = NULL" : ""}
       WHERE version_id = ? AND idx = ? AND part = 0`,
    )
      .bind(now, state, row.version_id, row.idx)
      .run();
  }
  if (touched.length) await env.DB.batch(touched);
  if (checked) console.log(JSON.stringify({ msg: "mod links checked", checked, broken }));
  return { checked, broken };
}
