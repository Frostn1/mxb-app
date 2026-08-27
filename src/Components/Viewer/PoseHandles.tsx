import { useEffect, useMemo, useRef, useState } from "react";
import { useFrame, useThree, type ThreeEvent } from "@react-three/fiber";
import * as THREE from "three";
import type { Bone } from "../../types";
import {
  applyPose,
  boneTip,
  POSE_HANDLES,
  toMatrix,
  turnToward,
  type RiderPose,
} from "../../lib/riderPose";

/**
 * The dots you take hold of.
 *
 * One per joint worth reaching for, sitting on the rider's own body. Grab one and the bone
 * above it swings so the dot follows the cursor, at the limb's own length — the wrist sweeps
 * on the forearm's arc about the elbow, the foot on the shin's about the knee. It writes the
 * same pose the sliders do, so the two are one thing.
 *
 * A drag turns in the plane you are looking at, so orbiting the camera changes what a drag
 * can say — which is the whole trick, and the reason there is nothing to choose between
 * "bend" and "splay" here.
 */

/** How big a dot is on screen: a fraction of its distance from the camera, so zoom is moot. */
const DOT_SIZE = 0.011;

interface Placed {
  /** The bone a drag turns, and the bone the dot rides, as indexes into the rig. */
  turns: number;
  on: number;
  /** Where on that bone the dot sits, in the bone's own space. */
  offset: THREE.Vector3;
  /** Where that lands on the model as authored — see the note where it is used. */
  rests: THREE.Vector3;
  /** The turned bone's name — what the pose is keyed by. */
  name: string;
}

export function PoseHandles({
  order,
  bones,
  pose,
  onPose,
}: {
  /** The live bone tree, in rig order — the same one the body is drawn from. */
  order: THREE.Bone[];
  bones: Bone[];
  pose: RiderPose;
  onPose: (pose: RiderPose) => void;
}) {
  const placed = useMemo<Placed[]>(() => {
    const at = (name: string) => bones.findIndex((b) => b.name === name);
    return POSE_HANDLES.flatMap((h) => {
      const turns = at(h.turns);
      const on = at(h.on);
      // A model that binds fewer bones simply offers fewer dots.
      if (turns < 0 || on < 0 || !order[turns] || !order[on]) return [];
      const tip = h.tip ? boneTip(bones, on) : [0, 0, 0];
      const offset = new THREE.Vector3(...tip);
      const rests = offset.clone().applyMatrix4(toMatrix(bones[on].bind));
      return [{ turns, on, name: h.turns, offset, rests }];
    });
  }, [bones, order]);

  const group = useRef<THREE.Group>(null);
  const dots = useRef<(THREE.Mesh | null)[]>([]);
  const camera = useThree((s) => s.camera);
  const gl = useThree((s) => s.gl);
  const invalidate = useThree((s) => s.invalidate);
  // OrbitControls (`makeDefault`) owns the pointer; a drag has to take it off them.
  const controls = useThree((s) => s.controls) as { enabled: boolean } | null;
  const [held, setHeld] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);

  /** The pose as the drag has it, which runs ahead of React between commits. */
  const live = useRef(pose);
  useEffect(() => {
    live.current = pose;
  }, [pose]);

  // Dots ride the bones. Done here rather than in a render so a limb can swing at pointer rate
  // without a React commit per move; `updateWorldMatrix` because the group above these can move
  // too — the whole rider is re-centred whenever the model changes.
  useFrame(() => {
    const g = group.current;
    if (!g) return;
    g.updateWorldMatrix(true, false);
    const into = new THREE.Matrix4().copy(g.matrixWorld).invert();
    const world = new THREE.Vector3();
    placed.forEach((h, i) => {
      const dot = dots.current[i];
      const bone = order[h.on];
      if (!dot || !bone) return;
      bone.updateWorldMatrix(true, false);
      world.copy(h.offset).applyMatrix4(bone.matrixWorld);
      dot.scale.setScalar(camera.position.distanceTo(world) * DOT_SIZE);
      dot.position.copy(world).applyMatrix4(into);
    });
  });

  useEffect(() => {
    if (held === null) return;
    const h = placed[held];
    if (!h) return;
    const bone = order[h.turns];
    const rides = order[h.on];
    if (!bone || !rides) return;
    const el = gl.domElement;
    // The joint stays put while the limb swings about it, so the pivot and the plane a drag
    // reads are fixed for the whole of it.
    bone.updateWorldMatrix(true, false);
    const pivot = new THREE.Vector3().setFromMatrixPosition(bone.matrixWorld);
    const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(
      camera.getWorldDirection(new THREE.Vector3()),
      pivot,
    );
    const caster = new THREE.Raycaster();
    const ndc = new THREE.Vector2();
    const to = new THREE.Vector3();
    const from = new THREE.Vector3();
    let frame = 0;
    const move = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) return;
      ndc.set(
        ((ev.clientX - r.left) / r.width) * 2 - 1,
        -((ev.clientY - r.top) / r.height) * 2 + 1,
      );
      caster.setFromCamera(ndc, camera);
      if (!caster.ray.intersectPlane(plane, to)) return;
      // Off the bone rather than off the dot: the dot only catches up once a frame is drawn,
      // and a pointer can move more often than that.
      rides.updateWorldMatrix(true, false);
      from.copy(h.offset).applyMatrix4(rides.matrixWorld);
      const next = turnToward(order, bones, live.current, h.name, from, to);
      if (next === live.current) return;
      live.current = next;
      // three moves now; the sliders and the gear catch up on the next frame.
      applyPose(order, next);
      invalidate();
      if (!frame) {
        frame = requestAnimationFrame(() => {
          frame = 0;
          onPose(live.current);
        });
      }
    };
    const drop = () => setHeld(null);
    // On the window, not the canvas: a drag that runs off the edge of the panel keeps going.
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
      if (controls) controls.enabled = true;
      el.style.cursor = "";
      onPose(live.current);
    };
  }, [held, placed, order, bones, camera, gl, controls, invalidate, onPose]);

  return (
    <group ref={group}>
      {placed.map((h, i) => (
        <mesh
          key={h.name}
          ref={(m: THREE.Mesh | null) => {
            dots.current[i] = m;
          }}
          // Placed and sized before a frame is drawn, because the `<Center>` above measures
          // its children once: a dot left at the origin sits well below the rider's feet and
          // would shove the whole model up the frame for as long as the handles are up.
          position={h.rests}
          scale={0.015}
          renderOrder={999}
          onPointerDown={(e: ThreeEvent<PointerEvent>) => {
            e.stopPropagation();
            setHeld(i);
          }}
          onPointerOver={(e: ThreeEvent<PointerEvent>) => {
            e.stopPropagation();
            setOver(i);
            if (held === null) gl.domElement.style.cursor = "grab";
          }}
          onPointerOut={() => {
            setOver((o) => (o === i ? null : o));
            if (held === null) gl.domElement.style.cursor = "";
          }}
        >
          <sphereGeometry args={[1, 16, 12]} />
          {/* Through the body rather than inside it: a hip you can't see is a hip you can't
              take hold of. */}
          <meshBasicMaterial
            color={held === i ? "#ffffff" : over === i ? "#dbeafe" : "#7cb8ff"}
            depthTest={false}
            transparent
            opacity={held === i || over === i ? 1 : 0.9}
            toneMapped={false}
          />
        </mesh>
      ))}
    </group>
  );
}
