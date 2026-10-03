import type { ConfigField } from "@/lib/api";
import { reloadClass, type ReloadClass } from "@/lib/reload";

/** The `:token` filters the Settings search box understands, with the hint shown for each. */
export const FILTER_TOKENS = [
  { token: ":stock", hint: "Every stock MX Bikes server setting" },
  { token: ":ours", hint: "Our extras: bots, cuts, deformation, listing, logging" },
  { token: ":live", hint: "Applies now, nobody is disconnected" },
  { token: ":session", hint: "Applies next session" },
  { token: ":track", hint: "Applies at the next track load" },
  { token: ":restart", hint: "Needs a server restart" },
  { token: ":unsupported", hint: "Stock settings not supported yet, or set elsewhere" },
] as const;

const CLASS_TOKENS: Record<string, ReloadClass> = {
  ":live": "hot",
  ":session": "next_session",
  ":track": "next_event",
  ":restart": "restart",
};

export interface ParsedQuery {
  /** Recognised `:tokens`, lowercase. */
  tokens: string[];
  /** The remaining words, lowercase. */
  words: string[];
}

export function parseQuery(query: string): ParsedQuery {
  const tokens: string[] = [];
  const words: string[] = [];
  for (const part of query.toLowerCase().split(/\s+/).filter(Boolean)) {
    if (FILTER_TOKENS.some((t) => t.token === part)) tokens.push(part);
    else words.push(part);
  }
  return { tokens, words };
}

/** A field is stock when the #979 work gave it a stock name. */
const isStock = (f: ConfigField) => f.stock !== "";

function matchesToken(f: ConfigField, token: string): boolean {
  switch (token) {
    case ":stock":
      return isStock(f);
    case ":ours":
      return !isStock(f);
    case ":unsupported":
      return isStock(f) && (f.status === "unsupported" || f.status === "elsewhere");
    default: {
      const cls = CLASS_TOKENS[token];
      return cls !== undefined && reloadClass(`${f.section}.${f.key}`) === cls;
    }
  }
}

/** Every token must match (they narrow); every plain word must appear in the label, key or group. */
export function fieldMatches(f: ConfigField, parsed: ParsedQuery): boolean {
  if (!parsed.tokens.every((t) => matchesToken(f, t))) return false;
  if (parsed.words.length === 0) return true;
  const hay = `${f.label} ${f.section}.${f.key} ${f.key} ${f.group}`.toLowerCase();
  return parsed.words.every((w) => hay.includes(w));
}

/** The tokens to hint while the user types a `:` word at the end of the box. */
export function tokenHints(query: string): readonly (typeof FILTER_TOKENS)[number][] {
  const last = query.split(/\s+/).pop()?.toLowerCase() ?? "";
  if (!last.startsWith(":")) return [];
  return FILTER_TOKENS.filter((t) => t.token.startsWith(last) && t.token !== last);
}

/** The query with its last word replaced by a chosen token. */
export function completeToken(query: string, token: string): string {
  return `${query.replace(/\S*$/, "")}${token} `;
}
