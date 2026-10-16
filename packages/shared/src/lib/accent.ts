/**
 * The supporter pack's looks, as plain data — no DOM, so it can be tested on its own. Applied by
 * `Context/Theme.tsx`.
 */

/**
 * The supporter pack's colorways. Never in `COLORWAYS` (Theme.tsx), which stays the free set: these
 * are applied only while the account has the pack, and fall back to `frost` otherwise — the
 * pick is kept, so it comes back the moment the pack does.
 */
export const PACK_COLORWAYS = ["night", "works", "lime"] as const;
export type PackColorway = (typeof PACK_COLORWAYS)[number];

export function isPackColorway(c: string): c is PackColorway {
  return (PACK_COLORWAYS as readonly string[]).includes(c);
}

export const PACK_SWATCH: Record<PackColorway, [string, string]> = {
  night: ["#5ce1ff", "#111a2b"],
  works: ["#ff4a3d", "#1b1717"],
  lime: ["#c6f432", "#191c16"],
};

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
