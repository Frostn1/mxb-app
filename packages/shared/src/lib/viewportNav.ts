import { useSyncExternalStore } from "react";
import * as THREE from "three";

/**
 * Blender-style navigation for the 3D viewports.
 *
 * One controller drives every viewport: the Bike tab's plain three.js preview and the
 * react-three-fiber viewers (model, rider, pose, track). It sits on top of the
 * `OrbitControls` each of them already has — the controls still own touch input, keep the
 * camera looking at `target`, and clamp distance and pitch — and takes over the mouse,
 * the wheel and the keyboard:
 *
 * | Blender style            | Classic style (what the viewers always did) |
 * |--------------------------|---------------------------------------------|
 * | MMB drag: orbit          | LMB drag: orbit                             |
 * | Shift + MMB: pan         | RMB drag: pan                               |
 * | Ctrl + MMB / wheel: zoom | MMB drag / wheel: zoom                      |
 * | Alt + LMB: as MMB        |                                             |
 *
 * Both styles get the views: numpad 1 / 3 / 7 (Ctrl for the opposite side), 5 for
 * ortho/perspective, `.` to frame the selection, Home to frame everything, and the
 * backtick for a pie with the same choices for a keyboard without a numpad.
 *
 * The frame is the viewers' own: +Y up, +Z the front of the bike. Blender's axes map onto
 * it the way a model exported from Blender lands in three.js — Blender X is X, Blender Y is
 * −Z, Blender Z is Y — so "front" here is what numpad 1 shows in Blender.
 *
 * Orbiting is Blender's turntable: a horizontal drag turns about the vertical, a vertical drag
 * tilts with no stop at the poles, so the view goes over the top and keeps going. The camera's
 * own `up` follows the turn, so OrbitControls (which aims the camera with `lookAt`) keeps the
 * view the right way round past the pole instead of flipping it. A viewer that does want a
 * stop (the track, which has a ground to stay above) sets OrbitControls' polar limits, and the
 * orbit then holds the pitch there, as it always did.
 */

// ---------------------------------------------------------------------------------------
// The setting

export type NavStyle = "blender" | "classic";

/** localStorage key holding the navigation style: `"blender"` or `"classic"`. */
export const NAV_STYLE_KEY = "frost-viewport-nav";
const NAV_STYLE_EVENT = "frost-viewport-nav-change";

let defaultStyle: NavStyle = "classic";

/**
 * What a viewer uses before anyone has picked. Each app says it once at startup: Studio is
 * a modelling tool and defaults to Blender; the mod manager and Coach keep the drag-to-turn
 * their users already know.
 */
export function setDefaultNavStyle(style: NavStyle) {
  defaultStyle = style;
}

export function readNavStyle(): NavStyle {
  try {
    const v = localStorage.getItem(NAV_STYLE_KEY);
    if (v === "blender" || v === "classic") return v;
  } catch {
    /* storage blocked — the default it is */
  }
  return defaultStyle;
}

export function writeNavStyle(style: NavStyle) {
  try {
    localStorage.setItem(NAV_STYLE_KEY, style);
  } catch {
    /* storage blocked — the pick just won't outlive the session */
  }
  if (typeof window !== "undefined") window.dispatchEvent(new Event(NAV_STYLE_EVENT));
}

/** Called whenever the style changes, here or in another window. */
export function subscribeNavStyle(fn: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const onStorage = (e: StorageEvent) => {
    if (e.key === NAV_STYLE_KEY) fn();
  };
  window.addEventListener(NAV_STYLE_EVENT, fn);
  window.addEventListener("storage", onStorage);
  return () => {
    window.removeEventListener(NAV_STYLE_EVENT, fn);
    window.removeEventListener("storage", onStorage);
  };
}

/** The navigation style and a setter — for the Settings page and the viewers' hints. */
export function useNavStyle(): [NavStyle, (style: NavStyle) => void] {
  const style = useSyncExternalStore(subscribeNavStyle, readNavStyle, readNavStyle);
  return [style, writeNavStyle];
}

// ---------------------------------------------------------------------------------------
// Input mapping (pure, so the tests drive exactly what the handlers do)

export type ViewName = "front" | "back" | "right" | "left" | "top" | "bottom";

/** Where the camera sits for each view, as a direction from what it looks at. */
export const VIEW_DIRS: Record<ViewName, readonly [number, number, number]> = {
  front: [0, 0, 1],
  back: [0, 0, -1],
  right: [1, 0, 0],
  left: [-1, 0, 0],
  top: [0, 1, 0],
  bottom: [0, -1, 0],
};

/**
 * Screen-up of the top and bottom views, as Blender has them: Blender's +Y (our −Z) at the
 * top of the top view, and Blender's −Y (our +Z) at the top of the bottom one. The side views
 * are world-up.
 */
const VIEW_UP: Partial<Record<ViewName, THREE.Vector3>> = {
  top: new THREE.Vector3(0, 0, -1),
  bottom: new THREE.Vector3(0, 0, 1),
};

/** The camera's turn for a view: looking along −`VIEW_DIRS[name]`, Blender's way up. */
export function viewQuaternion(name: ViewName): THREE.Quaternion {
  const dir = new THREE.Vector3(...VIEW_DIRS[name]);
  const up = VIEW_UP[name] ?? new THREE.Vector3(0, 1, 0);
  const m = new THREE.Matrix4().lookAt(dir, new THREE.Vector3(), up);
  return new THREE.Quaternion().setFromRotationMatrix(m);
}

export const OPPOSITE: Record<ViewName, ViewName> = {
  front: "back",
  back: "front",
  right: "left",
  left: "right",
  top: "bottom",
  bottom: "top",
};

export type NavCommand =
  | { kind: "view"; view: ViewName }
  | { kind: "toggleOrtho" }
  | { kind: "frame"; what: "selected" | "all" }
  | { kind: "orbitStep"; yaw: number; pitch: number }
  | { kind: "opposite" }
  | { kind: "pie" };

export interface KeyLike {
  code: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}

/** Degrees a numpad 2/4/6/8 press turns the view, as in Blender. */
export const ORBIT_STEP = 15;

/**
 * The command a key press means, or null for a key the viewport leaves alone.
 *
 * Read off `code`, not `key`: with Num Lock off the numpad reports Home/End/arrows as its
 * `key`, but its `code` is still `Numpad7`, and Blender doesn't care about Num Lock either.
 */
export function keyCommand(e: KeyLike): NavCommand | null {
  const ctrl = !!(e.ctrlKey || e.metaKey);
  if (e.altKey || e.shiftKey) return null;
  switch (e.code) {
    case "Numpad1":
      return { kind: "view", view: ctrl ? "back" : "front" };
    case "Numpad3":
      return { kind: "view", view: ctrl ? "left" : "right" };
    case "Numpad7":
      return { kind: "view", view: ctrl ? "bottom" : "top" };
    case "Numpad5":
      return ctrl ? null : { kind: "toggleOrtho" };
    case "Numpad9":
      return ctrl ? null : { kind: "opposite" };
    case "NumpadDecimal":
      return ctrl ? null : { kind: "frame", what: "selected" };
    case "Home":
      return ctrl ? null : { kind: "frame", what: "all" };
    case "Numpad4":
      return ctrl ? null : { kind: "orbitStep", yaw: -ORBIT_STEP, pitch: 0 };
    case "Numpad6":
      return ctrl ? null : { kind: "orbitStep", yaw: ORBIT_STEP, pitch: 0 };
    case "Numpad8":
      return ctrl ? null : { kind: "orbitStep", yaw: 0, pitch: ORBIT_STEP };
    case "Numpad2":
      return ctrl ? null : { kind: "orbitStep", yaw: 0, pitch: -ORBIT_STEP };
    case "Backquote":
      return ctrl ? null : { kind: "pie" };
    default:
      return null;
  }
}

/** A text field, select or editable element has the keys — the viewport must not take them. */
export function isEditableTarget(t: EventTarget | null | undefined): boolean {
  const el = t as (HTMLElement & { isContentEditable?: boolean }) | null | undefined;
  if (!el || typeof el !== "object" || !("tagName" in el)) return false;
  const tag = String(el.tagName).toUpperCase();
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag === "INPUT") {
    const type = String((el as HTMLInputElement).type ?? "text").toLowerCase();
    // A checkbox or a slider holds no text; arrows on a slider aren't ours anyway.
    return !["checkbox", "radio", "button", "submit", "reset", "range", "color", "file"].includes(type);
  }
  return !!el.isContentEditable;
}

export type MouseAction = "orbit" | "pan" | "zoom";

export interface MouseLike {
  button: number;
  shiftKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  metaKey?: boolean;
}

let leftOrbitsInBlender = false;

/**
 * Let a plain left drag orbit in Blender style too, for an app whose views select nothing
 * (MXB Cinema frames a shot; there is nothing to click on).
 */
export function setLeftOrbitsInBlender(on: boolean) {
  leftOrbitsInBlender = on;
}

/** What a mouse drag does in a style. Left mouse in Blender style is select: null. */
export function mouseAction(style: NavStyle, e: MouseLike): MouseAction | null {
  const ctrl = !!(e.ctrlKey || e.metaKey);
  if (style === "blender") {
    if (leftOrbitsInBlender && e.button === 0 && !e.altKey && !ctrl && !e.shiftKey) return "orbit";
    // Alt + left mouse is Blender's "emulate 3 button mouse", for a laptop's touchpad.
    const middle = e.button === 1 || (e.button === 0 && !!e.altKey);
    if (!middle) return null;
    if (ctrl) return "zoom";
    if (e.shiftKey) return "pan";
    return "orbit";
  }
  if (e.button === 0) return ctrl || e.shiftKey ? "pan" : "orbit";
  if (e.button === 1) return "zoom";
  if (e.button === 2) return "pan";
  return null;
}

/**
 * Whether the controller handles this press itself rather than leaving it to OrbitControls.
 *
 * Classic style in perspective leaves pan and zoom to OrbitControls, as they always were. Ortho
 * is drawn by this controller, so in ortho it takes the pan and zoom drags in either style.
 * Orbiting is always this controller's, since OrbitControls stops at the poles; a classic left
 * press is still passed on to the scene, so a pose handle can take it (see `sharedPress`).
 */
export function takesMouse(style: NavStyle, ortho: boolean, e: MouseLike): MouseAction | null {
  const action = mouseAction(style, e);
  if (!action) return null;
  if (style === "blender" || action === "orbit") return action;
  return ortho ? action : null;
}

/** A classic left drag: it orbits, but the press still reaches the scene, which may claim it. */
function sharedPress(style: NavStyle, e: MouseLike): boolean {
  return style === "classic" && e.button === 0 && mouseAction(style, e) === "orbit";
}

/** Whether the controller handles the wheel itself (see {@link takesMouse}). */
export function takesWheel(style: NavStyle, ortho: boolean): boolean {
  return style === "blender" || ortho;
}

/** The zoom factor (distance multiplier; < 1 is closer) for a wheel event's deltaY. */
export function wheelFactor(deltaY: number, deltaMode = 0): number {
  // Lines and pages to pixels, roughly: a notch of a mouse wheel is ~100px either way.
  const px = deltaMode === 1 ? deltaY * 33 : deltaMode === 2 ? deltaY * 400 : deltaY;
  const notches = Math.max(-5, Math.min(5, px / 100));
  return Math.pow(1.12, notches);
}

/** The zoom factor for a Ctrl+MMB drag: up is in, down is out. */
export function dragZoomFactor(dy: number): number {
  return Math.pow(1.006, dy);
}

/**
 * How far back a camera with this vertical fov and aspect has to stand for a sphere of
 * `radius` to fill the view without touching its edges.
 */
export function fitDistance(radius: number, fovDeg: number, aspect: number): number {
  const half = THREE.MathUtils.degToRad(fovDeg) / 2;
  const horiz = Math.atan(Math.tan(half) * aspect);
  return (radius * 1.08) / Math.sin(Math.min(half, horiz));
}

/** The pie's slots, clockwise from the top — Blender's View pie, near enough. */
export const PIE_ITEMS: readonly { id: ViewName | "frameSelected" | "frameAll"; angle: number }[] = [
  { id: "top", angle: -90 },
  { id: "front", angle: -45 },
  { id: "right", angle: 0 },
  { id: "frameAll", angle: 45 },
  { id: "bottom", angle: 90 },
  { id: "frameSelected", angle: 135 },
  { id: "left", angle: 180 },
  { id: "back", angle: -135 },
];

/** The pie slot a pointer offset from the pie's centre points at, or null near the centre. */
export function pieItemAt(dx: number, dy: number, deadZone = 24) {
  if (Math.hypot(dx, dy) < deadZone) return null;
  const a = (Math.atan2(dy, dx) * 180) / Math.PI;
  let best = PIE_ITEMS[0];
  let bestD = Infinity;
  for (const it of PIE_ITEMS) {
    const d = Math.abs(((a - it.angle + 540) % 360) - 180);
    if (d < bestD) {
      bestD = d;
      best = it;
    }
  }
  return best;
}

// ---------------------------------------------------------------------------------------
// The controller

/** The parts of three's (and three-stdlib's) OrbitControls this needs. */
export interface OrbitLike {
  target: THREE.Vector3;
  update: () => unknown;
  enabled: boolean;
  minDistance: number;
  maxDistance: number;
  minPolarAngle: number;
  maxPolarAngle: number;
  mouseButtons: { LEFT?: number | null; MIDDLE?: number | null; RIGHT?: number | null };
  addEventListener?: (type: "change", fn: () => void) => void;
  removeEventListener?: (type: "change", fn: () => void) => void;
}

export interface ViewportNavOptions {
  camera: THREE.PerspectiveCamera;
  controls: OrbitLike;
  /** The element the viewport fills. Input is read off it; keys only while it's hovered. */
  element: HTMLElement;
  /** Draw a frame — the plain three.js preview has no loop to notice a move. */
  onChange?: () => void;
  /** The selected item's centre to orbit about, or null to orbit about the view's centre. */
  getPivot?: () => THREE.Vector3 | null;
  /** The box to frame: the selection, or everything. Null when there is nothing. */
  getFrameBox?: (what: "selected" | "all") => THREE.Box3 | null;
  /** Milliseconds a view change glides over; 0 snaps (tests). */
  smoothMs?: number;
  /** Start in this style instead of the stored one (tests). */
  style?: NavStyle;
  /** Listen to the stored style (default true). */
  followSetting?: boolean;
}

interface CamState {
  target: THREE.Vector3;
  /** The camera's turn; it looks along its own −Z, so it stands along +Z of it from the target. */
  quat: THREE.Quaternion;
  dist: number;
  zoom: number;
}

const UP = new THREE.Vector3(0, 1, 0);

/** Whether OrbitControls has polar limits: a stop the orbit has to keep (see the header). */
function hasPoleStop(c: OrbitLike): boolean {
  return c.minPolarAngle > 1e-6 || c.maxPolarAngle < Math.PI - 1e-6;
}

/**
 * One turntable step: the camera's turn after `yaw` about the world vertical and `pitch` about
 * its own right, both radians. Pure, so the tests drive it directly.
 */
export function turntable(quat: THREE.Quaternion, yaw: number, pitch: number): THREE.Quaternion {
  const right = new THREE.Vector3(1, 0, 0).applyQuaternion(quat);
  const qYaw = new THREE.Quaternion().setFromAxisAngle(UP, yaw);
  const qPitch = new THREE.Quaternion().setFromAxisAngle(right, pitch);
  return qYaw.multiply(qPitch).multiply(quat).normalize();
}

interface OrthoSave {
  near: number;
  far: number;
  zoom: number;
  halfH: number;
  lastLRTB: [number, number, number, number] | null;
}

type OrthoCamera = THREE.PerspectiveCamera & {
  left?: number;
  right?: number;
  top?: number;
  bottom?: number;
  isOrthographicCamera?: boolean;
};

/** Pie state for the overlay: where it opened, relative to the element. */
export interface PieState {
  x: number;
  y: number;
}

export class ViewportNav {
  readonly camera: THREE.PerspectiveCamera;
  readonly controls: OrbitLike;
  readonly element: HTMLElement;
  style: NavStyle;
  pie: PieState | null = null;

  private opts: ViewportNavOptions;
  private listeners = new Set<() => void>();
  private ortho: OrthoSave | null = null;
  /** Ortho was switched on by snapping to a view; orbiting away switches it back off. */
  private autoOrtho = false;
  private savedButtons: OrbitLike["mouseButtons"];
  private hovered = false;
  private pointer = { x: 0, y: 0 };
  /**
   * The drag under way. `shared`: a classic left press, also handed to the scene; it becomes
   * an orbit once it moves with the controls still on (a pose handle switches them off to take
   * the drag for itself). `reverse`: the view was upside down when it began, as Blender reads
   * it, so a sideways drag keeps turning the way the hand goes.
   */
  private drag: {
    action: MouseAction;
    id: number;
    x: number;
    y: number;
    shared: boolean;
    started: boolean;
    reverse: boolean;
  } | null = null;
  private anim = 0;
  /** The view a glide is heading for, so a second click mid-glide still flips. */
  private goal: ViewName | null = null;
  private offs: (() => void)[] = [];

  constructor(opts: ViewportNavOptions) {
    this.opts = opts;
    this.camera = opts.camera;
    this.controls = opts.controls;
    this.element = opts.element;
    this.savedButtons = { ...opts.controls.mouseButtons };
    this.style = opts.style ?? readNavStyle();
    this.applyStyle();
    this.listen();
  }

  // ---- observers ------------------------------------------------------------------------

  /** Called after every camera move, style or ortho change. */
  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private emit() {
    for (const fn of this.listeners) fn();
  }

  /** Commit a camera move: let the controls settle it, redraw, tell the overlay. */
  private changed() {
    this.controls.update();
    this.camera.updateMatrixWorld();
    this.opts.onChange?.();
    this.emit();
  }

  get isOrtho(): boolean {
    return this.ortho !== null;
  }

  setStyle(style: NavStyle) {
    if (style === this.style) return;
    this.style = style;
    this.applyStyle();
    this.emit();
  }

  private applyStyle() {
    // Blender style takes every mouse button itself; left is the viewer's own (select).
    // Classic hands OrbitControls back the buttons it was built with, all but the orbit,
    // which is this controller's: OrbitControls would stop it at the poles.
    const mb = this.controls.mouseButtons;
    if (this.style === "blender") {
      mb.LEFT = -1;
      mb.MIDDLE = -1;
      mb.RIGHT = -1;
    } else {
      mb.LEFT = -1;
      mb.MIDDLE = this.savedButtons.MIDDLE;
      mb.RIGHT = this.savedButtons.RIGHT;
    }
  }

  // ---- reading the camera -----------------------------------------------------------------

  private state(): CamState {
    const target = this.controls.target.clone();
    const dist = this.camera.position.distanceTo(target) || 1;
    return { target, quat: this.camera.quaternion.clone(), dist, zoom: this.camera.zoom };
  }

  /**
   * Put the camera where a state says. Its `up` is set to the turn's own, so OrbitControls'
   * `lookAt` on the next update leaves the turn exactly as it is, upside down or not.
   */
  private apply(s: CamState) {
    this.controls.target.copy(s.target);
    const back = new THREE.Vector3(0, 0, 1).applyQuaternion(s.quat);
    this.camera.position.copy(s.target).addScaledVector(back, s.dist);
    this.camera.quaternion.copy(s.quat);
    this.camera.up.copy(UP).applyQuaternion(s.quat);
    if (this.ortho && this.camera.zoom !== s.zoom) {
      this.camera.zoom = s.zoom;
      this.camera.updateProjectionMatrix();
    }
  }

  /**
   * The view an axis-aligned camera is in, or null between views. Top and bottom also need
   * Blender's way up on screen: turned about the vertical, they're a user view, as in Blender.
   */
  currentView(): ViewName | null {
    const d = this.camera.position.clone().sub(this.controls.target).normalize();
    for (const name of Object.keys(VIEW_DIRS) as ViewName[]) {
      const v = new THREE.Vector3(...VIEW_DIRS[name]).normalize();
      if (d.dot(v) <= 0.9995) continue;
      const up = VIEW_UP[name];
      if (up && new THREE.Vector3(0, 1, 0).applyQuaternion(this.camera.quaternion).dot(up) <= 0.9995) return null;
      return name;
    }
    return null;
  }

  /** World units per screen pixel at the target, for panning. */
  private unitsPerPixel(): number {
    const h = this.element.clientHeight || 1;
    if (this.ortho) return (2 * this.ortho.halfH) / this.camera.zoom / h;
    const dist = this.camera.position.distanceTo(this.controls.target);
    return (2 * dist * Math.tan(THREE.MathUtils.degToRad(this.camera.fov) / 2)) / h;
  }

  // ---- moves ------------------------------------------------------------------------------

  /** Past a pole: the world's up points down the screen. */
  get upsideDown(): boolean {
    return new THREE.Vector3(0, 1, 0).applyQuaternion(this.camera.quaternion).y < 0;
  }

  /**
   * Orbit by a mouse drag of (dx, dy) pixels, about the selection if there is one. `reverse`
   * turns the sideways part the other way, for a drag that began upside down.
   */
  orbit(dx: number, dy: number, reverse = false) {
    const h = this.element.clientHeight || 1;
    this.rotate(((reverse ? 2 : -2) * Math.PI * dx) / h, (-2 * Math.PI * dy) / h);
  }

  /**
   * Turn the view, Blender's turntable: yaw about the world vertical, pitch about the
   * camera's right; radians. No stop at the poles unless OrbitControls has polar limits.
   */
  rotate(yaw: number, pitch: number, pivot: THREE.Vector3 | null = this.opts.getPivot?.() ?? null) {
    this.stopAnim();
    // Blender's auto perspective: a view snapped into ortho goes back to perspective as soon
    // as you orbit out of it.
    if (this.autoOrtho) this.setOrtho(false);
    const target = this.controls.target;
    const p = pivot ?? target;
    const before = this.camera.quaternion.clone();
    let quat = turntable(before, yaw, pitch);
    if (hasPoleStop(this.controls)) {
      // Past the stop OrbitControls would clamp the camera but not the target, which makes the
      // view lurch. Drop the pitch there and keep the yaw.
      const back = new THREE.Vector3(0, 0, 1).applyQuaternion(quat);
      const polar = Math.acos(THREE.MathUtils.clamp(back.y, -1, 1));
      const min = Math.max(this.controls.minPolarAngle, 0) + 1e-3;
      const max = Math.min(this.controls.maxPolarAngle, Math.PI) - 1e-3;
      if (polar < min || polar > max) quat = turntable(before, yaw, 0);
    }
    // The whole camera turns about the pivot: where it stands, what it looks at, which way is up.
    const q = quat.clone().multiply(before.clone().invert());
    this.camera.position.sub(p).applyQuaternion(q).add(p);
    target.sub(p).applyQuaternion(q).add(p);
    this.camera.quaternion.copy(quat);
    this.camera.up.copy(UP).applyQuaternion(quat);
    this.changed();
  }

  /** Pan by a mouse drag of (dx, dy) pixels: the scene follows the pointer. */
  pan(dx: number, dy: number) {
    this.stopAnim();
    const k = this.unitsPerPixel();
    const right = new THREE.Vector3(1, 0, 0).applyQuaternion(this.camera.quaternion);
    const up = new THREE.Vector3(0, 1, 0).applyQuaternion(this.camera.quaternion);
    const delta = right.multiplyScalar(-dx * k).addScaledVector(up, dy * k);
    this.camera.position.add(delta);
    this.controls.target.add(delta);
    this.changed();
  }

  /** Zoom: `factor` < 1 comes closer. Perspective dollies; ortho scales. */
  zoomBy(factor: number) {
    this.stopAnim();
    if (!(factor > 0) || !Number.isFinite(factor)) return;
    if (this.ortho) {
      this.camera.zoom = THREE.MathUtils.clamp(this.camera.zoom / factor, 1e-3, 1e4);
      this.camera.updateProjectionMatrix();
    } else {
      const s = this.state();
      s.dist = THREE.MathUtils.clamp(s.dist * factor, this.controls.minDistance, this.controls.maxDistance);
      this.apply(s);
    }
    this.changed();
  }

  /** Look along one of the six axes, keeping the view's centre and distance. */
  view(name: ViewName) {
    const s = this.state();
    const to = { ...s, target: s.target.clone(), quat: viewQuaternion(name) };
    // Blender's auto perspective: an axis view is an ortho view.
    if (!this.ortho) {
      this.setOrtho(true);
      this.autoOrtho = true;
      to.zoom = this.camera.zoom;
    }
    this.glide(to);
    this.goal = name;
  }

  /** The gizmo's click: that axis, or its opposite when the view is already on it. */
  viewAxis(name: ViewName) {
    const cur = this.anim ? this.goal : this.currentView();
    this.view(cur === name ? OPPOSITE[name] : name);
  }

  /** Look from the other side (numpad 9). */
  opposite() {
    const cur = this.currentView();
    if (cur) return this.view(OPPOSITE[cur]);
    this.rotate(Math.PI, 0, null);
  }

  /** Frame the selection (or everything, when nothing is selected). */
  frame(what: "selected" | "all") {
    const box =
      (what === "selected" ? this.opts.getFrameBox?.("selected") : null) ?? this.opts.getFrameBox?.("all") ?? null;
    if (!box || box.isEmpty()) return;
    const center = box.getCenter(new THREE.Vector3());
    const radius = Math.max(box.getSize(new THREE.Vector3()).length() / 2, 1e-3);
    const s = this.state();
    const to: CamState = { target: center, quat: s.quat.clone(), dist: s.dist, zoom: s.zoom };
    if (this.ortho) {
      const aspect = this.camera.aspect || 1;
      const need = (aspect >= 1 ? radius : radius / aspect) * 1.08;
      to.zoom = this.ortho.halfH / need;
    } else {
      to.dist = THREE.MathUtils.clamp(
        fitDistance(radius, this.camera.fov, this.camera.aspect || 1),
        this.controls.minDistance,
        this.controls.maxDistance,
      );
    }
    this.glide(to);
  }

  run(cmd: NavCommand) {
    switch (cmd.kind) {
      case "view":
        return this.view(cmd.view);
      case "toggleOrtho":
        this.autoOrtho = false;
        return this.setOrtho(!this.ortho);
      case "frame":
        return this.frame(cmd.what);
      case "orbitStep":
        return this.rotate(
          THREE.MathUtils.degToRad(cmd.yaw),
          THREE.MathUtils.degToRad(cmd.pitch),
          null,
        );
      case "opposite":
        return this.opposite();
      case "pie":
        return this.openPie();
    }
  }

  openPie(at: PieState = this.pointer) {
    this.pie = { x: at.x, y: at.y };
    this.emit();
  }

  closePie() {
    if (!this.pie) return;
    this.pie = null;
    this.emit();
  }

  /** A pie slot was chosen. */
  pick(id: (typeof PIE_ITEMS)[number]["id"]) {
    this.closePie();
    if (id === "frameAll") this.frame("all");
    else if (id === "frameSelected") this.frame("selected");
    else this.view(id);
  }

  // ---- ortho ------------------------------------------------------------------------------

  /**
   * Switch between perspective and an orthographic projection of the same camera.
   *
   * The camera object stays the one the viewer created — every viewer, effect and hit test
   * holds it — and only its projection changes: sized so the plane through the target looks
   * exactly as big as it did, and flagged orthographic so three's raycaster casts parallel
   * rays and the renderer shades for an ortho view. Zoom in ortho is the camera's own
   * `zoom`; switching back turns it into distance, so nothing jumps.
   */
  setOrtho(on: boolean) {
    const cam = this.camera as OrthoCamera;
    if (on === !!this.ortho) return;
    this.stopAnim();
    if (on) {
      const dist = cam.position.distanceTo(this.controls.target);
      const halfH = (dist * Math.tan(THREE.MathUtils.degToRad(cam.fov) / 2)) / (cam.zoom || 1);
      const save: OrthoSave = { near: cam.near, far: cam.far, zoom: cam.zoom, halfH, lastLRTB: null };
      this.ortho = save;
      cam.zoom = 1;
      // Ortho has no reason to clip what's behind the eye: pan and zoom never move it.
      cam.near = -save.far;
      cam.far = save.far;
      (cam as unknown as { isPerspectiveCamera: boolean }).isPerspectiveCamera = false;
      cam.isOrthographicCamera = true;
      cam.updateProjectionMatrix = function (this: OrthoCamera) {
        // r3f sizes an "orthographic" camera through left/right/top/bottom in pixels on a
        // resize and leaves `aspect` alone; the Bike tab sets `aspect`. Take whichever moved.
        const l = this.left ?? 0;
        const r = this.right ?? 0;
        const t = this.top ?? 0;
        const b = this.bottom ?? 0;
        const last = save.lastLRTB;
        if (last && r - l > 0 && t - b > 0 && (last[0] !== l || last[1] !== r || last[2] !== t || last[3] !== b)) {
          this.aspect = (r - l) / (t - b);
        }
        const hh = save.halfH / (this.zoom || 1);
        const hw = hh * (this.aspect || 1);
        this.left = -hw;
        this.right = hw;
        this.top = hh;
        this.bottom = -hh;
        save.lastLRTB = [-hw, hw, hh, -hh];
        this.projectionMatrix.makeOrthographic(-hw, hw, hh, -hh, this.near, this.far);
        this.projectionMatrixInverse.copy(this.projectionMatrix).invert();
      };
      cam.updateProjectionMatrix();
    } else {
      const save = this.ortho!;
      const halfH = save.halfH / (cam.zoom || 1);
      this.ortho = null;
      this.autoOrtho = false;
      delete (cam as Partial<OrthoCamera>).updateProjectionMatrix;
      delete cam.isOrthographicCamera;
      delete cam.left;
      delete cam.right;
      delete cam.top;
      delete cam.bottom;
      (cam as unknown as { isPerspectiveCamera: boolean }).isPerspectiveCamera = true;
      cam.near = save.near;
      cam.far = save.far;
      cam.zoom = save.zoom;
      cam.updateProjectionMatrix();
      const s = this.state();
      s.dist = THREE.MathUtils.clamp(
        (halfH * (save.zoom || 1)) / Math.tan(THREE.MathUtils.degToRad(cam.fov) / 2),
        this.controls.minDistance,
        this.controls.maxDistance,
      );
      this.apply(s);
    }
    this.changed();
  }

  // ---- gliding between views --------------------------------------------------------------

  private stopAnim() {
    if (this.anim) cancelAnimationFrame(this.anim);
    this.anim = 0;
    this.goal = null;
  }

  private glide(to: CamState) {
    this.stopAnim();
    const from = this.state();
    // Blender's smooth view takes about a quarter of a second.
    const ms = this.opts.smoothMs ?? 250;
    if (ms <= 0 || typeof requestAnimationFrame === "undefined") {
      this.apply(to);
      this.changed();
      return;
    }
    const start = performance.now();
    const step = () => {
      const t = Math.min(1, (performance.now() - start) / ms);
      const e = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
      if (t >= 1) {
        this.anim = 0;
        this.goal = null;
        this.apply(to);
      } else {
        this.apply({
          target: from.target.clone().lerp(to.target, e),
          // The turn itself, roll and all: a glide out of an upside-down view rights it.
          quat: from.quat.clone().slerp(to.quat, e),
          dist: from.dist * Math.pow(to.dist / from.dist, e),
          zoom: from.zoom * Math.pow(to.zoom / from.zoom, e),
        });
        this.anim = requestAnimationFrame(step);
      }
      this.changed();
    };
    this.anim = requestAnimationFrame(step);
  }

  // ---- input ------------------------------------------------------------------------------

  private listen() {
    const el = this.element;
    const on = <K extends keyof HTMLElementEventMap>(
      target: HTMLElement | Window,
      type: K,
      fn: (e: HTMLElementEventMap[K]) => void,
      opts?: AddEventListenerOptions,
    ) => {
      target.addEventListener(type, fn as EventListener, opts);
      this.offs.push(() => target.removeEventListener(type, fn as EventListener, opts));
    };
    const local = (e: { clientX: number; clientY: number }) => {
      const r = el.getBoundingClientRect();
      return { x: e.clientX - r.left, y: e.clientY - r.top };
    };

    on(el, "pointerenter", () => (this.hovered = true));
    on(el, "pointerleave", () => (this.hovered = false));
    on(el, "pointermove", (e) => {
      this.hovered = true;
      this.pointer = local(e);
    });

    // Capture, on the element around the canvas: this runs before OrbitControls (on the
    // canvas) and before react-three-fiber's handlers, so a press taken here reaches neither.
    on(
      el,
      "pointerdown",
      (e) => {
        if (e.pointerType !== "mouse" || !this.controls.enabled) return;
        // A click outside the pie's buttons dismisses it.
        if (this.pie && !(e.target as Element | null)?.closest?.("[data-nav-pie]")) this.closePie();
        const action = takesMouse(this.style, !!this.ortho, e);
        if (!action) return;
        const shared = sharedPress(this.style, e);
        const drag = { action, id: e.pointerId, x: e.clientX, y: e.clientY, shared, started: false, reverse: this.upsideDown };
        this.drag = drag;
        // A shared press still goes on to the scene; it's only claimed once it moves.
        if (!shared) {
          e.preventDefault();
          e.stopPropagation();
          this.claim(e.pointerId, drag);
        }
      },
      { capture: true },
    );
    // The middle button's default is autoscroll; the right's is a context menu after a drag.
    on(
      el,
      "mousedown",
      (e) => {
        if (e.button === 1) e.preventDefault();
      },
      { capture: true },
    );
    on(
      el,
      "contextmenu",
      (e) => {
        if (this.style === "classic" && this.ortho) e.preventDefault();
      },
      { capture: true },
    );
    on(
      el,
      "pointermove",
      (e) => {
        const d = this.drag;
        if (!d || e.pointerId !== d.id) return;
        const dx = e.clientX - d.x;
        const dy = e.clientY - d.y;
        if (!d.started) {
          // A pose handle took the press (and switched the controls off): the drag is its.
          if (!this.controls.enabled) {
            this.drag = null;
            return;
          }
          if (dx === 0 && dy === 0) return;
          this.claim(e.pointerId, d);
        }
        e.stopPropagation();
        d.x = e.clientX;
        d.y = e.clientY;
        if (d.action === "orbit") this.orbit(dx, dy, d.reverse);
        else if (d.action === "pan") this.pan(dx, dy);
        else this.zoomBy(dragZoomFactor(dy));
      },
      { capture: true },
    );
    const end = (e: PointerEvent) => {
      if (!this.drag || e.pointerId !== this.drag.id) return;
      const started = this.drag.started;
      this.drag = null;
      // An unmoved shared press was a click: the scene gets its release too.
      if (!started) return;
      e.stopPropagation();
      try {
        el.releasePointerCapture(e.pointerId);
      } catch {
        /* already released */
      }
    };
    on(el, "pointerup", end, { capture: true });
    on(el, "pointercancel", end, { capture: true });
    on(
      el,
      "wheel",
      (e) => {
        if (!this.controls.enabled || !takesWheel(this.style, !!this.ortho)) return;
        e.preventDefault();
        e.stopPropagation();
        this.zoomBy(wheelFactor(e.deltaY, e.deltaMode));
      },
      { capture: true, passive: false },
    );

    on(window, "keydown", (e) => {
      if (this.pie && e.code === "Escape") {
        e.preventDefault();
        this.closePie();
        return;
      }
      if (!this.hovered || e.repeat || isEditableTarget(e.target) || isEditableTarget(document.activeElement)) return;
      const cmd = keyCommand(e);
      if (!cmd) return;
      e.preventDefault();
      this.run(cmd);
    });

    // A classic-style left drag orbits through OrbitControls, not through `rotate`, so the
    // auto perspective has to be noticed here: a snapped view the controls turned off its axis
    // goes back to perspective, like any other orbit. Deferred so it never runs inside the
    // controls' own update.
    let checking = false;
    const onControls = () => {
      this.emit();
      if (!this.autoOrtho || checking) return;
      checking = true;
      queueMicrotask(() => {
        checking = false;
        if (this.autoOrtho && !this.anim && !this.currentView()) this.setOrtho(false);
      });
    };
    this.controls.addEventListener?.("change", onControls);
    this.offs.push(() => this.controls.removeEventListener?.("change", onControls));

    if (this.opts.followSetting !== false) {
      this.offs.push(subscribeNavStyle(() => this.setStyle(readNavStyle())));
    }
  }

  /** The drag is this controller's from here on: capture the pointer, stop the glide. */
  private claim(pointerId: number, d: NonNullable<ViewportNav["drag"]>) {
    d.started = true;
    this.stopAnim();
    try {
      this.element.setPointerCapture(pointerId);
    } catch {
      /* not capturable (synthetic) — moves still arrive while over the element */
    }
  }

  dispose() {
    this.stopAnim();
    for (const off of this.offs) off();
    this.offs = [];
    if (this.ortho) this.setOrtho(false);
    const mb = this.controls.mouseButtons;
    mb.LEFT = this.savedButtons.LEFT;
    mb.MIDDLE = this.savedButtons.MIDDLE;
    mb.RIGHT = this.savedButtons.RIGHT;
    this.listeners.clear();
  }
}
