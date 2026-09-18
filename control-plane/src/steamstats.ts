/**
 * How far the Steam sign-in has spread, and which builds it has spread to.
 *
 * ## Why this is not in `usage.ts`
 *
 * That module opens by promising there is no user in it: the key is an install id a machine
 * minted for itself, joined to nothing. The question here is the opposite one — *which
 * confirmed identities are on which build* — and answering it inside that file would quietly
 * break a promise it spends forty lines making. So: a separate module, reading separate
 * tables, returned beside the anonymous figures rather than mixed into them.
 *
 * ## What the number is, and what it is not
 *
 * The only place a Valve-confirmed identity and an app version sit in the same row is
 * `client_modules`, which the app writes while the *game* is running. So this counts accounts
 * whose game ran with the app reporting, not installs — a strictly smaller population than
 * the one every figure in `collectStats` is drawn from, typically by about half.
 *
 * That gap is the whole hazard of this panel, and it cannot be closed here: it is closed by
 * the app reporting its own sign-in state with its usage counters, which needs a release. Until
 * then the denominator travels with the figure — `seen` is returned for exactly that reason,
 * and the dashboard is expected to show it rather than present `linked` on its own.
 */

/** One build, and how many of the accounts on it have a confirmed Steam identity. */
export interface SteamVersionRow {
  label: string;
  accounts: number;
  linked: number;
}

export interface SteamAdoption {
  /**
   * Accounts that reported inside the window. The denominator, and not interchangeable with
   * anything in `Stats` — see the note above.
   */
  seen: number;
  /** Of those, how many Valve has confirmed. */
  linked: number;
  /** The highest version anyone reported in the window. Empty when nobody did. */
  latest: string;
  onLatest: number;
  onLatestLinked: number;
  /** Newest build first, so the one being rolled out is at the top where it is being watched. */
  byVersion: SteamVersionRow[];
}

/**
 * Compare two versions the way a release manager reads them, newest first when sorted.
 *
 * Needed because the builds arrive as text and `'0.9.0' > '0.17.0'` is true as a string, which
 * would put "latest" on a build from months ago and make the headline figure wrong in the one
 * direction nobody would notice — it would still look plausible.
 *
 * A pre-release loses to its own release (`0.17.2-beta.1` is older than `0.17.2`), which is
 * what `isAppVersion` allows and what the coach builds actually ship as.
 */
export function compareVersions(a: string, b: string): number {
  const split = (v: string) => {
    const [core, pre = ""] = v.split("-", 2);
    return { parts: core.split(".").map((n) => Number(n) || 0), pre };
  };
  const left = split(a);
  const right = split(b);
  for (let i = 0; i < 3; i++) {
    const diff = (left.parts[i] ?? 0) - (right.parts[i] ?? 0);
    if (diff !== 0) return diff;
  }
  if (left.pre === right.pre) return 0;
  // A missing pre-release tag is the release, and the release is the newer of the two.
  if (!left.pre) return 1;
  if (!right.pre) return -1;
  return left.pre < right.pre ? -1 : 1;
}

/**
 * The adoption split over the dashboard's own window.
 *
 * `days` rather than `PRESENCE_TTL_MS`: that constant answers "who is online right now", which
 * is a ten-minute question and the wrong one here. Taking the range picker's window instead
 * means this panel moves with every other figure on the page when the picker moves, which is
 * the only way two numbers beside each other can be compared at all.
 *
 * One query. The join is on a primary key and the filter is on a bounded timestamp, so the cost
 * is the number of accounts that have ever reported rather than the size of any history.
 */
export async function steamAdoption(env: Env, days: number, now = Date.now()): Promise<SteamAdoption> {
  const fresh = now - days * 86_400_000;

  const rows = await env.DB.prepare(
    "SELECT c.app_version AS label, COUNT(*) AS accounts," +
      " SUM(CASE WHEN a.steam_id IS NOT NULL THEN 1 ELSE 0 END) AS linked" +
      " FROM client_modules c JOIN accounts a ON a.id = c.account_id" +
      " WHERE c.updated_at > ? AND c.app_version <> ''" +
      " GROUP BY c.app_version",
  )
    .bind(fresh)
    .all<{ label: string; accounts: number; linked: number }>();

  const byVersion = (rows.results ?? [])
    .map((r) => ({ label: r.label, accounts: r.accounts ?? 0, linked: r.linked ?? 0 }))
    .sort((a, b) => compareVersions(b.label, a.label));

  const top = byVersion[0];
  return {
    seen: byVersion.reduce((n, r) => n + r.accounts, 0),
    linked: byVersion.reduce((n, r) => n + r.linked, 0),
    latest: top?.label ?? "",
    onLatest: top?.accounts ?? 0,
    onLatestLinked: top?.linked ?? 0,
    byVersion,
  };
}

