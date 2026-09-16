import { invoke } from "@tauri-apps/api/core";

/** Mirrors `coach.rs` and `analysis.rs`. */

export interface CoachStatus {
  gameDir: string;
  pluginPath: string | null;
  pluginInstalled: boolean;
  sessionDirs: string[];
  /** The recorder's own version, as it wrote it the last time the game ran it. */
  recorderVersion: string | null;
  /** The recorder that ran is older than the HUD and the spoken cues need. */
  recorderOutdated: boolean;
}

export interface LapSummary {
  num: number;
  /** The recording this lap is in: a session is every stint of one event. */
  path: string;
  /** Which stint it was ridden in, from 0. Every stint counts its laps from the start. */
  stint: number;
  timeMs: number;
  invalid: boolean;
  /** Started and finished at the line: it can be compared. */
  whole: boolean;
  /** Why it can't be compared: `out lap`, `unfinished`, `untimed`, `gap in the recording`. */
  issue: string | null;
  crashed: boolean;
  /** How long it took by the recording, for a lap the game left untimed. */
  riddenMs: number;
}

/** One stint on track: one recording. A session is every stint of one event. */
export interface Stint {
  path: string;
  started: string;
  /** The setup it was ridden on, as the game names it. */
  setup: string;
}

export interface SessionSummary {
  /** The first stint's file. Every stint is in `stints`. */
  path: string;
  /** `yyyymmdd-hhmmss-mmm`, local time the first stint started. */
  started: string;
  rider: string;
  trackId: string;
  trackName: string;
  bikeId: string;
  bikeName: string;
  category: string;
  /** 1 = testing, 2 = race, 4 = straight rhythm. */
  eventType: number;
  trackLength: number;
  limiter: number;
  complete: boolean;
  laps: LapSummary[];
  bestMs: number | null;
  /** The setup the last stint was ridden on, as the game names it. */
  setup: string;
  /** Every stint this session was ridden in, oldest first. */
  stints: Stint[];
}

export interface LapRef {
  path: string;
  lap: number;
  timeMs: number;
  started: string;
  bikeName: string;
}

export interface SectionBest {
  name: string;
  best: number;
  lap: number;
  spread: number;
}

export interface Ideal {
  time: number;
  sections: SectionBest[];
  leastConsistent: number | null;
}

export interface SessionDetail {
  summary: SessionSummary;
  reference: LapRef | null;
  ideal: Ideal | null;
}

export type SectionKind = "straight" | "corner" | "jump" | "rhythm" | "whoops";

export interface Finding {
  skill: string;
  title: string;
  detail: string;
  /** Metres into the lap. */
  at: number;
}

export interface SectionReview {
  kind: SectionKind;
  name: string;
  start: number;
  end: number;
  core: [number, number];
  dir: number;
  lapTime: number;
  refTime: number;
  /** Seconds lost to the reference; negative is a gain. */
  lost: number;
  findings: Finding[];
  /** The ground here, from the rear wheel. */
  soil: { kind: Soil; share: number; sand: number } | null;
}

export type Soil = "hard" | "hardpack" | "intermediate" | "soft" | "sand" | "grass" | "rocky" | "mud";

export interface Channel {
  lap: number[];
  reference: number[];
}

export interface Channels {
  /** Metres between points. */
  step: number;
  delta: number[];
  speed: Channel;
  throttle: Channel;
  brake: Channel;
  lean: Channel;
  gear: Channel;
  height: Channel;
  /** Share of the travel in use, percent; all zero when unknown. */
  fork: Channel;
  shock: Channel;
}

export interface Review {
  lapTime: number;
  refTime: number;
  sections: SectionReview[];
  focus: number[];
  /** The lap in a few lines: where the time went, by theme. */
  overall: Theme[];
  /** Bike setup advice for the whole lap: suspension, gearing, shifting, chassis. */
  setup: Finding[];
  /** Reviewed on its own, with no faster lap to compare with. */
  solo: boolean;
  channels: Channels;
  /** Both laps' world x/z every `step` metres, with the bike's height at each point. */
  paths: { step: number; lap: [number, number][]; reference: [number, number][]; lapY: number[]; referenceY: number[] };
}

/** One kind of mistake across the lap, with the time it cost. */
export interface Theme {
  name: string;
  /** Seconds; 0 on a lap reviewed on its own. */
  lost: number;
  sections: string[];
  /** The headline tip where it cost most. */
  tip: string;
}

export interface ReviewOut {
  trackId: string;
  trackName: string;
  lap: LapRef;
  reference: LapRef;
  review: Review;
  /** Other riders in the session worth comparing with. */
  rivals: Rival[];
}

export interface Rival {
  num: number;
  name: string;
  bike: string;
  /** Just faster than you, the fastest, or ahead of you on track in a race. */
  why: "closest" | "fastest" | "ahead";
  lap: number;
  timeMs: number;
  /** Where they're quicker than this lap, most first. */
  gains: { section: string; gain: number }[];
}

/** The ground under a session's laps, built from the laps. Heights row-major, row along z. */
export interface Surface {
  x0: number;
  z0: number;
  cell: number;
  width: number;
  height: number;
  heights: (number | null)[];
}

export interface LineNote {
  section: number;
  name: string;
  /** `line` for a line that pays, `cut` for ground cutting up. */
  /** A line that pays, ground cutting up, or where the other riders will wear it. */
  kind: "line" | "cut" | "wear";
  title: string;
  detail: string;
}

/** How the session's lines and the track changed; see `lines.rs`. */
export interface Lines {
  /** Every whole lap's line, every metre, with the bike's height. */
  laps: { lap: number; time: number; path: [number, number][]; heights: number[] }[];
  sections: Pick<SectionReview, "kind" | "name" | "start" | "end" | "core" | "dir">[];
  /** Per section, one row per lap: metres right of the fast line, and the section time. */
  offsets: { lap: number; offset: number; time: number }[][];
  notes: LineNote[];
}

/** The track's own terrain for a session, when it is installed, readable and lines up. */
export interface Ground {
  path: string;
  prefix: string | null;
  name: string;
  /** How high the bike rides above this terrain, metres. */
  lift: number;
}

export const coachStatus = () => invoke<CoachStatus>("coach_status");
/** Opens a folder in the file manager, making it first if the recorder hasn't yet. */
export const openFolder = (path: string) => invoke<void>("open_folder", { path });
/** The track's own terrain, or why the ground built from the laps is drawn instead. */
export interface GroundAnswer {
  ground: Ground | null;
  why: string | null;
}
export const coachGround = (path: string) => invoke<GroundAnswer>("coach_ground", { path });
export const coachLines = (path: string) => invoke<Lines | null>("coach_lines", { path });
export const coachSurface = (path: string) => invoke<Surface | null>("coach_surface", { path });
export const coachSessions = () => invoke<SessionSummary[]>("coach_sessions");
/** The whole session the recording belongs to: every stint of that event, with all its laps. */
export const coachSession = (path: string) => invoke<SessionDetail>("coach_session", { path });
/** `solo` reviews the lap on its own; so does the backend when there's nothing to compare with. */
export const coachReview = (path: string, lap: number, refPath?: string, refLap?: number, solo?: boolean) =>
  invoke<ReviewOut>("coach_review", { path, lap, refPath: refPath ?? null, refLap: refLap ?? null, solo: solo ?? false });
export type SetupField =
  | "forkOffset"
  | "swingarmLength"
  | "forkSpring"
  | "forkCompression"
  | "forkRebound"
  | "forkPreload"
  | "forkHeight"
  | "forkOil"
  | "shockSpring"
  | "shockLowCompression"
  | "shockHighCompression"
  | "shockRebound"
  | "shockPreload"
  | "rodLength"
  | "frontSprocket"
  | "rearSprocket"
  | "frontTyre"
  | "rearTyre"
  | "frontPressure"
  | "rearPressure";

/** One change behind a setup tip. Positions are in the bike's own list for the setting. */
export interface SetupChange {
  field: SetupField;
  /** Steps firmer, more oil or more teeth; negative is the other way. */
  steps: number;
  why: string;
  from: number | null;
  to: number | null;
  /** The values where the bike's file says, like "5.5 N/mm" or "13T". */
  fromValue: string | null;
  toValue: string | null;
  /** The coach can make this change in a copy of the setup. */
  writes: boolean;
}

export interface SetupFix {
  skill: string;
  changes: SetupChange[];
}

export interface SetupPlan {
  /** The most of each end's travel this session used, as a share. */
  travelUsed: [number, number] | null;
  /** The setup the rider had on. */
  name: string;
  file: string | null;
  /** The name a saved copy gets: the next free "(coach)", "(coach 2)" … beside it. */
  saveAs: string | null;
  /** Why the coach can't make the changes itself, when it can't. */
  why: string | null;
  fixes: SetupFix[];
  /** Sag measured in the session: standing still (what setup guides mean) or riding. */
  sag: { still: boolean; metres: [number, number]; share: [number, number] } | null;
}

/** The changes behind a lap's setup tips, against the setup the rider had on. */
export const coachSetupPlan = (path: string, skills: string[]) =>
  invoke<SetupPlan>("coach_setup_plan", { path, skills });

/** A setup the coach wrote, and whether the game will load it. */
export interface SavedSetup {
  name: string;
  /** The game is pointed at it for practice on this track. */
  selected: boolean;
  /** It isn't, because MX Bikes is open: the file is the game's, and it rewrites it on exit. */
  gameOpen: boolean;
}

/** Saves those changes as a new setup beside the rider's own, and selects it in the game. */
export const coachSaveSetup = (path: string, skills: string[]) =>
  invoke<SavedSetup>("coach_save_setup", { path, skills });
/** Points the game at a setup the coach already saved. Only works with MX Bikes closed. */
export const coachSelectSetup = (path: string, name: string) =>
  invoke<SavedSetup>("coach_select_setup", { path, name });
export type CueLevel = "new" | "intermediate" | "subPro" | "pro";
export type CueAmount = "few" | "normal" | "lots";

/** One live cue: a short call the recorder shows a moment before its spot. */
export interface CueOut {
  /** Metres into the lap. */
  at: number;
  kind: number;
  priority: number;
  text: string;
  section: string;
}

export interface CuesOut {
  file: string;
  cues: CueOut[];
}

/** Picks this lap's live cues for a rider's level and how much coaching they want, and writes
 *  them where the recorder reads them.
 *
 *  The calls move on each time: what the last sheets said is remembered, so a cue the rider
 *  has been hearing for a few laps rests and whatever is costing time now takes its place.
 *  `latest` picks from the newest lap on this track and bike rather than the one on screen,
 *  which is what to ask for while the rider is still out. */
export const coachWriteCues = (path: string, lap: number, level: CueLevel, amount: CueAmount, latest?: boolean) =>
  invoke<CuesOut>("coach_write_cues", { path, lap, level, amount, latest: latest ?? false });
/** Downloads the recorder, or copies it from `from`. Resolves to where it went. */
export const installRecorder = (from?: string) =>
  invoke<string>("coach_install_plugin", { from: from ?? null });
export const removeRecorder = () => invoke<void>("coach_uninstall_plugin");

/** One thing the recorder can draw over the game. Labels come from `hud.rs`. */
export interface HudPart {
  key: string;
  label: string;
  on: boolean;
}

/** `hud.ini` as the recorder will read it: missing keys are on, except the map when MXBMRP3
 *  is installed — `parts` already says what the recorder will really do. */
export interface Hud {
  enabled: boolean;
  parts: HudPart[];
  file: string;
  /** MXBMRP3 sits beside the recorder and draws a track map of its own. */
  mxbmrp3: boolean;
  /** Where the live cue sits: fractions of the width and height, (0,0) top left. */
  cuePos: [number, number];
}

export const coachHud = () => invoke<Hud>("coach_hud");
/** Turn one part on or off, or the whole HUD with `enabled`. Always written out, so the
 *  recorder does what the switch says even when another plugin would decide for it. */
export const coachSetHud = (key: string, on: boolean) => invoke<Hud>("coach_set_hud", { key, on });
/** Move the live cue. FrostMod 0.24 reads it; older recorders leave the cue where it was. */
export const coachSetCuePos = (x: number, y: number) => invoke<Hud>("coach_set_cue_pos", { x, y });

/** Whether the recorder speaks its cues (`cues/voice.ini`), and how loud, 0–100. */
export interface Voice {
  enabled: boolean;
  volume: number;
}

export const coachVoice = () => invoke<Voice>("coach_voice");
export const coachSetVoice = (enabled: boolean, volume: number) =>
  invoke<Voice>("coach_set_voice", { enabled, volume: Math.round(volume) });
