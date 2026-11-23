import { useCallback, useEffect, useState, type ReactNode } from "react";
import {
  CheckCircle2,
  FolderOpen,
  Loader2,
  RefreshCw,
  Undo2,
  XCircle,
} from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { useConfig } from "@frost/shared/Context/Config";
import SurveySetting from "@frost/shared/Components/Survey/SurveySetting";
import UninstallSetting from "@frost/shared/Components/Uninstall/UninstallSetting";
import {
  COLORWAYS,
  COLORWAY_SWATCH,
  useTheme,
  type Colorway,
  type ThemeMode,
} from "@frost/shared/Context/Theme";
import SupporterLooks from "@frost/shared/Components/Supporter/SupporterLooks";
import { cn } from "@frost/shared/lib/utils";
import { APP_NAME, LOCALE_OPTIONS, useI18n, useT, type TKey } from "@/i18n";
import { useUpdate } from "@/Context/Update";
import { useNavStyle, type NavStyle } from "@frost/shared/lib/viewportNav";
import { blenderInstalls, type BlenderInfo } from "@/api/bikebuild";
import {
  studioDetectModsFolder,
  studioInspectModsFolder,
  studioSetModsPath,
} from "@/api/config";
import type { SettingsSection } from "@/lib/openSettings";
import { useBlenderStatus } from "../Studio/BikeBuilder/useBlenderStatus";
import TrackModelSettings from "./TrackModelSettings";

type SectionId = "general" | "appearance" | "paths" | "ai" | "about";

/** The nav, in order. One section is on screen at a time, as in MXB App's Settings. */
const SECTIONS: { id: SectionId; label: TKey }[] = [
  { id: "general", label: "studioSettings.sectionGeneral" },
  { id: "appearance", label: "studioSettings.sectionAppearance" },
  { id: "paths", label: "studioSettings.sectionPaths" },
  { id: "ai", label: "studioSettings.model" },
  { id: "about", label: "studioSettings.sectionAbout" },
];

const SECTION_KEY = "frost-studio-settings-section";

function readSection(): SectionId {
  try {
    const v = localStorage.getItem(SECTION_KEY);
    return SECTIONS.some((s) => s.id === v) ? (v as SectionId) : "general";
  } catch {
    return "general";
  }
}

const COLORWAY_LABEL: Record<Colorway, TKey> = {
  frost: "studioSettings.colorwayFrost",
  ember: "studioSettings.colorwayEmber",
  moss: "studioSettings.colorwayMoss",
  violet: "studioSettings.colorwayViolet",
  rose: "studioSettings.colorwayRose",
  slate: "studioSettings.colorwaySlate",
  retro: "studioSettings.colorwayRetro",
};

/** A card with a title, the same shape as MXB App's settings sections. */
function Section({ title, desc, children }: { title: string; desc?: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-3 rounded-xl bg-card p-[18px]">
      <span className="text-[14px] font-bold">{title}</span>
      {desc && <span className="-mt-1.5 text-[12px] leading-relaxed text-muted-foreground">{desc}</span>}
      {children}
    </div>
  );
}

/** A label on the left, its control on the right. */
function Line({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <div className="text-[12.5px] text-foreground/85">{label}</div>
        {hint && <div className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function General() {
  const t = useT();
  const { locale, setLocale } = useI18n();
  const [navigation, setNavigation] = useNavStyle();
  return (
      <Section title={t("studioSettings.sectionGeneral")}>
        <Line label={t("studioSettings.language")}>
          <select
            value={locale}
            onChange={(e) => setLocale(e.target.value as typeof locale)}
            className="w-56 cursor-default border border-border bg-transparent px-3 py-1.5 text-[13px]"
          >
            {LOCALE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value} className="bg-background">
                {o.label}
              </option>
            ))}
          </select>
        </Line>
        {/* The 3D views read the same key (`@frost/shared/lib/viewportNav`). */}
        <Line label={t("studioSettings.navigation")} hint={t("studioSettings.navigationHint")}>
          <Segmented
            size="sm"
            value={navigation}
            onChange={(v) => setNavigation(v as NavStyle)}
            options={[
              { value: "blender", label: t("studioSettings.navigationBlender") },
              { value: "classic", label: t("studioSettings.navigationClassic") },
            ]}
          />
        </Line>
        {/* Whether the Studio may ask anything. The same row MXB App and Coach show. */}
        <SurveySetting />
      </Section>
  );
}

/**
 * Light, dark or the system's, the free colorways, and the Style Pack's looks: the same pickers
 * as MXB App's Settings → Appearance, on the same shared theme. `skin-follows-colorway` on
 * <html> (index.html) is what lets a colorway replace the Studio's own palette.
 */
function Appearance() {
  const t = useT();
  const { theme, setTheme, colorway, setColorway } = useTheme();
  return (
    <Section title={t("studioSettings.sectionAppearance")}>
      <Line label={t("studioSettings.theme")} hint={t("studioSettings.themeHint")}>
        <Segmented
          size="sm"
          value={theme}
          onChange={(v) => setTheme(v as ThemeMode)}
          options={[
            { value: "light", label: t("studioSettings.themeLight") },
            { value: "dark", label: t("studioSettings.themeDark") },
            { value: "system", label: t("studioSettings.themeSystem") },
          ]}
        />
      </Line>
      <Line label={t("studioSettings.colorway")}>
        <div className="flex gap-1.5">
          {COLORWAYS.map((c) => {
            const [tint, chrome] = COLORWAY_SWATCH[c];
            const label = t(COLORWAY_LABEL[c]);
            return (
              <button
                key={c}
                type="button"
                title={label}
                aria-label={label}
                aria-pressed={colorway === c}
                onClick={() => setColorway(c)}
                className={cn(
                  "size-6 cursor-default rounded-full border transition-shadow",
                  colorway === c
                    ? "border-primary ring-2 ring-primary/40"
                    : "border-input hover:border-foreground/30",
                )}
                style={{ background: `linear-gradient(135deg, ${tint} 0 50%, ${chrome} 50% 100%)` }}
              />
            );
          })}
        </div>
      </Line>
      <SupporterLooks />
    </Section>
  );
}

/** Where Blender is, choosing it by hand or from what Re-scan found, and looking again. */
function BlenderPath() {
  const t = useT();
  const { status, checking, ready, summary, check, onPickBlender, onUseBlender, onForgetBlender } =
    useBlenderStatus();
  const found = status?.found ?? null;
  const [installs, setInstalls] = useState<BlenderInfo[] | null>(null);
  const [scanning, setScanning] = useState(false);

  // `blender_installs` looks again on every call, newest first.
  const rescan = useCallback(() => {
    setScanning(true);
    blenderInstalls()
      .then(setInstalls)
      .catch(() => setInstalls([]))
      .finally(() => setScanning(false));
  }, []);
  useEffect(() => rescan(), [rescan]);

  const busy = checking || scanning;
  const inUse = (b: BlenderInfo) => !!found && found.path.toLowerCase() === b.path.toLowerCase();

  return (
    <Section title={t("studioSettings.blender")} desc={t("studioSettings.blenderHint")}>
      <div className="min-w-0">
        <div className="truncate font-mono text-[12px] text-muted-foreground" title={found?.path}>
          {found?.path ?? "—"}
        </div>
        <div className="mt-1.5 flex items-center gap-1.5 text-[12px] text-muted-foreground">
          {status === null ? (
            <Loader2 className="size-3.5 shrink-0 animate-spin text-faint" />
          ) : ready ? (
            <CheckCircle2 className="size-3.5 shrink-0 text-success" />
          ) : (
            <XCircle className="size-3.5 shrink-0 text-amber-500" />
          )}
          <span className="min-w-0 truncate">{summary}</span>
        </div>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={() => void onPickBlender()} disabled={busy}>
          <FolderOpen className="size-3.5" />
          {t("bike.chooseBlender")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            rescan();
            check();
          }}
          disabled={busy}
        >
          <RefreshCw className={cn("size-3.5", scanning && "animate-spin")} />
          {t("studioSettings.blenderRescan")}
        </Button>
        {status?.saved && (
          <Button size="sm" variant="ghost" onClick={() => void onForgetBlender()} disabled={busy}>
            <Undo2 className="size-3.5" />
            {t("bike.findBlender")}
          </Button>
        )}
      </div>
      {installs !== null && (
        <div className="min-w-0">
          <div className="pb-1 text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t("studioSettings.blenderFoundList")}
          </div>
          {installs.length === 0 ? (
            <div className="py-2 text-[12px] text-muted-foreground">{t("studioSettings.blenderNoneFound")}</div>
          ) : (
            <div className="flex flex-col gap-0.5">
              {installs.map((b) => (
                <div
                  key={b.path}
                  data-selected={inUse(b)}
                  className="flex items-center gap-3 rounded-md px-2.5 py-1.5"
                >
                  <span className="w-12 shrink-0 font-mono text-[12px] font-semibold">{b.version}</span>
                  <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-muted-foreground" title={b.path}>
                    {b.path}
                  </span>
                  {!b.supported && (
                    <span className="shrink-0 text-[11.5px] text-amber-500">{t("studioSettings.blenderOld")}</span>
                  )}
                  {inUse(b) ? (
                    <span className="shrink-0 text-[11.5px] font-semibold">{t("studioSettings.blenderInUse")}</span>
                  ) : (
                    <Button
                      size="sm"
                      variant="outline"
                      className="h-6 shrink-0 px-2 text-[11.5px]"
                      onClick={() => void onUseBlender(b.path)}
                      disabled={busy || !b.supported}
                    >
                      {t("studioSettings.blenderUse")}
                    </Button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </Section>
  );
}

/** Choose the MX Bikes mods folder used by Studio. */
function Paths() {
  const t = useT();
  const { config, reloadConfig } = useConfig();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();
  const [note, setNote] = useState<string | undefined>();

  const applyFolder = useCallback(
    async (path: string) => {
      const check = await studioInspectModsFolder(path);
      if (!check.usable) {
        setError(t("studioSettings.notMxbFolder"));
        setNote(undefined);
        return;
      }
      await studioSetModsPath(path);
      await reloadConfig();
      setError(undefined);
      setNote(
        check.correction || !check.matchesExpected
          ? t("studioSettings.savedUnusual")
          : undefined,
      );
    },
    [reloadConfig, t],
  );

  const browse = useCallback(async () => {
    const picked = await open({ directory: true, defaultPath: config.modsPath || undefined });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      await applyFolder(picked);
    } catch (e) {
      setError(String(e));
      setNote(undefined);
    } finally {
      setBusy(false);
    }
  }, [config.modsPath, applyFolder]);

  const autoDetect = useCallback(async () => {
    setBusy(true);
    setError(undefined);
    setNote(undefined);
    try {
      const detected = await studioDetectModsFolder();
      if (!detected) {
        setError(t("studioSettings.notFound"));
        return;
      }
      await applyFolder(detected);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [t, applyFolder]);

  return (
    <>
      <Section title={t("studioSettings.mods")}>
        <div className="flex min-w-0 items-center gap-2">
          <input
            readOnly
            value={config.modsPath || "—"}
            title={config.modsPath}
            className="min-w-0 flex-1 truncate border border-input bg-transparent px-3 py-1.5 font-mono text-[12px] text-muted-foreground"
          />
          <Button size="sm" variant="outline" onClick={() => void browse()} disabled={busy}>
            {t("studioSettings.browse")}
          </Button>
          <Button size="sm" variant="outline" onClick={() => void autoDetect()} disabled={busy}>
            {busy && <Loader2 className="size-3.5 animate-spin" />}
            {t("studioSettings.autoDetect")}
          </Button>
        </div>
        {error && <div className="text-[11.5px] text-destructive">{error}</div>}
        {!error && note && <div className="text-[11.5px] text-muted-foreground">{note}</div>}
      </Section>
      <BlenderPath />
    </>
  );
}

function About() {
  const t = useT();
  const { available, installing, install } = useUpdate();
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => {});
  }, []);
  return (
      <Section title={APP_NAME}>
        {version && (
          <span className="text-[12.5px] text-muted-foreground">
            {t("studioSettings.version", { version })}
          </span>
        )}
        {available && (
          <Line label={t("studioSettings.updateReady", { version: available.version })}>
            <Button size="sm" onClick={() => void install()} disabled={installing}>
              {t("studioSettings.updateInstall")}
            </Button>
          </Line>
        )}
        {/* Last on the page, the same row MXB App and Coach show. */}
        <UninstallSetting />
      </Section>
  );
}

/**
 * The studio's settings, in sections with a nav on the left — the same layout as MXB App's
 * Settings, so the two apps read alike. Only the section picked is on screen.
 */
export default function Settings({ section }: { section?: SettingsSection } = {}) {
  const t = useT();
  // Opened at a section (`openSettings("paths")`, e.g. the Bike tab's Blender link): start
  // there; otherwise the one picked last time.
  const [active, setActive] = useState<SectionId>(() => section ?? readSection());

  const goto = (id: SectionId) => {
    setActive(id);
    try {
      localStorage.setItem(SECTION_KEY, id);
    } catch {
      // Storage blocked: the pick just isn't remembered.
    }
  };

  return (
    <div className="flex h-full">
      <nav className="flex w-[170px] flex-none flex-col gap-0.5 overflow-y-auto px-4 pb-5 pt-8">
        <span className="px-3 pb-1 text-[10.5px] font-semibold uppercase tracking-wide text-faint">
          {t("studioSettings.title")}
        </span>
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            type="button"
            onClick={() => goto(s.id)}
            aria-current={active === s.id ? "page" : undefined}
            // The Studio's one selected look (`data-selected`, studio.css), not a tint of its own.
            data-selected={active === s.id}
            className={cn(
              "cursor-default rounded-md px-3 py-1.5 text-left text-[12.5px] transition-colors",
              active === s.id
                ? "font-semibold text-foreground"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {t(s.label)}
          </button>
        ))}
      </nav>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-8 pr-8">
        <div className="flex min-w-0 max-w-[820px] flex-col gap-[18px]">
          {active === "general" && <General />}
          {active === "appearance" && <Appearance />}
          {active === "paths" && <Paths />}
          {active === "ai" && (
            <Section title={t("studioSettings.model")}>
              <TrackModelSettings />
            </Section>
          )}
          {active === "about" && <About />}
        </div>
      </div>
    </div>
  );
}
