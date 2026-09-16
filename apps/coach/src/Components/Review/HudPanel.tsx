import { useEffect, useState } from "react";
import { Switch } from "@frost/shared/Components/ui/switch";
import { useT } from "@/i18n";
import { coachHud, coachSetHud, coachStatus, type CoachStatus, type Hud } from "@/api/coach";
import CuePosition from "./CuePosition";
import { Label } from "../Page";

/** What the recorder draws over the game, part by part. Shared by the review page and the
 *  overlay, so both switch the same `hud.ini`. */
export default function HudPanel() {
  const t = useT();
  const [hud, setHud] = useState<Hud | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<CoachStatus | null>(null);
  useEffect(() => {
    coachHud()
      .then(setHud)
      .catch((e) => setError(String(e)));
    coachStatus()
      .then(setStatus)
      .catch(() => {});
  }, []);
  const set = (key: string, on: boolean) =>
    coachSetHud(key, on)
      .then(setHud)
      .catch((e) => setError(String(e)));
  if (error) return <p className="text-[12.5px] text-muted-foreground">{error}</p>;
  if (!hud) return <p className="text-[12.5px] text-muted-foreground">{t("common.loading")}</p>;
  return (
    <div>
      <Label>{t("hud.title")}</Label>
      <div className="border border-border bg-card px-4 py-3">
        <p className="text-[12.5px] text-muted-foreground">{t("hud.body")}</p>
        {status?.recorderOutdated && (
          <p className="mt-2 text-[12px] text-warning">
            {t("recorder.tooOld", { version: status.recorderVersion ?? "" })}
          </p>
        )}
        <div className="mt-3 flex items-center justify-between gap-4 border-b border-border pb-3">
          <span className="text-[13px] font-semibold">{t("hud.enabled")}</span>
          <Switch checked={hud.enabled} onCheckedChange={(on) => void set("enabled", on)} />
        </div>
        <div className="divide-y divide-border">
          {hud.parts.map((p) => (
            <div key={p.key} className="py-2.5">
              <div className="flex items-center justify-between gap-4">
                <span className="text-[12.5px]">{p.label}</span>
                <Switch checked={p.on} disabled={!hud.enabled} onCheckedChange={(on) => void set(p.key, on)} />
              </div>
              {/* The map is a plain switch like the rest, so the reason two maps might show
                  is said here rather than left for the rider to work out. */}
              {p.key === "map" && hud.mxbmrp3 && (
                <p className="mt-1 text-[11.5px] text-muted-foreground">{t("hud.mxbmrp3")}</p>
              )}
              {/* The newest parts draw nothing on an older recorder, so say so rather than
                  leave a switch that looks broken. */}
              {p.needs === "0.24" && hud.preExtras && (
                <p className="mt-1 text-[11.5px] text-warning">{t("hud.needs024")}</p>
              )}
            </div>
          ))}
        </div>
        <CuePosition hud={hud} onChange={setHud} />
      </div>
    </div>
  );
}
