import { describe, expect, test } from "bun:test";
import { checkScript, scriptSettings } from "@frost/shared/api/convert";

// PiBoSo's bike template's export_script.ini, as the Convert screen gets it dropped.
const TEMPLATE = [
  "[Hierarchy]",
  "split = 1",
  "rotation = 0 0 0",
  String.raw`chassis = C:\Program Files (x86)\MX Bikes\bikes\template\model.edf`,
  "chassis_rot = 0 180 0",
  String.raw`steer = C:\Program Files (x86)\MX Bikes\bikes\template\model.edf`,
  "steer_rot = -90 180 0",
  "steer_append = steer",
  "fsusp_rot = -90 180 0",
  "rsusp_rot = 0 180 0",
  "",
].join("\r\n");

describe("a dropped export script", () => {
  test("says which parts it sets, in the game's order", () => {
    expect(checkScript(TEMPLATE)).toBe("ok");
    expect(scriptSettings(TEMPLATE)).toEqual(["chassis", "steer", "fsusp", "rsusp"]);
  });

  test("lists its [params] keys after the parts, once each", () => {
    const text = "[params]\nscale = 1\nscaling = 2 ; doubled\nscaling = 3\n[Hierarchy]\nsteer_rot = -90 180 0\n";
    expect(scriptSettings(text)).toEqual(["steer", "scale", "scaling"]);
  });

  test("says nothing for text that sets nothing", () => {
    expect(scriptSettings("just a note")).toEqual([]);
  });
});

describe("an export script for something that is not a bike", () => {
  const CHAIN = "[Hierarchy]\nchain_rot = 0 180 0\n[params]\nscale = 1\n";

  test("is refused for bike parts, naming no part", () => {
    expect(checkScript(CHAIN)).toBe("noParts");
    expect(checkScript(CHAIN, "parts")).toBe("noParts");
  });

  test("passes as one model, which has no bike part to name", () => {
    expect(checkScript(CHAIN, "whole")).toBe("ok");
    expect(checkScript("[params]\nscale = 1\n", "whole")).toBe("ok");
  });

  test("still needs a part for a bike", () => {
    expect(checkScript("[params]\nscale = 1\n", "parts")).toBe("noSection");
    expect(checkScript(TEMPLATE, "whole")).toBe("ok");
  });
});
