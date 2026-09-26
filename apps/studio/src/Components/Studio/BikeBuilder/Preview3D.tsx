import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { AlertTriangle } from "lucide-react";
import { useT } from "@/i18n";
import { partGlb, toThree, type Placed, type Role, type V3 } from "../../../api/bikebuild";

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

interface Props {
  placed: Placed[];
  anchors: Record<string, V3>;
  selected: Role | null;
  onSelect: (role: Role) => void;
  /** Bumped whenever the library changes, so refreshed parts load their new models. */
  version: number;
  /** A mount dot was clicked and nothing under it is a placed part — "what goes here". */
  onMountClick?: (mount: string) => void;
  /** A part card was dropped on the viewport, by the id it was dragged with. */
  onDropPart?: (partId: string) => void;
}

/**
 * The bike as it's put together: each slotted part's preview model at its offset, and the
 * anchors as small dots — themselves clickable, so an empty mount is something to click on,
 * not just a thing to look at. Plain three.js, drawn only when something changes. Click a
 * part to pick it for nudging; drop a part from the tray to fill its slot.
 */
export default function Preview3D({
  placed,
  anchors,
  selected,
  onSelect,
  version,
  onMountClick,
  onDropPart,
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
  const select = useRef(onSelect);
  select.current = onSelect;
  const mountClick = useRef(onMountClick);
  mountClick.current = onMountClick;
  const dropPart = useRef(onDropPart);
  dropPart.current = onDropPart;

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
    const grid = new THREE.GridHelper(4, 40, 0x555555, 0x333333);
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

    // A click (not a drag) picks the part under it, or — if nothing's there — a mount dot,
    // for "what goes here". Parts win the hit test: a mount that already has its part sitting
    // on it should still select the part, not ask what to put there.
    let down: { x: number; y: number } | null = null;
    const onDown = (e: PointerEvent) => (down = { x: e.clientX, y: e.clientY });
    const onUp = (e: PointerEvent) => {
      if (!down || Math.hypot(e.clientX - down.x, e.clientY - down.y) > 4) return;
      const r = renderer.domElement.getBoundingClientRect();
      const ray = new THREE.Raycaster();
      ray.setFromCamera(
        new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1),
        camera,
      );
      const hit = ray.intersectObjects(bike.children, true)[0];
      let o: THREE.Object3D | null = hit?.object ?? null;
      while (o && !o.userData.role) o = o.parent;
      if (o) {
        select.current(o.userData.role as Role);
        return;
      }
      const dotHit = ray.intersectObjects(dots.children, false)[0];
      if (dotHit?.object.userData.mount) mountClick.current?.(dotHit.object.userData.mount as string);
    };
    renderer.domElement.addEventListener("pointerdown", onDown);
    renderer.domElement.addEventListener("pointerup", onUp);

    // A part card dropped from the tray fills its own slot — the backend already knows
    // exactly where a role's part goes (its mount, its own attach empty or its centre), so
    // the drop doesn't need to hit-test a particular mount; landing anywhere on the viewport
    // is "put this where it goes".
    const onDragOver = (e: DragEvent) => e.preventDefault();
    const onDrop = (e: DragEvent) => {
      e.preventDefault();
      const id = e.dataTransfer?.getData("text/frost-bike-part");
      if (id) dropPart.current?.(id);
    };
    renderer.domElement.addEventListener("dragover", onDragOver);
    renderer.domElement.addEventListener("drop", onDrop);

    three.current = { renderer, scene, camera, controls, bike, dots, draw };
    return () => {
      ro.disconnect();
      controls.dispose();
      pmrem.dispose();
      renderer.dispose();
      renderer.domElement.remove();
      three.current = null;
    };
  }, []);

  // The parts, whenever the assembly changes.
  useEffect(() => {
    const t = three.current;
    if (!t) return;
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
        const [x, y, z] = toThree(p.offset);
        obj.position.set(x, y, z);
        obj.userData.role = p.role;
        if (p.role === selected) {
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
        t.camera.position.copy(c.clone().add(new THREE.Vector3(1.4, 0.6, 1.1).normalize().multiplyScalar(r * 2.6)));
        t.controls.update();
        framed.current = true;
      }
      t.draw();
    });
    return () => {
      stale = true;
    };
  }, [placed, selected, version]);

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
      const [x, y, z] = toThree(p);
      dot.position.set(x, y, z);
      t.dots.add(dot);
    }
    t.draw();
  }, [anchors, placed]);

  const errorEntries = Object.entries(partErrors) as [Role, string][];

  return (
    <div ref={host} className="relative h-full min-h-64 w-full overflow-hidden border border-border bg-background">
      {failed && <p className="absolute inset-0 p-4 text-sm text-muted-foreground">{failed}</p>}
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
