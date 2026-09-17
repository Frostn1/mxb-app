# Recording a replay

How Frost's Replay Mod and Frost's Studio between them turn a camera path into a video file,
and what each half has to do for that to happen without anybody pressing record.

Two halves, in two repositories:

| Half | Where it lives | What it does |
| --- | --- | --- |
| The mod (`frostreplay.dll`) | the plugin bundle, built outside this repo | flies the camera, and says when it is flying one |
| The recorder | [`apps/studio/src-tauri/src/replayrec.rs`](../../apps/studio/src-tauri/src/replayrec.rs) | watches for that, and encodes the game window while it lasts |

The piece they agree on is in the shared core:
[`crates/core/src/replay.rs`](../../crates/core/src/replay.rs).

## Why the Studio and not MXB App

Cutting a replay is the same errand as painting a bike or building a track: you are making
something. MXB App installs mods. So the Replay Mod's panels open in Frost's Studio, and the
manager keeps the half that is genuinely its own — buying the licence, installing the bundle
and updating it — with an **Open in Studio** button on Settings → Plugins.

**Open in Studio** starts the Studio with `--view <plugin id>`, and the Studio opens that
plugin's first panel — or a screen of its own, when the name is one of those (`--view replay`).

Mechanically, that is one field. `manifest.json` may carry `"host": "studio"` or
`"host": "manager"`, and **a manifest that says nothing means Studio** — which is what makes
the Replay Mod's existing bundle land in the right window without a byte of it changing. Each
app mounts only the plugins whose host it is, so nothing appears twice.

### What a moved panel can still reach

The plugin API is unchanged and still `version: 1` — `react`, `invoke`, `files`,
`installPayload`, `registerPanels`, plus two additions a bundle that knows nothing about them
ignores: `host`, and a `recorder` for a panel that wants a **Record** button of its own.

The one thing that does change underneath a moved panel is `invoke`. Both apps register the
plugin commands, the shared file sandbox and everything in `mxb_core`, but each also has
commands of its own — and a panel calling one of *MXB App's* (its mod browser, its installer,
FrostMod's controls) will now be calling it in a window that does not register it. If a panel
needs one, it moves to the shared core and both apps register it, as the plugin commands
themselves just did.

## The take signal

The mod writes one file, in its own folder inside the game's user folder:

```
Documents\PiBoSo\MX Bikes\FrostReplay\take.json
```

```jsonc
{
  "v": 1,              // format version; a newer one is refused, not half-read
  "state": "playing",  // "playing" while a path is being flown, "idle" otherwise
  "slot": 3,           // which of the nine slots, when it came from one
  "track": "Aztec MX", // for the file name
  "rider": "",         // who the camera is following, if anyone
  "startedAt": 1789670402000,  // unix milliseconds
  "beatAt":    1789670408000,  // unix milliseconds, rewritten as it plays
  "durationMs": 0      // how long the path runs, when the mod knows; 0 when it doesn't
}
```

Only `state` is load-bearing. Everything else is for the file name, and a recording is never
withheld because one of them is missing — a take recorded under a plain timestamp is worth far
more than no take and a tidy schema.

**`beatAt` is what makes this safe.** The mod rewrites it while a path plays. If it stops
moving for ten seconds the Studio treats the take as over, whatever the file still says: a
game that crashed mid-shot cannot write the closing `idle`, and a recorder that waits for one
records until the disk is full. A `take.json` left behind by a previous session is stale by
the same rule, so it cannot start a recording when the Studio opens.

A file, rather than a socket or a Windows event, for the reason FrostMod's command file is a
file: the game is a Windows process that may be running inside a Wine prefix with the Studio
outside it, and a file in the game's own folder is the one channel that works in every
arrangement the app already supports.

### What the mod has to do

1. Write `take.json` with `state: "playing"` when a path starts.
2. Rewrite it — `beatAt` at least — every second or two while it plays.
3. Write `state: "idle"` when it ends.

That is the whole contract. A mod build that does none of it still works: the Studio's
**Record** button and its hotkey are there, and the only thing lost is the *by itself* part.

## What the recorder does

`replayrec.rs` runs a watcher thread for the life of the Studio, polling twice a second. It
does nothing at all until a take file appears — which, for everybody who does not own the
Replay Mod, is forever.

When a take starts it spawns **ffmpeg** against the game's window and writes an `.mp4` into
`Videos\Frost Replays` (or wherever Settings says), named for the track and the slot:

```
Aztec MX - slot 3 - 2026-09-17 18-40-02.mp4
```

It stops when the take does, and also when:

- the heartbeat goes quiet for ten seconds — the game went away;
- the cap in Settings runs out (20 minutes by default) — somebody left a looping path running;
- ffmpeg dies on its own, which is reported rather than left on screen as a recording that
  isn't running.

Stopping is `q` on ffmpeg's stdin, not a kill: ffmpeg has to write the trailer and move the
index, and an `.mp4` without them is a file no player will open — the exact failure the
feature exists to prevent. It gets eight seconds, then it is killed and the file is reported
as possibly short.

A recording started by hand is **not** stopped by the mod going idle. It was started by a
person, so it is stopped by one.

### Capture

Two ways, picked by what the machine's ffmpeg can do:

- **`ddagrab`** (Desktop Duplication) whenever it is available. It reads frames the compositor
  already has, so it keeps working while the game holds the screen exclusively — which is how
  most people play, and where the other one records black.
- **`gdigrab`**, cropped to the game's window rectangle, otherwise. The Replay screen says so
  before the fact when it would produce black: *play in borderless windowed, or fetch the
  Studio's ffmpeg.*

Dimensions are rounded down to even numbers, because 4:2:0 chroma cannot describe an odd
number of pixels and ffmpeg's refusal for it names a pixel format rather than the window.

### Encoder

`auto` prefers the GPU — NVENC, then AMF, then Quick Sync — and falls back to `libx264`. Not
for quality: the processor is busy running the game whose replay this is, and an encoder that
steals frames from it is worse than one that keeps slightly less detail. A named encoder
always wins over the probe, because somebody who picked x264 on a machine with a flaky driver
meant it.

Audio is silent unless a DirectShow device is named, exactly as ffmpeg lists it. A device that
does not exist fails the *whole* recording, video included, so it is not a thing to guess at
on somebody's behalf.

### Where ffmpeg comes from

In order: the path in Settings, then the one the Studio fetched into its own data folder, then
whatever is on `PATH`. Each step is a stronger statement of intent than the next.

The fetch is Windows-only and takes the published SHA-256 beside the build, which is worth
being precise about: the zip and the digest come from the same host, so this catches a
truncated download and not a compromised one — the same guarantee the publisher offers
everyone. Only `bin/ffmpeg.exe` is unpacked; nothing is installed into Windows, and deleting
the folder undoes all of it. Elsewhere the Studio says to install ffmpeg from a package
manager and finds it on `PATH`.

### The hotkey

`Ctrl+Shift+R` by default, changeable in the Replay screen. It exists because the rider is
inside a full-screen game and the Studio is behind it: alt-tabbing out to press a button in
another window loses the shot.

## What the Studio shows

Everything above happens with the window closed. The Replay screen is for the parts that
can't:

- the status — recording, ready, or which piece is missing (no ffmpeg, no mod, no game);
- the recordings, with their size and when they were taken;
- the camera paths on disk, so a `.fcam` can be found and passed to someone;
- the settings, and the **Get ffmpeg** button.

Nothing here reads inside a `.fcam`. That format belongs to the mod, and the Studio has no
business parsing a file it did not write — it lists them, and the panels the mod itself
contributes are what edit them.
