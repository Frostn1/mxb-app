import type { CatalogTrack } from "@frost/shared/api/mods";

/**
 * What a server's main button offers. One decision shared by the tile and the detail pane, so
 * a server offers the same thing whichever way it is being looked at.
 *
 * Plain "join" is only for a track the player has — matched by its exact id against the
 * installed library (or stock). A catalogue name that merely resembles the id never gets
 * there: a missing track with nothing to install or buy is `missing`, not `join`. That fall-
 * through is how a server on `…_RD01_PRO` showed a bare Join because a different round
 * resembled it.
 */
export type JoinAction =
  | { kind: "queued" }
  | { kind: "installing" }
  /** On disk but switched off in Manage: switch it on, then join. Never a download. */
  | { kind: "activate"; rel: string }
  | { kind: "install"; product: CatalogTrack }
  /** A free post whose name only resembles the track. Never installed on its own: the player
   *  sees the candidate, opens its page and installs it by name if it is the right one. */
  | { kind: "guess"; product: CatalogTrack }
  | { kind: "buy"; product: CatalogTrack }
  /** An install of this track failed, stalled or found nothing to download. Joining must
   *  still be one click away — the game handles a missing track itself — next to Retry
   *  (when there is something to retry) and picking the file by hand. */
  | { kind: "failed"; product?: CatalogTrack }
  /** The player doesn't have it and nobody we know sells or hosts it. */
  | { kind: "missing" }
  | { kind: "wait" }
  | { kind: "join" };

export interface JoinInputs {
  /** The installed library said no to this exact id. False until that's known. */
  missing: boolean;
  /** The `rel` of a parked copy, when Manage has switched the track off. */
  inactive?: string;
  product?: CatalogTrack;
  installing: boolean;
  /** The last install of this track failed or timed out. */
  failed?: boolean;
  queued: boolean;
  joinable: boolean;
  full: boolean;
}

export function joinAction(i: JoinInputs): JoinAction {
  if (i.queued) return { kind: "queued" };
  if (i.installing) return { kind: "installing" };
  if (i.inactive) return { kind: "activate", rel: i.inactive };
  if (i.missing) {
    // Ahead of Install: offering the same download again as the only button is how a 403
    // left a player unable to join from this tab at all.
    if (i.failed) {
      const retry =
        i.product?.source === "mods" && i.product.exact && i.product.slug ? i.product : undefined;
      return retry ? { kind: "failed", product: retry } : { kind: "failed" };
    }
    if (i.product?.source === "mods" && i.product.slug) {
      // Only a name that is the track's own is installed (and joined) in one click. A
      // resemblance is a candidate: the wrong numbered sibling is a different track.
      return i.product.exact
        ? { kind: "install", product: i.product }
        : { kind: "guess", product: i.product };
    }
    if (i.product?.source === "shop") return { kind: "buy", product: i.product };
    return { kind: "missing" };
  }
  if (i.joinable && i.full) return { kind: "wait" };
  return { kind: "join" };
}
