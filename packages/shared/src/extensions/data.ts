/**
 * The data an extension can read, in the shape it gets it.
 *
 * Every type here is a deliberate subset of what the app holds. Paths, account ids, tokens,
 * friend codes, server agent URLs and anything else that identifies this PC or its owner
 * beyond what the permission names is left out *here*, by building a new object field by
 * field, so a field added to an app type later never reaches an extension by accident.
 */

import type { InstalledMod } from "../types";
import type { MasterServer } from "../api/mods";

export interface ExtMod {
  /** File or folder name, e.g. `Mosctesting.pkz`. */
  name: string;
  /** The mod type's id: `tracks`, `bikes`, `rider`… */
  type: string;
  /** Sub-folder under that type, `""` at the top. */
  folder: string;
  size: number;
}

export interface ExtServer {
  name: string;
  address: string;
  players: number;
  maxPlayers: number;
  pingMs: number | null;
  passworded: boolean;
  location: string;
  track: string;
  session: string;
  rating: string;
}

export interface ExtFriend {
  name: string;
  online: boolean;
  /** Where they ride, when they share it. */
  server: string | null;
  track: string | null;
  riders: number | null;
  lastSeenAt: number | null;
}

export interface ExtGameState {
  /** `mxb`, or another title the app manages. */
  game: string;
  running: boolean;
}

/** An MXB Coach session: what was ridden and the times. Built by the backend, without paths. */
export interface ExtCoachSession {
  started: string;
  track: string;
  bike: string;
  category: string;
  event: "testing" | "race" | "straight-rhythm" | "other";
  cleanLaps: number[];
  laps: number;
  bestMs: number | null;
}

export interface ExtPreset {
  name: string;
}

/** The friend shape the app's friends API returns; only what's read below. */
export interface FriendLike {
  riderName: string;
  presence: { serverName: string; track: string | null; riders: number | null } | null;
  lastSeenAt: number | null;
}

const str = (v: unknown, max = 200) => (typeof v === "string" ? v.slice(0, max) : "");
const n = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : 0);

export function toExtMod(m: InstalledMod, type: string): ExtMod {
  return { name: str(m.name), type: str(type, 40), folder: str(m.folder), size: n(m.size) };
}

export function toExtServer(s: MasterServer): ExtServer {
  return {
    name: str(s.name),
    address: str(s.address, 64),
    players: n(s.players),
    maxPlayers: n(s.maxPlayers),
    pingMs: typeof s.pingMs === "number" ? s.pingMs : null,
    passworded: s.passworded === true,
    location: str(s.location, 80),
    track: str(s.track),
    session: str(s.session, 80),
    rating: str(s.rating, 4),
  };
}

/** Online means their app reported in within the control plane's two-minute window. */
export function toExtFriend(f: FriendLike, now = Date.now()): ExtFriend {
  const p = f.presence;
  const recent = typeof f.lastSeenAt === "number" && now - f.lastSeenAt < 2 * 60_000;
  return {
    name: str(f.riderName, 60),
    online: p !== null || recent,
    server: p ? str(p.serverName) : null,
    track: p && typeof p.track === "string" ? str(p.track) : null,
    riders: p && typeof p.riders === "number" ? p.riders : null,
    lastSeenAt: typeof f.lastSeenAt === "number" ? f.lastSeenAt : null,
  };
}

/**
 * What an app hands the extension host. Each source is optional: an app that has no friends
 * list (the Studio) simply leaves it out, and the call answers "not available in this app".
 */
export interface ExtensionDataSources {
  app: string;
  appVersion: string;
  mods?: () => Promise<ExtMod[]>;
  servers?: () => Promise<ExtServer[]>;
  friends?: () => Promise<ExtFriend[]>;
  gameState?: () => Promise<ExtGameState>;
  presets?: () => Promise<ExtPreset[]>;
  coach?: () => Promise<ExtCoachSession[]>;
  /** Open Frost's Studio on one of its screens. */
  studio?: (view: StudioScreen) => Promise<void>;
}

/** The Studio screens an extension may open. Fixed here: the name is passed to a process. */
export const STUDIO_SCREENS = ["bike", "rider", "designer", "track", "pose", "replay", "diagnose"] as const;
export type StudioScreen = (typeof STUDIO_SCREENS)[number];

let sources: ExtensionDataSources = { app: "unknown", appVersion: "0.0.0" };

/** Called once by the app, before any extension view mounts. */
export function provideExtensionData(s: ExtensionDataSources): void {
  sources = s;
}

export function extensionData(): ExtensionDataSources {
  return sources;
}
