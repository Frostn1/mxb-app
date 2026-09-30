/** 3725 -> "1h 2m", 95 -> "1m 35s". */
export function duration(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s % 60}s`;
  return `${s}s`;
}

/** 71.25 -> "1:11.250". */
export function lapTime(seconds: number | null): string {
  if (seconds == null || !Number.isFinite(seconds)) return "—";
  const m = Math.floor(seconds / 60);
  const rest = seconds - m * 60;
  return `${m}:${rest.toFixed(3).padStart(6, "0")}`;
}

/** "practice" or "running(practice)" -> "Practice"; "countdown(race)" -> "Race countdown". */
export function sessionName(stage: string): string {
  if (!stage) return "—";
  const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
  const m = /^(\w+)\((\w+)\)$/.exec(stage);
  const countdown = /^countdown\s*\{\s*next:\s*(\w+)\s*\}$/i.exec(stage);
  if (countdown) return `${cap(countdown[1])} countdown`;
  if (!m) return cap(stage);
  return m[1] === "running" ? cap(m[2]) : `${cap(m[2])} ${m[1]}`;
}
