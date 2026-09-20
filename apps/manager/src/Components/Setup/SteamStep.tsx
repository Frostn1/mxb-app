import { useEffect, useState, type ReactNode } from "react";
import { Check, Loader2, ShoppingBag, Trophy, Unlock } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { steamLinkStatus } from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { useSteamLink } from "@/lib/useSteamLink";
import { useT, type TKey } from "@/i18n";
import { Plate } from "../Shell/Brand";

interface SteamStepProps {
  /** The account is linked and the flow can move on to the folders. */
  onDone: () => void;
  /** The step counter, rendered by the parent so every step shows the same one. */
  progress: ReactNode;
}

/** Why the sign-in is worth the click, in the order a rider cares about. */
const REASONS: { icon: LucideIcon; body: TKey }[] = [
  { icon: Unlock, body: "setup.steamReasonSealed" },
  { icon: ShoppingBag, body: "setup.steamReasonStore" },
  { icon: Trophy, body: "setup.steamReasonRanked" },
];

/**
 * Sign in with Steam — the second step of first-run setup, and a required one.
 *
 * The Steam account is the identity everything else hangs off: sealed content unlocks on
 * the account that owns it, a store purchase is delivered to it, and Ranked finds the rider
 * by it. Asked here, before the library is ever scanned, so all three are already true the
 * first time those tabs are opened instead of being repaired later from Settings.
 *
 * No skip. MX Bikes is bought on Steam, so there is no rider this step cannot serve, and an
 * install that walks past it spends its first session looking broken — locked tracks that
 * won't open, an empty Owned tab. Ranked still takes a typed GUID for the case this can't
 * fix (the wrong account), but that is a repair, not a way around.
 *
 * The link itself is `useSteamLink`: it opens the browser, polls the control plane, and runs
 * the unlock pass on success. Nothing about that is reimplemented here.
 */
export default function SteamStep({ onDone, progress }: SteamStepProps) {
  const t = useT();
  const [linkedId, setLinkedId] = useState<string | null>(null);
  const { linking, linkSteam } = useSteamLink({ onLinked: setLinkedId });

  // A reinstall (or a config wiped from under a live account) is already linked, and asking
  // someone to sign in again to be told they always were is a bad first screen. One quiet
  // ask on arrival; anything that goes wrong just leaves the button as the way forward.
  useEffect(() => {
    let cancelled = false;
    steamLinkStatus()
      .then((id) => {
        if (!cancelled && id) setLinkedId(id);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="grid min-h-0 flex-1 place-items-center px-10">
      <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
        {progress}

        <div className="flex flex-col items-center gap-3.5">
          <Plate className="size-14" />
          <div className="flex flex-col items-center gap-1.5">
            <h1 className="font-cond text-[26px] font-bold leading-[1.05] tracking-[-0.045em]">
              {t("setup.steamTitle")}
            </h1>
            <p className="max-w-[400px] text-center text-[13.5px] leading-relaxed text-muted-foreground">
              {t("setup.steamLead")}
            </p>
          </div>
        </div>

        <div className="flex w-full flex-col gap-2.5">
          {REASONS.map(({ icon: Icon, body }) => (
            <div
              key={body}
              className="flex items-start gap-3 rounded-xl bg-card px-4 py-3.5"
            >
              <Icon className="mt-px size-4 flex-none text-primary" />
              <span className="flex-1 text-[13px] leading-relaxed text-muted-foreground">
                {t(body)}
              </span>
            </div>
          ))}
        </div>

        {linkedId ? (
          <div className="flex w-full flex-col items-center gap-3">
            <div className="flex w-full items-center gap-2.5 rounded-xl border border-primary/40 bg-card px-3.5 py-3">
              <Check className="size-4 flex-none text-primary" strokeWidth={2.5} />
              <span className="flex-1 truncate font-mono text-[12.5px] text-muted-foreground">
                {t("setup.steamSignedIn", { id: linkedId })}
              </span>
            </div>
            <Button className="h-12 w-full text-[14.5px]" onClick={onDone}>
              {t("setup.steamContinue")}
            </Button>
          </div>
        ) : (
          <div className="flex w-full flex-col items-center gap-3">
            <Button
              className="h-12 w-full text-[14.5px]"
              disabled={linking}
              onClick={() => void linkSteam()}
            >
              {linking && <Loader2 className="animate-spin" />}
              {linking ? t("setup.steamWaiting") : t("setup.steamTitle")}
            </Button>
            <p className="max-w-[400px] text-center text-[12px] leading-relaxed text-muted-foreground">
              {t("setup.steamNote")}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
