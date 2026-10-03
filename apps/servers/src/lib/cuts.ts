/** Track cuts: the admin `/v1/cuts` answer, its geometry helpers and the zones the Settings tab
 *  edits. World coordinates are metres, x to the right and z up (the map draws y = -z). */

/** `[x, z, half_width_m, s_m]`, about every 2 m along the line. */
export type CutPoint = [number, number, number, number];

export interface CutZone {
  track: string;
  name: string;
  /** A track-distance range (`from_m`..`to_m`), an `area` polygon, or both. */
  from_m: number | null;
  to_m: number | null;
  area: [number, number][] | null;
  /** 0 = warning only. */
  seconds: number;
  enable: boolean;
}

export interface CutTrack {
  track: string;
  source: "auto" | "file";
  closed: boolean;
  length_m: number;
  points: CutPoint[];
  zones: CutZone[];
}

export interface CutsInfo {
  enabled: boolean;
  mode: "auto" | "file" | "off";
  current_track: string | null;
  settings: { auto_half_width_m: number; min_excursion_seconds: number; min_gain_m: number } | null;
  penalties: { enable: boolean; cut_time_seconds: number; cut_offences_for_dsq: number; jump_start_seconds: number; holeshot: boolean } | null;
  tracks: CutTrack[];
  /** Every track the server has a package for (older servers: none listed). */
  folder_tracks: { track: string; file: string; secured: boolean }[];
}

/** Why a track has no outline. `unsupported`: a server older than `/v1/cuts/outline`. */
export type OutlineStatus = "ok" | "secured" | "no_trh" | "no_outline" | "unreadable" | "unknown_track" | "unsupported";

/** The admin `/v1/cuts/outline` answer for one track. `track` is set only when `status` is `ok`. */
export interface CutOutline {
  status: OutlineStatus;
  message: string;
  track: CutTrack | null;
}

const OUTLINE_STATUSES: OutlineStatus[] = ["ok", "secured", "no_trh", "no_outline", "unreadable", "unknown_track", "unsupported"];

/** The raw `server_cut_outline` answer for the track called `name`. */
export function parseOutline(raw: unknown, name: string): CutOutline {
  const r = raw as Record<string, unknown> | null;
  if (!r || r.supported === false) {
    return { status: "unsupported", message: "This server is too old to send track outlines. Update it, or type the distances.", track: null };
  }
  const status = OUTLINE_STATUSES.find((s) => s === r.status) ?? "unreadable";
  if (status === "unknown_track") {
    return { status, message: `The server has no track package called "${name}", so it has no outline for it. Type the distances.`, track: null };
  }
  const points = (Array.isArray(r.points) ? r.points : []).filter((p): p is CutPoint => Array.isArray(p) && p.length >= 4 && p.slice(0, 4).every(isNum));
  if (status === "ok" && points.length >= 2) {
    return {
      status,
      message: "",
      track: {
        track: typeof r.track === "string" ? r.track : name,
        source: "auto",
        closed: r.closed === true,
        length_m: isNum(r.length_m) ? r.length_m : (points[points.length - 1]?.[3] ?? 0),
        points,
        zones: [],
      },
    };
  }
  const message = typeof r.message === "string" && r.message ? r.message : "The server could not build an outline for this track. Type the distances.";
  return { status: status === "ok" ? "no_outline" : status, message, track: null };
}

export type CutOutcome = "penalty" | "warning" | "disqualified" | "allowed" | "report_only";

/** One item of `/v1/events` `cuts.recent`. */
export interface RecentCut {
  track: string;
  race: number;
  name: string;
  session_seconds: number;
  route_metres: number;
  outside_metres: number;
  duration_seconds: number;
  from_m?: number | null;
  to_m?: number | null;
  skipped_m?: number | null;
  exit?: [number, number] | null;
  rejoin?: [number, number] | null;
  path?: [number, number][] | null;
  zone?: string | null;
  outcome?: CutOutcome | null;
  penalty_seconds?: number | null;
}

const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);

/** The raw `server_cuts` answer -> typed, or null when the server doesn't have the endpoint. */
export function parseCuts(raw: unknown): CutsInfo | null {
  const r = raw as Record<string, unknown> | null;
  if (!r || r.supported === false || typeof r.enabled !== "boolean") return null;
  const tracks = (Array.isArray(r.tracks) ? r.tracks : []).flatMap((t): CutTrack[] => {
    const o = t as Record<string, unknown>;
    if (typeof o?.track !== "string") return [];
    const points = (Array.isArray(o.points) ? o.points : []).filter(
      (p): p is CutPoint => Array.isArray(p) && p.length >= 4 && p.slice(0, 4).every(isNum),
    );
    return [
      {
        track: o.track,
        source: o.source === "file" ? "file" : "auto",
        closed: o.closed === true,
        length_m: isNum(o.length_m) ? o.length_m : (points[points.length - 1]?.[3] ?? 0),
        points,
        zones: zonesFrom(o.zones, o.track),
      },
    ];
  });
  const mode = r.mode === "auto" || r.mode === "file" ? r.mode : "off";
  return {
    enabled: r.enabled,
    mode,
    current_track: typeof r.current_track === "string" ? r.current_track : null,
    settings: (r.settings as CutsInfo["settings"]) ?? null,
    penalties: (r.penalties as CutsInfo["penalties"]) ?? null,
    tracks,
    folder_tracks: (Array.isArray(r.folder_tracks) ? r.folder_tracks : []).flatMap((t) => {
      const o = t as Record<string, unknown>;
      return typeof o?.track === "string" ? [{ track: o.track, file: typeof o.file === "string" ? o.file : "", secured: o.secured === true }] : [];
    }),
  };
}

/** `/v1/events` -> its `cuts.recent` (empty when absent or unsupported). */
export function parseRecentCuts(raw: unknown): RecentCut[] {
  const recent = (raw as { cuts?: { recent?: unknown } } | null)?.cuts?.recent;
  return Array.isArray(recent) ? (recent.filter((c) => c && typeof c.track === "string") as RecentCut[]) : [];
}

/** Zones out of JSON (the server's answer, or the settings value) with defaults filled in. */
export function zonesFrom(raw: unknown, track = ""): CutZone[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((z): CutZone[] => {
    const o = z as Record<string, unknown>;
    if (!o || typeof o !== "object") return [];
    const area = Array.isArray(o.area)
      ? (o.area.filter((p): p is [number, number] => Array.isArray(p) && isNum(p[0]) && isNum(p[1])).map((p) => [p[0], p[1]]) as [number, number][])
      : null;
    return [
      {
        track: typeof o.track === "string" ? o.track : track,
        name: typeof o.name === "string" ? o.name : "",
        from_m: isNum(o.from_m) ? o.from_m : null,
        to_m: isNum(o.to_m) ? o.to_m : null,
        area: area && area.length > 0 ? area : null,
        seconds: isNum(o.seconds) ? o.seconds : 10,
        enable: o.enable !== false,
      },
    ];
  });
}

/** The nearest spot on the line to a world point: its track distance and how far off it is. */
export function projectToLine(points: CutPoint[], x: number, z: number): { s: number; dist: number } | null {
  if (points.length === 0) return null;
  let best = { s: points[0][3], dist: Math.hypot(x - points[0][0], z - points[0][1]) };
  for (let i = 0; i + 1 < points.length; i++) {
    const [ax, az, , as] = points[i];
    const [bx, bz, , bs] = points[i + 1];
    const dx = bx - ax;
    const dz = bz - az;
    const len2 = dx * dx + dz * dz;
    const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((x - ax) * dx + (z - az) * dz) / len2));
    const dist = Math.hypot(x - (ax + t * dx), z - (az + t * dz));
    if (dist < best.dist) best = { s: as + t * (bs - as), dist };
  }
  return best;
}

/** The world position at a track distance. */
export function pointAt(points: CutPoint[], s: number): [number, number] | null {
  if (points.length === 0) return null;
  if (s <= points[0][3]) return [points[0][0], points[0][1]];
  for (let i = 0; i + 1 < points.length; i++) {
    const a = points[i];
    const b = points[i + 1];
    if (s <= b[3]) {
      const t = b[3] === a[3] ? 0 : (s - a[3]) / (b[3] - a[3]);
      return [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
    }
  }
  const last = points[points.length - 1];
  return [last[0], last[1]];
}

/** The points of the line between two track distances (ends interpolated). */
export function segment(points: CutPoint[], from: number, to: number): [number, number][] {
  const lo = Math.min(from, to);
  const hi = Math.max(from, to);
  const out: [number, number][] = [];
  const start = pointAt(points, lo);
  const end = pointAt(points, hi);
  if (start) out.push(start);
  for (const p of points) if (p[3] > lo && p[3] < hi) out.push([p[0], p[1]]);
  if (end) out.push(end);
  return out;
}

/** Left and right track edges from the half widths (normal to the local direction of travel). */
export function edges(points: CutPoint[]): { left: [number, number][]; right: [number, number][] } {
  const left: [number, number][] = [];
  const right: [number, number][] = [];
  points.forEach((p, i) => {
    const a = points[Math.max(0, i - 1)];
    const b = points[Math.min(points.length - 1, i + 1)];
    const dx = b[0] - a[0];
    const dz = b[1] - a[1];
    const len = Math.hypot(dx, dz) || 1;
    const nx = -dz / len;
    const nz = dx / len;
    left.push([p[0] + nx * p[2], p[1] + nz * p[2]]);
    right.push([p[0] - nx * p[2], p[1] - nz * p[2]]);
  });
  return { left, right };
}

/** SVG `points`/path data with world z up: y = -z. */
export const svgPoints = (pts: [number, number][]) => pts.map(([x, z]) => `${x.toFixed(1)},${(-z).toFixed(1)}`).join(" ");

export function bounds(track: CutTrack, extra: [number, number][] = []) {
  let minX = Infinity;
  let maxX = -Infinity;
  let minZ = Infinity;
  let maxZ = -Infinity;
  const add = (x: number, z: number, pad = 0) => {
    minX = Math.min(minX, x - pad);
    maxX = Math.max(maxX, x + pad);
    minZ = Math.min(minZ, z - pad);
    maxZ = Math.max(maxZ, z + pad);
  };
  for (const p of track.points) add(p[0], p[1], p[2]);
  for (const [x, z] of extra) add(x, z);
  if (!Number.isFinite(minX)) return { minX: -10, maxX: 10, minZ: -10, maxZ: 10 };
  return { minX, maxX, minZ, maxZ };
}

/** A short description of where a zone is. */
export function zoneWhere(z: CutZone): string {
  const parts: string[] = [];
  if (z.from_m != null && z.to_m != null) parts.push(`${z.from_m.toFixed(0)}-${z.to_m.toFixed(0)} m`);
  if (z.area) parts.push(`area of ${z.area.length} points`);
  return parts.join(" + ") || "no place yet";
}

export const OUTCOME_LABEL: Record<CutOutcome, string> = {
  penalty: "Penalty",
  warning: "Warning",
  disqualified: "Disqualified",
  allowed: "Allowed",
  report_only: "Report only",
};
