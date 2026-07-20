import { useEffect, useState } from "react";
import RegisterSwapsDialog from "./RegisterSwapsDialog";
import { detectLooseSwaps } from "../../api/mods";
import type { LooseSwapBike } from "../../types";

/** Set once the launch prompt has been shown, so we don't nag on every startup. */
const SEEN_KEY = "mxb:looseSwapsSeen:v1";

/**
 * On launch, scan for model-set folders sitting loose in the user's bikes and, the first
 * time any are found, offer to register them. After it's shown once the prompt snoozes
 * (the Model Swaps page keeps a persistent banner) so subsequent launches stay quiet.
 */
export default function LooseSwapPrompt() {
  const [bikes, setBikes] = useState<LooseSwapBike[]>([]);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (localStorage.getItem(SEEN_KEY) === "1") return;
    let cancelled = false;
    (async () => {
      try {
        const found = await detectLooseSwaps();
        if (cancelled || found.length === 0) return;
        setBikes(found);
        setOpen(true);
        localStorage.setItem(SEEN_KEY, "1"); // shown once — snooze from here on
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
