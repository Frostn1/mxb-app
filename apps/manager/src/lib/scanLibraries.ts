import { scanLibrary } from "@frost/shared/api/mods";

type LibraryScan = Awaited<ReturnType<typeof scanLibrary>>;

/**
 * Walk mod roots one at a time. Each native scan already parallelises the useful file work;
 * starting every root together only turns a large collection into competing disk walks.
 */
export async function scanLibrariesSequentially(
  subpaths: readonly string[],
  scan: (subpath: string) => Promise<LibraryScan> = scanLibrary,
): Promise<LibraryScan[]> {
  const results: LibraryScan[] = [];
  for (const subpath of new Set(subpaths)) {
    results.push(await scan(subpath).catch(() => []));
  }
  return results;
}
