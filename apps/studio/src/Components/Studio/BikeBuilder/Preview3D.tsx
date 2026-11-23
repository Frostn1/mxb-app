import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { AlertTriangle } from "lucide-react";
import { useT } from "@/i18n";
import { ViewportNav, isEditableTarget } from "@frost/shared/lib/viewportNav";
import { ViewportNavOverlay } from "@frost/shared/Components/Viewer/ViewportNav";
import { partGlb, toThree, type Placed, type Role, type V3 } from "../../../api/bikebuild";
import {
  accumulateSweep,
  bikeKeyCommand,
  fromThree,
  grabDelta,
  modalKey,
  modalReadout,
  rotateResult,
  snapAngle,
  snapGrab,
  startModal,
  type BikeCommand,
  type Modal,
} from "./bikeKeys";

/** A part's model, loaded once per part and version, cloned onto the bike. */
const models = new Map<string, Promise<THREE.Object3D>>();

function loadModel(id: string, version: number): Promise<THREE.Object3D> {
  const key = `${id}:${version}`;
  let p = models.get(key);
  if (!p) {
    p = partGlb(id).then(
      (bytes) =>
        new Promise<THREE.Object3D>((resolve, reject) =>
          new GLTFLoader().parse(bytes, "", (gltf) => resolve(gltf.scene), reject),
        ),
    );
    // A failed load isn't kept: the next look tries again.
    p.catch(() => models.delete(key));
    models.set(key, p);
  }
  return p;
}

/** Why a part didn't end up in the preview: read the message where a rider (or a bug
 *  report) can actually see it, instead of the pane just staying empty with nothing in
 *  the console but a swallowed rejection. */
function describeLoadFailure(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return String(e);
}

/** Blender's frame → three's, as a matrix: what `toThree` does to a point. */
const B2T = new THREE.Matrix4().set(1, 0, 0, 0, 0, 0, 1, 0, 0, -1, 0, 0, 0, 0, 0, 1);
const T2B = B2T.clone().invert();

/** Where a placed part's model goes, in three.js' frame: a kit part turned onto its node
 *  first (`basis`), then moved by its offset, then turned by the rider's rotation about its
 *  pivot — the same transform the build's `assemble` job applies in Blender (`part_basis`,
 *  `part_turn` in `frost_bike.py`). Blender's XYZ Euler (Rz·Ry·Rx about Blender's X, Y, Z)
 *  is, in three's Y-up frame, a turn about X, then about -Z, then about Y. */
export function placedMatrix(p: Placed): THREE.Matrix4 {
  const [ox, oy, oz] = toThree(p.offset);
  const m = new THREE.Matrix4().makeTranslation(ox, oy, oz);
  if (p.basis) {
    const [r0, r1, r2] = p.basis;
    const b = new THREE.Matrix4().set(...r0, 0, ...r1, 0, ...r2, 0, 0, 0, 0, 1);
    // The basis is in Blender's frame; the model is in three's.
    m.multiply(B2T.clone().multiply(b).multiply(T2B));
  }
  const [a, b, c] = (p.rotation ?? [0, 0, 0]).map((d) => THREE.MathUtils.degToRad(d));
  if (!a && !b && !c) return m;
  const [px, py, pz] = toThree(p.pivot ?? p.offset);
  const turn = new THREE.Matrix4()
    .makeTranslation(px, py, pz)
    .multiply(new THREE.Matrix4().makeRotationY(c))
    .multiply(new THREE.Matrix4().makeRotationZ(-b))
    .multiply(new THREE.Matrix4().makeRotationX(a))
    .multiply(new THREE.Matrix4().makeTranslation(-px, -py, -pz));
  return turn.multiply(m);
}

const add3 = (a: V3, b: V3): V3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

interface Props {
  placed: Placed[];
  anchors: Record<string, V3>;
  /** The roles selected: one part, every part (A), or none. */
  selected: Role[];
  /** A click on a part; null for a click on nothing, which selects nothing (as in Blender). */
  onSelect: (role: Role | null) => void;
  /** Bumped whenever the library changes, so refreshed parts load their new models. */
  version: number;
  /** An open mount dot was clicked — "what goes here" — as every mount at that same point
   *  (the placeholder's `steer_axis` and `fork_clamp` sit on top of each other, since the
   *  steer hasn't been placed to move `fork_clamp` off it yet), so the popover can offer
   *  every role one of them takes rather than only whichever the raycast happened to hit. */
  onMountClick?: (mounts: string[]) => void;
  /** The part G and R act on: the selected one on the bike, or null. */
  transformRole: Role | null;
  /** A Bike key other than G and R (see `bikeKeys`), pressed over the view. */
  onCommand?: (cmd: BikeCommand) => void;
  /** G confirmed: move `role` by `delta` metres, Blender's frame. */
  onGrab?: (role: Role, delta: V3) => void;
  /** R confirmed: `role`'s turn is now `rotation` (XYZ degrees, Blender's frame). */
  onRotate?: (role: Role, rotation: V3) => void;
  /** G or R with nothing to act on. */
  onNothingToTransform?: () => void;
}

/** What a running G or R holds besides the `Modal` the header shows. */
interface Live {
  role: Role;
  base: Placed;
  from: [number, number];
  at: [number, number];
  /** R's sweep so far, in degrees, and the pointer it was last measured to. */
  swept: number;
  sweptTo: [number, number];
  /** Ctrl held: step the move (see `SNAP_MM`, `SNAP_DEG`). */
  snap: boolean;
}

/**
 * The bike as it's put together: each slotted part's preview model at its offset, and the
 * anchors as small dots — themselves clickable, so an empty mount is something to click on,
 * not just a thing to look at. Plain three.js, drawn only when something changes. Click a
 * part to select it, or an open mount to say what should fill it. A part dropped from the
 * outliner lands on this same element from outside — `usePartDrag` checks for it by a DOM
 * marker, not through this component.
 *
 * With the pointer over it, the Bike tab's Blender keys work here (see `bikeKeys`): G and R
 * run right in this component, since they follow the mouse in the view's own space; the
 * rest go up as commands.
 */
export default function Preview3D({
  placed,
  anchors,
  selected,
  onSelect,
  version,
  onMountClick,
  transformRole,
  onCommand,
  onGrab,
  onRotate,
  onNothingToTransform,
}: Props) {
  const t = useT();
  const host = useRef<HTMLDivElement>(null);
  const three = useRef<{
    renderer: THREE.WebGLRenderer;
    scene: THREE.Scene;
    camera: THREE.PerspectiveCamera;
    controls: OrbitControls;
    bike: THREE.Group;
    dots: THREE.Group;
    draw: () => void;
  } | null>(null);
  const framed = useRef(false);
  const [failed, setFailed] = useState<string | null>(null);
  /** One part failing to load must not blank the whole preview: the others still draw, and
   *  this is what used to be a silently empty pane with nothing to go on. Keyed by role so a
   *  part that loads fine on a later pass clears its own entry rather than needing all of
   *  them to succeed at once. */
  const [partErrors, setPartErrors] = useState<Partial<Record<Role, string>>>({});
  const [nav, setNav] = useState<ViewportNav | null>(null);
  /** G or R running: what the header line shows. */
  const [modal, setModal] = useState<(Modal & { live: number }) | null>(null);

  // The latest props, for the listeners the scene sets up once.
  const props = useRef({ onSelect, onMountClick, selected, transformRole, onCommand, onGrab, onRotate, onNothingToTransform, placed });
  props.current = { onSelect, onMountClick, selected, transformRole, onCommand, onGrab, onRotate, onNothingToTransform, placed };

  // The scene, once.
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    } catch (e) {
      setFailed(String(e));
      return;
    }
    renderer.setPixelRatio(window.devicePixelRatio);
    el.appendChild(renderer.domElement);
    const scene = new THREE.Scene();
    const pmrem = new THREE.PMREMGenerator(renderer);
    scene.environment = pmrem.fromScene(new RoomEnvironment(renderer), 0.04).texture;
    const camera = new THREE.PerspectiveCamera(35, 1, 0.05, 50);
    camera.position.set(2.2, 1.2, 1.6);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.target.set(0, 0.6, 0);
    // Fewer, dimmer lines than a modelling app's grid needs — this is a neutral floor to judge
    // scale against, not something to study, and 40 divisions of 0x555555 read as a wall of
    // lines under a bike this small.
    const grid = new THREE.GridHelper(4, 16, 0x3a3a3a, 0x232323);
    scene.add(grid);
    const bike = new THREE.Group();
    const dots = new THREE.Group();
    scene.add(bike, dots);
    const draw = () => renderer.render(scene, camera);
    controls.addEventListener("change", draw);
    const size = () => {
      const w = el.clientWidth || 1;
      const h = el.clientHeight || 1;
      renderer.setSize(w, h, false);
      renderer.domElement.style.width = "100%";
      renderer.domElement.style.height = "100%";
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      draw();
    };
    const ro = new ResizeObserver(size);
    ro.observe(el);
    size();

    const ndc = (x: number, y: number) => {
      const r = renderer.domElement.getBoundingClientRect();
      return new THREE.Vector2(((x - r.left) / r.width) * 2 - 1, -((y - r.top) / r.height) * 2 + 1);
    };

    // ---- G and R
    let live: Live | null = null;
    let current: Modal | null = null;
    const partObject = (role: Role) => bike.children.find((o) => o.userData.role === role);
    const show = (role: Role, p: Placed) => {
      const obj = partObject(role);
      if (!obj) return;
      obj.position.set(0, 0, 0);
      obj.quaternion.identity();
      obj.scale.set(1, 1, 1);
      obj.applyMatrix4(placedMatrix(p));
      draw();
    };
    /** Towards the viewer, in Blender's frame. */
    const viewAxis = (): V3 => {
      const d = camera.getWorldDirection(new THREE.Vector3()).negate();
      return fromThree([d.x, d.y, d.z]);
    };
    /** Where the pointer's move takes the part, from the modal and the pointer now. */
    const outcome = (m: Modal, l: Live): { placed: Placed; value: number; delta?: V3; rotation?: V3 } => {
      const pivot = new THREE.Vector3(...toThree(l.base.pivot ?? l.base.offset));
      if (m.mode === "grab") {
        // On the plane through the part facing the view: the mouse's move, in metres.
        const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(camera.getWorldDirection(new THREE.Vector3()), pivot);
        const ray = new THREE.Raycaster();
        const hit = (p: [number, number]) => {
          ray.setFromCamera(ndc(p[0], p[1]), camera);
          return ray.ray.intersectPlane(plane, new THREE.Vector3());
        };
        const a = hit(l.from);
        const b = hit(l.at);
        const moved = a && b ? b.sub(a) : new THREE.Vector3();
        let delta = grabDelta(m, fromThree([moved.x, moved.y, moved.z]));
        if (l.snap && !m.typed) delta = snapGrab(delta);
        const value = m.axis === null ? Math.hypot(...delta) * 1000 : delta[m.axis] * 1000;
        return { placed: { ...l.base, offset: add3(l.base.offset, delta), pivot: add3(l.base.pivot, delta) }, value, delta };
      }
      const s = pivot.clone().project(camera);
      const r = renderer.domElement.getBoundingClientRect();
      const centre: [number, number] = [r.left + ((s.x + 1) / 2) * r.width, r.top + ((1 - s.y) / 2) * r.height];
      const view = viewAxis();
      l.swept = accumulateSweep(l.swept, centre, l.sweptTo, l.at);
      l.sweptTo = l.at;
      let angle = l.swept;
      if (l.snap && !m.typed) angle = snapAngle(angle);
      // About a locked axis that points away from the viewer, the same sweep turns it the
      // other way round: kept following the mouse on screen, as in Blender.
      if (m.axis !== null && view[m.axis] < 0) angle = -angle;
      const start = (l.base.rotation ?? [0, 0, 0]) as V3;
      const rotation = rotateResult(m, start, angle, view);
      return { placed: { ...l.base, rotation }, value: angle, rotation };
    };
    const update = () => {
      if (!live || !current) return;
      const o = outcome(current, live);
      show(live.role, o.placed);
      setModal({ ...current, live: o.value });
    };
    const finish = (confirm: boolean) => {
      if (!live || !current) return;
      const l = live;
      const o = outcome(current, l);
      const m = current;
      live = null;
      current = null;
      setModal(null);
      controls.enabled = true;
      if (!confirm) {
        show(l.role, l.base);
        return;
      }
      if (m.mode === "grab" && o.delta && o.delta.some((v) => Math.abs(v) > 1e-6)) props.current.onGrab?.(l.role, o.delta);
      else if (m.mode === "rotate" && o.rotation) props.current.onRotate?.(l.role, o.rotation);
      else show(l.role, l.base);
    };
    let pointer: [number, number] = [0, 0];
    let hovered = false;
    /** The press that ended a move isn't also a click on the view, nor a context menu. */
    let skipClick = false;
    let eatMenu = false;
    const start = (mode: Modal["mode"]) => {
      const role = props.current.transformRole;
      const base = role ? props.current.placed.find((p) => p.role === role) : null;
      if (!role || !base || !partObject(role)) {
        props.current.onNothingToTransform?.();
        return;
      }
      live = { role, base, from: pointer, at: pointer, swept: 0, sweptTo: pointer, snap: false };
      current = startModal(mode);
      controls.enabled = false;
      update();
    };

    const onKey = (e: KeyboardEvent) => {
      if (current && live && (e.key === "Control" || e.key === "Meta")) {
        e.preventDefault();
        live.snap = true;
        update();
        return;
      }
      if (current) {
        // Every key is the move's while it runs: nothing else (a view key on the numpad, a
        // shortcut in another panel) acts mid-move.
        e.preventDefault();
        e.stopImmediatePropagation();
        const step = modalKey(current, e);
        if (!step) return;
        if ("confirm" in step) finish(true);
        else if ("cancel" in step) finish(false);
        else {
          current = step.modal;
          update();
        }
        return;
      }
      if (!hovered || e.repeat || isEditableTarget(e.target) || isEditableTarget(document.activeElement)) return;
      const cmd = bikeKeyCommand(e);
      if (!cmd) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      if (cmd.kind === "grab" || cmd.kind === "rotate") start(cmd.kind);
      else props.current.onCommand?.(cmd);
    };
    // Capture, on the window: ahead of `ViewportNav`'s own key listener, so a digit typed
    // mid-move never reaches it as a numpad view key.
    window.addEventListener("keydown", onKey, { capture: true });
    const onKeyUp = (e: KeyboardEvent) => {
      if (live && (e.key === "Control" || e.key === "Meta")) {
        live.snap = false;
        update();
      }
    };
    window.addEventListener("keyup", onKeyUp, { capture: true });
    const onEnter = () => (hovered = true);
    const onLeave = () => (hovered = false);
    const onMoveAny = (e: PointerEvent) => {
      pointer = [e.clientX, e.clientY];
      if (live) {
        live.at = pointer;
        live.snap = e.ctrlKey || e.metaKey;
        update();
      }
    };
    el.addEventListener("pointerenter", onEnter);
    el.addEventListener("pointerleave", onLeave);
    window.addEventListener("pointermove", onMoveAny);
    // A click confirms a running move, a right-click cancels it; before anything else sees
    // the press (the view's own drag, a selection click).
    const onModalPress = (e: PointerEvent) => {
      if (!live) return;
      e.preventDefault();
      e.stopPropagation();
      skipClick = true;
      eatMenu = e.button === 2;
      finish(e.button === 0);
    };
    const noMenu = (e: MouseEvent) => {
      if (live || eatMenu) {
        e.preventDefault();
        e.stopPropagation();
      }
      eatMenu = false;
    };
    // After the view's own pointerup has seen (and skipped) it.
    const afterUp = () => {
      skipClick = false;
    };
    window.addEventListener("pointerdown", onModalPress, { capture: true });
    window.addEventListener("contextmenu", noMenu, { capture: true });
    window.addEventListener("pointerup", afterUp);

    // A click (not a drag) picks a mount dot, or — if it missed every dot — the part under
    // it, or nothing. Dots go first: they're drawn with `depthTest: false` so they always
    // show through the bike's own geometry, and a raycast that let the (honestly
    // depth-tested) mesh win would pick the part sitting behind a dot the rider can plainly
    // see and clicked on.
    let down: { x: number; y: number } | null = null;
    const onDown = (e: PointerEvent) => {
      down = e.button === 0 ? { x: e.clientX, y: e.clientY } : null;
    };
    const onUp = (e: PointerEvent) => {
      if (skipClick) return;
      if (!down || Math.hypot(e.clientX - down.x, e.clientY - down.y) > 4) return;
      down = null;
      const ray = new THREE.Raycaster();
      ray.setFromCamera(ndc(e.clientX, e.clientY), camera);
      // Only the open (nothing-mounted-yet) dots pre-empt the part hit test — a filled
      // mount's small, faint marker is a hint, not a click target, and shouldn't steal a
      // click meant for the part sitting right there.
      const dotHits = ray.intersectObjects(dots.children, false).filter((h) => h.object.userData.open);
      if (dotHits.length) {
        // More than one mount can sit at the same point — the placeholder's `steer_axis`
        // and `fork_clamp` do, until the steer is placed and moves `fork_clamp` off it —
        // so every dot within a hair of the nearest is offered, not just whichever the
        // raycast happened to return first.
        const nearest = dotHits[0].distance;
        const mounts = dotHits
          .filter((h) => h.distance <= nearest + 0.02)
          .map((h) => h.object.userData.mount as string);
        props.current.onMountClick?.(mounts);
        return;
      }
      const hit = ray.intersectObjects(bike.children, true)[0];
      let o: THREE.Object3D | null = hit?.object ?? null;
      while (o && !o.userData.role) o = o.parent;
      props.current.onSelect(o ? (o.userData.role as Role) : null);
    };
    renderer.domElement.addEventListener("pointerdown", onDown);
    renderer.domElement.addEventListener("pointerup", onUp);

    // Blender-style navigation, orbiting about the picked part and framing it on numpad `.`.
    const partBox = (roles: Role[]) => {
      const box = new THREE.Box3();
      for (const o of bike.children) if (roles.includes(o.userData.role as Role)) box.expandByObject(o);
      return box.isEmpty() ? null : box;
    };
    const nav = new ViewportNav({
      camera,
      controls,
      element: el,
      onChange: draw,
      getPivot: () => partBox(props.current.selected)?.getCenter(new THREE.Vector3()) ?? null,
      getFrameBox: (what) => {
        if (what === "selected") return partBox(props.current.selected);
        const box = new THREE.Box3().setFromObject(bike);
        return box.isEmpty() ? null : box;
      },
    });
    setNav(nav);

    three.current = { renderer, scene, camera, controls, bike, dots, draw };
    return () => {
      window.removeEventListener("keydown", onKey, { capture: true });
      window.removeEventListener("keyup", onKeyUp, { capture: true });
      window.removeEventListener("pointermove", onMoveAny);
      window.removeEventListener("pointerdown", onModalPress, { capture: true });
      window.removeEventListener("contextmenu", noMenu, { capture: true });
      window.removeEventListener("pointerup", afterUp);
      el.removeEventListener("pointerenter", onEnter);
      el.removeEventListener("pointerleave", onLeave);
      setNav(null);
      nav.dispose();
      ro.disconnect();
      controls.dispose();
      pmrem.dispose();
      renderer.dispose();
      renderer.domElement.remove();
      three.current = null;
    };
  }, []);

  // The parts, whenever the assembly changes.
  const selectedKey = selected.join(",");
  useEffect(() => {
    const t = three.current;
    if (!t) return;
    const picked = new Set(selectedKey ? selectedKey.split(",") : []);
    let stale = false;
    Promise.all(
      placed.map((p) =>
        loadModel(p.partId, version)
          .then((m) => ({ p, m, error: null as string | null }))
          .catch((e) => ({ p, m: null, error: describeLoadFailure(e) })),
      ),
    ).then((loaded) => {
      if (stale) return;
      const errors: Partial<Record<Role, string>> = {};
      for (const { p, error } of loaded) if (error) errors[p.role] = error;
      setPartErrors(errors);
      t.bike.clear();
      for (const item of loaded) {
        const { p, m } = item;
        if (!m) continue;
        const obj = m.clone(true);
        obj.applyMatrix4(placedMatrix(p));
        obj.userData.role = p.role;
        if (picked.has(p.role)) {
          obj.traverse((o) => {
            const mesh = o as THREE.Mesh;
            if (!mesh.isMesh) return;
            const mats = (Array.isArray(mesh.material) ? mesh.material : [mesh.material]).map((mat) => {
              const c = (mat as THREE.MeshStandardMaterial).clone();
              if ("emissive" in c) {
                c.emissive = new THREE.Color(0x2266ff);
                c.emissiveIntensity = 0.45;
              }
              return c;
            });
            mesh.material = Array.isArray(mesh.material) ? mats : mats[0];
          });
        }
        t.bike.add(obj);
      }
      if (!framed.current && t.bike.children.length) {
        const box = new THREE.Box3().setFromObject(t.bike);
        const c = box.getCenter(new THREE.Vector3());
        const r = box.getSize(new THREE.Vector3()).length() / 2;
        t.controls.target.copy(c);
        t.camera.up.set(0, 1, 0);
        t.camera.position.copy(c.clone().add(new THREE.Vector3(1.4, 0.6, 1.1).normalize().multiplyScalar(r * 2.6)));
        t.controls.update();
        framed.current = true;
      }
      t.draw();
    });
    return () => {
      stale = true;
    };
  }, [placed, selectedKey, version]);

  // The anchors: bigger and brighter where nothing is mounted yet, since those are now
  // something to click — "put a part here" — not just a picture of where things snap.
  useEffect(() => {
    const t = three.current;
    if (!t) return;
    t.dots.clear();
    const filled = new Set(placed.map((p) => p.mount).filter((m): m is string => !!m));
    const openGeo = new THREE.SphereGeometry(0.018, 12, 8);
    const openMat = new THREE.MeshBasicMaterial({ color: 0xffb020, depthTest: false });
    const filledGeo = new THREE.SphereGeometry(0.01, 8, 6);
    const filledMat = new THREE.MeshBasicMaterial({ color: 0xffb020, opacity: 0.35, transparent: true, depthTest: false });
    for (const [mount, p] of Object.entries(anchors)) {
      const open = !filled.has(mount);
      const dot = new THREE.Mesh(open ? openGeo : filledGeo, open ? openMat : filledMat);
      dot.renderOrder = 10;
      dot.userData.mount = mount;
      dot.userData.open = open;
      const [x, y, z] = toThree(p);
      dot.position.set(x, y, z);
      t.dots.add(dot);
    }
    t.draw();
  }, [anchors, placed]);

  const errorEntries = Object.entries(partErrors) as [Role, string][];
  const axisName = modal?.axis === null || modal === null ? null : (["X", "Y", "Z"] as const)[modal.axis];

  return (
    <div ref={host} className="relative h-full min-h-64 w-full overflow-hidden border border-border bg-background">
      {failed && <p className="absolute inset-0 p-4 text-sm text-muted-foreground">{failed}</p>}
      <ViewportNavOverlay nav={nav} />
      {modal && (
        <div
          role="status"
          data-bike-modal
          className="pointer-events-none absolute inset-x-0 top-0 z-20 flex items-center gap-3 border-b border-border bg-popover px-3 py-1.5 text-[12px] shadow"
        >
          <span className="font-semibold">{modal.mode === "grab" ? t("bike.keys.moving") : t("bike.keys.turning")}</span>
          <span
            className={
              axisName === "X"
                ? "font-semibold text-red-400"
                : axisName === "Y"
                  ? "font-semibold text-green-400"
                  : axisName === "Z"
                    ? "font-semibold text-blue-400"
                    : "text-faint"
            }
          >
            {axisName ? t("bike.keys.alongAxis", { axis: axisName }) : t("bike.keys.free")}
          </span>
          <span className="font-mono tabular-nums">{modalReadout(modal, modal.live)}</span>
          <span className="ml-auto truncate text-faint">{t("bike.keys.modalHint")}</span>
        </div>
      )}
      {!failed && errorEntries.length > 0 && (
        <div className="absolute inset-x-0 bottom-0 flex flex-col gap-1 bg-background/90 p-2 text-[11px] text-destructive">
          {errorEntries.map(([role, message]) => (
            <p key={role} className="flex items-center gap-1.5">
              <AlertTriangle className="size-3 shrink-0" />
              {t("bike.previewPartFailed", { role: t(`bike.role.${role}`), message })}
            </p>
          ))}
        </div>
      )}
    </div>
  );
}
