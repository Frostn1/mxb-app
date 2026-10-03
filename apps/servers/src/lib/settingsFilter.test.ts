import { describe, expect, it } from "vitest";
import type { ConfigField } from "@/lib/api";
import { completeToken, fieldMatches, parseQuery, tokenHints } from "./settingsFilter";

const f = (o: Partial<ConfigField>): ConfigField => ({
  section: "server", key: "name", group: "server", stock: "", status: "", advanced: false,
  label: "", help: "", defaultText: "", unit: "", kind: { type: "bool" } as ConfigField["kind"], ...o,
});
const pen = f({ section: "penalties", key: "cut_time", group: "penalties", stock: "penalty.cut", label: "Cut penalty" });
const bots = f({ section: "ghost", key: "fill_to", group: "ghosts", label: "Fill with bots" });
const nope = f({ section: "rules", key: "tyres", group: "rules", stock: "rules.tyres", status: "unsupported", label: "Tyre wear" });
const elsewhere = f({ section: "server", key: "port", group: "server", stock: "server.port", status: "elsewhere", label: "Port" });
const name = f({ section: "server", key: "name", group: "server", stock: "server.name", label: "Server name" });
const m = (q: string, x: ConfigField) => fieldMatches(x, parseQuery(q));

describe("settings filter", () => {
  it("empty query matches everything", () => {
    expect(m("", bots)).toBe(true);
  });
  it(":stock and :ours split by origin", () => {
    expect(m(":stock", pen)).toBe(true);
    expect(m(":stock", bots)).toBe(false);
    expect(m(":ours", bots)).toBe(true);
    expect(m(":ours", pen)).toBe(false);
  });
  it("tokens combine with plain text over label, key and group", () => {
    expect(m(":stock pen", pen)).toBe(true);
    expect(m(":stock pen", name)).toBe(false);
    expect(m("cut_time", pen)).toBe(true);
    expect(m("GHOSTS", bots)).toBe(true);
    expect(m("penalties cut", pen)).toBe(true);
  });
  it("reload tokens follow the badge class", () => {
    expect(m(":live", name)).toBe(true);
    expect(m(":restart", name)).toBe(false);
    expect(m(":session", pen)).toBe(true);
    expect(m(":track", f({ section: "sessions", key: "practice", group: "sessions" }))).toBe(true);
    expect(m(":restart", nope)).toBe(true);
    expect(m(":live :restart", name)).toBe(false);
  });
  it(":unsupported covers not supported and set elsewhere, stock only", () => {
    expect(m(":unsupported", nope)).toBe(true);
    expect(m(":unsupported", elsewhere)).toBe(true);
    expect(m(":unsupported", name)).toBe(false);
    expect(m(":unsupported", f({ status: "unsupported" }))).toBe(false);
  });
  it("unknown colon words are plain text", () => {
    expect(parseQuery(":bogus x")).toEqual({ tokens: [], words: [":bogus", "x"] });
  });
  it("hints and completion", () => {
    expect(tokenHints("pen :s").map((t) => t.token)).toEqual([":stock", ":session"]);
    expect(tokenHints(":").length).toBe(7);
    expect(tokenHints("pen")).toEqual([]);
    expect(tokenHints(":stock")).toEqual([]);
    expect(completeToken("pen :s", ":stock")).toBe("pen :stock ");
  });
});
