import { describe, expect, test } from "bun:test";
import { modSourcePageUrl } from "../src/lib/modSourcePage";

describe("modSourcePageUrl", () => {
  test("prefers the page's own permalink", () => {
    expect(modSourcePageUrl("https://mxb-mods.com/fort-red/", "mxb-mods.com", "fort-red")).toBe(
      "https://mxb-mods.com/fort-red/",
    );
  });

  test("falls back to the slug on the catalog domain", () => {
    expect(modSourcePageUrl("", "mxb-mods.com", "fort-red")).toBe("https://mxb-mods.com/fort-red/");
    expect(modSourcePageUrl(null, "mxb-mods.com", "fort-red")).toBe(
      "https://mxb-mods.com/fort-red/",
    );
  });

  test("never passes a non-http link on", () => {
    expect(modSourcePageUrl("javascript:alert(1)", "mxb-mods.com", "x")).toBe(
      "https://mxb-mods.com/x/",
    );
    expect(modSourcePageUrl("file:///c:/x", "mxb-mods.com", "x")).toBe("https://mxb-mods.com/x/");
  });
});
