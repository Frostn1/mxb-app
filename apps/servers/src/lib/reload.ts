/** How the running mxbserver applies a changed `server.toml` key on a config reload. Mirrors
 *  crates/mxbserver/src/admin/reload.rs `classify` (mxbserver repo); the server's own answer
 *  (`/v1/config/validate` with the candidate) wins whenever it is available. */
export type ReloadClass = "hot" | "next_session" | "next_event" | "restart";

/** Keys whose change is applied to the queue or the bootstraps now and loaded by the next event
 *  (a track change): the rotation, the bike set, event options and the ruts. */
const NEXT_EVENT = new Set([
  "track.package",
  "rotation.tracks",
  "world.ruts",
  "world.soil",
  "world.soils",
  "world.wet",
  "world.seed",
  "world.raises",
  "world.rut_calibration",
  "world.live_ruts",
  "world.deform_sample",
]);

const HOT = new Set([
  "server.name",
  "server.password",
  "server.max_clients",
  "server.observe",
  "events.collisions",
  "ghost.react",
<<<<<<< ours
  // Restart-only on a server started without bots; the server's own answer says so.
=======
>>>>>>> theirs
  "ghost.fill_to",
  "world.suppress_client_relay",
]);

/** The class of one `section.key`. The server's own answer corrects this where it knows better
 *  (a server without live rotation, or one running in relay mode). */
export function reloadClass(key: string): ReloadClass {
  const under = (section: string) => key.startsWith(`${section}.`);
  if (HOT.has(key) || under("admission") || under("master") || under("admin") || under("recording")) return "hot";
  if (NEXT_EVENT.has(key) || under("bike_set") || under("bike_sets") || under("event") || under("event_probe")) return "next_event";
  if (key === "native.track_bounds") return "next_session";
  if (under("native")) return "hot";
  if (key === "sessions.race_extra_laps") return "next_session";
  if (under("sessions")) return "next_event";
  if (under("penalties") || under("cuts")) return "next_session";
  // The bots (how many, who, what they ride), and the exporters, change when no race is under way.
  if (under("ghost") || key === "track.roster") return "next_session";
  if (under("results") || under("points") || under("championship") || under("rating") || under("paints")) return "next_session";
  // The game port every rider is connected to, and development tools bound to the track's grid.
  return "restart";
}

export const BADGES: Record<ReloadClass, { label: string; title: string; tone: "ok" | "info" | "warn" }> = {
  hot: { label: "applies now", title: "The running server picks this up at once; nobody is disconnected.", tone: "ok" },
  next_session: {
    label: "next session",
    title: "Applies at once when no race is running, otherwise when the current race ends. Nobody is disconnected.",
    tone: "info",
  },
  next_event: {
    label: "next event",
    title: "Applies from the next event, when the track changes: session lengths, the track rotation, the bike set. The running event is untouched and nobody is disconnected.",
    tone: "info",
  },
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
