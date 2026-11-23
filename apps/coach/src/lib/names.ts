import type { TFunc } from "@frost/shared/i18n/core";
import type { TKey } from "@/i18n";

/**
 * The backend names sections and themes in English everywhere, because the name is also how
 * the window, the track's map and the cue history find one. Only the words the rider reads are
 * put into their language, here.
 */
const WORDS: Record<string, string> = { Turn: "turn", Jump: "jump", Rhythm: "rhythm", Whoops: "whoops", Straight: "straight" };

/** "Turn 5" as the rider reads it: "Virage 5" in French. A name a track's map gave stays as it is. */
export function sectionName(t: TFunc<TKey>, name: string): string {
  const m = /^(Turn|Jump|Rhythm|Whoops|Straight) (\d+)$/.exec(name);
  return m ? t(`sectionName.${WORDS[m[1]]}` as TKey, { n: m[2] }) : name;
}

/** The label on the map and the strip: "T5", "J2". Straights get none. */
export function shortName(t: TFunc<TKey>, name: string): string {
  const m = /^(Turn|Jump|Rhythm|Whoops) (\d+)$/.exec(name);
  return m ? t(`sectionShort.${WORDS[m[1]]}` as TKey, { n: m[2] }) : "";
}

const THEMES: Record<string, string> = {
  Braking: "braking",
  "Corner speed": "cornerSpeed",
  "Corner exits": "cornerExits",
  Jumps: "jumps",
  Whoops: "whoops",
  Straights: "straights",
  "Body position": "bodyPosition",
  Other: "other",
};

/** One of `analysis.rs`'s themes, "Braking" or "Corner speed", as the rider reads it. */
export function themeName(t: TFunc<TKey>, name: string): string {
  const k = THEMES[name];
  return k ? t(`theme.${k}` as TKey) : name;
}

const ISSUES: Record<string, string> = {
  "out lap": "outLap",
  unfinished: "unfinished",
  untimed: "untimed",
  "gap in the recording": "gap",
};

/** Why a lap can't be compared, from `telemetry.rs`, as the rider reads it. */
export function lapIssue(t: TFunc<TKey>, issue: string): string {
  const k = ISSUES[issue];
  return k ? t(`lapIssue.${k}` as TKey) : issue[0].toUpperCase() + issue.slice(1);
}
