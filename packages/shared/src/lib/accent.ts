/**
 * The supporter pack's looks, as plain data — no DOM, so it can be tested on its own. Applied by
 * `Context/Theme.tsx`.
 */

/**
 * The supporter pack's colorways. Never in `COLORWAYS` (Theme.tsx), which stays the free set: these
 * are applied only while the account has the pack, and fall back to `frost` otherwise — the
 * pick is kept, so it comes back the moment the pack does.
 */
export const PACK_COLORWAYS = ["night", "works", "lime", "halloween"] as const;
export type PackColorway = (typeof PACK_COLORWAYS)[number];

export function isPackColorway(c: string): c is PackColorway {
  return (PACK_COLORWAYS as readonly string[]).includes(c);
}

export const PACK_SWATCH: Record<PackColorway, [string, string]> = {
  night: ["#5ce1ff", "#111a2b"],
  works: ["#ff4a3d", "#1b1717"],
  lime: ["#c6f432", "#191c16"],
  halloween: ["#ff8a24", "#181020"],
};

/**
 * Pack colorways free for everyone until a date: the moment (local time) each goes back to the
 * Style Pack. Halloween is free through the end of Oct 31.
 */
export const FREE_UNTIL: Partial<Record<PackColorway, Date>> = {
  halloween: new Date(2026, 10, 1),
};

/** Whether `c` is free for everyone right now. */
export function isFreeNow(c: string, now: Date = new Date()): boolean {
  const until = isPackColorway(c) ? FREE_UNTIL[c] : undefined;
  return until !== undefined && now < until;
}

/** Whether `c` can be used: a free colorway, a pack one with the looks, or a pack one free right now. */
export function colorwayUnlocked(c: string, looks: boolean, now: Date = new Date()): boolean {
  return !isPackColorway(c) || looks || isFreeNow(c, now);
}

/**
 * The colorway actually drawn for the pick `c`. A pack colorway that isn't unlocked falls back
 * quietly: to `previous` (the pick before a free one was tried) when there is one, else frost.
 */
export function drawnColorway(
  c: string,
  looks: boolean,
  now: Date,
  previous: string | null,
): string {
  if (colorwayUnlocked(c, looks, now)) return c;
  // Only a colorway that was free goes back to what came before it; the rest of the pack
  // has always been drawn as frost without it.
  const wasFree = isPackColorway(c) && FREE_UNTIL[c] !== undefined;
  return wasFree && previous && previous !== c && !isPackColorway(previous) ? previous : "frost";
}

/** The next moment a free colorway goes back to the pack, or null when none is still free. */
export function nextFreeEnd(now: Date = new Date()): Date | null {
  const ends = Object.values(FREE_UNTIL).filter((d): d is Date => d !== undefined && now < d);
  return ends.length ? new Date(Math.min(...ends.map((d) => d.getTime()))) : null;
}

/** A `#rrggbb` the accent picker accepts, lower-cased; anything else is null. */
export function parseAccent(value: string | null | undefined): string | null {
  const v = (value ?? "").trim().toLowerCase();
  return /^#[0-9a-f]{6}$/.test(v) ? v : null;
}

/**
 * The custom accent (supporter pack) as the CSS variables it overrides. The text on a filled
 * accent is black or white by the colour's luminance, so a pale pick stays readable.
 */
export function accentVars(hex: string, mode: "light" | "dark"): Record<string, string> {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  const luminance = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
  return {
    "--primary": hex,
    "--ring": hex,
    "--primary-foreground": luminance > 0.4 ? "#111111" : "#ffffff",
    "--accent": `rgba(${r}, ${g}, ${b}, ${mode === "dark" ? 0.14 : 0.12})`,
    "--accent-foreground":
      mode === "dark"
        ? `color-mix(in oklab, ${hex} 70%, #ffffff)`
        : `color-mix(in oklab, ${hex} 75%, #000000)`,
  };
}

/** Every property {@link accentVars} sets, for clearing them again. */
export const ACCENT_PROPS = ["--primary", "--ring", "--primary-foreground", "--accent", "--accent-foreground"];
