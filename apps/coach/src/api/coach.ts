import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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

/** Where a reference lap came from: one of your own, one you imported from another rider, or
 *  your ideal lap, which is a time per section and was never ridden whole. */
export type RefKind = "own" | "imported" | "ideal";

/** A lap somewhere on disk — or the ideal lap, which is nowhere: no file, no lap number. */
export interface LapRef {
  path: string;
  lap: number;
  timeMs: number;
  started: string;
  bikeName: string;
  /** The bike it was ridden on: a 250 and a 450 don't take a corner the same way. */
  bikeId: string;
  /** Whose lap it is, as the recorder saved it. */
  rider: string;
  kind: RefKind;
}

export interface SectionBest {
  /** The section's stable id: what per-corner progress over time keys on. */
  id: string;
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
  /** Stable across sessions once the track's roster has named it: `t5`, `j2`, `w1`. */
  id: string;
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
  /** There is a reference lap to draw: its traces are in `channels`, its line in `paths`.
   *  False against the ideal lap, which nobody ever rode whole. */
  traced: boolean;
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
  /** How many of your own laps the ideal lap was stitched from, when that's the reference. */
  idealFrom: number | null;
  /** Other riders in the session worth comparing with. */
  rivals: Rival[];
  /** No lap of yours on this track is faster than this one, so a reference picked for you is a
   *  slower lap and the review has nothing to hold it against. */
  bestHere: boolean;
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

/** Which lap of the session a line belongs to. Every stint starts counting at lap 1 again, so
 *  it takes both the number and the stint to name a lap. */
export interface LapId {
  lap: number;
  stint: number;
}

/** How the session's lines and the track changed; see `lines.rs`. Every stint of the session,
 *  not just the recording being reviewed. */
export interface Lines {
  /** Every whole lap's line, every metre, with the bike's height. */
  laps: (LapId & { time: number; path: [number, number][]; heights: number[] })[];
  sections: Pick<SectionReview, "kind" | "id" | "name" | "start" | "end" | "core" | "dir">[];
  /** Per section, one row per lap: metres right of the fast line, and the section time. */
  offsets: (LapId & { offset: number; time: number })[][];
  notes: LineNote[];
  /** Nobody else was on track, so where the track will rut can't be read off anyone's lines. */
  alone: boolean;
  /** Enough whole laps in the session to tell one line from another. */
  enoughLaps: boolean;
  /** Which stint of the session the lap being reviewed was ridden in. */
  stint: number;
}

/** The track's own terrain for a session, when it is installed, readable and lines up. */
export interface Ground {
  path: string;
  prefix: string | null;
  name: string;
  /** How high the bike rides above this terrain, metres. */
  lift: number;
  /** The laps didn't sit steadily above it, so `lift` is a best guess and the lines may float
   *  or sink a little. The track itself is drawn either way. */
  roughFit: boolean;
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
/** One sample of a section: the bike in the pose the rider had it in. */
export interface ReplayFrame {
  /** Seconds since the section's first frame. */
  t: number;
  /** Metres along the centreline since the section's first frame. */
  dist: number;
  /**
   * Where the bike was, world metres, as the recording gives it — the same space as
   * `Review.paths`, which is these fields on the review's metre grid. So a bike put here sits
   * on its own line once it gets what `Track3D` gives the line: the terrain grid's origin off
   * x and z, and `Ground.lift` off the height.
   */
  x: number;
  y: number;
  z: number;
  /** Which way the bike pointed, degrees: zero down +z and climbing towards +x, so it is a
   *  rotation about +y as it stands. */
  yaw: number;
  /** Ground speed, m/s. */
  v: number;
  /** Bar angle in degrees, positive to the rider's left, as the recording gives it. */
  steer: number;
  /** Share of the travel in use, 0 extended to 1 bottomed, front then rear. */
  used: [number, number];
  roll: number;
  pitch: number;
  /** Degrees the wheels have turned since the section started. Only ever climbs. */
  spin: number;
  throttle: number;
  front: number;
  rear: number;
  gear: number;
  air: boolean;
  /**
   * Where the rider was asking to put their body, left/right then forward/back.
   *
   * `null` is an axis the recorder never read, which is not the same fact as a centred rider,
   * so it stays apart all the way to the bones that would have moved for it.
   */
  lean: [number | null, number | null];
  /** 0 not read, 1 standing, 2 sitting. */
  stance: number;
}

export interface Replay {
  sectionId: string;
  name: string;
  kind: SectionKind;
  /** Section length, metres. */
  length: number;
  frames: ReplayFrame[];
  /** The best lap through the same section. Empty when this lap is the best one. */
  best: ReplayFrame[];
  bestLap: number | null;
  /** Whether the recorder read each lean axis, and the stance, at all. */
  leanKnown: [boolean, boolean];
  stanceKnown: boolean;
}

/** One section of one lap, frame by frame, with the same section of the best lap beside it. */
export const coachReplay = (path: string, lap: number, sectionId: string) =>
  invoke<Replay>("coach_replay", { path, lap, sectionId });
export const coachLines = (path: string) => invoke<Lines | null>("coach_lines", { path });
export const coachSurface = (path: string) => invoke<Surface | null>("coach_surface", { path });
export const coachSessions = () => invoke<SessionSummary[]>("coach_sessions");
/** Fires when the recorder writes a session or adds to the one being ridden now. */
export const onSessionsChanged = (run: () => void): Promise<UnlistenFn> =>
  listen("coach-sessions-changed", () => run());

/** The whole session the recording belongs to: every stint of that event, with all its laps. */
export const coachSession = (path: string) => invoke<SessionDetail>("coach_session", { path });
/** What a lap is held against. Nothing given means the fastest lap on the track. */
export interface RefArgs {
  /** One particular lap: the recording it's in, and its number. An imported lap is one of these. */
  refPath?: string;
  refLap?: number;
  /** Review it on its own, with nothing to compare with. */
  solo?: boolean;
  /** Your own best sections on this track, added up: the ideal lap. */
  ideal?: boolean;
}

/** Reviews a lap against the reference asked for; the backend reviews it on its own when
 *  there's nothing to compare with. */
export const coachReview = (path: string, lap: number, ref: RefArgs = {}) =>
  invoke<ReviewOut>("coach_review", {
    path,
    lap,
    refPath: ref.refPath ?? null,
    refLap: ref.refLap ?? null,
    solo: ref.solo ?? false,
    ideal: ref.ideal ?? false,
  });
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
  /** Another tip wants this setting the other way, so the coach leaves it to the rider. */
  conflict: boolean;
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

/** A setup the coach wrote: its name, the settings it really changed, and whether the game
 *  will load it. */
export interface SavedSetup {
  name: string;
  changed: SetupField[];
  /** The game's own record names it, so practice on this track loads it. */
  selected: boolean;
  /** It doesn't, because MX Bikes is open: the file is the game's, and it rewrites it on exit.
   *  Neither set means the record was written and didn't come back naming the setup. */
  gameOpen: boolean;
}

/** Saves those changes as a new setup for this track — named off the rider's own, or after the
 *  track when they rode the game's default — and points the game at it. */
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
  /** That section's stable id, which the cue rotation keys on. */
  sectionId: string;
}

export interface CuesOut {
  file: string;
  cues: CueOut[];
  /** The lap the in-game HUD's gap and ghost run against. It's the reference you picked
   *  wherever somebody rode that lap; the ideal lap nobody did, so there the HUD races your
   *  fastest lap on the track instead. */
  ghost: LapRef;
}

/** Picks this lap's live cues for a rider's level and how much coaching they want, and writes
 *  them — and the HUD sheet beside them — where the recorder reads them. The cues come from the
 *  same reference the review is against.
 *
 *  The calls move on each time: what the last sheets said is remembered, so a cue the rider
 *  has been hearing for a few laps rests and whatever is costing time now takes its place.
 *  `latest` picks from the newest lap on this track and bike rather than the one on screen,
 *  which is what to ask for while the rider is still out. The reference still applies: it is
 *  the lap they chose to be held against, not the lap being reviewed. */
export const coachWriteCues = (
  path: string,
  lap: number,
  level: CueLevel,
  amount: CueAmount,
  ref: RefArgs = {},
  latest?: boolean,
) =>
  invoke<CuesOut>("coach_write_cues", {
    path,
    lap,
    level,
    amount,
    refPath: ref.refPath ?? null,
    refLap: ref.refLap ?? null,
    ideal: ref.ideal ?? false,
    latest: latest ?? false,
  });

/** A recording that couldn't be imported, and why. */
export interface Skipped {
  file: string;
  why: string;
}

export interface Imported {
  /** How many recordings went in. */
  added: number;
  skipped: Skipped[];
}

/** Laps imported from other riders. They're kept apart from your own sessions, so they never
 *  count towards your bests. */
export const coachImports = () => invoke<SessionSummary[]>("coach_imports");
/** Copies recordings in: the files picked, or every recording in a folder picked. */
export const coachImportLaps = (paths: string[]) => invoke<Imported>("coach_import_laps", { paths });
export const coachRemoveImport = (path: string) => invoke<void>("coach_remove_import", { path });

/** Downloads the recorder, or copies it from `from`. Resolves to where it went. */
export const installRecorder = (from?: string) =>
  invoke<string>("coach_install_plugin", { from: from ?? null });
export const removeRecorder = () => invoke<void>("coach_uninstall_plugin");
/** Put the newest recorder in place if what's there is older, or missing. Returns the version
 *  it installed, or null when nothing needed doing. */
export const refreshRecorder = () => invoke<string | null>("coach_refresh_plugin");
/** Point Coach at the MX Bikes folder itself, rather than sending the rider to MXB App. */
export const setGameDir = (dir: string) => invoke<CoachStatus>("coach_set_game_dir", { dir });

/** One thing the recorder can draw over the game. Labels come from `hud.rs`. */
export interface HudPart {
  key: string;
  label: string;
  on: boolean;
  /** The recorder this part needs, e.g. "0.24" for the newest ones. */
  needs: string;
}

/** `hud.ini` as the recorder will read it: missing keys are on, except the map when MXBMRP3
 *  is installed — `parts` already says what the recorder will really do. */
export interface Hud {
  enabled: boolean;
  parts: HudPart[];
  file: string;
  /** MXBMRP3 sits beside the recorder and draws a track map of its own. */
  mxbmrp3: boolean;
  /** Where the live cue sits: the box's centre across, its top edge down, as fractions. */
  cuePos: [number, number];
  /** The recorder that last ran is older than the newest settings need. */
  preExtras: boolean;
}

export const coachHud = () => invoke<Hud>("coach_hud");
/** Turn one part on or off, or the whole HUD with `enabled`. Always written out, so the
 *  recorder does what the switch says even when another plugin would decide for it. */
export const coachSetHud = (key: string, on: boolean) => invoke<Hud>("coach_set_hud", { key, on });
/** Move the live cue: `x` is the box's centre across the screen, `y` its top edge down it.
 *  FrostMod 0.24 reads them; older recorders leave the cue where it was. */
export const coachSetCuePos = (x: number, y: number) => invoke<Hud>("coach_set_cue_pos", { x, y });

/** Whether the recorder speaks its cues (`cues/voice.ini`), how loud, 0–100, and in which voice. */
export interface Voice {
  enabled: boolean;
  volume: number;
  voice: CueVoice;
  /** The recorder that last ran is older than the voice choice needs. */
  preExtras: boolean;
}

/** The voices the recorder has clips for. */
export type CueVoice = "female" | "male";

export const coachVoice = () => invoke<Voice>("coach_voice");
export const coachSetVoice = (enabled: boolean, volume: number, voice: CueVoice) =>
  invoke<Voice>("coach_set_voice", { enabled, volume: Math.round(volume), voice });
