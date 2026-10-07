/**
 * A server's bike classes, as the browser shows and searches them.
 *
 * A server advertises its `[event] category` as one `/`-separated list — the MXB server sends
 * `MX1/MX1 OEM/MX2/MX2 OEM`, public servers up to ten classes — and the backend hands it over
 * already split. The tile used to show only `categories[0]`, so every multi-class server read
 * as MX1-only; these helpers are the one place the list is turned into text, so nothing else
 * picks a single entry again.
 */

import type { MasterServer } from "@frost/shared/api/mods";

/** Every class, in the server's own order, trimmed, blanks and repeats (any case) dropped. */
export function serverClasses(categories: readonly string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of categories) {
    const c = raw.trim();
    const key = c.toLowerCase();
    if (!c || seen.has(key)) continue;
    seen.add(key);
    out.push(c);
  }
  return out;
}

/** The classes as one line of text; empty when the server takes any class. */
export function classLine(categories: readonly string[], sep = " · "): string {
  return serverClasses(categories).join(sep);
}

/** Whether a lowercased search query hits the server — its name, track, location, address,
 *  or any one of its classes. */
export function serverMatchesQuery(
  s: Pick<MasterServer, "name" | "track" | "location" | "address" | "categories">,
  q: string,
): boolean {
  if (!q) return true;
  return (
    s.name.toLowerCase().includes(q) ||
    s.track.toLowerCase().includes(q) ||
    s.location.toLowerCase().includes(q) ||
    s.address.toLowerCase().includes(q) ||
    s.categories.some((c) => c.toLowerCase().includes(q))
  );
}
