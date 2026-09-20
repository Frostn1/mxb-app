/**
 * Turning a store's download rows into the products a player owns.
 *
 * Both stores answer with one row per *file* — a product that ships a PRO and an AMS build
 * is two rows under one name — and both decide "installed" the same way: the folders the
 * install recorded, believed only while they are still on disk, with the fuzzy name match as
 * the fallback for anything installed before that record existed. That rule lived in three
 * places once the Library grew an Owned list, which is two too many for a rule this easy to
 * disagree with.
 */
import type { ShopItem } from "@frost/shared/api/mods";
import { buildInstalledIndex } from "./installedMatch";

/** Packaged-file extensions, stripped before two names are compared. */
const EXT = /\.(pkz|zip|rar|7z|pnt)$/;

/** One owned product, with every file the store lists under it. */
export interface Owned<F extends ShopItem, L> {
  /** The product's name — the grouping key, and what the catalog was matched on. */
  product: string;
  /** Never empty; length > 1 means variants (PRO/AMS/…). */
  files: F[];
  /** The catalog entry for this product, when the store still publishes one. */
  listing: L | null;
  /** Whether something by this name is already in the library. */
  installed: boolean;
}

export function groupPurchases<F extends ShopItem, L>(
  items: F[],
  listings: Record<string, L | null>,
  installedNames: string[],
  installRecord: Record<string, string[]>,
): Owned<F, L>[] {
  const fuzzy = buildInstalledIndex(installedNames);
  // Both the file name and its stem: the library lists `X.pkz` while an archive that
  // extracts lands in a folder called `X`, and a record may name either.
  const onDisk = new Set<string>();
  for (const n of installedNames) {
    const lower = n.toLowerCase();
    onDisk.add(lower);
    onDisk.add(lower.replace(EXT, ""));
  }
  const byProduct = new Map<string, F[]>();
  for (const item of items) {
    const files = byProduct.get(item.product);
    if (files) files.push(item);
    else byProduct.set(item.product, [item]);
  }
  return [...byProduct].map(([product, files]) => ({
    product,
    files,
    listing: listings[product] ?? null,
    installed:
      (installRecord[product] ?? []).some((f) =>
        onDisk.has(f.toLowerCase().replace(EXT, "")),
      ) || fuzzy.has(product),
  }));
}
