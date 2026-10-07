import { expect, test } from "bun:test";
import { installNoteKey } from "../src/lib/installNote";

test("a reload that got through says the mod is live", () => {
  expect(installNoteKey("signaled", true)).toBe("install.reloadedDesc");
});

test("a running game that was not reloaded is told to restart", () => {
  expect(installNoteKey(null, true)).toBe("install.restartDesc");
  expect(installNoteKey("not_running", true)).toBe("install.restartDesc");
});

test("with the game closed there is nothing to restart", () => {
  expect(installNoteKey(null, false)).toBe("install.addedDesc");
  expect(installNoteKey("not_running", false)).toBe("install.addedDesc");
});
