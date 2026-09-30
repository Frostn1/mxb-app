/** Refresh a visible library only when an automatic key pass changed its local state. */
export async function refreshAfterAutoUnlock(
  unlock: () => Promise<number>,
  refresh: () => void | Promise<void>,
): Promise<number> {
  const unlocked = await unlock();
  if (unlocked > 0) await refresh();
  return unlocked;
}
