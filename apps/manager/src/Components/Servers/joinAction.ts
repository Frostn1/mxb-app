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
  | { kind: "buy"; product: CatalogTrack }
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
  queued: boolean;
  joinable: boolean;
  full: boolean;
}

export function joinAction(i: JoinInputs): JoinAction {
  if (i.queued) return { kind: "queued" };
  if (i.installing) return { kind: "installing" };
  if (i.inactive) return { kind: "activate", rel: i.inactive };
  if (i.missing) {
    if (i.product?.source === "mods" && i.product.slug) {
      return { kind: "install", product: i.product };
    }
    if (i.product?.source === "shop") return { kind: "buy", product: i.product };
    return { kind: "missing" };
  }
  if (i.joinable && i.full) return { kind: "wait" };
  return { kind: "join" };
}
