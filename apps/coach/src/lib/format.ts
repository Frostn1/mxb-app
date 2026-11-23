/** `1:02.345`, or `58.901` under a minute. */
export function lapTime(ms: number | null | undefined): string {
  if (!ms || ms <= 0) return "—";
  const m = Math.floor(ms / 60000);
  const s = (ms % 60000) / 1000;
  return m > 0 ? `${m}:${s.toFixed(3).padStart(6, "0")}` : s.toFixed(3);
}

/** A time gap in seconds, signed: `+0.234`, `−0.120`. */
export function gap(s: number): string {
  const sign = s > 0.0005 ? "+" : s < -0.0005 ? "−" : "±";
  return `${sign}${Math.abs(s).toFixed(3)}`;
}

/** The recorder's `yyyymmdd-hhmmss-mmm` file stamp, as a local date and time. */
export function started(stamp: string): string {
  const m = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})/.exec(stamp);
  if (!m) return stamp;
  const d = new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]);
  return d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** Colour for time lost (red), gained (green) or level. */
export function lossColor(lost: number): string {
  if (lost > 0.05) return "var(--destructive)";
  if (lost < -0.05) return "var(--success)";
  return "var(--muted-foreground)";
}
