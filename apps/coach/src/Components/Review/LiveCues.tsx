import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import type { UnlistenFn } from "@tauri-apps/api/event";
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
  onSessionsChanged,
  type CoachStatus,
  type CueAmount,
  type CueLevel,
  type CuesOut,
  type CueVoice,
  type Voice,
} from "@/api/coach";
import { BEST, refArgs, type Reference } from "@/lib/reference";
import { Label } from "../Page";
import { ReferenceLine } from "./RefPicker";

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
/** The voices the recorder has clips for, as `hud.rs` writes them. */
const VOICES = ["female", "male"] as const;

/** The least time between two automatic re-picks. The sessions watcher fires whenever the
 *  recording grows, which is every couple of seconds while the rider is out — not once a lap —
 *  and a re-pick is a full review of the lap. A lap takes far longer than this, so the sheet
 *  still follows the rider lap by lap without reviewing the same one over and over. */
const AUTO_GAP_MS = 45_000;

/** Live cues for this track and bike: short calls the recorder shows in practice, picked from
 *  where this lap loses time to the lap it's held against, for the rider's level and how much
 *  coaching they want. Shared by the review page and the overlay. */
export default function LiveCues({
  path,
  lap,
  reference = BEST,
}: {
  path: string;
  lap: number;
  /** What the cues are picked against — the review's own reference, so the calls, the gap on
   *  the in-game HUD and the review all come from the lap the rider chose. */
  reference?: Reference;
}) {
  const t = useT();
  const [level, setLevel] = useState<CueLevel>(() => remembered(CUE_LEVEL_KEY, LEVELS, "intermediate"));
  const [amount, setAmount] = useState<CueAmount>(() => remembered(CUE_AMOUNT_KEY, AMOUNTS, "normal"));
  const [sent, setSent] = useState<CuesOut | null>(null);
  const [busy, setBusy] = useState(false);
  // Picking another lap to be held against changes every call: what was sent isn't what these
  // settings would send now.
  useEffect(() => setSent(null), [reference]);
  /** A re-pick in progress, and when the last automatic one ran. */
  const inFlight = useRef(false);
  const lastAuto = useRef(0);
  /** `latest` coaches the last lap ridden on this track rather than the one on screen: sent
   *  mid-session, that is the one the rider wants calls about. The reference stands either way:
   *  it is the lap they chose to be held against. `quiet` is for the automatic re-picks below,
   *  which shouldn't put a toast up every time a lap lands. */
  const send = async (latest = false, quiet = false) => {
    // One at a time. Two overlapping re-picks would each read the same history and write it
    // back, so one lap's worth of "the rider has heard this" would be lost.
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    try {
      const out = await coachWriteCues(path, lap, level, amount, refArgs(reference), latest);
      setSent(out);
      if (!quiet) toast.success(out.cues.length ? t("cues.sent", { n: out.cues.length }) : t("cues.none"));
    } catch (e) {
      if (!quiet) toast.error(String(e));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  };

  // Once the rider has sent a sheet, keep it current: the recorder writes a lap, the watcher
  // says so, and the calls are picked again from that lap. This is the whole point of them
  // moving on — a sheet written three laps ago is about a lap already ridden past.
  const sendRef = useRef(send);
  useEffect(() => {
    sendRef.current = send;
  });
  const following = sent != null;
  useEffect(() => {
    if (!following) return;
    let alive = true;
    let off: UnlistenFn | undefined;
    const onLap = () => {
      const now = Date.now();
      if (now - lastAuto.current < AUTO_GAP_MS) return;
      lastAuto.current = now;
      void sendRef.current(true, true);
    };
    void onSessionsChanged(onLap).then((stop) => {
      if (alive) off = stop;
      else stop();
    });
    return () => {
      alive = false;
      off?.();
    };
  }, [following]);
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
        <div className="flex flex-wrap items-center gap-2 pt-1">
          {/* Called, not passed: the click event would arrive as `latest` and be truthy. */}
          <Button size="sm" onClick={() => void send()} disabled={busy}>
            {t("cues.send")}
          </Button>
          <Button size="sm" variant="outline" onClick={() => void send(true)} disabled={busy}>
            {t("cues.fromLatest")}
          </Button>
        </div>
        <p className="text-[12px] text-muted-foreground">{t("cues.fromLatestHint")}</p>
        <p className="text-[12px] text-muted-foreground">{t("cues.where")}</p>
        <p className="text-[12px] text-muted-foreground">{t("cues.moveOn")}</p>
        {sent && (
          <p className="text-[12px] text-muted-foreground">
            {t("cues.ghost")} <ReferenceLine reference={sent.ghost} lapPath={path} />
          </p>
        )}
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
  const save = (enabled: boolean, vol: number, who: CueVoice = voice?.voice ?? "female") => {
    coachSetVoice(enabled, vol, who)
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
        <>
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
          <div className="space-y-1.5">
            <div className="eyebrow">{t("cues.voiceWho")}</div>
            <Segmented
              size="sm"
              value={voice.voice}
              onChange={(v) => save(true, volume, v)}
              options={VOICES.map((v) => ({ value: v, label: t(`cues.voice.${v}` as TKey) }))}
            />
            {voice.preExtras && <p className="text-[12px] text-warning">{t("hud.needs024")}</p>}
          </div>
        </>
      )}
    </div>
  );
}
