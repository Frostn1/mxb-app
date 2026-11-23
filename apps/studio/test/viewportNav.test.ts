import { beforeEach, describe, expect, test } from "bun:test";
import * as THREE from "three";
import {
  ViewportNav,
  fitDistance,
  isEditableTarget,
  keyCommand,
  mouseAction,
  pieItemAt,
  takesMouse,
  takesWheel,
  type OrbitLike,
} from "@frost/shared/lib/viewportNav";

// No DOM under bun: a window to hang the key listener on, and an element that is an
// EventTarget with a size. The controller is driven through the same events the viewers get.
const g = globalThis as unknown as { window: EventTarget; document: { activeElement: unknown } };
g.window = new EventTarget();
g.document = { activeElement: null };

class FakeElement extends EventTarget {
  clientWidth = 800;
  clientHeight = 600;
  getBoundingClientRect() {
    return { left: 0, top: 0, width: 800, height: 600, right: 800, bottom: 600 };
  }
  setPointerCapture() {}
  releasePointerCapture() {}
}

/** Enough of OrbitControls: keeps the camera looking at the target and says it moved. */
function fakeControls(camera: THREE.PerspectiveCamera): OrbitLike & { changes: number } {
  const c = {
    target: new THREE.Vector3(),
    enabled: true,
    minDistance: 0.01,
    maxDistance: 1000,
    minPolarAngle: 0,
    maxPolarAngle: Math.PI,
    mouseButtons: { LEFT: 0, MIDDLE: 1, RIGHT: 2 } as OrbitLike["mouseButtons"],
    changes: 0,
    update() {
      camera.lookAt(c.target);
      camera.updateMatrixWorld();
      c.changes++;
    },
  };
  return c;
}

function setup(opts: { pivot?: THREE.Vector3 | null; box?: THREE.Box3 | null; style?: "blender" | "classic" } = {}) {
  const camera = new THREE.PerspectiveCamera(40, 800 / 600, 0.05, 100);
  camera.position.set(3, 2, 4);
  const controls = fakeControls(camera);
  controls.update();
  const el = new FakeElement();
  const nav = new ViewportNav({
    camera,
    controls,
    element: el as unknown as HTMLElement,
    smoothMs: 0,
    style: opts.style ?? "blender",
    followSetting: false,
    getPivot: () => opts.pivot ?? null,
    getFrameBox: () => opts.box ?? null,
  });
  return { camera, controls, el, nav };
}

function fire(target: EventTarget, type: string, props: Record<string, unknown>) {
  const e = new Event(type, { cancelable: true });
  Object.assign(e, props);
  target.dispatchEvent(e);
  return e;
}

const dirOf = (camera: THREE.PerspectiveCamera, target: THREE.Vector3) =>
  camera.position.clone().sub(target).normalize();
const screenUp = (camera: THREE.PerspectiveCamera) =>
  new THREE.Vector3(0, 1, 0).applyQuaternion(camera.quaternion);
const near = (a: THREE.Vector3, b: [number, number, number], eps = 1e-3) =>
  expect(a.distanceTo(new THREE.Vector3(...b))).toBeLessThan(eps);

function press(code: string, mods: Record<string, boolean> = {}) {
  return fire(g.window, "keydown", { code, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods });
}

beforeEach(() => {
  g.document.activeElement = null;
});

describe("key mapping", () => {
  test("numpad views, Ctrl for the other side", () => {
    expect(keyCommand({ code: "Numpad1" })).toEqual({ kind: "view", view: "front" });
    expect(keyCommand({ code: "Numpad1", ctrlKey: true })).toEqual({ kind: "view", view: "back" });
    expect(keyCommand({ code: "Numpad3" })).toEqual({ kind: "view", view: "right" });
    expect(keyCommand({ code: "Numpad3", ctrlKey: true })).toEqual({ kind: "view", view: "left" });
    expect(keyCommand({ code: "Numpad7" })).toEqual({ kind: "view", view: "top" });
    expect(keyCommand({ code: "Numpad7", ctrlKey: true })).toEqual({ kind: "view", view: "bottom" });
  });
  test("ortho, framing and the pie", () => {
    expect(keyCommand({ code: "Numpad5" })).toEqual({ kind: "toggleOrtho" });
    expect(keyCommand({ code: "NumpadDecimal" })).toEqual({ kind: "frame", what: "selected" });
    expect(keyCommand({ code: "Home" })).toEqual({ kind: "frame", what: "all" });
    expect(keyCommand({ code: "Backquote" })).toEqual({ kind: "pie" });
  });
  test("other keys and modifier combos are left alone", () => {
    expect(keyCommand({ code: "KeyA" })).toBeNull();
    expect(keyCommand({ code: "Digit1" })).toBeNull();
    expect(keyCommand({ code: "Numpad1", shiftKey: true })).toBeNull();
    expect(keyCommand({ code: "Numpad1", altKey: true })).toBeNull();
    expect(keyCommand({ code: "Backquote", ctrlKey: true })).toBeNull();
  });
  test("text fields keep their keys", () => {
    expect(isEditableTarget({ tagName: "INPUT", type: "text" } as unknown as EventTarget)).toBe(true);
    expect(isEditableTarget({ tagName: "INPUT", type: "number" } as unknown as EventTarget)).toBe(true);
    expect(isEditableTarget({ tagName: "TEXTAREA" } as unknown as EventTarget)).toBe(true);
    expect(isEditableTarget({ tagName: "DIV", isContentEditable: true } as unknown as EventTarget)).toBe(true);
    expect(isEditableTarget({ tagName: "INPUT", type: "checkbox" } as unknown as EventTarget)).toBe(false);
    expect(isEditableTarget({ tagName: "DIV" } as unknown as EventTarget)).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});

describe("mouse mapping", () => {
  test("Blender: middle orbits, Shift pans, Ctrl zooms, left selects", () => {
    expect(mouseAction("blender", { button: 1 })).toBe("orbit");
    expect(mouseAction("blender", { button: 1, shiftKey: true })).toBe("pan");
    expect(mouseAction("blender", { button: 1, ctrlKey: true })).toBe("zoom");
    expect(mouseAction("blender", { button: 0 })).toBeNull();
    expect(mouseAction("blender", { button: 2 })).toBeNull();
    // Emulate 3 button mouse.
    expect(mouseAction("blender", { button: 0, altKey: true })).toBe("orbit");
    expect(mouseAction("blender", { button: 0, altKey: true, shiftKey: true })).toBe("pan");
  });
  test("Classic: left orbits, right pans, middle zooms; the orbit is always ours, pan and zoom OrbitControls' in perspective", () => {
    expect(mouseAction("classic", { button: 0 })).toBe("orbit");
    expect(mouseAction("classic", { button: 2 })).toBe("pan");
    expect(mouseAction("classic", { button: 1 })).toBe("zoom");
    expect(takesMouse("classic", false, { button: 2 })).toBeNull();
    expect(takesWheel("classic", false)).toBe(false);
    expect(takesMouse("classic", true, { button: 2 })).toBe("pan");
    // OrbitControls stops at the poles, so the orbit is never its.
    expect(takesMouse("classic", true, { button: 0 })).toBe("orbit");
    expect(takesMouse("classic", false, { button: 0 })).toBe("orbit");
    expect(takesWheel("blender", false)).toBe(true);
  });
  test("Blender style takes the buttons off OrbitControls; classic gives back all but the orbit", () => {
    const { nav, controls } = setup();
    expect(controls.mouseButtons).toEqual({ LEFT: -1, MIDDLE: -1, RIGHT: -1 });
    nav.setStyle("classic");
    expect(controls.mouseButtons).toEqual({ LEFT: -1, MIDDLE: 1, RIGHT: 2 });
    nav.dispose();
    expect(controls.mouseButtons).toEqual({ LEFT: 0, MIDDLE: 1, RIGHT: 2 });
  });
});

describe("views", () => {
  test("numpad 1 / 3 / 7 look from the front, right and top, Blender's way up", () => {
    const { nav, el, camera, controls } = setup();
    fire(el, "pointermove", { clientX: 10, clientY: 10, pointerId: 1 });
    press("Numpad1");
    near(dirOf(camera, controls.target), [0, 0, 1]);
    near(screenUp(camera), [0, 1, 0]);
    press("Numpad3");
    near(dirOf(camera, controls.target), [1, 0, 0]);
    press("Numpad7");
    near(dirOf(camera, controls.target), [0, 1, 0]);
    // Blender's +Y (our −Z) at the top of the screen.
    near(screenUp(camera), [0, 0, -1]);
    press("Numpad1", { ctrlKey: true });
    near(dirOf(camera, controls.target), [0, 0, -1]);
    press("Numpad3", { ctrlKey: true });
    near(dirOf(camera, controls.target), [-1, 0, 0]);
    press("Numpad7", { ctrlKey: true });
    near(dirOf(camera, controls.target), [0, -1, 0]);
    near(screenUp(camera), [0, 0, 1]);
    nav.dispose();
  });
  test("a view keeps the distance and goes ortho; orbiting goes back to perspective", () => {
    const { nav, el, camera, controls } = setup();
    const d = camera.position.distanceTo(controls.target);
    fire(el, "pointermove", { clientX: 10, clientY: 10, pointerId: 1 });
    press("Numpad1");
    expect(camera.position.distanceTo(controls.target)).toBeCloseTo(d, 5);
    expect(nav.isOrtho).toBe(true);
    expect((camera as unknown as { isOrthographicCamera: boolean }).isOrthographicCamera).toBe(true);
    nav.orbit(30, 0);
    expect(nav.isOrtho).toBe(false);
    expect(camera.isPerspectiveCamera).toBe(true);
    nav.dispose();
  });
  test("numpad 5 keeps the target plane the same size both ways", () => {
    const { nav, el, camera, controls } = setup();
    const d = camera.position.distanceTo(controls.target);
    const scale = camera.projectionMatrix.elements[5] / d;
    fire(el, "pointermove", { clientX: 10, clientY: 10, pointerId: 1 });
    press("Numpad5");
    expect(nav.isOrtho).toBe(true);
    expect(camera.projectionMatrix.elements[5]).toBeCloseTo(scale, 6);
    // Zoom in ortho, then back: the distance is what that zoom looked like.
    nav.zoomBy(0.5);
    press("Numpad5");
    expect(nav.isOrtho).toBe(false);
    expect(camera.position.distanceTo(controls.target)).toBeCloseTo(d * 0.5, 4);
    nav.dispose();
  });
  test("the gizmo flips to the far side when clicked on the view it's already in", () => {
    const { nav, camera, controls } = setup();
    nav.viewAxis("right");
    near(dirOf(camera, controls.target), [1, 0, 0]);
    nav.viewAxis("right");
    near(dirOf(camera, controls.target), [-1, 0, 0]);
    nav.dispose();
  });
  test("Home frames everything, numpad . the selection", () => {
    const box = new THREE.Box3(new THREE.Vector3(9, 0, 0), new THREE.Vector3(11, 2, 2));
    const { nav, el, camera, controls } = setup({ box });
    fire(el, "pointermove", { clientX: 10, clientY: 10, pointerId: 1 });
    press("Home");
    near(controls.target, [10, 1, 1]);
    const r = box.getSize(new THREE.Vector3()).length() / 2;
    expect(camera.position.distanceTo(controls.target)).toBeCloseTo(fitDistance(r, 40, 800 / 600), 4);
    controls.target.set(0, 0, 0);
    press("NumpadDecimal");
    near(controls.target, [10, 1, 1]);
    nav.dispose();
  });
  test("keys do nothing while typing, or with the pointer elsewhere", () => {
    const { nav, el, camera } = setup();
    const before = camera.position.clone();
    press("Numpad1"); // never hovered
    expect(camera.position.distanceTo(before)).toBe(0);
    fire(el, "pointermove", { clientX: 10, clientY: 10, pointerId: 1 });
    g.document.activeElement = { tagName: "INPUT", type: "text" };
    press("Numpad1");
    expect(camera.position.distanceTo(before)).toBe(0);
    g.document.activeElement = null;
    fire(el, "pointerleave", {});
    press("Numpad1");
    expect(camera.position.distanceTo(before)).toBe(0);
    nav.dispose();
  });
  test("backtick opens the pie at the pointer, Escape closes it", () => {
    const { nav, el } = setup();
    fire(el, "pointermove", { clientX: 120, clientY: 80, pointerId: 1 });
    press("Backquote");
    expect(nav.pie).toEqual({ x: 120, y: 80 });
    fire(g.window, "keydown", { code: "Escape" });
    expect(nav.pie).toBeNull();
    nav.openPie();
    nav.pick("top");
    expect(nav.pie).toBeNull();
    expect(nav.currentView()).toBe("top");
    nav.dispose();
  });
  test("the pie's slots by direction", () => {
    expect(pieItemAt(0, -100)?.id).toBe("top");
    expect(pieItemAt(0, 100)?.id).toBe("bottom");
    expect(pieItemAt(100, 0)?.id).toBe("right");
    expect(pieItemAt(-100, 0)?.id).toBe("left");
    expect(pieItemAt(70, -70)?.id).toBe("front");
    expect(pieItemAt(-70, -70)?.id).toBe("back");
    expect(pieItemAt(5, 5)).toBeNull();
  });
});

describe("mouse drags", () => {
  const down = (el: EventTarget, props: Record<string, unknown>) =>
    fire(el, "pointerdown", { pointerType: "mouse", pointerId: 7, clientX: 100, clientY: 100, ...props });
  const move = (el: EventTarget, x: number, y: number) =>
    fire(el, "pointermove", { pointerType: "mouse", pointerId: 7, clientX: x, clientY: y });
  const up = (el: EventTarget) => fire(el, "pointerup", { pointerType: "mouse", pointerId: 7 });

  test("middle drag orbits about the target, at the same distance", () => {
    const { nav, el, camera, controls } = setup();
    const d = camera.position.distanceTo(controls.target);
    const before = camera.position.clone();
    const e = down(el, { button: 1 });
    expect(e.defaultPrevented).toBe(true);
    move(el, 160, 120);
    up(el);
    expect(camera.position.distanceTo(before)).toBeGreaterThan(0.1);
    expect(camera.position.distanceTo(controls.target)).toBeCloseTo(d, 5);
    near(controls.target, [0, 0, 0]);
    nav.dispose();
  });
  test("orbit turns about the selection when there is one", () => {
    const pivot = new THREE.Vector3(1, 0.5, -1);
    const { nav, el, camera } = setup({ pivot });
    const d = camera.position.distanceTo(pivot);
    down(el, { button: 1 });
    move(el, 180, 100);
    up(el);
    expect(camera.position.distanceTo(pivot)).toBeCloseTo(d, 5);
    nav.dispose();
  });
  test("Shift + middle pans: camera and target move together", () => {
    const { nav, el, camera, controls } = setup();
    const off = camera.position.clone().sub(controls.target);
    down(el, { button: 1, shiftKey: true });
    move(el, 150, 100);
    up(el);
    expect(controls.target.length()).toBeGreaterThan(0.01);
    near(camera.position.clone().sub(controls.target), [off.x, off.y, off.z], 1e-6);
    nav.dispose();
  });
  test("Ctrl + middle drag up zooms in; the wheel rolled back zooms out", () => {
    const { nav, el, camera, controls } = setup();
    const d = camera.position.distanceTo(controls.target);
    down(el, { button: 1, ctrlKey: true });
    move(el, 100, 40);
    up(el);
    const d2 = camera.position.distanceTo(controls.target);
    expect(d2).toBeLessThan(d);
    fire(el, "wheel", { deltaY: 100, deltaMode: 0 });
    expect(camera.position.distanceTo(controls.target)).toBeGreaterThan(d2);
    nav.dispose();
  });
  test("left mouse is left for selecting", () => {
    const { nav, el, camera } = setup();
    const before = camera.position.clone();
    const e = down(el, { button: 0 });
    expect(e.defaultPrevented).toBe(false);
    move(el, 200, 200);
    up(el);
    expect(camera.position.distanceTo(before)).toBe(0);
    nav.dispose();
  });
});

describe("gizmo snap", () => {
  test("the six balls are Blender's views, with Blender's up on screen", () => {
    const { nav, camera, controls } = setup();
    const want: [Parameters<ViewportNav["viewAxis"]>[0], [number, number, number], [number, number, number]][] = [
      ["right", [1, 0, 0], [0, 1, 0]],
      ["left", [-1, 0, 0], [0, 1, 0]],
      // Blender +Y is our −Z: the back view looks from there.
      ["back", [0, 0, -1], [0, 1, 0]],
      ["front", [0, 0, 1], [0, 1, 0]],
      // Top: Blender +Y (our −Z) up the screen; bottom: Blender −Y (our +Z).
      ["top", [0, 1, 0], [0, 0, -1]],
      ["bottom", [0, -1, 0], [0, 0, 1]],
    ];
    for (const [view, dir, up] of want) {
      nav.view(view);
      near(dirOf(camera, controls.target), dir);
      near(screenUp(camera), up);
      expect(nav.currentView()).toBe(view);
      expect(nav.isOrtho).toBe(true);
    }
    nav.dispose();
  });
  test("a snap keeps the orbit target and distance", () => {
    const { nav, camera, controls } = setup();
    controls.target.set(1, 0.5, -2);
    controls.update();
    const d = camera.position.distanceTo(controls.target);
    nav.viewAxis("top");
    near(controls.target, [1, 0.5, -2]);
    expect(camera.position.distanceTo(controls.target)).toBeCloseTo(d, 5);
    nav.dispose();
  });
  test("an OrbitControls drag off a snapped view goes back to perspective", async () => {
    const camera = new THREE.PerspectiveCamera(40, 800 / 600, 0.05, 100);
    camera.position.set(3, 2, 4);
    const base = fakeControls(camera);
    const bus = new EventTarget();
    const controls = Object.assign(base, {
      update() {
        camera.lookAt(base.target);
        camera.updateMatrixWorld();
        bus.dispatchEvent(new Event("change"));
      },
      addEventListener: (type: "change", fn: () => void) => bus.addEventListener(type, fn),
      removeEventListener: (type: "change", fn: () => void) => bus.removeEventListener(type, fn),
    });
    controls.update();
    const nav = new ViewportNav({
      camera,
      controls,
      element: new FakeElement() as unknown as HTMLElement,
      smoothMs: 0,
      style: "classic",
      followSetting: false,
    });
    nav.viewAxis("right");
    await Promise.resolve();
    expect(nav.isOrtho).toBe(true);
    // A pan or a zoom keeps the axis, and the ortho with it.
    controls.target.y += 0.3;
    camera.position.y += 0.3;
    controls.update();
    await Promise.resolve();
    expect(nav.isOrtho).toBe(true);
    // A touch drag: OrbitControls turns the camera itself.
    camera.position.sub(controls.target).applyAxisAngle(new THREE.Vector3(0, 1, 0), 0.3).add(controls.target);
    controls.update();
    await Promise.resolve();
    expect(nav.isOrtho).toBe(false);
    expect(camera.isPerspectiveCamera).toBe(true);
    // Toggled by hand, ortho stays through an orbit, as in Blender.
    nav.run({ kind: "toggleOrtho" });
    camera.position.sub(controls.target).applyAxisAngle(new THREE.Vector3(0, 1, 0), 0.3).add(controls.target);
    controls.update();
    await Promise.resolve();
    expect(nav.isOrtho).toBe(true);
    nav.dispose();
  });
});

test("a top view turned about the vertical is a user view, so its ball snaps back to top", () => {
  const { nav } = setup();
  nav.view("top");
  expect(nav.currentView()).toBe("top");
  nav.setOrtho(false);
  nav.rotate(0.4, 0, null);
  expect(nav.currentView()).toBeNull();
  nav.viewAxis("top");
  expect(nav.currentView()).toBe("top");
  nav.dispose();
});

describe("turntable orbit", () => {
  const backOf = (q: THREE.Quaternion) => new THREE.Vector3(0, 0, 1).applyQuaternion(q);
  test("a vertical drag goes over the top and keeps going, with no jump", () => {
    const { nav, camera, controls } = setup();
    nav.view("front");
    nav.setOrtho(false);
    const d = camera.position.distanceTo(controls.target);
    // Tilt in 5 degree steps through a full turn: each step moves the view by exactly 5
    // degrees (no clamp, no flip at either pole) and the camera never rolls off the vertical
    // plane it started in.
    let prev = camera.quaternion.clone();
    for (let i = 0; i < 72; i++) {
      nav.rotate(0, THREE.MathUtils.degToRad(5), null);
      const step = THREE.MathUtils.radToDeg(prev.angleTo(camera.quaternion));
      expect(step).toBeCloseTo(5, 4);
      expect(Math.abs(camera.position.x)).toBeLessThan(1e-6);
      expect(camera.position.distanceTo(controls.target)).toBeCloseTo(d, 5);
      // The controls' lookAt agrees with the turn: `up` follows it past the pole.
      near(new THREE.Vector3(0, 1, 0).applyQuaternion(camera.quaternion), [camera.up.x, camera.up.y, camera.up.z]);
      prev = camera.quaternion.clone();
    }
    // A full turn later it is back where it started, the right way up.
    near(dirOf(camera, controls.target), [0, 0, 1]);
    near(screenUp(camera), [0, 1, 0]);
    nav.dispose();
  });
  test("past the pole the view is upside down, and a sideways drag still turns with the hand", () => {
    const { nav, camera, controls } = setup();
    nav.view("front");
    nav.setOrtho(false);
    nav.rotate(0, THREE.MathUtils.degToRad(120), null);
    expect(nav.upsideDown).toBe(true);
    expect(screenUp(camera).y).toBeLessThan(0);
    // Blender reverses the sideways turn for a drag that starts upside down, so dragging right
    // still moves what's under the cursor right. Upright, a drag right swings the camera to
    // the -X side (the model turns right); upside down, the reversed turn does the same on screen.
    const at = dirOf(camera, controls.target);
    nav.orbit(40, 0, nav.upsideDown);
    const moved = dirOf(camera, controls.target).sub(at);
    const right = new THREE.Vector3(1, 0, 0).applyQuaternion(camera.quaternion);
    expect(moved.dot(right)).toBeLessThan(0);
    nav.dispose();
  });
  test("a viewer with polar limits still stops at them", () => {
    const { nav, camera, controls } = setup();
    controls.maxPolarAngle = Math.PI / 2.05;
    nav.view("front");
    nav.setOrtho(false);
    for (let i = 0; i < 30; i++) nav.rotate(0, THREE.MathUtils.degToRad(-10), null);
    const back = dirOf(camera, controls.target);
    expect(Math.acos(back.y)).toBeLessThanOrEqual(Math.PI / 2.05 + 1e-3);
    nav.dispose();
  });
  test("snaps land exactly after an orbit past the pole, and the gizmo still flips", () => {
    const { nav, camera, controls } = setup();
    nav.view("front");
    nav.setOrtho(false);
    nav.rotate(0.7, THREE.MathUtils.degToRad(150), null);
    expect(nav.upsideDown).toBe(true);
    type V = [number, number, number];
    const snaps: ["top" | "right" | "bottom" | "back", V, V][] = [
      ["top", [0, 1, 0], [0, 0, -1]],
      ["right", [1, 0, 0], [0, 1, 0]],
      ["bottom", [0, -1, 0], [0, 0, 1]],
      ["back", [0, 0, -1], [0, 1, 0]],
    ];
    for (const [name, dir, up] of snaps) {
      nav.view(name);
      expect(nav.currentView()).toBe(name);
      near(dirOf(camera, controls.target), dir);
      near(screenUp(camera), up);
      expect(backOf(camera.quaternion).distanceTo(new THREE.Vector3(...dir))).toBeLessThan(1e-9);
    }
    nav.viewAxis("back");
    expect(nav.currentView()).toBe("front");
    nav.dispose();
  });
  test("a classic left drag orbits past the pole; a pose handle that takes the press keeps it", () => {
    const { nav, el, camera, controls } = setup({ style: "classic" });
    nav.view("front");
    nav.setOrtho(false);
    const down = fire(el, "pointerdown", { pointerType: "mouse", pointerId: 3, button: 0, clientX: 100, clientY: 100 });
    // The press goes on to the scene: a pose handle or a click must still see it.
    expect(down.defaultPrevented).toBe(false);
    // Half the viewport's height is half a turn: from the front over the top.
    fire(el, "pointermove", { pointerType: "mouse", pointerId: 3, clientX: 100, clientY: 400 });
    fire(el, "pointerup", { pointerType: "mouse", pointerId: 3 });
    near(dirOf(camera, controls.target), [0, 0, -1]);
    expect(nav.upsideDown).toBe(true);
    // A handle switches the controls off on its press: the drag is the handle's, not an orbit.
    const before = camera.quaternion.clone();
    fire(el, "pointerdown", { pointerType: "mouse", pointerId: 4, button: 0, clientX: 100, clientY: 100 });
    controls.enabled = false;
    fire(el, "pointermove", { pointerType: "mouse", pointerId: 4, clientX: 200, clientY: 300 });
    fire(el, "pointerup", { pointerType: "mouse", pointerId: 4 });
    expect(before.angleTo(camera.quaternion)).toBe(0);
    nav.dispose();
  });
});
