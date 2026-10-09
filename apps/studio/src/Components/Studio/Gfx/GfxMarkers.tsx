import { useEffect, useMemo, useRef, useState } from "react";
import { useFrame, useThree, type ThreeEvent } from "@react-three/fiber";
import * as THREE from "three";
import type { BikeRig, Vec3 } from "@frost/shared/types";
import type { Frame } from "@/api/gfx";

/**
 * The gfx.cfg points on the model, in the bike's own frame (the one `EdfMesh` draws in).
 *
 * gfx.cfg writes a point in its part's frame, the way the game reads it: +x right, +y up,
 * +z forward. The viewer mirrors x to draw right-handed, and the backend's assembly moved the
 * steer by the rake about the head, so a point goes through the same two steps its part's
 * vertices did. With no rig the parts were never moved, and the mirror is all there is.
 */

const X = new THREE.Vector3(1, 0, 0);

export function toView(p: Vec3, frame: Frame, rig: BikeRig | null): THREE.Vector3 {
  const v = new THREE.Vector3(-p[0], p[1], p[2]);
  if (!rig) return v;
  if (frame === "steer") {
    v.applyAxisAngle(X, THREE.MathUtils.degToRad(rig.rake));
    return v.add(new THREE.Vector3(...rig.steerOrigin));
  }
  return v.add(new THREE.Vector3(...rig.chassisOrigin));
}

export function fromView(v: THREE.Vector3, frame: Frame, rig: BikeRig | null): Vec3 {
  const p = v.clone();
  if (rig) {
    if (frame === "steer") {
      p.sub(new THREE.Vector3(...rig.steerOrigin));
      p.applyAxisAngle(X, -THREE.MathUtils.degToRad(rig.rake));
    } else {
      p.sub(new THREE.Vector3(...rig.chassisOrigin));
    }
  }
  return [-p.x, p.y, p.z];
}

export interface Marker {
  id: string;
  /** In the part's frame, as gfx.cfg writes it. */
  at: Vec3;
  frame: Frame;
  /** A direction to draw from the point, same frame. */
  dir?: Vec3;
  /** Not draggable: a point read off the mesh, not one gfx.cfg holds. */
  fixed?: boolean;
  /** Already in the view frame, for `fixed` points read off the drawn mesh. */
  view?: Vec3;
  color: string;
}

/** On-screen size: a fraction of the distance to the camera, so zoom doesn't change it. */
const DOT = 0.009;

export function GfxMarkers({
  markers,
  rig,
  selected,
  onSelect,
  onMove,
}: {
  markers: Marker[];
  rig: BikeRig | null;
  selected: string | null;
  onSelect: (id: string) => void;
  onMove: (id: string, at: Vec3) => void;
}) {
  const camera = useThree((s) => s.camera);
  const gl = useThree((s) => s.gl);
  const invalidate = useThree((s) => s.invalidate);
  const controls = useThree((s) => s.controls) as { enabled: boolean } | null;
  const group = useRef<THREE.Group>(null);
  const dots = useRef<Record<string, THREE.Mesh | null>>({});
  const [held, setHeld] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);

  const placed = useMemo(
    () =>
      markers.map((m) => {
        const at = m.view ? new THREE.Vector3(...m.view) : toView(m.at, m.frame, rig);
        let tip: THREE.Vector3 | null = null;
        if (m.dir) {
          const d = toView(m.dir, m.frame, rig).sub(toView([0, 0, 0], m.frame, rig));
          if (d.lengthSq() > 1e-9) tip = at.clone().add(d.normalize().multiplyScalar(0.15));
        }
        return { m, at, tip };
      }),
    [markers, rig],
  );

  useEffect(() => invalidate(), [placed, selected, over, held, invalidate]);

  // Same size on screen at any zoom.
  useFrame(() => {
    const g = group.current;
    if (!g) return;
    const w = new THREE.Vector3();
    for (const { m, at } of placed) {
      const dot = dots.current[m.id];
      if (!dot) continue;
      w.copy(at).applyMatrix4(g.matrixWorld);
      dot.scale.setScalar(camera.position.distanceTo(w) * DOT * (m.fixed ? 0.7 : 1));
    }
  });

  useEffect(() => {
    if (held === null) return;
    const g = group.current;
    const p = placed.find((x) => x.m.id === held);
    if (!g || !p) return;
    const el = gl.domElement;
    g.updateWorldMatrix(true, false);
    const start = p.at.clone().applyMatrix4(g.matrixWorld);
    const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(camera.getWorldDirection(new THREE.Vector3()), start);
    const caster = new THREE.Raycaster();
    const ndc = new THREE.Vector2();
    const hit = new THREE.Vector3();
    const into = new THREE.Matrix4().copy(g.matrixWorld).invert();
    let frame = 0;
    let last: Vec3 | null = null;
    const move = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) return;
      ndc.set(((ev.clientX - r.left) / r.width) * 2 - 1, -((ev.clientY - r.top) / r.height) * 2 + 1);
      caster.setFromCamera(ndc, camera);
      if (!caster.ray.intersectPlane(plane, hit)) return;
      last = fromView(hit.clone().applyMatrix4(into), p.m.frame, rig);
      if (!frame)
        frame = requestAnimationFrame(() => {
          frame = 0;
          if (last) onMove(held, last);
        });
    };
    const drop = () => setHeld(null);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", drop);
    window.addEventListener("pointercancel", drop);
    if (controls) controls.enabled = false;
    el.style.cursor = "grabbing";
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", drop);
      window.removeEventListener("pointercancel", drop);
      if (frame) cancelAnimationFrame(frame);
      if (last) onMove(held, last);
      if (controls) controls.enabled = true;
      el.style.cursor = "";
    };
    // `placed` changes on every move; the drag keeps the plane it started with.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [held]);

  return (
    <group ref={group}>
      {placed.map(({ m, at, tip }) => {
        const hot = selected === m.id || held === m.id || over === m.id;
        return (
          <group key={m.id}>
            <mesh
              ref={(x: THREE.Mesh | null) => {
                dots.current[m.id] = x;
              }}
              position={at}
              scale={0.012}
              renderOrder={999}
              onPointerDown={(e: ThreeEvent<PointerEvent>) => {
                e.stopPropagation();
                onSelect(m.id);
                if (!m.fixed) setHeld(m.id);
              }}
              onPointerOver={(e: ThreeEvent<PointerEvent>) => {
                e.stopPropagation();
                setOver(m.id);
                if (held === null) gl.domElement.style.cursor = m.fixed ? "pointer" : "grab";
              }}
              onPointerOut={() => {
                setOver((o) => (o === m.id ? null : o));
                if (held === null) gl.domElement.style.cursor = "";
              }}
            >
              <sphereGeometry args={[1, 16, 12]} />
              <meshBasicMaterial color={hot ? "#ffffff" : m.color} depthTest={false} transparent opacity={hot ? 1 : 0.9} toneMapped={false} />
            </mesh>
            {tip && <Line from={at} to={tip} color={m.color} />}
          </group>
        );
      })}
    </group>
  );
}

function Line({ from, to, color }: { from: THREE.Vector3; to: THREE.Vector3; color: string }) {
  const geom = useMemo(() => new THREE.BufferGeometry().setFromPoints([from, to]), [from, to]);
  useEffect(() => () => geom.dispose(), [geom]);
  return (
    <lineSegments geometry={geom} renderOrder={998}>
      <lineBasicMaterial color={color} depthTest={false} transparent toneMapped={false} />
    </lineSegments>
  );
}
