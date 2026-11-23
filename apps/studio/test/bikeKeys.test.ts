import { describe, expect, test } from "bun:test";
import * as THREE from "three";
import {
  accumulateSweep,
  bikeKeyCommand,
  fromThree,
  grabDelta,
  modalKey,
  modalReadout,
  nudgeSteps,
  rotateResult,
  snapAngle,
  snapGrab,
  startModal,
  sweptAngle,
  typedValue,
  type Modal,
  type ModalStep,
} from "../src/Components/Studio/BikeBuilder/bikeKeys";
import { History } from "../src/Components/Studio/BikeBuilder/useBikeHistory";
import { toThree, type V3 } from "../src/api/bikebuild";

const key = (code: string, mods: { ctrl?: boolean; shift?: boolean; alt?: boolean; key?: string } = {}) => ({
  code,
  key: mods.key,
  ctrlKey: !!mods.ctrl,
  shiftKey: !!mods.shift,
  altKey: !!mods.alt,
});

/** Run keys through a modal, as the viewport does, stopping at a confirm or cancel. */
function type(m: Modal, codes: string[]): Modal | "confirm" | "cancel" {
  let now: Modal = m;
  for (const c of codes) {
    const step: ModalStep | null = modalKey(now, key(c));
    if (!step) continue;
    if ("confirm" in step) return "confirm";
    if ("cancel" in step) return "cancel";
    now = step.modal;
  }
  return now;
}

const close = (a: number[], b: number[], eps = 1e-6) => a.every((v, i) => Math.abs(v - b[i]) < eps);

describe("Bike keys over the 3D view", () => {
  test("Blender's letters", () => {
    expect(bikeKeyCommand(key("KeyG"))).toEqual({ kind: "grab" });
    expect(bikeKeyCommand(key("KeyR"))).toEqual({ kind: "rotate" });
    expect(bikeKeyCommand(key("KeyH"))).toEqual({ kind: "hide" });
    expect(bikeKeyCommand(key("KeyH", { alt: true }))).toEqual({ kind: "unhideAll" });
    expect(bikeKeyCommand(key("KeyX"))).toEqual({ kind: "takeOff" });
    expect(bikeKeyCommand(key("Delete"))).toEqual({ kind: "takeOff" });
    expect(bikeKeyCommand(key("KeyA"))).toEqual({ kind: "selectAll" });
    expect(bikeKeyCommand(key("KeyA", { alt: true }))).toEqual({ kind: "deselectAll" });
    expect(bikeKeyCommand(key("KeyN"))).toEqual({ kind: "toggleProperties" });
    expect(bikeKeyCommand(key("KeyA", { shift: true }))).toEqual({ kind: "add" });
    expect(bikeKeyCommand(key("KeyP"))).toEqual({ kind: "split" });
    expect(bikeKeyCommand(key("KeyP", { ctrl: true }))).toEqual({ kind: "place" });
  });

  test("undo and redo", () => {
    expect(bikeKeyCommand(key("KeyZ", { ctrl: true }))).toEqual({ kind: "undo" });
    expect(bikeKeyCommand(key("KeyZ", { ctrl: true, shift: true }))).toEqual({ kind: "redo" });
    expect(bikeKeyCommand(key("KeyY", { ctrl: true }))).toEqual({ kind: "redo" });
    // A plain Z or Y is nothing on its own here (only an axis mid-move).
    expect(bikeKeyCommand(key("KeyZ"))).toBeNull();
    expect(bikeKeyCommand({ code: "KeyZ", metaKey: true })).toEqual({ kind: "undo" });
  });

  test("the help, and keys left alone", () => {
    expect(bikeKeyCommand(key("Slash", { shift: true }))).toEqual({ kind: "help" });
    // A layout where "?" is somewhere else still gets it by the character.
    expect(bikeKeyCommand(key("Minus", { shift: true, key: "?" }))).toEqual({ kind: "help" });
    // The view's own keys stay ViewportNav's.
    for (const code of ["Numpad1", "Numpad3", "Numpad7", "NumpadDecimal", "Home", "Backquote"]) {
      expect(bikeKeyCommand(key(code))).toBeNull();
    }
    expect(bikeKeyCommand(key("KeyG", { ctrl: true }))).toBeNull();
    expect(bikeKeyCommand(key("KeyG", { alt: true }))).toBeNull();
    expect(bikeKeyCommand(key("KeyH", { alt: true, shift: true }))).toBeNull();
  });
});

describe("G and R", () => {
  test("X/Y/Z lock an axis; the same key again frees it", () => {
    let m = type(startModal("grab"), ["KeyZ"]) as Modal;
    expect(m.axis).toBe(2);
    m = type(m, ["KeyX"]) as Modal;
    expect(m.axis).toBe(0);
    m = type(m, ["KeyX"]) as Modal;
    expect(m.axis).toBeNull();
  });

  test("typing a value: digits, a point, minus flips, backspace", () => {
    const m = type(startModal("grab"), ["KeyY", "Digit2", "Numpad5", "Period", "Digit5"]) as Modal;
    expect(m.typed).toBe("25.5");
    expect(typedValue(m.typed)).toBe(25.5);
    const neg = type(m, ["Minus"]) as Modal;
    expect(typedValue(neg.typed)).toBe(-25.5);
    expect(typedValue((type(neg, ["Minus"]) as Modal).typed)).toBe(25.5);
    expect((type(m, ["Backspace", "Backspace"]) as Modal).typed).toBe("25");
    // A second point is ignored; nothing typed yet is no value.
    expect((type(m, ["Period"]) as Modal).typed).toBe("25.5");
    expect(typedValue("")).toBeNull();
    expect(typedValue("-")).toBeNull();
    expect(typedValue("3.")).toBe(3);
  });

  test("Enter or Esc ends it; other keys change nothing", () => {
    expect(type(startModal("grab"), ["Digit1", "Enter"])).toBe("confirm");
    expect(type(startModal("rotate"), ["NumpadEnter"])).toBe("confirm");
    expect(type(startModal("grab"), ["Escape"])).toBe("cancel");
    expect(modalKey(startModal("grab"), key("KeyQ"))).toBeNull();
    // Ctrl+Z mid-move isn't an axis.
    expect(modalKey(startModal("grab"), key("KeyZ", { ctrl: true }))).toBeNull();
  });

  test("G: typed millimetres along the locked axis, X when none", () => {
    const along = type(startModal("grab"), ["KeyZ", "Digit2", "Digit5"]) as Modal;
    expect(grabDelta(along, [0.3, 0.2, 0.1])).toEqual([0, 0, 0.025]);
    const noAxis = type(startModal("grab"), ["Digit1", "Digit0"]) as Modal;
    expect(grabDelta(noAxis, [0, 0, 0])).toEqual([0.01, 0, 0]);
  });

  test("G: the mouse's move, kept to the locked axis", () => {
    const free = startModal("grab");
    expect(grabDelta(free, [0.01, 0.02, 0.03])).toEqual([0.01, 0.02, 0.03]);
    const y = type(free, ["KeyY"]) as Modal;
    expect(grabDelta(y, [0.01, 0.02, 0.03])).toEqual([0, 0.02, 0]);
  });

  test("R: typed degrees about the locked axis, on top of the turn so far", () => {
    const z90 = type(startModal("rotate"), ["KeyZ", "Digit9", "Digit0"]) as Modal;
    expect(close(rotateResult(z90, [0, 0, 0], 0, [0, 0, 1]), [0, 0, 90])).toBe(true);
    // Turned 10° about Z already: 90 more makes 100.
    expect(close(rotateResult(z90, [0, 0, 10], 0, [0, 0, 1]), [0, 0, 100])).toBe(true);
    // Past 180 it wraps, as the Properties fields keep it.
    expect(close(rotateResult(z90, [0, 0, 120], 0, [0, 0, 1]), [0, 0, -150])).toBe(true);
    const x = type(startModal("rotate"), ["KeyX", "Digit3", "Digit0"]) as Modal;
    expect(close(rotateResult(x, [0, 0, 0], 0, [0, 0, 1]), [30, 0, 0])).toBe(true);
  });

  test("R's result is Blender's XYZ Euler, as bike_rotate keeps it", () => {
    // X then Z: Rz·Rx. Turn a point with the result and with the two turns in order: the same.
    const x = type(startModal("rotate"), ["KeyX", "Digit9", "Digit0"]) as Modal;
    const afterX = rotateResult(x, [0, 0, 0], 0, [0, 0, 1]);
    const z = type(startModal("rotate"), ["KeyZ", "Digit9", "Digit0"]) as Modal;
    const both = rotateResult(z, afterX, 0, [0, 0, 1]);
    const r = THREE.MathUtils.degToRad;
    const m = new THREE.Matrix4().makeRotationFromEuler(new THREE.Euler(r(both[0]), r(both[1]), r(both[2]), "ZYX"));
    const p = new THREE.Vector3(0, 1, 0).applyMatrix4(m);
    // +Y → (X 90) +Z → (Z 90) stays +Z.
    expect(close([p.x, p.y, p.z], [0, 0, 1])).toBe(true);
    // Free, about the view's axis: looking down -Y (the bike's front), it turns about Y.
    const free = startModal("rotate");
    expect(close(rotateResult(free, [0, 0, 0], 45, [0, -1, 0]), [0, -45, 0])).toBe(true);
  });

  test("R follows the mouse round the part on screen", () => {
    // From right of the centre to above it (screen y down): a quarter turn anticlockwise.
    expect(sweptAngle([100, 100], [150, 100], [100, 50])).toBeCloseTo(90);
    expect(sweptAngle([100, 100], [150, 100], [100, 150])).toBeCloseTo(-90);
    // Across the ±180 seam it doesn't jump.
    expect(sweptAngle([0, 0], [-10, -1], [-10, 1])).toBeCloseTo(11.42, 1);
  });

  test("the readout says what's typed, else the live value", () => {
    const g = startModal("grab");
    expect(modalReadout(g, 12.34)).toBe("12.3 mm");
    expect(modalReadout(type(g, ["Digit5"]) as Modal, 12.34)).toBe("5 mm");
    expect(modalReadout(type(startModal("rotate"), ["Minus"]) as Modal, 3)).toBe("-°");
  });

  test("three's frame back to Blender's", () => {
    const p: V3 = [0.1, -0.2, 0.3];
    expect(fromThree(toThree(p))).toEqual(p);
  });

  test("a big move goes to the backend in steps it takes", () => {
    expect(nudgeSteps([0.01, 0, 0])).toEqual([[0.01, 0, 0]]);
    const steps = nudgeSteps([1.2, 0, -0.3]);
    expect(steps.length).toBe(3);
    for (const s of steps) expect(Math.max(...s.map(Math.abs))).toBeLessThanOrEqual(0.5);
    const sum = steps.reduce((a, s) => a.map((v, i) => v + s[i]) as V3, [0, 0, 0] as V3);
    expect(close(sum, [1.2, 0, -0.3])).toBe(true);
  });
});

describe("undo history", () => {
  test("undo, redo, and something new clears redo", async () => {
    let value = 0;
    const h = new History();
    const set = (v: number) => async () => {
      value = v;
    };
    value = 1;
    h.push({ label: "one", undo: set(0), redo: set(1) });
    value = 2;
    h.push({ label: "two", undo: set(1), redo: set(2) });
    expect(h.nextUndo).toBe("two");
    expect(await h.undo()).toBe("two");
    expect(value).toBe(1);
    expect(await h.undo()).toBe("one");
    expect(value).toBe(0);
    expect(await h.undo()).toBeNull();
    expect(await h.redo()).toBe("one");
    expect(value).toBe(1);
    h.push({ label: "three", undo: set(1), redo: set(3) });
    expect(h.canRedo).toBe(false);
    expect(await h.redo()).toBeNull();
  });

  test("a step that fails stays where it was", async () => {
    const h = new History();
    h.push({
      label: "bad",
      undo: async () => {
        throw new Error("backend said no");
      },
      redo: async () => {},
    });
    await expect(h.undo()).rejects.toThrow("backend said no");
    expect(h.canUndo).toBe(true);
    expect(h.canRedo).toBe(false);
  });
});

describe("Ctrl snapping and long turns", () => {
  test("G with Ctrl steps each axis by 5 mm", () => {
    const [x, y, z] = snapGrab([0.0123, -0.0026, 0.0024]);
    expect(x).toBeCloseTo(0.01, 9);
    expect(y).toBeCloseTo(-0.005, 9);
    expect(z).toBe(0);
  });
  test("R with Ctrl steps by 5 degrees", () => {
    expect(snapAngle(12.4)).toBe(10);
    expect(snapAngle(-2.4)).toBe(0);
    expect(snapAngle(178)).toBe(180);
  });
  test("R keeps counting past half a turn", () => {
    const c: [number, number] = [0, 0];
    // Anticlockwise on screen (y grows down): right, up, left, down, right again.
    const path: [number, number][] = [[10, 0], [0, -10], [-10, 0], [0, 10], [10, 0]];
    let total = 0;
    for (let i = 1; i < path.length; i++) total = accumulateSweep(total, c, path[i - 1], path[i]);
    expect(total).toBeCloseTo(360, 6);
  });
});
