import { describe, expect, test } from "bun:test";
import { STEPS, stepsFor } from "../src/Components/Tour/steps";
import { en } from "../src/i18n/locales/en";

describe("tour steps", () => {
  test("a game without a capability drops the steps gated on it", () => {
    const noViewer = stepsFor({ viewer: false, frostmod: true });
    expect(noViewer.some((s) => s.cap === "viewer")).toBe(false);
    expect(noViewer.some((s) => s.cap === "frostmod")).toBe(true);
    expect(stepsFor({ viewer: true, frostmod: false }).some((s) => s.cap === "frostmod")).toBe(false);
  });

  test("ungated steps survive any capability set", () => {
    const ungated = STEPS.filter((s) => !s.cap).length;
    expect(stepsFor({}).length).toBe(ungated);
  });

  test("starts and ends on a centred, un-anchored step", () => {
    const steps = stepsFor({ viewer: true, frostmod: true });
    expect(steps[0].selector).toBeUndefined();
    expect(steps[steps.length - 1].selector).toBeUndefined();
  });

  test("every step has copy in the English dictionary", () => {
    for (const s of STEPS) {
      expect(en[s.title as keyof typeof en]).toBeTruthy();
      expect(en[s.body as keyof typeof en]).toBeTruthy();
    }
  });

  test("every selector is a data-tour attribute", () => {
    for (const s of STEPS) if (s.selector) expect(s.selector).toMatch(/^\[data-tour="[a-z-]+"\]$/);
  });
});
