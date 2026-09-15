/**
 * Paging for the admin listings the site reads (`webadmin.ts`): one page size, one count
 * ceiling and one way to read a search box, so every listing answers the same way.
 */

/** How many rows a listing shows at once. */
export const PAGE_SIZE = 50;

/** The most rows a count query will count exactly. Past it the site says "10,000+". */
export const MAX_COUNT = 10_000;

export interface Paged<T> {
  rows: T[];
  /** Matching rows, counted up to `MAX_COUNT`. */
  total: number;
  /** 1-based. */
  page: number;
  size: number;
}

/** `?page=` as a 1-based page number. Anything unusable is page 1. */
export function parsePage(value: string | null): number {
  const asked = Number(value ?? "1");
  if (!Number.isFinite(asked)) return 1;
  return Math.min(10_000, Math.max(1, Math.trunc(asked)));
}

/**
 * What was typed in a search box, as a LIKE pattern.
 *
 * The wildcards are escaped, so a typed underscore matches an underscore rather than
 * anything at all — which matters when the thing being searched for is a file name.
 */
export function likeTerm(query: string): string {
  const trimmed = query.trim().slice(0, 96);
  if (!trimmed) return "";
  return `%${trimmed.replace(/[\\%_]/g, (c) => `\\${c}`)}%`;
}
