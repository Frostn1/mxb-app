/**
 * The shared server book.
 *
 * ## What it is for
 *
 * MXB App already survives a dead master server, and the mechanism is worth stating because
 * this is only the missing half of it. The master is the sole source of *discovery* — it is the
 * only thing that can tell you a server exists — but it is not the source of anything else: a
 * server answers `GETINFO` to whoever asks, with no account, no ticket and no challenge, and
 * that reply carries the name, the riders, the seats, the password flag and the whole event
 * blob. So the app keeps a book of every address it has been told about, and when the master
 * won't answer it rebuilds the entire list by asking the servers directly.
 *
 * That already works. It works for the wrong people. The book is per-install and starts empty,
 * so it is worth nothing to a fresh install, and nothing to anybody who had not opened the
 * Servers tab before the outage began — which is exactly the population in the channel asking
 * what happened. Pooling the book is what makes the fallback arrive before the outage does.
 *
 * It is also, precisely, the roster a fallback master would serve if one is ever stood up. That
 * is not why it is being built, and it needs no master to be useful today.
 *
 * ## Addresses, and nothing else
 *
 * No names, no locations, no operator free text. Partly because `GETINFO` already carries all
 * of it, so a stored copy is only ever staler. Mostly because an unauthenticated endpoint that
 * takes free text from anonymous clients and serves it to every install is a content-injection
 * channel, and nothing here needs one. The two fields that genuinely are master-only — the
 * operator's `location` string and the licence class — are cosmetic, and not worth that.
 *
 * ## Why an address has to be corroborated
 *
 * This list tells thousands of apps where to send a datagram. An endpoint that served whatever
 * it was handed would be a reflection amplifier with a public API: one POST naming a victim's
 * `host:port`, and every MXB App in the world probes them on the next outage.
 *
 * Two things stop that. [`isPublicGameAddress`] refuses loopback, private space, carrier NAT,
 * link-local — where cloud metadata lives — and multicast, before anything is stored. And an
 * address is only *served* once [`MIN_REPORTERS`] distinct reporters have independently seen it
 * in the game's own master list on the same day. A reporter is the day-salted digest of the
 * caller's address, so clearing that bar means controlling that many distinct networks rather
 * than minting that many identifiers.
 */

import { ipDigest } from "./voice";
import { isPublicGameAddress } from "./validate";

/** A report is a few kilobytes of `host:port` at most. */
export const MAX_REPORT_BYTES = 32 * 1024;

/**
 * Addresses one report may carry.
 *
 * A busy evening's master list is a few hundred servers and the app contributes what it saw in
 * one go, so this has to clear a whole list comfortably. It is a shape check, not a budget:
 * what a report actually costs is bounded by the fast path below, not by this.
 */
export const MAX_ADDRESSES = 512;

/** Reports accepted from one address a day. The app contributes once per successful sweep. */
export const MAX_REPORTS_PER_DAY = 500;

/**
 * Distinct reporters, on one day, before an address is served to anybody.
 *
 * Two is the smallest number that is not one. The threshold is not really about confidence —
 * an address in the game's own master list is real by construction — it is about who can put
 * something on this list, and two distinct networks is a meaningfully harder bar than one.
 * Raising it makes a genuinely quiet server take longer to appear, which is a real cost: the
 * servers with two riders on them are the ones a player most needs the book to remember.
 */
export const MIN_REPORTERS = 2;

/**
 * How long an address is kept after it was last reported.
 *
 * The app's own book keeps 30 days for the same reason and it is the right number here too: a
 * host taken down for a weekend should come back to its own row, and "the box rebooted" and
 * "the box is gone" look identical for the first few minutes.
 */
export const KEEP_MS = 30 * 24 * 60 * 60 * 1000;

/**
 * The most addresses served in one answer.
 *
 * A real budget rather than tidiness. The app probes one datagram per row, so an unbounded
 * roster would turn a tab refresh into a port scan — the app's own book caps itself at 2000 for
 * exactly that reason, and there is no point handing it more than it will keep.
 */
export const MAX_SERVED = 2000;

/**
 * `POST /v1/roster` — addresses an app saw in the game's own master list.
 *
 * Unauthenticated, like the usage counters and the outage probes: most people who run the app
 * have never claimed an invite, and a book only enrolled accounts could fill would be a book
 * that stayed empty. What would otherwise be abusable is bounded instead — the body, the count,
 * a per-address daily cap, the address shape, and above all the corroboration threshold, which
 * is what makes an unauthenticated write safe to serve back.
 */
export async function reportRoster(request: Request, env: Env): Promise<Response> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_REPORT_BYTES) return json(413, { error: "report too large" });

  const raw = await readText(request);
  if (raw === null || raw.length > MAX_REPORT_BYTES) return json(413, { error: "report too large" });

  const addresses = parseReport(raw);
  if (typeof addresses === "string") return json(400, { error: addresses });

  const now = Date.now();
  const day = new Date(now).toISOString().slice(0, 10);
  const reporter = await ipDigest(request.headers.get("CF-Connecting-IP"), day, env);
  const seen = await env.DB.prepare(
    "SELECT claims FROM device_claims WHERE ip_digest = ? AND day = ? AND kind = 'roster'",
  )
    .bind(reporter, day)
    .first<{ claims: number }>();
  if (seen && seen.claims >= MAX_REPORTS_PER_DAY) {
    return json(429, { error: "too many roster reports from here today" });
  }

  const accepted = await absorb(addresses, reporter, day, now, env);
  await env.DB.prepare(
    "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at)" +
      " VALUES (?, ?, 'roster', 1, ?)" +
      " ON CONFLICT(ip_digest, day, kind) DO UPDATE SET" +
      "  claims = claims + 1, updated_at = excluded.updated_at",
  )
    .bind(reporter, day, now)
    .run();

  return json(202, { ok: true, ...accepted });
}

/**
 * Fold one report into the roster.
 *
 * Split in two on purpose, because the two halves cost wildly different amounts and the cheap
 * one is almost the whole report. An address that is already corroborated needs nothing but a
 * fresher `last_seen` — no sighting row, no counting — and in the steady state that is every
 * address in the list. Only the handful that are new or still short of the threshold go through
 * the sighting-and-count path. Without that split, contributing a 300-server list would be six
 * hundred statements, every few minutes, from every install that has the tab open.
 */
async function absorb(
  addresses: string[],
  reporter: string,
  day: string,
  now: number,
  env: Env,
): Promise<{ known: number; pending: number }> {
  const marks = addresses.map(() => "?").join(",");
  const already = await env.DB.prepare(
    `SELECT address FROM server_roster WHERE corroborated_at IS NOT NULL AND address IN (${marks})`,
  )
    .bind(...addresses)
    .all<{ address: string }>();
  const corroborated = new Set((already.results ?? []).map((r) => r.address));
  const fresh = addresses.filter((a) => !corroborated.has(a));

  const statements = [];
  if (corroborated.size > 0) {
    const seen = [...corroborated];
    statements.push(
      env.DB.prepare(
        `UPDATE server_roster SET last_seen = ? WHERE address IN (${seen.map(() => "?").join(",")})`,
      ).bind(now, ...seen),
    );
  }
  for (const address of fresh) {
    statements.push(
      env.DB.prepare(
        "INSERT INTO server_roster (address, first_seen, last_seen) VALUES (?, ?, ?)" +
          " ON CONFLICT(address) DO UPDATE SET last_seen = excluded.last_seen",
      ).bind(address, now, now),
      env.DB.prepare(
        "INSERT INTO server_sightings (address, reporter, day, seen_at) VALUES (?, ?, ?, ?)" +
          // Same reporter, same address, same day is one sighting. Without this a single
          // caller could clear the threshold by reporting twice, which is the whole attack.
          " ON CONFLICT(address, reporter, day) DO NOTHING",
      ).bind(address, reporter, day, now),
    );
  }
  if (statements.length > 0) await env.DB.batch(statements);
  if (fresh.length > 0) await promote(fresh, day, now, env);

  return { known: corroborated.size, pending: fresh.length };
}

/**
 * Mark every address that has just reached the threshold.
 *
 * Counted within one day, which the day-salted reporter digest forces: the same network hashes
 * to a different value tomorrow, so a count across days would read one persistent reporter as
 * several and hand the injection back. One grouped query for the whole report rather than one
 * per address.
 */
async function promote(addresses: string[], day: string, now: number, env: Env): Promise<void> {
  const marks = addresses.map(() => "?").join(",");
  const ready = await env.DB.prepare(
    `SELECT address FROM server_sightings WHERE day = ? AND address IN (${marks})` +
      " GROUP BY address HAVING COUNT(DISTINCT reporter) >= ?",
  )
    .bind(day, ...addresses, MIN_REPORTERS)
    .all<{ address: string }>();

  const promoted = (ready.results ?? []).map((r) => r.address);
  if (promoted.length === 0) return;
  await env.DB.prepare(
    `UPDATE server_roster SET corroborated_at = ? WHERE corroborated_at IS NULL` +
      ` AND address IN (${promoted.map(() => "?").join(",")})`,
  )
    .bind(now, ...promoted)
    .run();
}

/** Check a report, returning the reason it was refused rather than a bare false. */
export function parseReport(raw: string): string[] | string {
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return "expected a JSON body";
  }
  if (!body || typeof body !== "object") return "expected a JSON body";
  const { addresses } = body as Record<string, unknown>;
  if (!Array.isArray(addresses)) return "addresses must be an array";
  if (addresses.length === 0) return "addresses was empty";
  if (addresses.length > MAX_ADDRESSES) return `at most ${MAX_ADDRESSES} addresses in one report`;

  const clean = new Set<string>();
  for (const entry of addresses) {
    // Silently dropped, not refused. A client that saw one IPv6-only server must not have its
    // whole list rejected over a row nothing could have joined anyway — and `isPublicGameAddress`
    // is doing safety work here, not validation work, so what it drops is not the caller's fault.
    if (!isPublicGameAddress(entry)) continue;
    clean.add((entry as string).trim());
  }
  if (clean.size === 0) return "no usable addresses in that report";
  return [...clean];
}

/**
 * `GET /v1/roster` — every address worth remembering.
 *
 * Public, CORS-open and cacheable. It is a list of public game servers and carries nothing
 * belonging to anyone; the app reads it to seed a book it would otherwise have to wait for an
 * outage-free evening to fill.
 *
 * Servers registered through mxbsecure's own registry are folded in here rather than kept
 * separate: they are addresses the app should remember for exactly the same reason, and a
 * caller asking "what servers exist" should not have to know we have two lists.
 */
export async function readRoster(env: Env): Promise<Response> {
  let addresses: string[] = [];
  try {
    const rows = await env.DB.prepare(
      "SELECT address FROM server_roster" +
        " WHERE corroborated_at IS NOT NULL AND last_seen > ?" +
        " UNION" +
        " SELECT address FROM servers WHERE published = 1 AND address IS NOT NULL AND address != ''" +
        " LIMIT ?",
    )
      .bind(Date.now() - KEEP_MS, MAX_SERVED)
      .all<{ address: string }>();
    addresses = (rows.results ?? []).map((r) => r.address);
  } catch (err) {
    console.error(JSON.stringify({ msg: "roster read failed", error: String(err) }));
  }

  return new Response(JSON.stringify({ addresses, count: addresses.length }), {
    status: 200,
    headers: {
      "content-type": "application/json",
      // The list moves on the scale of servers appearing and disappearing, which is hours. Five
      // minutes of cache costs nothing and this is read on exactly the afternoon everyone is
      // asking at once.
      "cache-control": "public, max-age=300",
      "access-control-allow-origin": "*",
    },
  });
}

/**
 * `POST /v1/roster/mine` — a server's own operator adding it, signed in.
 *
 * The one path onto this list that does not need corroborating, because the account behind it
 * is the corroboration: a person took responsibility, and unlike a sighting it is recorded
 * against them. That makes it the way an unlisted or brand-new server gets remembered without
 * waiting for two strangers to happen to see it.
 */
export async function claimRoster(
  request: Request,
  accountId: string,
  env: Env,
): Promise<Response> {
  const raw = await readText(request);
  if (raw === null || raw.length > MAX_REPORT_BYTES) return json(413, { error: "body too large" });
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return json(400, { error: "expected a JSON body" });
  }
  const { address } = (body ?? {}) as Record<string, unknown>;
  if (!isPublicGameAddress(address)) {
    return json(400, { error: "address must be a public host:port the game can be pointed at" });
  }

  const now = Date.now();
  await env.DB.prepare(
    "INSERT INTO server_roster (address, first_seen, last_seen, corroborated_at, owner_account, owner_at)" +
      " VALUES (?, ?, ?, ?, ?, ?)" +
      " ON CONFLICT(address) DO UPDATE SET" +
      "  last_seen = excluded.last_seen," +
      // Sticky, like every other corroboration: an address that got here on sightings keeps the
      // moment it earned that, rather than having it rewritten by whoever claimed it later.
      "  corroborated_at = COALESCE(server_roster.corroborated_at, excluded.corroborated_at)," +
      "  owner_account = excluded.owner_account, owner_at = excluded.owner_at",
  )
    .bind(address, now, now, now, accountId, now)
    .run();

  return json(202, { ok: true, address });
}

/**
 * Drop what nobody has seen in a month, and the sightings that have done their job.
 *
 * Sightings are evidence rather than history: once an address is corroborated the rows that
 * corroborated it say nothing anyone will ask again, and a day old is long past the window they
 * are counted in. Failing quietly, like every other sweep — a sweep that didn't happen is the
 * next sweep's problem, and must never be why a cron run that also reaps idle servers gives up.
 */
export async function pruneRoster(env: Env): Promise<void> {
  const today = new Date().toISOString().slice(0, 10);
  try {
    await env.DB.batch([
      env.DB.prepare("DELETE FROM server_roster WHERE last_seen < ?").bind(Date.now() - KEEP_MS),
      env.DB.prepare("DELETE FROM server_sightings WHERE day < ?").bind(today),
    ]);
  } catch (err) {
    console.error(JSON.stringify({ msg: "roster sweep failed", error: String(err) }));
  }
}

async function readText(request: Request): Promise<string | null> {
  try {
    return await request.text();
  } catch {
    return null;
  }
}

// index.ts has its own copy; duplicating four lines beats importing the entry point back into
// a module it imports.
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
