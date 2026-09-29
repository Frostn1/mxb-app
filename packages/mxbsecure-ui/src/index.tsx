/** The mxbsecure brand for React apps: the wordmark and the light/dark/system theme switch.
 *  Styles come from `tokens.css` / `theme.css` in this package. */

import { useEffect, useState } from "react";

/** The wordmark: the name itself, no icon (brand-logos/mxbsecure/logo.tsx). */
export function Logo({ className = "" }: { className?: string }) {
  return (
    <span
      className={className}
      style={{
        fontFamily: "var(--font-heading)",
        fontWeight: 800,
        letterSpacing: "-0.06em",
        color: "var(--primary)",
      }}
    >
      mxbsecure
    </span>
  );
}

export type ThemeChoice = "light" | "dark" | "system";

const KEY = "mxbsecure-theme";
const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

function stored(): ThemeChoice {
  try {
    const value = localStorage.getItem(KEY);
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}

/** Puts `dark` on <html> for the chosen theme, following the OS for "system". */
export function useTheme(): [ThemeChoice, (choice: ThemeChoice) => void] {
  const [choice, setChoice] = useState<ThemeChoice>(stored);
  const [systemDark, setSystemDark] = useState(() => darkQuery().matches);

  useEffect(() => {
    const query = darkQuery();
    const onChange = () => setSystemDark(query.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    const dark = choice === "dark" || (choice === "system" && systemDark);
    document.documentElement.classList.toggle("dark", dark);
  }, [choice, systemDark]);

  const set = (next: ThemeChoice) => {
    setChoice(next);
    try {
      if (next === "system") localStorage.removeItem(KEY);
      else localStorage.setItem(KEY, next);
    } catch {
      /* private mode: the choice lasts until the window closes */
    }
  };
  return [choice, set];
}
