import { useCallback, useEffect, useState } from "react";
import { Toaster } from "sonner";
import { Gauge, Lightbulb, Megaphone, RefreshCw, Wrench } from "lucide-react";
import OverlayFrame, { peerTabLabel, type FrameTab } from "@frost/shared/Components/Overlay/OverlayFrame";
import { Switch } from "@frost/shared/Components/ui/switch";
import { ThemeProvider, useTheme } from "@frost/shared/Context/Theme";
import {
  getOverlayPeer,
  getOverlayState,
  initialOverlayTab,
  onOverlayPeer,
  onOverlayTab,
  overlayHandoff,
  overlayHide,
  overlayOpenMain,
  type OverlayPeer,
} from "@frost/shared/api/overlay";
import { I18nProvider, useT, type TKey } from "@/i18n";
import { coachHud, coachReview, coachSetHud, type Hud, type ReviewOut } from "@/api/coach";
import { lastLap, type LastLap } from "@/lib/lastLap";
import { gap, lapTime, lossColor } from "@/lib/format";
import SetupFixes from "../Review/SetupFixes";
import LiveCues from "../Review/LiveCues";
import { Label } from "../Page";

type Tab = "tips" | "setup" | "cues" | "hud";

const TABS: { id: Tab; icon: FrameTab["icon"] }[] = [
  { id: "tips", icon: Lightbulb },
  { id: "setup", icon: Wrench },
  { id: "cues", icon: Megaphone },
  { id: "hud", icon: Gauge },
];

const isTab = (id: string | null): id is Tab => TABS.some((t) => t.id === id);

/** The last lap's review, loaded each time the overlay comes up: the rider has been riding. */
function useLastReview() {
  const [last, setLast] = useState<LastLap | null | undefined>(undefined);
  const [data, setData] = useState<ReviewOut | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(async () => {
    setError(null);
    try {
      const l = await lastLap();
      setLast(l);
      setData(l ? await coachReview(l.session.path, l.lap) : null);
    } catch (e) {
      setError(String(e));
    }
  }, []);
  useEffect(() => {
    void load();
    const again = () => void load();
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, [load]);
  return { last, data, error, load };
}

function Tips({ last, data, error }: { last: LastLap | null | undefined; data: ReviewOut | null; error: string | null }) {
  const t = useT();
  if (error) return <p className="text-[12.5px] text-muted-foreground">{error}</p>;
  if (last === undefined) return <p className="text-[12.5px] text-muted-foreground">{t("common.loading")}</p>;
  if (!last || !data) return <p className="text-[12.5px] text-muted-foreground">{t("otips.none")}</p>;
  const { review, reference } = data;
  const total = (data.lap.timeMs - reference.timeMs) / 1000;
  const focus = review.focus.slice(0, 3).map((i) => review.sections[i]);
  return (
    <div className="space-y-5">
      <div>
        <div className="eyebrow">{data.trackName || data.trackId}</div>
        <div className="mt-0.5 headline text-[22px]">
          {t("otips.lap", { lap: last.lap + 1 })} · {lapTime(data.lap.timeMs)}
        </div>
        <div className="mt-0.5 text-[12.5px] text-muted-foreground">
          {review.solo ? (
            t("review.aloneSub")
          ) : (
            <>
              <span className="font-mono" style={{ color: lossColor(total) }}>
                {gap(total)} s
              </span>{" "}
              {t("otips.against", { time: lapTime(reference.timeMs) })}
            </>
          )}
        </div>
      </div>

      <div>
        <Label>{t("review.focus")}</Label>
        {focus.length === 0 ? (
          <p className="text-[12.5px] text-muted-foreground">{t("review.nothing")}</p>
        ) : (
          <div className="space-y-1.5">
            {focus.map((s) => (
              <div key={s.name} className="border border-border bg-card px-3 py-2.5">
                <div className="flex items-baseline justify-between gap-3">
                  <span className="text-[13px] font-semibold">{s.name}</span>
                  {!review.solo && (
                    <span className="font-mono text-[12px] tabular-nums" style={{ color: lossColor(s.lost) }}>
                      {gap(s.lost)}
                    </span>
                  )}
                </div>
                {s.findings[0] && (
                  <>
                    <div className="mt-1 text-[12.5px] font-medium">{s.findings[0].title}</div>
                    <div className="mt-0.5 text-[12px] leading-snug text-muted-foreground">{s.findings[0].detail}</div>
                  </>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {review.overall.length > 0 && (
        <div>
          <Label>{t("review.overall")}</Label>
          <div className="divide-y divide-border border border-border bg-card">
            {review.overall.map((th) => (
              <div key={th.name} className="flex items-start gap-3 px-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="text-[12.5px] font-semibold">{th.name}</div>
                  <div className="mt-0.5 text-[12px] text-muted-foreground">{th.tip}</div>
                </div>
                {!review.solo && (
                  <span className="font-mono text-[12px] tabular-nums" style={{ color: lossColor(th.lost) }}>
                    {gap(th.lost)}
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/** What the recorder draws over the game, part by part. */
function HudTab() {
  const t = useT();
  const [hud, setHud] = useState<Hud | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    coachHud().then(setHud).catch((e) => setError(String(e)));
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
        <div className="mt-3 flex items-center justify-between gap-4 border-b border-border pb-3">
          <span className="text-[13px] font-semibold">{t("hud.enabled")}</span>
          <Switch checked={hud.enabled} onCheckedChange={(on) => void set("enabled", on)} />
        </div>
        <div className="divide-y divide-border">
          {hud.parts.map((p) => (
            <div key={p.key} className="flex items-center justify-between gap-4 py-2.5">
              <span className="text-[12.5px]">{p.label}</span>
              <Switch checked={p.on} disabled={!hud.enabled} onCheckedChange={(on) => void set(p.key, on)} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Panel() {
  const t = useT();
  const [tab, setTab] = useState<Tab>(() => {
    const first = initialOverlayTab();
    return isTab(first) ? first : "tips";
  });
  const [peer, setPeer] = useState<OverlayPeer | null>(null);
  const [hotkey, setHotkey] = useState("");
  const { last, data, error, load } = useLastReview();

  // The site's skin, as the main window wears it.
  useEffect(() => {
    document.documentElement.classList.add("coach-skin");
  }, []);

  useEffect(() => {
    getOverlayState()
      .then((s) => setHotkey(s.hotkey))
      .catch(() => {});
    getOverlayPeer().then(setPeer).catch(() => {});
    const offPeer = onOverlayPeer(setPeer);
    const offTab = onOverlayTab((id) => {
      if (isTab(id)) setTab(id);
    });
    return () => {
      void offPeer.then((f) => f());
      void offTab.then((f) => f());
    };
  }, []);

  const close = useCallback(() => void overlayHide().catch(() => {}), []);
  const path = last?.session.path;

  return (
    <OverlayFrame
      appName="MXB Coach"
      tabs={TABS.map(({ id, icon }) => ({ id, icon, label: t(`overlay.tab.${id}` as TKey) }))}
      active={tab}
      onTab={(id) => {
        if (isTab(id)) setTab(id);
      }}
      peerName={peer?.app === "manager" ? "MXB App" : undefined}
      peerTabs={peer?.tabs.map((id) => ({ id, label: peerTabLabel(t, id) }))}
      onPeerTab={(id) => void overlayHandoff(id).catch(() => {})}
      hotkey={hotkey}
      onOpenMain={() => void overlayOpenMain().catch(() => {})}
      onClose={close}
    >
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {tab === "tips" && (
          <>
            <div className="mb-3 flex justify-end">
              <button
                onClick={() => void load()}
                className="flex items-center gap-1 text-[12px] text-muted-foreground hover:text-foreground"
              >
                <RefreshCw className="size-3" />
                {t("common.refresh")}
              </button>
            </div>
            <Tips last={last} data={data} error={error} />
          </>
        )}
        {tab === "setup" &&
          (path && data ? (
            <SetupFixes path={path} findings={data.review.setup} />
          ) : (
            <p className="text-[12.5px] text-muted-foreground">{t("otips.none")}</p>
          ))}
        {tab === "cues" &&
          (path && last ? (
            <LiveCues path={path} lap={last.lap} />
          ) : (
            <p className="text-[12.5px] text-muted-foreground">{t("otips.none")}</p>
          ))}
        {tab === "hud" && <HudTab />}
      </div>
    </OverlayFrame>
  );
}

function ThemedToaster() {
  const { resolved } = useTheme();
  return <Toaster position="bottom-right" theme={resolved} richColors />;
}

/** MXB Coach's in-game overlay: the last lap's tips, setup fixes, live cues and the HUD. */
export default function CoachOverlay() {
  return (
    <ThemeProvider defaultTheme="dark" scalable={false}>
      <I18nProvider>
        <Panel />
        <ThemedToaster />
      </I18nProvider>
    </ThemeProvider>
  );
}
