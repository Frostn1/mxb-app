import { Lightbulb } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Switch } from "@frost/shared/Components/ui/switch";
import { useT } from "@/i18n";
import { readTipsState, writeTipsState } from "../../lib/tips";
import { useTips } from "./useTips";
import { useTipsState } from "./useTipsState";

/**
 * Settings → Tips: "Don't show tips", and every tip there is, so one closed too quickly on the
 * home screen can still be read (and acted on) here.
 */
export default function TipsSettings() {
  const t = useT();
  const state = useTipsState();
  const { tips, dialogs } = useTips();

  return (
    <>
      <div className="flex items-start justify-between gap-4">
        <div className="flex flex-col gap-0.5">
          <span className="text-[12.5px] text-foreground/85">{t("tips.off")}</span>
          <span className="text-[11.5px] leading-relaxed text-muted-foreground">
            {t("tips.offDesc")}
          </span>
        </div>
        <div className="pt-0.5">
          <Switch
            checked={state.off}
            onCheckedChange={(off) => writeTipsState({ ...readTipsState(), off })}
          />
        </div>
      </div>

      <div className="flex flex-col divide-y divide-border">
        {tips.map((tip) => (
          <div key={tip.id} className="flex items-start gap-2.5 py-3">
            <Lightbulb className="mt-0.5 size-3.5 flex-none text-muted-foreground" />
            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
              <span className="text-[12.5px] text-foreground/85">{t(tip.title)}</span>
              <span className="text-[11.5px] leading-relaxed text-muted-foreground">
                {t(tip.body)}
              </span>
            </div>
            {/* Only where it still applies: no "Turn it on" for something already on. */}
            {tip.action && tip.eligible === true && (
              <Button
                size="sm"
                variant="outline"
                className="h-7 flex-none px-2.5 text-xs"
                onClick={() => tip.action?.run()}
              >
                {t(tip.action.label)}
              </Button>
            )}
          </div>
        ))}
      </div>
      {dialogs}
    </>
  );
}
