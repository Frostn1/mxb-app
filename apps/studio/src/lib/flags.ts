/**
 * Features that are built but not shown yet.
 *
 * Off by default; a dev turns one on for themselves with `localStorage["studio.flag.<name>"] = "1"`
 * and a reload. Read once per call, so nothing needs a provider. Storage can be missing or
 * blocked (a locked-down WebView profile), which reads as off.
 */
export type Flag =
  /** The Bike tab's "Make a part…" dialog (the Part Maker). Hidden for 0.10: the feature
   *  stays in the tree, nothing in the UI points at it. */
  "partMaker";

export function flag(name: Flag): boolean {
  try {
    return window.localStorage.getItem(`studio.flag.${name}`) === "1";
  } catch {
    return false;
  }
}
