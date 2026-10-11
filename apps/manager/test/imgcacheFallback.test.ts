import { beforeEach, describe, expect, test } from "bun:test";
import {
  noteDirectRecovery,
  resetDirectHosts,
  schemeIsBroken,
} from "@frost/shared/lib/imgcache";

const MODS = "https://mxb-mods.com/wp-content/uploads/2026/10/a.jpg";
const SHOP = "https://cdn.mxbikes-shop.com/wp-content/uploads/2026/09/THUMBNAIL.png";

describe("direct fallback", () => {
  beforeEach(() => resetDirectHosts());

  test("a host goes direct only after three recoveries", () => {
    noteDirectRecovery(MODS);
    noteDirectRecovery(MODS);
    expect(schemeIsBroken(MODS)).toBe(false);
    noteDirectRecovery(MODS);
    expect(schemeIsBroken(MODS)).toBe(true);
  });

  // The bug: mxb-mods recovering direct switched the cache off for the Shop too, whose CDN
  // refuses the webview's direct requests, so every Shop card went blank.
  test("one catalog recovering direct leaves the others on the cache", () => {
    for (let i = 0; i < 5; i++) noteDirectRecovery(MODS);
    expect(schemeIsBroken(MODS)).toBe(true);
    expect(schemeIsBroken(SHOP)).toBe(false);
  });

  test("a broken scheme still ends up direct for every host", () => {
    for (const url of [MODS, SHOP]) for (let i = 0; i < 3; i++) noteDirectRecovery(url);
    expect(schemeIsBroken(MODS)).toBe(true);
    expect(schemeIsBroken(SHOP)).toBe(true);
  });

  test("a malformed url is never direct and never counted", () => {
    noteDirectRecovery("not a url");
    expect(schemeIsBroken("not a url")).toBe(false);
  });
});
