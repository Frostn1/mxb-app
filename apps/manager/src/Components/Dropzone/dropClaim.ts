/**
 * A screen that wants dropped files for itself (the upload dialog) claims them here, and the
 * whole-window drop target hands them over instead of staging an install.
 */
type Claim = { onOver: (over: boolean) => void; onDrop: (paths: string[]) => void };

let current: Claim | null = null;

export function claimDrops(claim: Claim): () => void {
  current = claim;
  return () => {
    if (current === claim) current = null;
  };
}

export function dropClaim(): Claim | null {
  return current;
}
