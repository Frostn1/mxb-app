import { usePurchaseWatch } from "@/lib/usePurchaseWatch";

/**
 * Renders nothing. It exists so the purchase watch can reach the install queue.
 *
 * The watch is started by a store page opening in the browser, which can happen from the Shop
 * grid, a mod's detail page or a server's "buy this track" button — so it cannot live in any
 * one of those views, all of which unmount the moment the player looks elsewhere. It has to be
 * mounted for the life of the window, and inside `InstallProvider`, because what it finds is
 * queued like any other install.
 */
export default function PurchaseWatcher() {
  usePurchaseWatch();
  return null;
}
