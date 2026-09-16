import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Slider } from "@frost/shared/Components/ui/slider";
import { Switch } from "@frost/shared/Components/ui/switch";
import { useT, type TKey } from "@/i18n";
import {
  coachSetVoice,
  coachStatus,
  coachVoice,
  coachWriteCues,
  type CoachStatus,
  type CueAmount,
  type CueLevel,
  type CuesOut,
  type Voice,
} from "@/api/coach";
import { Label } from "../Page";

const CUE_LEVEL_KEY = "coach-cue-level";
const CUE_AMOUNT_KEY = "coach-cue-amount";

function remembered<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}

function remember(key: string, v: string) {
  try {
    localStorage.setItem(key, v);
  } catch {
    /* no storage: the choice lasts the session */
  }
}

const LEVELS = ["new", "intermediate", "subPro", "pro"] as const;
const AMOUNTS = ["few", "normal", "lots"] as const;

/** Live cues for this track and bike: short calls the recorder shows in practice, picked from
 *  where this lap loses time, for the rider's level and how much coaching they want. Shared by
 *  the review page and the overlay. */
export default function LiveCues({ path, lap }: { path: string; lap: number }) {
  const t = useT();
  const [level, setLevel] = useState<CueLevel>(() => remembered(CUE_LEVEL_KEY, LEVELS, "intermediate"));
  const [amount, setAmount] = useState<CueAmount>(() => remembered(CUE_AMOUNT_KEY, AMOUNTS, "normal"));
  const [sent, setSent] = useState<CuesOut | null>(null);
  const [busy, setBusy] = useState(false);
  const send = async () => {
    setBusy(true);
    try {
      const out = await coachWriteCues(path, lap, level, amount);
      setSent(out);
      toast.success(out.cues.length ? t("cues.sent", { n: out.cues.length }) : t("cues.none"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div>
      <Label>{t("cues.title")}</Label>
      <div className="space-y-3 border border-border bg-card px-4 py-3">
        <p className="text-[12.5px] text-muted-foreground">{t("cues.body")}</p>
        <div className="space-y-1.5">
          <div className="eyebrow">{t("cues.level")}</div>
          <Segmented
            size="sm"
            value={level}
            onChange={(v) => {
              setLevel(v);
              setSent(null);
              remember(CUE_LEVEL_KEY, v);
            }}
            options={LEVELS.map((v) => ({ value: v, label: t(`cues.level.${v}` as TKey) }))}
          />
        </div>
        <div className="space-y-1.5">
          <div className="eyebrow">{t("cues.amount")}</div>
          <Segmented
            size="sm"
            value={amount}
            onChange={(v) => {
              setAmount(v);
              setSent(null);
              remember(CUE_AMOUNT_KEY, v);
            }}
            options={AMOUNTS.map((v) => ({ value: v, label: t(`cues.amount.${v}` as TKey) }))}
          />
        </div>
        <div className="flex flex-wrap items-center gap-3 pt-1">
          <Button size="sm" onClick={send} disabled={busy}>
            {t("cues.send")}
          </Button>
          <span className="text-[12px] text-muted-foreground">{t("cues.where")}</span>
        </div>
        {sent && sent.cues.length > 0 && (
          <ol className="space-y-1 border-t border-border pt-2">
            {sent.cues.map((c, i) => (
              <li key={i} className="grid grid-cols-[14px_1fr_auto] gap-x-2 text-[12.5px]">
                <span className="font-mono text-faint">{i + 1}</span>
                <span className="text-muted-foreground">{c.section}</span>
                <span className="font-mono text-accent-foreground">{c.text}</span>
              </li>
            ))}
          </ol>
        )}
        <SpokenCues />
      </div>
    </div>
  );
}

/** Whether the recorder says the cues out loud, and how loud. Applies as it changes. */
function SpokenCues() {
  const t = useT();
  const [voice, setVoice] = useState<Voice | null>(null);
  const [volume, setVolume] = useState(80);
  const [status, setStatus] = useState<CoachStatus | null>(null);
  useEffect(() => {
    coachVoice()
      .then((v) => {
        setVoice(v);
        setVolume(v.volume);
      })
      .catch(() => {});
    coachStatus().then(setStatus).catch(() => {});
  }, []);
  const save = (enabled: boolean, vol: number) => {
    coachSetVoice(enabled, vol)
      .then(setVoice)
      .catch((e) => toast.error(String(e)));
  };
  return (
    <div className="space-y-2 border-t border-border pt-3">
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="text-[13px] font-semibold">{t("cues.voice")}</div>
          <p className="mt-0.5 text-[12px] text-muted-foreground">{t("cues.voiceBody")}</p>
        </div>
        <Switch checked={voice?.enabled ?? false} disabled={!voice} onCheckedChange={(on) => save(on, volume)} />
      </div>
      {status?.recorderOutdated && (
        <p className="text-[12px] text-warning">{t("recorder.tooOld", { version: status.recorderVersion ?? "" })}</p>
      )}
      {voice?.enabled && (
        <div className="flex items-center gap-3">
          <span className="w-16 text-[12px] text-muted-foreground">{t("cues.volume")}</span>
          <Slider
            className="flex-1"
            min={0}
            max={100}
            step={5}
            value={[volume]}
            onValueChange={([v]) => setVolume(v)}
            onValueCommit={([v]) => save(true, v)}
          />
          <span className="w-9 text-right font-mono text-[12px] text-muted-foreground">{volume}</span>
        </div>
      )}
    </div>
  );
}
