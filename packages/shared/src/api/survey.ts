import { invoke } from "@tauri-apps/api/core";

/**
 * The survey prompt, in every app.
 *
 * All three register the same commands from `crates/core/src/survey.rs`, which is where the
 * schedule, the consent checks and the sending live. Nothing here decides whether to ask — this
 * side only draws the question and hands the answer back.
 */

/** Text in whatever languages it was written in, keyed by locale. */
export type Localized = Record<string, string>;

/** One answer the player can tap, or one chip on the follow-up. */
export interface PollChoice {
  /** What travels back and what the dashboard groups on. */
  id: string;
  label: Localized;
}

export interface Poll {
  id: string;
  /**
   * `mood` — the standing question, whose three answers this app draws in the player's own
   * language. `choice` — anything else, which speaks whatever languages it was written in.
   */
  kind: "mood" | "choice";
  apps: string[];
  ask: Localized;
  choices: PollChoice[];
  /** Empty means the built-in chips, which ship translated. */
  reasons: PollChoice[];
  /** Choice ids that always get the follow-up. */
  followUp: string[];
  followUpChance: number;
  /** Whether the optional note box is offered at all. */
  note: boolean;
  againDays: number;
  minVersion: string;
}

export interface Prompt {
  poll: Poll;
  /** The mood poll's answer ids, in the order they should be drawn. */
  moodChoices: string[];
  /** The chip ids to draw when the poll names none of its own. */
  builtInReasons: string[];
}

export interface Answered {
  /** Whether to ask the second question. Decided in Rust, per poll and per answer. */
  followUp: boolean;
}

/** Is there a question to put on screen? Null almost every time, which is the intended shape. */
export function surveyDue(): Promise<Prompt | null> {
  return invoke<Prompt | null>("survey_due");
}

/** The card is on screen. Starts the once-a-day clock whether or not it is answered. */
export function surveyShown(): Promise<void> {
  return invoke("survey_shown");
}

/**
 * Send an answer.
 *
 * Called twice when there is a follow-up: once with the choice alone — so a player who closes
 * the card at the second question has still answered the first — and again with the chips and
 * the note, which the endpoint folds onto the same row.
 */
export function surveyAnswer(
  pollId: string,
  choice: string,
  reasons: string[] = [],
  note: string | null = null,
): Promise<Answered> {
  return invoke<Answered>("survey_answer", { pollId, choice, reasons, note });
}

/** The card was waved away. Three in a row and the prompt stops asking for half a year. */
export function surveyDismiss(): Promise<void> {
  return invoke("survey_dismiss");
}

/** The switch in Settings. */
export function setSurveyEnabled(enabled: boolean): Promise<void> {
  return invoke("set_survey_enabled", { enabled });
}

/**
 * The text for the language on screen.
 *
 * Falls back through the language without its region (`pt-BR` → `pt`) and then to English,
 * which the endpoint requires every poll to carry. A poll written in English alone still asks
 * everybody — in English — rather than showing an empty card.
 */
export function pick(text: Localized, locale: string): string {
  return text[locale] ?? text[locale.split("-")[0]] ?? text.en ?? "";
}
