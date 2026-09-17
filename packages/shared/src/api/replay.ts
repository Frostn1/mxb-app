import { invoke } from "@tauri-apps/api/core";

/**
 * The Replay Mod's half that lives outside the game: the camera paths on disk, and the
 * recorder that keeps what the mod flies.
 *
 * Every command here is registered by **Frost's Studio** and by nothing else. Calling one
 * from the mod manager rejects with "command not found", which is the honest answer: the
 * recorder runs a child encoder against the game window and belongs in the app the replay
 * panels are in, not in the one that installs mods.
 *
 * See `crates/core/src/replay.rs` for the file formats these describe, and
 * `docs/replay/RECORDING.md` for what the in-game mod has to write for any of it to happen
 * on its own.
 */

/** What the mod says it is doing. `playing` is what starts a recording. */
export type TakeState = "playing" | "idle";

/** The take signal, as the mod writes it to `FrostReplay/take.json`. */
export interface Take {
  v: number;
  state: TakeState;
  /** Which of the nine slots is playing, when it came from one. */
  slot: number | null;
  track: string;
  rider: string;
  /** Unix milliseconds. */
  startedAt: number;
  /** Unix milliseconds of the mod's last heartbeat. */
  beatAt: number;
  /** How long the path runs, when the mod knows. `0` means it doesn't. */
  durationMs: number;
}

/** One saved camera path. Nothing reads inside a `.fcam` — that format is the mod's. */
export interface Slot {
  name: string;
  path: string;
  /** The slot number for one of the mod's own nine; `null` for a path someone shared. */
  slot: number | null;
  bytes: number;
  /** Unix milliseconds. */
  modified: number;
}

/** What started the recording that is running. */
export type RecordSource = "auto" | "manual";

/** Everything the Replay screen draws. Also arrives on the `replay-status` event. */
export interface ReplayStatus {
  recording: boolean;
  /** The file being written, or the one just finished. */
  file: string | null;
  seconds: number;
  source: RecordSource | null;
  /** What the mod is saying about itself, when it is saying anything. */
  take: Take | null;
  /** The mod has written a take file here at least once. Tells "install the mod" apart
   *  from "press play in the game", which are different sentences. */
  modSeen: boolean;
  gameRunning: boolean;
  /** The ffmpeg that would be used, if there is one on this machine. */
  ffmpeg: string | null;
  /** Whether that ffmpeg can capture through Desktop Duplication — the capture that keeps
   *  working while the game owns the screen. `null` until `replayCheck` has asked. */
  ddagrab: boolean | null;
  /** The game has taken the screen exclusively. Only a problem without `ddagrab`. */
  exclusiveFullscreen: boolean;
  error: string | null;
}

/** How much the encoder is asked to keep. */
export type Quality = "high" | "balanced" | "small";

/** Which encoder does the work. `auto` takes the GPU's when there is one. */
export type Encoder = "auto" | "x264" | "nvenc" | "amf" | "qsv";

/** What the recorder is set to do. Stored in the shared app config. */
export interface RecordingSettings {
  /** Record on its own when the mod says a take has started. On by default. */
  auto: boolean;
  /** Where recordings land. Blank means `Videos\Frost Replays`. */
  dir: string;
  fps: number;
  quality: Quality;
  encoder: Encoder;
  /** ffmpeg to run. Blank means the fetched one, then whatever is on `PATH`. */
  ffmpegPath: string;
  /** A DirectShow audio device, exactly as ffmpeg names it. Blank records silence. */
  audioDevice: string;
  /** Start/stop combo, Tauri accelerator syntax. */
  hotkey: string;
  /** Stop after this many minutes whatever the mod says. */
  maxMinutes: number;
}

/** One finished recording on disk. */
export interface Recorded {
  name: string;
  path: string;
  bytes: number;
  /** Unix milliseconds. */
  modified: number;
}

/** The event carrying [`ReplayStatus`] whenever any of it moves. */
export const REPLAY_EVENT = "replay-status";

/** What the Replay screen shows. Cheap — the watcher calls this twice a second. */
export function replayStatus(): Promise<ReplayStatus> {
  return invoke<ReplayStatus>("replay_status");
}

/** Ask the resolved ffmpeg what it can do, then answer with the status that follows.
 *  Costs two process launches, so it is called when the screen opens and after a save —
 *  never on a timer. */
export function replayCheck(): Promise<ReplayStatus> {
  return invoke<ReplayStatus>("replay_check");
}

/** The camera paths saved on this machine, newest first. */
export function replaySlots(): Promise<Slot[]> {
  return invoke<Slot[]>("replay_slots");
}

export function replaySettings(): Promise<RecordingSettings> {
  return invoke<RecordingSettings>("replay_settings");
}

/** Save them. Resolves with what was actually stored — some of it is clamped. */
export function saveReplaySettings(settings: RecordingSettings): Promise<RecordingSettings> {
  return invoke<RecordingSettings>("replay_save_settings", { settings });
}

/** Record now, without waiting for the mod. Resolves with the file being written. */
export function replayRecord(): Promise<string> {
  return invoke<string>("replay_record");
}

/** Stop whatever is recording. Resolves with the finished file, or null if nothing was. */
export function replayStop(): Promise<string | null> {
  return invoke<string | null>("replay_stop");
}

export function replayRecordings(): Promise<Recorded[]> {
  return invoke<Recorded[]>("replay_recordings");
}

/** Delete one recording. Refuses anything that isn't an mp4 in the recordings folder. */
export function deleteRecording(path: string): Promise<void> {
  return invoke<void>("replay_delete", { path });
}

/** Where recordings are going, whether or not anything is there yet. */
export function replayOutDir(): Promise<string> {
  return invoke<string>("replay_out_dir");
}

/** Fetch an ffmpeg into the Studio's own folder. Resolves with where it landed. */
export function fetchFfmpeg(): Promise<string> {
  return invoke<string>("replay_fetch_ffmpeg");
}
