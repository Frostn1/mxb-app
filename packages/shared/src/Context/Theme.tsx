import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { hasLooks, useSupporter } from "../api/supporter";
import {
  ACCENT_PROPS,
  accentVars,
  drawnColorway,
  isFreeNow,
  isPackColorway,
  nextFreeEnd,
  parseAccent,
  type PackColorway,
} from "../lib/accent";

export type ThemeMode = "light" | "dark" | "system";

/**
 * The palettes the app ships. `frost` is the original icy blue; the rest are
 * hue-shifted chrome as well as accent, so a colorway reads as a different app
 * rather than a blue app wearing a different button. `retro` goes further and
 * changes the type, the corners and the glass too — see `index.css`.
 */
export const COLORWAYS = [
  "frost",
  "ember",
  "moss",
  "violet",
  "rose",
  "slate",
  "retro",
] as const;
export type Colorway = (typeof COLORWAYS)[number];
export type AnyColorway = Colorway | PackColorway;


export {
  FREE_UNTIL,
  colorwayUnlocked,
  isFreeNow,
  PACK_COLORWAYS,
  PACK_SWATCH,
  accentVars,
  isPackColorway,
  parseAccent,
  type PackColorway,
} from "../lib/accent";

/**
 * How big the interface is drawn, as a multiplier.
 *
 * This is the webview's own zoom, not a font size. The app writes 778 type sizes as
 * arbitrary pixel values (`text-[13px]`), so scaling the root font would move the rem-based
 * spacing and leave nearly all of the text where it was. Zoom scales the rendered page
 * whole — type, spacing, icons and borders together — which is what "the font is too small
 * on 1440p" is actually asking for.
 */
export const UI_SCALES = [0.9, 1, 1.1, 1.25, 1.4, 1.6] as const;
export type UiScale = (typeof UI_SCALES)[number];

/** `[accent, chrome]` — what a colorway's swatch is drawn from. */
export const COLORWAY_SWATCH: Record<Colorway, [string, string]> = {
  frost: ["#0a84ff", "#1c1c1e"],
  ember: ["#f0a878", "#221b16"],
  moss: ["#8fd6a8", "#16211c"],
  violet: ["#b9a3f0", "#1e1a28"],
  rose: ["#f0a0b8", "#24181e"],
  slate: ["#cfd3d8", "#1c1c1e"],
  retro: ["#f0b429", "#16110a"],
};

interface ThemeContextValue {
  /** The user's preference. */
  theme: ThemeMode;
  /** What's actually applied right now (system resolved). */
  resolved: "light" | "dark";
  setTheme: (mode: ThemeMode) => void;
  /** The palette picked. A pack colorway without the pack is drawn as `frost`. */
  colorway: AnyColorway;
  setColorway: (colorway: AnyColorway) => void;
  /** The custom accent (Style Pack), `#rrggbb`, or null for the colorway's own. */
  accent: string | null;
  setAccent: (accent: string | null) => void;
  /** How big the interface is drawn. 1 is the design size. */
  scale: UiScale;
  setScale: (scale: UiScale) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const STORAGE_KEY = "frost-theme";
const COLORWAY_KEY = "frost-colorway";
const SCALE_KEY = "frost-ui-scale";
const ACCENT_KEY = "frost-accent";
/** The pick before a free pack colorway was tried, to go back to when it stops being free. */
const BEFORE_FREE_KEY = "frost-colorway-before-free";

/** The in-game overlay loads the same bundle with `?overlay=1` — see `main.tsx`. */
const IS_OVERLAY = new URLSearchParams(window.location.search).has("overlay");

function readStored(): ThemeMode {
  const v = localStorage.getItem(STORAGE_KEY);
  if (v === "light" || v === "dark" || v === "system") return v;
  // Back-compat: the old app stored the resolved "dark"/"light" string.
  return v ? (v as ThemeMode) : "system";
}

function readStoredColorway(): AnyColorway {
  const v = localStorage.getItem(COLORWAY_KEY) ?? "";
  return COLORWAYS.includes(v as Colorway) || isPackColorway(v) ? (v as AnyColorway) : "frost";
}

function readStoredAccent(): string | null {
  return parseAccent(localStorage.getItem(ACCENT_KEY));
}

function readStoredScale(): UiScale {
  const v = Number(localStorage.getItem(SCALE_KEY));
  return (UI_SCALES as readonly number[]).includes(v) ? (v as UiScale) : 1;
}

function systemPrefersDark() {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function ThemeProvider({
  children,
  defaultTheme = "system",
  scalable = true,
}: {
  children: ReactNode;
  /** What an app that has never been told starts in. The Studio starts light — see `studio.css`. */
  defaultTheme?: ThemeMode;
  /** Whether this app applies the interface scale. The Studio doesn't offer one. */
  scalable?: boolean;
}) {
  const stored = () => (localStorage.getItem(STORAGE_KEY) ? readStored() : defaultTheme);
  const [theme, setThemeState] = useState<ThemeMode>(stored);
  const [colorway, setColorwayState] = useState<AnyColorway>(readStoredColorway);
  const [accent, setAccentState] = useState<string | null>(readStoredAccent);
  const looks = hasLooks(useSupporter());
  const [scale, setScaleState] = useState<UiScale>(readStoredScale);
  const [systemDark, setSystemDark] = useState(systemPrefersDark);
  // Bumped when a free colorway goes back to the pack, so a window left open over midnight
  // on Oct 31 falls back without a restart.
  const [freeTick, setFreeTick] = useState(0);
  useEffect(() => {
    const end = nextFreeEnd();
    if (!end) return;
    // setTimeout overflows past ~24.8 days; re-arm from a shorter wait instead.
    const wait = Math.min(end.getTime() - Date.now() + 1000, 2 ** 31 - 1);
    const id = window.setTimeout(() => setFreeTick((n) => n + 1), wait);
    return () => window.clearTimeout(id);
  }, [freeTick]);

  // Track OS theme changes for `system`.
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSystemDark(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  // The in-game overlay is a second webview on the same origin, so a pick made
  // in Settings reaches it through the storage event rather than a reload.
  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key === STORAGE_KEY) setThemeState(readStored());
      if (e.key === COLORWAY_KEY) setColorwayState(readStoredColorway());
      if (e.key === SCALE_KEY) setScaleState(readStoredScale());
      if (e.key === ACCENT_KEY) setAccentState(readStoredAccent());
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const resolved: "light" | "dark" =
    theme === "system" ? (systemDark ? "dark" : "light") : theme;

  // Apply exactly one class to <html> so the token overrides + Tailwind's
  // `dark:` variant stay in sync. The colorway rides alongside as an attribute:
  // its rules are `.dark[data-colorway="…"]`, so they outrank the base palette
  // without needing !important.
  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove("light", "dark");
    root.classList.add(resolved);
  }, [resolved]);

  useEffect(() => {
    document.documentElement.dataset.colorway = drawnColorway(
      colorway,
      looks,
      new Date(),
      localStorage.getItem(BEFORE_FREE_KEY),
    );
  }, [colorway, looks, freeTick]);

  // Inline on <html>, so it outranks whichever colorway's rules are in play.
  useEffect(() => {
    const style = document.documentElement.style;
    for (const prop of ACCENT_PROPS) style.removeProperty(prop);
    if (!looks || !accent) return;
    for (const [prop, value] of Object.entries(accentVars(accent, resolved))) {
      style.setProperty(prop, value);
    }
  }, [accent, looks, resolved]);

  // Zoom is per-webview and does not persist across launches, so it is applied on mount as
  // well as on change. Never to the overlay: it is sized against the game's screen, not the
  // desk this window sits on, and scaling it would push it over what it is meant to annotate.
  useEffect(() => {
    if (IS_OVERLAY || !scalable) return;
    getCurrentWebview()
      .setZoom(scale)
      .catch((e) => console.warn("could not set the interface scale", e));
  }, [scale, scalable]);

  const setTheme = (mode: ThemeMode) => {
    localStorage.setItem(STORAGE_KEY, mode);
    setThemeState(mode);
  };

  const setAccent = (next: string | null) => {
    const clean = parseAccent(next);
    if (clean) localStorage.setItem(ACCENT_KEY, clean);
    else localStorage.removeItem(ACCENT_KEY);
    setAccentState(clean);
  };

  const setColorway = useCallback((next: AnyColorway) => {
    // Trying a colorway that's only free for now: remember what to go back to afterwards.
    if (next !== colorway && !looks && isFreeNow(next) && !isPackColorway(colorway)) {
      localStorage.setItem(BEFORE_FREE_KEY, colorway);
    }
    localStorage.setItem(COLORWAY_KEY, next);
    setColorwayState(next);
  }, [colorway, looks]);

  const setScale = (next: UiScale) => {
    localStorage.setItem(SCALE_KEY, String(next));
    setScaleState(next);
  };

  const value = useMemo(
    () => ({ theme, resolved, setTheme, colorway, setColorway, accent, setAccent, scale, setScale }),
    [theme, resolved, colorway, setColorway, accent, scale],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
