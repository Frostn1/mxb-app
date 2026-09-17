import { useEffect, useState } from "react";
import { Switch } from "../ui/switch";
import { useT } from "../../i18n/context";
import { getConfig } from "../../api/mods";
import { setSurveyEnabled } from "../../api/survey";

/**
 * The switch that stops the survey prompt — shared, so all three apps offer the same one.
 *
 * Separate from the anonymous-counters switch because they are different bargains: one is a
 * number nobody notices, the other interrupts you to ask something. The backend takes them the
 * other way round — counters off turns this off too, since an answer carries the same install
 * id — so this switch can only ever narrow what is sent, never widen it.
 *
 * Reads its own state rather than taking it as a prop: it is dropped into three different
 * Settings pages, and a row that needs plumbing in each of them is a row one of them will get
 * wrong.
 */
export default function SurveySetting() {
  const t = useT();
  const [on, setOn] = useState<boolean | null>(null);

  useEffect(() => {
    let stopped = false;
    getConfig()
      .then((cfg) => {
        if (!stopped) setOn(cfg.surveyEnabled ?? true);
      })
      .catch(() => {
        if (!stopped) setOn(true);
      });
    return () => {
      stopped = true;
    };
  }, []);

  const flip = (next: boolean) => {
    // Shown as flipped straight away; the backend saves before it acts on it, so a save that
    // failed leaves the config alone and the next read puts the switch back where it was.
    setOn(next);
    void setSurveyEnabled(next).catch(() => setOn(!next));
  };

  return (
    <div className="flex items-start justify-between gap-4">
      <div className="flex flex-col gap-0.5">
        <span className="text-[12.5px] text-foreground/85">{t("survey.setting.label")}</span>
        <span className="text-[11.5px] leading-relaxed text-muted-foreground">
          {t("survey.setting.desc")}
        </span>
      </div>
      <div className="pt-0.5">
        <Switch checked={on ?? true} disabled={on === null} onCheckedChange={flip} />
      </div>
    </div>
  );
}
