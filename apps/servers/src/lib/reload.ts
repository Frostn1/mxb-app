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
  // The ground is rebuilt at the new level when the session ends or is restarted; the first
  // time the key is added the server says "needs restart" itself (its answer wins).
  if (key === "world.deformation") return "next_session";
  if (key === "world.ruts_persist") return "hot";
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

/** The words an operator sees for each class (the wire names stay the server's own). */
export const BADGES: Record<ReloadClass, { label: string; title: string; tone: "ok" | "info" | "warn" }> = {
  hot: { label: "Applies now", title: "The running server picks this up at once; nobody is disconnected.", tone: "ok" },
  next_session: {
    label: "Next session",
    title: "Applies when the current session ends or is restarted (at once when no race is running). Nobody is disconnected.",
    tone: "info",
  },
  next_event: {
    label: "Next track load",
    title: "Applies when the server loads the next event or track in the rotation: session lengths, the track rotation, the bike set, the starting ruts. The running event is untouched and nobody is disconnected. \"Apply now\" reloads the current event on the same track (riders go back to the pits).",
    tone: "info",
  },
  restart: { label: "Needs restart", title: "Only takes effect after the server restarts, which disconnects riders.", tone: "warn" },
};

/** The badge words for a class: what the settings page and the review step print. */
export const badgeLabel = (kind: ReloadClass): string => BADGES[kind].label;

/** Whether any of these classes waits for the next track load, so "Apply now" (reload the
 *  current event) makes it take effect. */
export const waitsForTrackLoad = (classes: Iterable<ReloadClass>): boolean => {
  for (const kind of classes) if (kind === "next_event") return true;
  return false;
};

/** Whether a change touches which tracks the server plays: only then is the server's slow
 *  check (it loads the tracks) worth running on top of its quick dry run. */
export const changesTracks = (keys: string[]): boolean => keys.some((key) => key === "track.package" || key.startsWith("rotation."));

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
