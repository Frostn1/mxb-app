/**
 * How long an app sign-in has, measured from the right moment, and what became of it.
 *
 * This is the MXB App half of the Steam round trip — the one `SigninGate` drives: the app asks
 * for a URL (`/v1/steam/login`), the person's browser opens it (`/v1/steam/start`), Steam sends
 * them back (`/v1/steam/return`). The site's own sign-in is a different flow with a different
 * clock; see `websession.ts`.
 *
 * ## The clock started in the wrong place
 *
 * There was one deadline, ten minutes, counted from the moment the *app* minted the login row —
 * and checked only at the very end, when the person came back from Steam. Both halves of that
 * are wrong in the same direction.
 *
 * Counted from the mint, the window pays for time the rider was never spending: the app asks for
 * the URL, then the browser has to launch — a cold Chrome on a machine that is also starting MX
 * Bikes is not quick — and only then does the person see Steam at all. What is left is spent on
 * the thing that actually takes the time, which is Steam: a password on a machine that isn't
 * signed in, and then Steam Guard, which for a code sent by email is a round trip through a mail
 * client. Ten minutes is not a generous budget for that. It is roughly the budget.
 *
 * Checked only at the end, the refusal lands after the work: the rider does everything right,
 * Valve confirms them, and *then* the deadline is consulted and they are told the sign-in
 * expired. So they press the button and do it again, against a window that will run out the
 * same way. The rows say this is what happened — half of every login ever minted was never
 * consumed, a third of the retries came within a minute of the one before (somebody reacting to
 * an error, not somebody wandering off), and the accounts behind the worst of them tried nine,
 * eleven, sixteen times without ever getting in.
 *
 * So: two deadlines, each counted from the moment it is about, and each checked at the start of
 * the leg it governs rather than at the end.
 *
 * - [`LOGIN_START_TTL_MS`] — from minting to the browser arriving. A URL that has sat unopened
 *   for an hour is a stale link, and refusing it at `/v1/steam/start` costs the rider nothing:
 *   they have not signed in to anything yet, and they are told before Steam rather than after.
 * - [`LOGIN_TTL_MS`] — from the browser arriving to Valve's answer. This is the one the rider
 *   actually spends, and it is the one that was too short.
 *
 * ## What that widens, honestly
 *
 * A pending login is a UUID in a URL, and whoever finishes it attaches *their* Steam identity to
 * the account that started it. That was true at ten minutes and is true at thirty; what changes
 * is how long a leaked URL stays usable. The bound is `LOGIN_START_TTL_MS + LOGIN_TTL_MS`,
 * because [`markStarted`] stamps `started_at` once and never again — without that, re-opening
 * the start page would roll the deadline forward for as long as anybody kept clicking, and there
 * would be no bound at all.
 *
 * ## And why every refusal is written down
 *
 * The refusals above are invisible: a refused sign-in leaves a row that looks exactly like a
 * sign-in somebody abandoned. That is why the question "why can this rider not sign in" had no
 * answer except to read the code and guess. [`markRefused`] writes the verdict onto the row, so
 * the next rider who reports this is a query.
 */

import type { SteamResult } from "./page";

/**
 * How long the rider has at Steam, from their browser reaching us to Valve's answer.
 *
 * Thirty minutes, against a measured distribution where every sign-in that has ever succeeded
 * finished inside eight. It is not sized for the median — the median is half a minute — it is
 * sized so that the slow tail (a password typed carefully, a Steam Guard code fetched from a
 * phone that is charging in another room) is a sign-in that works rather than one that gets
 * refused after the fact.
 */
export const LOGIN_TTL_MS = 30 * 60 * 1000;

/**
 * How long a minted sign-in URL stays openable before it is a stale link.
 *
 * This is the leg the rider is not in: the app has the URL and the browser has not arrived. An
 * hour is loose on purpose — nothing is at stake in being patient here, and being impatient
 * refuses somebody whose browser was simply slow to launch. The two together are the whole life
 * of a pending login.
 */
export const LOGIN_START_TTL_MS = 60 * 60 * 1000;

/** The columns a verdict is reached from. Narrow on purpose: this decides nothing else. */
export interface PendingLogin {
  created_at: number;
  started_at: number | null;
  consumed_at: number | null;
}

/**
 * Why a sign-in may not go on. Finer than what the rider is shown, because the rider needs one
 * sentence and we need to know which of four different things happened.
 */
export type LoginRefusal =
  /** No such login: a mangled URL, or one from before a rollback. */
  | "unknown"
  /** Already been through Valve. Usually a reloaded tab, and harmless. */
  | "spent"
  /** Minted, then left unopened past [`LOGIN_START_TTL_MS`]. The browser never arrived. */
  | "never-opened"
  /** The browser arrived and Steam never sent them back inside [`LOGIN_TTL_MS`]. */
  | "timed-out"
  /**
   * The rider declined at Steam. Not a failure at all, and kept apart from `unconfirmed` for
   * exactly that reason: a column that cannot tell "changed their mind" from "we could not
   * confirm them" would answer the question this table exists to answer with a shrug.
   */
  | "cancelled"
  /** Valve would not confirm the assertion. */
  | "unconfirmed";

export type LoginVerdict = { ok: true } | { ok: false; reason: LoginRefusal };

const OK: LoginVerdict = { ok: true };

/** May this login send the rider on to Steam? */
export function mayStart(login: PendingLogin | null, now: number): LoginVerdict {
  if (!login) return { ok: false, reason: "unknown" };
  if (login.consumed_at !== null) return { ok: false, reason: "spent" };
  if (now - login.created_at > LOGIN_START_TTL_MS) return { ok: false, reason: "never-opened" };
  return OK;
}

/**
 * May this login be completed, now that Steam has sent the rider back?
 *
 * Counted from `started_at` — when the browser reached us — falling back to `created_at` for a
 * return that never came through the start page, and for the rows written before it existed.
 */
export function mayReturn(login: PendingLogin | null, now: number): LoginVerdict {
  if (!login) return { ok: false, reason: "unknown" };
  if (login.consumed_at !== null) return { ok: false, reason: "spent" };
  if (now - (login.started_at ?? login.created_at) > LOGIN_TTL_MS) {
    return { ok: false, reason: "timed-out" };
  }
  return OK;
}

/**
 * What the rider is shown for a refusal.
 *
 * The four that are about the window collapse to "expired", which is the true and useful
 * sentence for all of them: start it again. The two that happened at Steam say something else,
 * because "that sign-in expired" is a lie told to somebody who pressed Cancel a second ago.
 *
 * This is where the finer reasons stop: they are for us, and the rider gets the sentence that
 * tells them what to do next.
 */
export function pageResult(reason: LoginRefusal): SteamResult {
  return reason === "unconfirmed" || reason === "cancelled" ? "unconfirmed" : "expired";
}

/**
 * Stamp the moment the browser arrived — once.
 *
 * `WHERE started_at IS NULL` is the whole of the "once": it is in the statement rather than
 * checked first, so two tabs racing cannot both win, and a reload cannot roll the deadline
 * forward. Returns nothing; a start that fails to stamp still starts, because refusing to sign
 * somebody in over a bookkeeping write would be the worse failure.
 */
export async function markStarted(env: Env, loginId: string, now: number): Promise<void> {
  try {
    await env.DB.prepare("UPDATE steam_logins SET started_at = ? WHERE id = ? AND started_at IS NULL")
      .bind(now, loginId)
      .run();
  } catch {
    // This sits on the happy path of every sign-in there is. A write that fails here costs the
    // rider the wider window — `mayReturn` falls back to `created_at` — and nothing else;
    // letting it throw would cost them the sign-in, which is the failure this file exists to
    // stop causing.
  }
}

/**
 * Write down how a sign-in was refused.
 *
 * Deliberately not a consumption: the row stays open, and a retry mints a fresh one. A run of
 * refused rows is then legible as a run — which is what tells a rider the flow is failing apart
 * from a rider who tried once and went to bed.
 *
 * The latest refusal wins if a row somehow collects two. Best effort throughout: this is
 * bookkeeping, and a failure here must never turn into a refusal the rider can see.
 */
export async function markRefused(env: Env, loginId: string, reason: LoginRefusal, now: number): Promise<void> {
  try {
    await env.DB.prepare("UPDATE steam_logins SET refused_at = ?, reason = ? WHERE id = ?")
      .bind(now, reason, loginId)
      .run();
  } catch {
    /* the refusal still stands; only the record of it is lost */
  }
}
