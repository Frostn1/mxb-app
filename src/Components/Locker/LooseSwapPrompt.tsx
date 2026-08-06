import { useEffect, useState } from "react";
import RegisterSwapsDialog from "./RegisterSwapsDialog";
import { detectLooseSwaps } from "../../api/mods";
import type { LooseSwapBike } from "../../types";

/**
 * The set of loose swaps we last prompted about. Keyed on the candidates themselves, not
 * on "have we ever asked" — a swap installed months later is new information and deserves
 * a prompt, while the same unregistered folders stay quiet across launches.
 */
const SEEN_KEY = "mxb:looseSwapsSeen:v2";

/** Stable fingerprint of what's currently loose, so re-prompting tracks real changes. */
function fingerprint(bikes: LooseSwapBike[]): string {
  return bikes
    .flatMap((b) => b.candidates.map((c) => `${b.bike}/${c.source}`))
    .sort()
    .join("|");
}

/**
 * On launch, scan for model-set folders sitting loose in the user's bikes and offer to
 * register them. Asks again whenever a *new* loose set turns up; otherwise it stays quiet
 * and the Locker's persistent banner carries the reminder.
 */
export default function LooseSwapPrompt() {
  const [bikes, setBikes] = useState<LooseSwapBike[]>([]);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const found = await detectLooseSwaps();
        if (cancelled || found.length === 0) return;
        const seen = new Set((localStorage.getItem(SEEN_KEY) ?? "").split("|").filter(Boolean));
        const fresh = found.some((b) =>
          b.candidates.some((c) => !seen.has(`${b.bike}/${c.source}`)),
        );
        if (!fresh) return;
        setBikes(found);
        setOpen(true);
        localStorage.setItem(SEEN_KEY, fingerprint(found));
      } catch {
        // Detection is best-effort; a failure here should never block startup.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (bikes.length === 0) return null;
  return <RegisterSwapsDialog open={open} onOpenChange={setOpen} bikes={bikes} />;
}
