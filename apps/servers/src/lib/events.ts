/** The Events tab's feed: what mxbserver says happened, as typed events.
 *
 *  Three sources, all already reachable from this app:
 *  - the server log (`server_logs`: the local log file or `journalctl -u mxbserver`), whose
 *    `event collision:` / `event cut:` / `event holeshot:` / `race` / `session` / `join` /
 *    `refused` lines are parsed here;
 *  - the observe `/timing` feed, polled and diffed: a lap count going up is a lap finish, a new
 *    split time is a time check, a penalty going up is a penalty;
 *  - the admin `/v1/riders` list (only with an admin token), diffed for named joins and leaves.
 */
import type { Rider } from "./api";

export type EventKind =
  | "lap"
  | "split"
  | "collision"
  | "cut"
  | "crash"
  | "penalty"
  | "race"
  | "join"
  | "leave"
  | "session"
  | "refused"
  | "info";

export const EVENT_KINDS: { kind: EventKind; label: string }[] = [
  { kind: "lap", label: "Lap finish" },
  { kind: "split", label: "Time check" },
  { kind: "collision", label: "Collision" },
  { kind: "cut", label: "Track cut" },
  { kind: "crash", label: "Crash" },
  { kind: "penalty", label: "Penalty" },
  { kind: "race", label: "Race" },
  { kind: "join", label: "Join" },
  { kind: "leave", label: "Leave" },
  { kind: "session", label: "Session" },
  { kind: "refused", label: "Refused" },
  { kind: "info", label: "Info" },
];

export type FieldValue = string | number | boolean | null;

export interface ServerEvent {
  id: string;
  /** When this app saw it (ms since epoch). Log lines carry no time of their own. */
  at: number;
  kind: EventKind;
  /** One line for the list. */
  summary: string;
  /** The rider it is about, when there is one (both riders for a collision). */
  riders: string[];
  /** Every detail, for the expanded row. */
  fields: Record<string, FieldValue>;
  source: "log" | "timing" | "riders";
  /** Already in the log when the tab opened. */
  backlog?: boolean;
  /** The log line it came from. */
  raw?: string;
}

/** What a parsed log line becomes before it gets an id and a time. */
export type ParsedEvent = Omit<ServerEvent, "id" | "at" | "source" | "backlog">;

/** Stops the log from flooding the buffer. */
export const MAX_EVENTS = 5000;

// eslint-disable-next-line no-control-regex
const ANSI = /\x1b\[[0-9;]*m/g;

const num = (text: string | undefined): number | null => {
  if (text == null) return null;
  const value = Number(text);
  return Number.isFinite(value) ? value : null;
};

/** `"+1200s"` / `"+75.5s"` / `"+250ms"` (Rust's Duration Debug) -> seconds. */
const debugSeconds = (text: string): number | null => {
  const m = /^([\d.]+)(ms|µs|us|ns|s)$/.exec(text.trim());
  if (!m) return null;
  const value = Number(m[1]);
  const scale = m[2] === "s" ? 1 : m[2] === "ms" ? 1e-3 : m[2] === "ns" ? 1e-9 : 1e-6;
  return value * scale;
};

/** `Running(Qualifying)` -> "Qualifying", `Countdown { next: Race }` -> "Race countdown". */
export function stageText(stage: string): string {
  const s = stage.trim();
  const countdown = /^Countdown\s*\{\s*next:\s*(\w+)\s*\}$/.exec(s);
  if (countdown) return `${countdown[1]} countdown`;
  const running = /^Running\((\w+)\)$/.exec(s);
  if (running) return running[1];
  if (s === "RotationReady") return "Rotation ready";
  return s;
}

/** One log line -> an event, or null for noise (diagnostics, blank lines, banners). */
export function parseLogLine(input: string): ParsedEvent | null {
  const raw = input.replace(ANSI, "").replace(/\s+$/, "");
  const line = raw.trim();
  if (!line) return null;

  let m = /^event collision: #(-?\d+) (.*?) -> #(-?\d+) (.*?) · ([\w-]+) · ([\d.]+) km\/h closing(?: · s=([\d.]+) m)?$/.exec(line);
  if (m) {
    const [, hitterRace, hitter, hitRace, hit, kind, kmh, s] = m;
    return {
      kind: "collision",
      summary: `#${hitterRace} ${hitter} hit #${hitRace} ${hit} · ${kind} · ${Number(kmh).toFixed(0)} km/h`,
      riders: [hitter, hit],
      fields: {
        "Rider": hitter,
        "Race number": num(hitterRace),
        "Other rider": hit,
        "Other race number": num(hitRace),
        "Impact": kind,
        "Closing speed (km/h)": num(kmh),
        "Track position (m)": num(s),
      },
      raw,
    };
  }

  m = /^event cut: #(-?\d+) "((?:\\.|[^"])*)" · (.*?) · route ([\d.]+) m · ([\d.]+) m outside for ([\d.]+) s$/.exec(line);
  if (m) {
    const [, race, name, track, route, outside, seconds] = m;
    const rider = name.replace(/\\(.)/g, "$1");
    return {
      kind: "cut",
      summary: `#${race} ${rider} cut the track · ${Number(outside).toFixed(1)} m outside for ${Number(seconds).toFixed(2)} s`,
      riders: [rider],
      fields: {
        "Rider": rider,
        "Race number": num(race),
        "Track": track,
        "Track position (m)": num(route),
        "Distance outside (m)": num(outside),
        "Time outside (s)": num(seconds),
      },
      raw,
    };
  }

  m = /^event holeshot: #(-?\d+) · ([\d.]+) s since the gate drop/.exec(line);
  if (m) {
    return {
      kind: "race",
      summary: `Holeshot · #${m[1]} · ${Number(m[2]).toFixed(2)} s after the gate drop`,
      riders: [],
      fields: { "Event": "Holeshot", "Race number": num(m[1]), "Time since gate drop (s)": num(m[2]) },
      raw,
    };
  }

  m = /^diag ghost: bot #(-?\d+) (crashes|is pushed) at ([\d.]+) km\/h$/.exec(line);
  if (m) {
    const crashed = m[2] === "crashes";
    return {
      kind: "crash",
      summary: `Bot #${m[1]} ${crashed ? "crashed" : "was pushed"} at ${m[3]} km/h`,
      riders: [],
      fields: { "Race number": num(m[1]), "Bot": true, "Outcome": crashed ? "Crash" : "Pushed", "Impact speed (km/h)": num(m[3]) },
      raw,
    };
  }

  m = /^diag disconnect: (\S+) \(client (\d+)\) sent DISCONNECTION \((\w+)\) in (\w+)$/.exec(line);
  if (m) {
    return {
      kind: "leave",
      summary: `${m[1]} disconnected`,
      riders: [],
      fields: { "Address": m[1], "Client": num(m[2]), "Message": m[3], "Phase": m[4] },
      raw,
    };
  }

  m = /^race\s+chequered flag · leader (-?\d+) on lap (\d+)$/.exec(line);
  if (m) {
    return {
      kind: "race",
      summary: `Chequered flag · leader #${m[1]} on lap ${m[2]}`,
      riders: [],
      fields: { "Event": "Chequered flag", "Race number": num(m[1]), "Lap": num(m[2]) },
      raw,
    };
  }
  m = /^race\s+(-?\d+) finished · (\d+) laps$/.exec(line);
  if (m) {
    return {
      kind: "race",
      summary: `#${m[1]} finished · ${m[2]} laps`,
      riders: [],
      fields: { "Event": "Finished", "Race number": num(m[1]), "Laps": num(m[2]) },
      raw,
    };
  }
  m = /^race\s+(-?\d+) retired · (.*)$/.exec(line);
  if (m) {
    return {
      kind: "race",
      summary: `#${m[1]} retired · ${m[2]}`,
      riders: [],
      fields: { "Event": "Retired", "Race number": num(m[1]), "Reason": m[2] },
      raw,
    };
  }
  m = /^race\s+over · (\d+) finished, (\d+) retired$/.exec(line);
  if (m) {
    return {
      kind: "race",
      summary: `Race over · ${m[1]} finished, ${m[2]} retired`,
      riders: [],
      fields: { "Event": "Race over", "Finished": num(m[1]), "Retired": num(m[2]) },
      raw,
    };
  }

  m = /^session\s+(operator(?: jump)? )?\+(\S+) -> (.+)$/.exec(line);
  if (m) {
    const stage = stageText(m[3]);
    return {
      kind: "session",
      summary: `${m[1] ? "Operator: " : ""}${stage}`,
      riders: [],
      fields: { "Stage": stage, "By operator": !!m[1], "Event clock (s)": debugSeconds(m[2]) },
      raw,
    };
  }
  m = /^session\s+operator restarted (\w+) at \+(\S+)$/.exec(line);
  if (m) {
    return {
      kind: "session",
      summary: `Operator restarted ${m[1]}`,
      riders: [],
      fields: { "Stage": m[1], "By operator": true, "Event clock (s)": debugSeconds(m[2]) },
      raw,
    };
  }
  m = /^session\s+rotation · (starting|restarting) the event on (.+)$/.exec(line);
  if (m) {
    return {
      kind: "session",
      summary: `${m[1] === "starting" ? "New event" : "Event restarted"} on ${m[2]}`,
      riders: [],
      fields: { "Stage": "Rotation", "Track": m[2] },
      raw,
    };
  }
  m = /^session\s+practice forever: clock re-armed/.exec(line);
  if (m) {
    return { kind: "session", summary: "Practice clock re-armed", riders: [], fields: { "Stage": "Practice" }, raw };
  }

  m = /^join\s+(\S+) (.+)$/.exec(line);
  if (m) {
    const [, address, what] = m;
    const bike = /change accepted · bike (.+)$/.exec(what)?.[1] ?? null;
    const ping = /^ping (\d+) ms$/.exec(what)?.[1];
    const step = what.split(" · ")[0];
    const fields: Record<string, FieldValue> = { "Address": address, "Step": step };
    if (bike) fields["Bike"] = bike;
    if (ping) fields["Ping (ms)"] = num(ping);
    return { kind: "join", summary: `${address} ${what}`, riders: [], fields, raw };
  }

  m = /^refused\s+(\S+) (\S+): (.*)$/.exec(line);
  if (m) {
    return {
      kind: m[1] === "timeout" ? "leave" : "refused",
      summary: m[1] === "timeout" ? `${m[2]} timed out · ${m[3]}` : `Refused ${m[2]} at ${m[1]} · ${m[3]}`,
      riders: [],
      fields: { "Stage": m[1], "Address": m[2], "Reason": m[3] },
      raw,
    };
  }
  m = /^paint lock: refused (\S+) for (.+)$/.exec(line);
  if (m) {
    return { kind: "refused", summary: `Locked paint refused for ${m[2]}`, riders: [], fields: { "Paint": m[1], "Rider": m[2] }, raw };
  }

  // Diagnostics and per-packet chatter stay in the Logs tab.
  if (/^(diag |dev: |native |mxbserver: (test-rut|live-rut))/.test(line)) return null;
  if (/^(recording|bots?|ghost|ruts|live-ruts|rut-calibration|traffic|probe|handlers|manifest|bike set|packets|inventory|upstream)\s/.test(line)) return null;
  if (/^[=─-]{3,}$/.test(line)) return null;
  return { kind: "info", summary: line, riders: [], fields: {}, raw };
}

/** The lines of `next` that weren't in `prev`, for two tails of the same log: `next` is a later
 *  window, so `prev` ends where some prefix of `next` does. No overlap at all means everything
 *  in `next` is new (more lines arrived than the window holds, or the log was rotated). */
export function newLogLines(prev: string[], next: string[]): string[] {
  if (prev.length === 0) return next;
  for (let start = 0; start < prev.length; start++) {
    const overlap = prev.length - start;
    if (overlap > next.length) continue;
    let same = true;
    for (let i = 0; i < overlap; i++) {
      if (prev[start + i] !== next[i]) {
        same = false;
        break;
      }
    }
    if (same) return next.slice(overlap);
  }
  return next;
}

/** One row of the observe `/timing` feed (crates/mxbserver/src/observability.rs). */
export interface TimingEntry {
  position: number;
  race: number;
  name: string;
  bike: string | null;
  laps: number;
  laps_down?: number | null;
  last_lap_seconds: number | null;
  best_lap_seconds: number | null;
  last_split1_seconds: number | null;
  last_split2_seconds: number | null;
  gap_to_leader_seconds: number | null;
  interval_seconds: number | null;
  finished: boolean;
  retired: boolean;
  penalty_seconds: number | null;
  disqualified: boolean;
}

export interface Timing {
  available: boolean;
  track?: string;
  session?: string;
  flag?: string | null;
  entries?: TimingEntry[];
}

const secs = (value: number | null | undefined) => (value == null ? "—" : `${value.toFixed(3)} s`);

/** 71.25 -> "1:11.250". */
export const clock = (seconds: number | null | undefined) => {
  if (seconds == null || !Number.isFinite(seconds)) return "—";
  const m = Math.floor(seconds / 60);
  return `${m}:${(seconds - m * 60).toFixed(3).padStart(6, "0")}`;
};

/** Events between two `/timing` answers: lap finishes, time checks, penalties. A rider seen for
 *  the first time sets a baseline and makes no event. */
export function diffTiming(prev: Timing | null, next: Timing): ParsedEvent[] {
  if (!next.available || !next.entries) return [];
  if (!prev?.available || !prev.entries || prev.track !== next.track || prev.session !== next.session) return [];
  const before = new Map(prev.entries.map((e) => [e.race, e]));
  const out: ParsedEvent[] = [];
  for (const e of next.entries) {
    const was = before.get(e.race);
    if (!was) continue;
    const common = { "Rider": e.name, "Race number": e.race, "Bike": e.bike, "Session": next.session ?? null, "Track": next.track ?? null };
    for (const split of [1, 2] as const) {
      const key = split === 1 ? "last_split1_seconds" : "last_split2_seconds";
      const value = e[key];
      if (value != null && value !== was[key]) {
        out.push({
          kind: "split",
          summary: `${e.name} · split ${split} · ${clock(value)}`,
          riders: [e.name],
          fields: { ...common, "Split": split, "Split time": clock(value), "Split time (s)": value, "Lap": e.laps + 1, "Position": e.position },
        });
      }
    }
    if (e.laps > was.laps) {
      const best = e.last_lap_seconds != null && e.last_lap_seconds === e.best_lap_seconds;
      out.push({
        kind: "lap",
        summary: `${e.name} · lap ${e.laps} · ${clock(e.last_lap_seconds)}${best ? " · best" : ""}`,
        riders: [e.name],
        fields: {
          ...common,
          "Lap": e.laps,
          "Lap time": clock(e.last_lap_seconds),
          "Lap time (s)": e.last_lap_seconds,
          "Best lap": clock(e.best_lap_seconds),
          "Personal best": best,
          "Split 1": clock(e.last_split1_seconds),
          "Split 2": clock(e.last_split2_seconds),
          "Position": e.position,
          "Gap to leader": secs(e.gap_to_leader_seconds),
          "Interval": secs(e.interval_seconds),
        },
      });
    }
    const penalty = e.penalty_seconds ?? 0;
    const hadPenalty = was.penalty_seconds ?? 0;
    if (penalty > hadPenalty) {
      out.push({
        kind: "penalty",
        summary: `${e.name} · +${(penalty - hadPenalty).toFixed(0)} s penalty (${penalty.toFixed(0)} s total)`,
        riders: [e.name],
        fields: { ...common, "Added (s)": penalty - hadPenalty, "Total penalty (s)": penalty, "Position": e.position },
      });
    }
    if (e.disqualified && !was.disqualified) {
      out.push({ kind: "penalty", summary: `${e.name} · disqualified`, riders: [e.name], fields: { ...common, "Disqualified": true } });
    }
  }
  return out;
}

/** Named joins and leaves between two admin `/v1/riders` answers. */
export function diffRiders(prev: Rider[] | null, next: Rider[]): ParsedEvent[] {
  if (!prev) return [];
  const before = new Map(prev.map((r) => [r.connection_id, r]));
  const after = new Map(next.map((r) => [r.connection_id, r]));
  const out: ParsedEvent[] = [];
  for (const r of next) {
    if (!before.has(r.connection_id)) {
      out.push({
        kind: "join",
        summary: `${r.name} joined${r.bike ? ` · ${r.bike}` : ""}`,
        riders: [r.name],
        fields: { "Rider": r.name, "Bike": r.bike, "Connection": r.connection_id, "State": r.state, "Ping (ms)": r.ping_ms },
      });
    }
  }
  for (const r of prev) {
    if (!after.has(r.connection_id)) {
      out.push({
        kind: "leave",
        summary: `${r.name} left · ${r.laps} laps`,
        riders: [r.name],
        fields: { "Rider": r.name, "Bike": r.bike, "Connection": r.connection_id, "Laps": r.laps, "Best lap": clock(r.best_lap_seconds), "Connected (s)": r.connected_seconds },
      });
    }
  }
  return out;
}

/** Fill in the rider name for events that only carry a race number, from the last `/timing`. */
export function withNames(event: ParsedEvent, names: Map<number, string>): ParsedEvent {
  if (event.riders.length > 0) return event;
  const race = event.fields["Race number"];
  const name = typeof race === "number" ? names.get(race) : undefined;
  if (!name) return event;
  return { ...event, riders: [name], summary: event.summary.replace(`#${race}`, `#${race} ${name}`), fields: { "Rider": name, ...event.fields } };
}

/** New events go first; the oldest fall off past `cap`. */
export function prepend(buffer: ServerEvent[], fresh: ServerEvent[], cap = MAX_EVENTS): ServerEvent[] {
  if (fresh.length === 0) return buffer;
  const newestFirst = [...fresh].reverse();
  return [...newestFirst, ...buffer].slice(0, cap);
}

/** The events to show: one of `kinds`, and `query` (case-insensitive) in a rider name or the summary. */
export function filterEvents(events: ServerEvent[], kinds: ReadonlySet<EventKind>, query: string): ServerEvent[] {
  const q = query.trim().toLowerCase();
  return events.filter(
    (e) => kinds.has(e.kind) && (!q || e.riders.some((r) => r.toLowerCase().includes(q)) || e.summary.toLowerCase().includes(q)),
  );
}

/** How many of each kind, for the chip counts. */
export function countKinds(events: ServerEvent[]): Record<EventKind, number> {
  const counts = Object.fromEntries(EVENT_KINDS.map(({ kind }) => [kind, 0])) as Record<EventKind, number>;
  for (const e of events) counts[e.kind]++;
  return counts;
}
