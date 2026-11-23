import { SPLIT_NOTHING, type LibraryPart } from "@/api/bikebuild";
import type { TKey } from "@/i18n";

/** Split is off only when Blender already looked and found one piece whichever way. Parts
 *  added before the backend kept that answer (`splitChecked` false) are still worth a try. */
export function nothingToSplit(part: Pick<LibraryPart, "splitBy" | "splitChecked">): boolean {
  return !!part.splitChecked && part.splitBy == null;
}

/**
 * What went wrong with a split, in words a rider can act on — never the exception itself.
 * Sean saw "ValueError: this file only has one part in it — nothing to split" in a toast;
 * that's Blender's Python talking, not Studio.
 *
 * `bike_part_split` rejects with exactly `SPLIT_NOTHING` when the part is one piece whichever
 * way. Anything else is a plain string (Blender's message somewhere inside it) or a structured
 * one with a `code`. Codes it doesn't know, and strings that match nothing here, get the
 * generic "try again" — still not the raw text.
 */
export function splitErrorMessage(e: unknown): TKey {
  if (e === SPLIT_NOTHING || (e instanceof Error && e.message === SPLIT_NOTHING)) return "bike.splitNothing";
  const code =
    e && typeof e === "object" && "code" in e && typeof (e as { code: unknown }).code === "string"
      ? (e as { code: string }).code
      : null;
  if (code) {
    if (/nothing|single|one_part|one-part|no_parts/i.test(code)) return "bike.splitNothing";
    if (/missing|gone|not_found/i.test(code)) return "bike.splitFileGone";
    return "bike.splitTryAgain";
  }
  const text = e instanceof Error ? e.message : typeof e === "string" ? e : String(e);
  if (/only has one part|nothing to split|single mesh|one single/i.test(text)) return "bike.splitNothing";
  if (/file'?s gone|no such file|cannot find the file|not found/i.test(text)) return "bike.splitFileGone";
  return "bike.splitTryAgain";
}
