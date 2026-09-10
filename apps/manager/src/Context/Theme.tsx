import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

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
  frost: ["#9ccfec", "#1a1d22"],
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
  /** The palette applied on top of light/dark. */
  colorway: Colorway;
  setColorway: (colorway: Colorway) => void;
  /** How big the interface is drawn. 1 is the design size. */
  scale: UiScale;
  setScale: (scale: UiScale) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const STORAGE_KEY = "frost-theme";
const COLORWAY_KEY = "frost-colorway";
const SCALE_KEY = "frost-ui-scale";

/** The in-game overlay loads the same bundle with `?overlay=1` — see `main.tsx`. */
const IS_OVERLAY = new URLSearchParams(window.location.search).has("overlay");

function readStored(): ThemeMode {
  const v = localStorage.getItem(STORAGE_KEY);
  if (v === "light" || v === "dark" || v === "system") return v;
  // Back-compat: the old app stored the resolved "dark"/"light" string.
  return v ? (v as ThemeMode) : "system";
}

function readStoredColorway(): Colorway {
  const v = localStorage.getItem(COLORWAY_KEY);
  return COLORWAYS.includes(v as Colorway) ? (v as Colorway) : "frost";
}

function readStoredScale(): UiScale {
  const v = Number(localStorage.getItem(SCALE_KEY));
  return (UI_SCALES as readonly number[]).includes(v) ? (v as UiScale) : 1;
}

function systemPrefersDark() {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemeMode>(readStored);
  const [colorway, setColorwayState] = useState<Colorway>(readStoredColorway);
  const [scale, setScaleState] = useState<UiScale>(readStoredScale);
  const [systemDark, setSystemDark] = useState(systemPrefersDark);

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
    document.documentElement.dataset.colorway = colorway;
  }, [colorway]);

  // Zoom is per-webview and does not persist across launches, so it is applied on mount as
  // well as on change. Never to the overlay: it is sized against the game's screen, not the
  // desk this window sits on, and scaling it would push it over what it is meant to annotate.
  useEffect(() => {
    if (IS_OVERLAY) return;
    getCurrentWebview()
      .setZoom(scale)
      .catch((e) => console.warn("could not set the interface scale", e));
  }, [scale]);

  const setTheme = (mode: ThemeMode) => {
    localStorage.setItem(STORAGE_KEY, mode);
    setThemeState(mode);
  };

  const setColorway = (next: Colorway) => {
    localStorage.setItem(COLORWAY_KEY, next);
    setColorwayState(next);
  };

  const setScale = (next: UiScale) => {
    localStorage.setItem(SCALE_KEY, String(next));
    setScaleState(next);
  };

  const value = useMemo(
    () => ({ theme, resolved, setTheme, colorway, setColorway, scale, setScale }),
    [theme, resolved, colorway, scale],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
