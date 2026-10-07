import type { DuplicateGroup, StorageMod } from "../../api/storage";

/** Total bytes of whatever is selected. */
export function totalBytes(items: readonly { size: number }[]): number {
  return items.reduce((n, i) => n + i.size, 0);
}

/**
 * The copies a duplicate group would remove: every one but the kept copy. A keep choice
 * that isn't in the group falls back to the suggested first copy, so a group can never
 * lose all of its copies.
 */
export function duplicateRemovals(group: DuplicateGroup, keepPath: string | undefined): StorageMod[] {
  const keep = group.items.some((i) => i.path === keepPath) ? keepPath : group.items[0]?.path;
  return group.items.filter((i) => i.path !== keep);
}

/** What duplicates waste: the size of every copy past the first. */
export function duplicateWaste(groups: readonly DuplicateGroup[]): number {
  return groups.reduce((n, g) => n + g.size * Math.max(0, g.items.length - 1), 0);
}
