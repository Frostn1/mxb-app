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

// --- The shared snapshot -------------------------------------------------------------------
//
// The book above answers "what servers exist". This answers "what were they doing a minute
// ago", and it exists for one reason: the Servers tab takes seconds to fill. A sweep is a Steam
// sign-in, a master login and a datagram to every server that comes back, and until all of that
// lands there is nothing on the screen. The app paints its own last sweep instead — but a fresh
// install has no last sweep, and neither has anyone opening the tab for the first time this
// week. They get this one, which somebody else's app wrote a minute ago.
//
// ## What is different about this from the roster
//
// This does carry operator text — the server's name, its track, its location string — and that
// is a thing to be careful with rather than to wave through, because it is written by anonymous
// callers and drawn in everybody's app. Three rules make it safe enough for what it is, and it
// is worth being precise about what "enough" means:
//
//  1. **Only corroborated addresses.** A row whose address the roster does not already serve is
//     dropped. So this cannot put a *new* address in front of anyone, which is the part that
//     would matter — the join button goes to the address, and the address is one the game's own
//     master server has been independently seen listing.
//  2. **Text is cleaned and capped**: control characters out, length limited, counts clamped.
//     What survives is drawn as text and nothing else.
//  3. **It is replaced in seconds.** The app fires its own sweep the moment it paints this, and
//     shows the snapshot's age while it waits.
//
// What remains is that a liar could mis-state a real server's name or rider count for up to a
// minute, in the apps that load it in that minute. That is worth the tab filling instantly.
// Inventing a server, or pointing anybody at an address of their choosing, is not possible here.

/** A snapshot is a few hundred rows of short strings. Six times the roster's cap. */
export const MAX_SNAPSHOT_BYTES = 192 * 1024;

/** Rows one snapshot may carry, matching the roster's address cap. */
export const MAX_SNAPSHOT_SERVERS = MAX_ADDRESSES;

/**
 * How long the stored snapshot is left alone before another is accepted.
 *
 * The write is one row, but the check that makes it safe is a query per hundred addresses, and
 * every install with the tab open would otherwise pay it on every refresh. A minute is well
 * inside how stale the app is willing to paint, and it means what this endpoint costs does not
 * grow with how many people have MXB App open.
 */
export const SNAPSHOT_MIN_GAP_MS = 60 * 1000;

/**
 * How old a stored snapshot may be and still be served.
 *
 * The app draws this age beside the list, so nothing here is passed off as live. Past half an
 * hour it stops being a head start and becomes a list of who was online earlier, which the
 * sweep the app is already running answers better.
 */
export const SNAPSHOT_MAX_AGE_MS = 30 * 60 * 1000;

/** The one row. There is only ever one snapshot, and it is the latest. */
const SNAPSHOT_ID = "live";

/** One server as the tab draws it. Deliberately not everything the app knows. */
export interface SnapshotRow {
  address: string;
  name: string;
  players: number;
  maxPlayers: number;
  track: string;
  trackLayout: string;
  location: string;
  session: string;
  conditions: string;
  categories: string[];
  passworded: boolean;
  joinable: boolean;
}

/**
 * `POST /v1/roster/snapshot` — the list one app just read from the game's master server.
 *
 * Unauthenticated, like the roster itself and for the same reason: a snapshot only invited
 * accounts could write would be one that stayed empty, and the people with nothing of their own
 * are exactly who it is for. What makes it safe to hand back is above.
 */
export async function reportSnapshot(request: Request, env: Env): Promise<Response> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_SNAPSHOT_BYTES) return json(413, { error: "snapshot too large" });

  const raw = await readText(request);
  if (raw === null || raw.length > MAX_SNAPSHOT_BYTES) {
    return json(413, { error: "snapshot too large" });
  }
  const rows = parseSnapshot(raw);
  if (typeof rows === "string") return json(400, { error: rows });

  const now = Date.now();
  // Asked before anything expensive, and answered as a success: an app whose contribution was
  // not needed has done nothing wrong and has nothing to retry.
  const held = await env.DB.prepare("SELECT updated_at FROM server_snapshot WHERE id = ?")
    .bind(SNAPSHOT_ID)
    .first<{ updated_at: number }>();
  if (held && now - held.updated_at < SNAPSHOT_MIN_GAP_MS) {
    return json(202, { ok: true, stored: false });
  }

  const known = await corroborated(
    rows.map((r) => r.address),
    env,
  );
  const keep = rows.filter((r) => known.has(r.address));
  if (keep.length === 0) {
    return json(202, { ok: true, stored: false, reason: "no corroborated addresses" });
  }

  await env.DB.prepare(
    "INSERT INTO server_snapshot (id, payload, servers, updated_at) VALUES (?, ?, ?, ?)" +
      " ON CONFLICT(id) DO UPDATE SET payload = excluded.payload," +
      "  servers = excluded.servers, updated_at = excluded.updated_at",
  )
    .bind(SNAPSHOT_ID, JSON.stringify(keep), keep.length, now)
    .run();

  return json(202, { ok: true, stored: true, servers: keep.length });
}

/**
 * `GET /v1/roster/snapshot` — the most recent one, and the moment it was true.
 *
 * Public, CORS-open and cached for half a minute at the edge, which is what keeps this from
 * costing a database read per app that opens the tab. `asOf` is not decoration: the app draws
 * it, so nobody is shown a rider count from twenty minutes ago as though it were now.
 */
export async function readSnapshot(env: Env): Promise<Response> {
  let servers: SnapshotRow[] = [];
  let asOf = 0;
  try {
    const row = await env.DB.prepare("SELECT payload, updated_at FROM server_snapshot WHERE id = ?")
      .bind(SNAPSHOT_ID)
      .first<{ payload: string; updated_at: number }>();
    if (row && Date.now() - row.updated_at <= SNAPSHOT_MAX_AGE_MS) {
      servers = JSON.parse(row.payload) as SnapshotRow[];
      asOf = row.updated_at;
    }
  } catch (err) {
    console.error(JSON.stringify({ msg: "snapshot read failed", error: String(err) }));
  }

  return new Response(JSON.stringify({ asOf, servers, count: servers.length }), {
    status: 200,
    headers: {
      "content-type": "application/json",
      "cache-control": "public, max-age=30",
      "access-control-allow-origin": "*",
    },
  });
}

/**
 * Server names that exist to sell cheats, folded the same way the names are.
 *
 * These are not servers in any useful sense. They sit on the master list at 42/42 with a
 * password set so nobody can ever join one, because the row itself is the product: the name is
 * a billboard in everybody's server browser. On 2026-09-18, 28 of the 69 rows in the shared
 * snapshot were this one advertiser.
 *
 * A domain list is narrow on purpose. The snapshot is the one place the control plane repeats
 * operator text into everybody's app, so a rule here has to be something we can defend rather
 * than a guess at what looks spammy — "this is a shop that sells cheats for this game" is a
 * fact, and the list is short enough to keep honest.
 */
export const CHEAT_SHOPS = ["kaizopro"];

/**
 * Fold the tricks out of a name so a match survives them.
 *
 * `BUY CHE4TS 4TH JULY 50% OFF WWW.KAlZ0.PR0` is written that way for a reason: the digits and
 * the lowercase L are there to slip a literal match while still reading as the domain to a
 * human. So confusable characters collapse to one representative and everything else goes,
 * and the needles are folded with the same function — that is what makes the comparison fair
 * rather than a list of hand-written spellings to be kept up to date.
 */
export function fold(name: string): string {
  const swap: Record<string, string> = {
    "0": "o",
    "1": "i",
    l: "i",
    "|": "i",
    "!": "i",
    "3": "e",
    "4": "a",
    "@": "a",
    "5": "s",
    $: "s",
    "7": "t",
    "8": "b",
    "9": "g",
    "6": "g",
  };
  return name
    .toLowerCase()
    .split("")
    .map((c) => swap[c] ?? c)
    .join("")
    .replace(/[^a-z]/g, "");
}

/**
 * Whether a row is an advertisement rather than a server.
 *
 * Two rules. A known cheat shop's domain in the name, which is the narrow and certain one. And
 * a name that both talks about cheats and carries a web address, which catches the same
 * advertiser the day they move domain — `NO CHEATING` stays, `BUY CHEATS WWW.SOMEWHERE` goes.
 *
 * A false positive costs little and costs it briefly: this only drops a row from the head start
 * the app paints while its own sweep runs, and the sweep — which this never touches — puts the
 * server back a second later. Getting it wrong in the other direction means the control plane
 * spends its own bandwidth putting a cheat shop in front of every player who opens the tab.
 */
export function isAdvert(name: string): boolean {
  const folded = fold(name);
  if (CHEAT_SHOPS.some((shop) => folded.includes(fold(shop)))) return true;
  return folded.includes("cheat") && (folded.includes("www") || folded.includes("http"));
}

/** Check a snapshot, returning the reason it was refused rather than a bare false. */
export function parseSnapshot(raw: string): SnapshotRow[] | string {
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return "expected a JSON body";
  }
  if (!body || typeof body !== "object") return "expected a JSON body";
  const { servers } = body as Record<string, unknown>;
  if (!Array.isArray(servers)) return "servers must be an array";
  if (servers.length === 0) return "servers was empty";
  if (servers.length > MAX_SNAPSHOT_SERVERS) {
    return `at most ${MAX_SNAPSHOT_SERVERS} servers in one snapshot`;
  }

  const rows: SnapshotRow[] = [];
  const seen = new Set<string>();
  for (const entry of servers) {
    if (!entry || typeof entry !== "object") continue;
    const s = entry as Record<string, unknown>;
    // Dropped rather than refused, exactly as in `parseReport`: one row the caller could never
    // have joined anyway must not cost them the whole snapshot.
    if (!isPublicGameAddress(s.address)) continue;
    const address = (s.address as string).trim();
    if (seen.has(address)) continue;
    const name = text(s.name, 64);
    // Dropped here rather than at the read, so the stored payload is already clean and the
    // served answer stays one lookup. The cost is that a change to the list takes effect on
    // the next write rather than at once, which for a row rewritten every minute is nothing.
    if (isAdvert(name)) continue;
    seen.add(address);
    rows.push({
      address,
      name,
      players: count(s.players, 999),
      maxPlayers: count(s.maxPlayers, 999),
      track: text(s.track, 64),
      trackLayout: text(s.trackLayout, 48),
      location: text(s.location, 48),
      session: text(s.session, 32),
      conditions: text(s.conditions, 32),
      categories: Array.isArray(s.categories)
        ? s.categories
            .slice(0, 4)
            .map((c) => text(c, 24))
            .filter(Boolean)
        : [],
      passworded: s.passworded === true,
      joinable: s.joinable !== false,
    });
  }
  if (rows.length === 0) return "no usable servers in that snapshot";
  return rows;
}

/**
 * One field of operator text, as it will be drawn.
 *
 * Control characters go — they are never in a server name, and they are what turns a string
 * into something other than a string in whatever reads it next — and the length is capped at
 * what a tile shows. Anything that is not a string at all becomes "".
 */
function text(value: unknown, max: number): string {
  if (typeof value !== "string") return "";
  // eslint-disable-next-line no-control-regex
  return value
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .trim()
    .slice(0, max);
}

/** One count, as a whole number inside a sane range. */
function count(value: unknown, max: number): number {
  const n = typeof value === "number" && Number.isFinite(value) ? Math.trunc(value) : 0;
  return Math.min(Math.max(n, 0), max);
}

/**
 * Which of these addresses the roster already serves.
 *
 * Chunked, because D1 binds a limited number of parameters to one statement and a snapshot
 * carries several hundred addresses. The chunks are read in parallel: this sits in front of a
 * write that happens at most once a minute, not in front of a player.
 */
async function corroborated(addresses: string[], env: Env): Promise<Set<string>> {
  const CHUNK = 90;
  const chunks: string[][] = [];
  for (let i = 0; i < addresses.length; i += CHUNK) chunks.push(addresses.slice(i, i + CHUNK));

  const found = new Set<string>();
  const answers = await Promise.all(
    chunks.map((chunk) =>
      env.DB.prepare(
        "SELECT address FROM server_roster WHERE corroborated_at IS NOT NULL" +
          ` AND address IN (${chunk.map(() => "?").join(",")})`,
      )
        .bind(...chunk)
        .all<{ address: string }>(),
    ),
  );
  for (const answer of answers) {
    for (const row of answer.results ?? []) found.add(row.address);
  }
  return found;
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
