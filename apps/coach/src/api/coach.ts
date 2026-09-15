import { invoke } from "@tauri-apps/api/core";

/** Mirrors `coach.rs` and `analysis.rs`. */

export interface CoachStatus {
  gameDir: string;
  pluginPath: string | null;
  pluginInstalled: boolean;
  sessionDirs: string[];
}

export interface LapSummary {
  num: number;
  timeMs: number;
  invalid: boolean;
  /** Started and finished at the line, no crash: it can be compared. */
  whole: boolean;
}

export interface SessionSummary {
  path: string;
  /** `yyyymmdd-hhmmss-mmm`, local time. */
  started: string;
  rider: string;
  trackId: string;
  trackName: string;
  bikeId: string;
  bikeName: string;
  category: string;
  trackLength: number;
  limiter: number;
  complete: boolean;
  laps: LapSummary[];
  bestMs: number | null;
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
}

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
  /** Suspension advice for the whole lap. */
  setup: Finding[];
  channels: Channels;
  /** World x/z every 2 m. */
  paths: { lap: [number, number][]; reference: [number, number][] };
}

export interface ReviewOut {
  trackName: string;
  lap: LapRef;
  reference: LapRef;
  review: Review;
}

export const coachStatus = () => invoke<CoachStatus>("coach_status");
export const coachSessions = () => invoke<SessionSummary[]>("coach_sessions");
export const coachSession = (path: string) => invoke<SessionDetail>("coach_session", { path });
export const coachReview = (path: string, lap: number, refPath?: string, refLap?: number) =>
  invoke<ReviewOut>("coach_review", { path, lap, refPath: refPath ?? null, refLap: refLap ?? null });
/** Downloads the recorder, or copies it from `from`. Resolves to where it went. */
export const installRecorder = (from?: string) =>
  invoke<string>("coach_install_plugin", { from: from ?? null });
export const removeRecorder = () => invoke<void>("coach_uninstall_plugin");
