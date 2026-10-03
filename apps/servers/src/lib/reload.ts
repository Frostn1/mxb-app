/** How the running mxbserver applies a changed `server.toml` key on a config reload. Mirrors
 *  crates/mxbserver/src/admin/reload.rs `classify` (mxbserver repo); the server's own answer
 *  (`/v1/config/validate` with the candidate) wins whenever it is available. */
export type ReloadClass = "hot" | "next_session" | "next_event" | "restart";

/** `[ghost]` keys that only change how the existing bots ride: a rebuild picks them up. */
const GHOST_RIDING = new Set([
  "ghost.speed_jitter_pct",
  "ghost.lateral_m",
  "ghost.skill_pct",
  "ghost.racing",
  "ghost.race_starts",
  "ghost.laps",
  "ghost.synth",
  "ghost.synth_lift_m",
  "ghost.library_min_laps",
]);

const HOT = new Set([
  "server.name",
  "server.password",
  "server.max_clients",
  "events.collisions",
  "ghost.react",
  "world.suppress_client_relay",
]);

/** The class of one `section.key`. `admission.allowed_bikes` is hot only while it stays
 *  within the bikes the server advertised at startup; only the server knows that, so this
 *  says "hot" and the server's answer corrects it. */
export function reloadClass(key: string): ReloadClass {
  const under = (section: string) => key.startsWith(`${section}.`);
  if (HOT.has(key) || under("admission") || under("master")) return "hot";
  if (key === "native.track_bounds") return "restart";
  if (under("native")) return "hot";
  if (key === "sessions.race_extra_laps") return "next_session";
  if (under("sessions")) return "next_event";
  if (under("penalties") || under("cuts")) return "next_session";
  if (GHOST_RIDING.has(key) || under("ghost.personality")) return "next_session";
  return "restart";
}

export const BADGES: Record<ReloadClass, { label: string; title: string; tone: "ok" | "info" | "warn" }> = {
  hot: { label: "applies now", title: "The running server picks this up at once; nobody is disconnected.", tone: "ok" },
  next_session: {
    label: "next session",
    title: "Applies at once when no race is running, otherwise when the current race ends. Nobody is disconnected.",
    tone: "info",
  },
  next_event: { label: "next session", title: "Session lengths apply from the next event. Nobody is disconnected.", tone: "info" },
  restart: { label: "needs restart", title: "Only takes effect after the server restarts, which disconnects riders.", tone: "warn" },
};

/** One changed key as the server reports it (`changes[]` of reload/validate). */
export interface ReloadChange {
  field: string;
  from: unknown;
  to: unknown;
  class: ReloadClass;
  note: string;
  /** After applying: what happened to it. */
  outcome?: "applied" | "next_session" | "next_event" | "restart_required";
}

/** The class of each changed key: the server's answer where it has one, else ours. */
export function classesFor(keys: string[], server: ReloadChange[] | null): Record<string, ReloadClass> {
  const known = new Map((server ?? []).map((c) => [c.field, c.class]));
  return Object.fromEntries(keys.map((k) => [k, known.get(k) ?? reloadClass(k)]));
}
