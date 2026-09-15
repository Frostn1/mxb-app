import { en as base } from "@frost/shared/i18n/base/en";

/**
 * MXB Coach — English, the only locale for now.
 *
 * Spreads the shared base so a component from @frost/shared finds its strings here too.
 * The advice itself is written by the analysis in `analysis.rs`, not here.
 */
export const en = {
  ...base,
  "nav.sessions": "Sessions",
  "nav.settings": "Settings",

  "common.refresh": "Refresh",
  "common.loading": "Loading…",

  "sessions.title": "Sessions",
  "sessions.sub": "Every stint you ride with the recorder on. Pick one to see its laps.",
  "sessions.emptyTitle": "No sessions yet",
  "sessions.emptyBody":
    "Install the recorder in Settings, ride a few laps in MX Bikes, and they show up here.",
  "sessions.when": "When",
  "sessions.track": "Track",
  "sessions.bike": "Bike",
  "sessions.laps": "Laps",
  "sessions.best": "Best",

  "session.best": "Best lap",
  "session.ideal": "Ideal lap",
  "session.vsBest": "vs best",
  "session.reference": "Compared with",
  "session.noReference": "Ride a whole lap to compare with",
  "session.laps": "Laps",
  "session.lap": "Lap",
  "session.bestTag": "best",
  "session.noLaps": "No timed laps in this session.",
  "session.invalid": "Invalid",
  "session.partial": "Partial",
  "session.review": "Review",
  "session.sections": "Your best for each section",
  "session.leastConsistent": "Least consistent",

  "review.title": "Lap review",
  "review.back": "Laps",
  "review.against": "against",
  "review.focus": "Work on these",
  "review.nothing": "This lap is as fast as the reference everywhere.",
  "review.sections": "All sections",
  "review.legend":
    "Red loses time to the reference, green gains it. Blue is this lap, grey the reference.",
  "review.gap": "Gap (s)",
  "review.speed": "Speed (km/h)",
  "review.throttle": "Throttle",
  "review.brake": "Brake",
  "review.lean": "Lean (°)",

  "recorder.title": "Recorder",
  "recorder.on": "The recorder is installed",
  "recorder.off": "The recorder isn't installed",
  "recorder.body":
    "A small MX Bikes plugin that saves your laps for the coach. It only records, and changes nothing in the game.",
  "recorder.install": "Install",
  "recorder.update": "Update",
  "recorder.fromFile": "Install from file…",
  "recorder.remove": "Remove",
  "recorder.installed": "Recorder installed. Restart MX Bikes if it's open.",
  "recorder.removed": "Recorder removed.",
  "recorder.noGame": "The MX Bikes folder wasn't found. Set it in MXB App.",
  "recorder.missingTitle": "The recorder isn't installed",
  "recorder.missingBody": "Install it to start saving your laps.",
  "recorder.setUp": "Set up",

  "coachSettings.title": "Settings",
  "coachSettings.where": "Where the coach looks",
  "coachSettings.game": "Game",
  "coachSettings.gameFolder": "Game install",
  "coachSettings.plugin": "Recorder",
  "coachSettings.sessions": "Session files",
};
