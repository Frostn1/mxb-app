import { useEffect, useState } from "react";
import { Lightbulb, X } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import { useInstall } from "../../Context/Install";
import { markTipDone, pickTip, readTipsState, writeTipsState } from "../../lib/tips";
import { useTips } from "./useTips";
import { useTipsState } from "./useTipsState";

/**
 * This launch's tip, once picked. Module-level so leaving the home screen and coming back
 * shows the same one rather than picking again: at most one tip per launch.
 */
let launchTip: string | null | undefined;
/** Closed this launch: nothing more until the app is opened again. */
let launchClosed = false;

/**
 * The occasional tip on the home screen.
 *
 * `paused` covers everything else that wants the player's attention first — the intro, the
 * tour, the release showcase, the first-run bar, another suggestion. On top of that it never
 * shows while a download or install is running, so a tip never competes with progress.
 */
export default function TipHost({ paused }: { paused: boolean }) {
  const t = useT();
  const state = useTipsState();
  const { tips, dialogs } = useTips();
  const { active, queueLength } = useInstall();
  const installing =
    queueLength > 0 || active.some((a) => a.stage !== "done" && a.stage !== "error");
  const [closed, setClosed] = useState(launchClosed);

  const quiet = paused || installing || closed;
  const candidates = tips.map((tip) => ({ id: tip.id, eligible: tip.eligible }));
  const picked = launchTip === undefined && !quiet ? pickTip(candidates, state, Date.now()) : null;

  // Pick once per launch, and count it as shown the moment it appears.
  useEffect(() => {
    if (launchTip !== undefined || !picked || picked === "wait") return;
    launchTip = picked;
    writeTipsState(markTipDone(readTipsState(), picked, Date.now()));
  }, [picked]);

  const close = () => {
    launchClosed = true;
    setClosed(true);
  };

  const tip = tips.find((x) => x.id === launchTip);
  // A tip whose condition stopped holding (paint sync turned on in Settings) goes quietly.
  const visible = !quiet && !state.off && tip !== undefined && tip.eligible === true;

  return (
    <>
      {visible && (
        <div className="border-b border-border px-3 py-2">
          <div className="flex items-start gap-2.5 rounded-lg border border-border bg-card/60 p-3.5">
            <Lightbulb className="mt-0.5 size-4 flex-none text-primary" />
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <p className="text-[12.5px] font-semibold text-foreground">{t(tip.title)}</p>
              <p className="text-[12px] leading-relaxed text-muted-foreground">{t(tip.body)}</p>
              <div className="flex items-center gap-3 pt-1.5">
                {tip.action && (
                  <Button
                    size="sm"
                    className="h-7 px-2.5 text-xs"
                    onClick={() => {
                      tip.action?.run();
                      close();
                    }}
                  >
                    {t(tip.action.label)}
                  </Button>
                )}
                <button
                  type="button"
                  onClick={close}
                  className="cursor-default text-[11.5px] font-semibold text-muted-foreground hover:text-foreground"
                >
                  {t(tip.action ? "tips.notNow" : "tips.gotIt")}
                </button>
              </div>
            </div>
            <button
              type="button"
              aria-label={t("tips.notNow")}
              onClick={close}
              className="flex-none cursor-default text-muted-foreground hover:text-foreground"
            >
              <X className="size-3.5" />
            </button>
          </div>
        </div>
      )}
      {dialogs}
    </>
  );
}
