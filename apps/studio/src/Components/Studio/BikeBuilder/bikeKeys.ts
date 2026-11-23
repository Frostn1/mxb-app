import * as THREE from "three";
import type { V3 } from "../../../api/bikebuild";

/**
 * The Bike tab's Blender keys, as plain functions so the tests drive exactly what the
 * viewport does with a key press.
 *
 * Over the 3D view, with no text field focused:
 *
 * | Key                  | Does                                                        |
 * |----------------------|-------------------------------------------------------------|
 * | G / R                | move / turn the selected part (see {@link modalKey})        |
 * | H, Alt+H             | hide the selected part; show every part again               |
 * | X, Delete            | take the selected part off the bike (asks first)            |
 * | A, Alt+A             | select every part on the bike; select nothing              |
 * | N                    | show or hide the Properties panel                           |
 * | Ctrl+Z, Ctrl+Shift+Z | undo, redo (Ctrl+Y redoes too)                              |
 * | Shift+A              | add parts                                                   |
 * | P, Ctrl+P            | split the selected part; place it                           |
 * | ?                    | this list                                                   |
 *
 * The view's own keys (numpad, Home, the backtick pie) are `ViewportNav`'s and stay there.
 */

export type BikeCommand =
  | { kind: "grab" }
  | { kind: "rotate" }
  | { kind: "hide" }
  | { kind: "unhideAll" }
  | { kind: "takeOff" }
  | { kind: "selectAll" }
  | { kind: "deselectAll" }
  | { kind: "toggleProperties" }
  | { kind: "undo" }
  | { kind: "redo" }
  | { kind: "add" }
  | { kind: "split" }
  | { kind: "place" }
  | { kind: "help" };

export interface KeyLike {
  code: string;
  key?: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}

/** What a key press over the 3D view means, or null for a key the Bike tab leaves alone. */
export function bikeKeyCommand(e: KeyLike): BikeCommand | null {
  const ctrl = !!(e.ctrlKey || e.metaKey);
  const shift = !!e.shiftKey;
  const alt = !!e.altKey;
  if (ctrl) {
    if (alt) return null;
    if (e.code === "KeyZ") return shift ? { kind: "redo" } : { kind: "undo" };
    if (e.code === "KeyY" && !shift) return { kind: "redo" };
    if (e.code === "KeyP" && !shift) return { kind: "place" };
    return null;
  }
  // "?" is Shift+/ on most layouts; read the character too, for the ones where it isn't.
  if (e.key === "?" || (shift && e.code === "Slash")) return { kind: "help" };
  if (alt) {
    if (shift) return null;
    if (e.code === "KeyH") return { kind: "unhideAll" };
    if (e.code === "KeyA") return { kind: "deselectAll" };
    return null;
  }
  if (shift) return e.code === "KeyA" ? { kind: "add" } : null;
  switch (e.code) {
    case "KeyG":
      return { kind: "grab" };
    case "KeyR":
      return { kind: "rotate" };
    case "KeyH":
      return { kind: "hide" };
    case "KeyX":
    case "Delete":
      return { kind: "takeOff" };
    case "KeyA":
      return { kind: "selectAll" };
    case "KeyN":
      return { kind: "toggleProperties" };
    case "KeyP":
      return { kind: "split" };
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------------------
// G and R: the modal move and turn

export type Axis = 0 | 1 | 2;

export interface Modal {
  mode: "grab" | "rotate";
  /** Locked to Blender's X, Y or Z; null is free (the view's plane, or the view's axis). */
  axis: Axis | null;
  /** What's been typed so far: an exact value (mm for G, degrees for R), as text. */
  typed: string;
}

export function startModal(mode: Modal["mode"]): Modal {
  return { mode, axis: null, typed: "" };
}

export type ModalStep = { modal: Modal } | { confirm: true } | { cancel: true };

const AXIS_KEYS: Record<string, Axis> = { KeyX: 0, KeyY: 1, KeyZ: 2 };

/** A digit key, main row or numpad: its digit, else null. */
function digit(code: string): string | null {
  const m = /^(?:Digit|Numpad)([0-9])$/.exec(code);
  return m ? m[1] : null;
}

/**
 * A key while G or R is running. X / Y / Z lock that axis (the same key again frees it);
 * digits, `.` and `-` type an exact value, Backspace takes one back; Enter confirms and Esc
 * cancels. Null for a key that changes nothing — the viewport still swallows it, so nothing
 * else (a view key on the numpad) acts mid-move.
 */
export function modalKey(m: Modal, e: KeyLike): ModalStep | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  if (e.code === "Enter" || e.code === "NumpadEnter") return { confirm: true };
  if (e.code === "Escape") return { cancel: true };
  if (e.code in AXIS_KEYS) {
    const axis = AXIS_KEYS[e.code];
    return { modal: { ...m, axis: m.axis === axis ? null : axis } };
  }
  const d = digit(e.code);
  if (d !== null) return { modal: { ...m, typed: m.typed + d } };
  if (e.code === "Period" || e.code === "NumpadDecimal" || e.code === "Comma") {
    return m.typed.includes(".") ? null : { modal: { ...m, typed: (m.typed || "0") + "." } };
  }
  if (e.code === "Minus" || e.code === "NumpadSubtract") {
    // Blender's minus flips the sign of what's typed, wherever it's pressed.
    return { modal: { ...m, typed: m.typed.startsWith("-") ? m.typed.slice(1) : "-" + m.typed } };
  }
  if (e.code === "Backspace") return m.typed ? { modal: { ...m, typed: m.typed.slice(0, -1) } } : null;
  return null;
}

/** The typed value, or null while nothing (or only a sign) is typed. */
export function typedValue(typed: string): number | null {
  if (!/\d/.test(typed)) return null;
  const v = Number(typed.endsWith(".") ? typed.slice(0, -1) : typed);
  return Number.isFinite(v) ? v : null;
}

/** What the header line shows: "X 25" while typing, else the live value. */
export function modalReadout(m: Modal, live: number): string {
  const unit = m.mode === "grab" ? " mm" : "°";
  const typed = typedValue(m.typed) !== null || m.typed === "-" ? m.typed : null;
  return `${typed ?? (Math.round(live * 10) / 10).toString()}${unit}`;
}

/**
 * How far G moves the part, in metres, Blender's frame: the typed millimetres along the
 * locked axis (X when none is, as in Blender), else the mouse's own move — kept to the
 * locked axis when there is one.
 */
export function grabDelta(m: Modal, mouse: V3): V3 {
  const typed = typedValue(m.typed);
  if (typed !== null) {
    const out: V3 = [0, 0, 0];
    out[m.axis ?? 0] = typed / 1000;
    return out;
  }
  if (m.axis === null) return mouse;
  const out: V3 = [0, 0, 0];
  out[m.axis] = mouse[m.axis];
  return out;
}

const AXIS_VECS: V3[] = [
  [1, 0, 0],
  [0, 1, 0],
  [0, 0, 1],
];

/**
 * Where R leaves the part's turn: its turn so far (`start`, an XYZ Euler in degrees in
 * Blender's frame, the way `bike_rotate` keeps it) turned further by `angle` degrees about
 * the locked axis, or `viewAxis` (Blender's frame, towards the viewer) when none is locked.
 * The typed degrees stand in for `angle` once anything's typed. Returned the same way the
 * Properties panel's fields write it, each in (-180, 180].
 */
export function rotateResult(m: Modal, start: V3, angle: number, viewAxis: V3): V3 {
  const deg = typedValue(m.typed) ?? angle;
  const axis = new THREE.Vector3(...(m.axis === null ? viewAxis : AXIS_VECS[m.axis])).normalize();
  // Blender's XYZ Euler is Rz·Ry·Rx: three's "ZYX" order builds that same matrix.
  const r = THREE.MathUtils.degToRad;
  const now = new THREE.Matrix4().makeRotationFromEuler(new THREE.Euler(r(start[0]), r(start[1]), r(start[2]), "ZYX"));
  const turn = new THREE.Matrix4().makeRotationAxis(axis, r(deg));
  const e = new THREE.Euler().setFromRotationMatrix(turn.multiply(now), "ZYX");
  const out = [e.x, e.y, e.z].map((a) => {
    let d = THREE.MathUtils.radToDeg(a);
    if (d <= -180) d += 360;
    if (d > 180) d -= 360;
    // A hundredth of a degree, as the backend keeps it; and no "-0".
    return Math.round(d * 100) / 100 || 0;
  });
  return out as V3;
}

/** The angle the pointer has swept about `centre` from `from` to `to`, screen pixels in,
 *  degrees out, anticlockwise on screen positive (screen y grows downward). */
export function sweptAngle(centre: [number, number], from: [number, number], to: [number, number]): number {
  const a0 = Math.atan2(from[1] - centre[1], from[0] - centre[0]);
  const a1 = Math.atan2(to[1] - centre[1], to[0] - centre[0]);
  let d = THREE.MathUtils.radToDeg(a0 - a1);
  while (d > 180) d -= 360;
  while (d <= -180) d += 360;
  return d;
}

/** Ctrl held during G or R steps the move, as Blender's increment snap does: G by this many
 *  millimetres on each axis, R by this many degrees. A typed value is never snapped. */
export const SNAP_MM = 5;
export const SNAP_DEG = 5;

/** `delta` (metres) with each axis rounded to `SNAP_MM`. */
export function snapGrab(delta: V3): V3 {
  return delta.map((d) => (Math.round((d * 1000) / SNAP_MM) * SNAP_MM) / 1000 || 0) as V3;
}

/** `deg` rounded to `SNAP_DEG`. */
export function snapAngle(deg: number): number {
  return Math.round(deg / SNAP_DEG) * SNAP_DEG || 0;
}

/** The sweep so far plus the pointer's latest step about `centre`: R keeps counting past a
 *  half turn, as Blender's does, instead of flipping to the other side at 180°. */
export function accumulateSweep(total: number, centre: [number, number], from: [number, number], to: [number, number]): number {
  return total + sweptAngle(centre, from, to);
}

/** three.js' frame (Y up, -Z Blender's +Y) → Blender's: the inverse of `toThree`. */
export function fromThree(p: V3): V3 {
  return [p[0], -p[2], p[1]];
}

/** A nudge of any size as steps `bike_nudge` takes (each axis at most half a metre). */
export function nudgeSteps(delta: V3): V3[] {
  const n = Math.max(1, Math.ceil(Math.max(...delta.map((d) => Math.abs(d))) / 0.5 - 1e-9));
  return Array.from({ length: n }, () => delta.map((d) => d / n) as V3);
}
