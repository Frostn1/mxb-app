import { invoke } from "@tauri-apps/api/core";

/**
 * Anonymous "nothing found" reports from Browse.
 *
 * Only the settled query text leaves the webview (the host adds the game id and applies the
 * usage-counter switch, the build signature and its own dedupe — see
 * `crates/core/src/searchmiss.rs`). No account, install id, Steam id or GUID is involved.
 */

export const MAX_QUERY_CHARS = 80;
export const MIN_QUERY_CHARS = 2;

/** Trimmed, lowercased, one line, capped. Null when too short to be a search. */
export function normaliseMissQuery(query: string): string | null {
  const q = query
    .replace(/[\p{Cc}\p{Cf}]/gu, " ")
    .trim()
    .replace(/\s+/g, " ")
    .toLowerCase()
    .slice(0, MAX_QUERY_CHARS)
    .trim();
  return q.length >= MIN_QUERY_CHARS ? q : null;
}

/**
 * A reporter that sends each distinct (query, scope) at most once per session. The caller is
 * responsible for passing only the final, debounced query — never each keystroke.
 */
export function createMissReporter(send: (query: string) => void) {
  const seen = new Set<string>();
  return (query: string, scope: string | number = ""): boolean => {
    const q = normaliseMissQuery(query);
    if (q === null) return false;
    const key = `${scope}\u0000${q}`;
    if (seen.has(key)) return false;
    seen.add(key);
    send(q);
    return true;
  };
}

/** Fire and forget: nothing about a report may surface in the UI. */
export const reportSearchMiss = createMissReporter((query) => {
  invoke("report_search_miss", { query }).catch(() => {});
});
