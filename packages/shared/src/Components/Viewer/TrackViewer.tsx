import { createContext, useContext, useEffect, useMemo, useRef, useState } from "react";
import { Canvas, useThree } from "@react-three/fiber";
import { Line, OrbitControls } from "@react-three/drei";
import { Move, Rotate3d, ZoomIn } from "lucide-react";
import * as THREE from "three";
import { cn } from "../../lib/utils";
import type {
  BikeRig,
  EdfNode,
  PaintTexture,
  RiderPart,
  TrackBackdrop,
  TrackGround,
  TrackGroundLayer,
  TrackMeshArrays,
  TrackOverview,
  TrackPlacement,
  TrackScenery,
  TrackSceneryTexture,
  TrackTerrain,
} from "../../types";
import {
  BikeOnGround,
  canSeatRider,
  settledPose,
  useTextureMap,
  type BikePose,
} from "./ModelViewer";
import type { RiderPose } from "../../lib/riderPose";
import { ErrorBoundary } from "../ErrorBoundary";
import { useT } from "../../i18n/context";
import { reportRenderer } from "../../lib/glInfo";

/** The terrain is scaled to sit this many units across, whatever its real size. */
const VIEW_SPAN = 10;

/**
 * Relief is drawn at its true height, the way the game draws it.
 *
 * It used to be half as tall again, so faces and lips cast a shadow to be seen by, which made
 * every jump look bigger than it rides. The hollows are darkened instead: see [`cavityShade`].
 */
/** How far back to sit when the camera first lands on a bike, in metres. Close enough to see a
 *  rider's shoulders move, far enough to see the corner they are moving them in. */
const WATCH_M = 14;

const RELIEF_EXAGGERATION = 1;

/**
 * Elevation ramp, low to high. Earth rather than atlas colours — a motocross track is dirt
 * with grass around it, and a rainbow ramp reads as data rather than as ground.
 */
const RAMP: [number, THREE.Color][] = [
  [0.0, new THREE.Color("#2c3626")],
  [0.35, new THREE.Color("#55532f")],
  [0.62, new THREE.Color("#8a7346")],
  [0.85, new THREE.Color("#b89a68")],
  [1.0, new THREE.Color("#ded0ae")],
];

function rampAt(t: number, out: THREE.Color): THREE.Color {
  const c = Math.min(Math.max(t, 0), 1);
  for (let i = 1; i < RAMP.length; i += 1) {
    const [hi, hiColor] = RAMP[i];
    if (c <= hi) {
      const [lo, loColor] = RAMP[i - 1];
      const span = hi - lo;
      return out.copy(loColor).lerp(hiColor, span === 0 ? 0 : (c - lo) / span);
    }
  }
  return out.copy(RAMP[RAMP.length - 1][1]);
}

/**
 * How sunk each sample is against the ground around it, as a brightness multiplier.
 *
 * A directional light can only shade a slope by which way it faces, so a rut running along
 * the light and a ridge running along it are lit identically, and the hollows a track is
 * actually made of come out flat. This measures each point against a blurred copy of the
 * terrain — below its surroundings is a hollow, above is a ridge — which is the cheap
 * standing-in for ambient occlusion, and on a heightfield it is most of what the eye reads
 * as depth.
 *
 * Two separable box passes rather than a gathered kernel: the same answer for a couple of
 * million samples in a few milliseconds instead of a few seconds.
 */
function cavityShade(heights: Float32Array, width: number, height: number): Float32Array {
  const n = width * height;

  // Blurred once tight and once wide, and read against both. One radius can only find
  // hollows of one size: a wide blur sees the bowl a turn sits in and steps straight over a
  // rut, a tight one finds the rut and cannot see the bowl at all. Together they give a track
  // its shape at both scales, which is what the eye actually reads as depth.
  const blur = (radius: number): Float32Array => {
    const tmp = new Float32Array(n);
    const out = new Float32Array(n);
    for (let y = 0; y < height; y += 1) {
      const row = y * width;
      let total = 0;
      let count = 0;
      // Running sum: the window moves one sample at a time, so re-adding every cell of it
      // would make a wide radius cost many times a tight one for no better answer.
      for (let x = 0; x < width; x += 1) {
        if (x === 0) {
          for (let d = 0; d <= radius && d < width; d += 1) {
            total += heights[row + d];
            count += 1;
          }
        } else {
          const add = x + radius;
          const drop = x - radius - 1;
          if (add < width) {
            total += heights[row + add];
            count += 1;
          }
          if (drop >= 0) {
            total -= heights[row + drop];
            count -= 1;
          }
        }
        tmp[row + x] = total / count;
      }
    }
    for (let x = 0; x < width; x += 1) {
      let total = 0;
      let count = 0;
      for (let y = 0; y < height; y += 1) {
        if (y === 0) {
          for (let d = 0; d <= radius && d < height; d += 1) {
            total += tmp[d * width + x];
            count += 1;
          }
        } else {
          const add = y + radius;
          const drop = y - radius - 1;
          if (add < height) {
            total += tmp[add * width + x];
            count += 1;
          }
          if (drop >= 0) {
            total -= tmp[drop * width + x];
            count -= 1;
          }
        }
        out[y * width + x] = total / count;
      }
    }
    return out;
  };

  const fine = blur(3);
  const broad = blur(14);

  // Each scale is measured against how much this track actually undulates at that scale, so
  // a supercross floor and an alpine hillside both read rather than one washing out and the
  // other going to soot.
  let fineSpread = 0;
  let broadSpread = 0;
  let sampled = 0;
  for (let i = 0; i < n; i += 7) {
    fineSpread += Math.abs(heights[i] - fine[i]);
    broadSpread += Math.abs(heights[i] - broad[i]);
    sampled += 1;
  }
  fineSpread = fineSpread / sampled || 1;
  broadSpread = broadSpread / sampled || 1;

  const out = new Float32Array(n);
  for (let i = 0; i < n; i += 1) {
    const f = (heights[i] - fine[i]) / (fineSpread * 2.2);
    const b = (heights[i] - broad[i]) / (broadSpread * 2.2);
    // Hollows darken further than ridges brighten: light fills a dip from fewer directions
    // than it leaves a rise, and an over-brightened ridge just looks chalky.
    out[i] = Math.min(1.16, Math.max(0.42, 1 + f * 0.40 + b * 0.40));
  }
  return out;
}

/**
 * The height band the colour ramp is spread across: the 2nd to 98th percentile of the grid.
 *
 * Sampled rather than fully sorted — a million heights is a lot of sorting to place a colour
 * ramp, and every hundredth is plenty to find a percentile.
 */
function reliefBand(heights: Float32Array): [number, number] {
  const step = Math.max(1, Math.floor(heights.length / 20000));
  const sample: number[] = [];
  for (let i = 0; i < heights.length; i += step) {
    const v = heights[i];
    if (Number.isFinite(v)) sample.push(v);
  }
  if (sample.length < 2) return [0, 1];
  sample.sort((a, b) => a - b);
  const lo = sample[Math.floor(sample.length * 0.02)];
  const hi = sample[Math.floor(sample.length * 0.98)];
  // A track genuinely flat across that band still has to get a ramp rather than a divide.
  return hi > lo ? [lo, hi] : [sample[0], sample[sample.length - 1] || sample[0] + 1];
}

/**
 * The normal at one grid sample, straight from its neighbours' heights.
 *
 * `computeVertexNormals` is the general answer: walk every triangle, accumulate a face normal
 * onto each of its three vertices, normalise. Over the fine grid that is 8.4 million triangles
 * and 25 million index lookups, and it measured at 664 ms — most of the time it took to show a
 * track at all. A height grid doesn't need the general answer: the surface is a function of x
 * and y, so its slope is a central difference and its normal follows in constant time.
 *
 * Writing the tangents in world units — X runs backwards, Y is scaled, Z runs forwards:
 *
 *     Tx = (-step,  k·dh/dx, 0)      Ty = (0, k·dh/dy, step)
 *     Tx × Ty = (k·dh/dx·step, step², -k·dh/dy·step)  ∝  (k·dh/dx, step, -k·dh/dy)
 *
 * which points up for any slope, as it must. Measured against `computeVertexNormals` over
 * 21 316 interior vertices, the two agree to 0.02° — and this runs 6.3x faster.
 *
 * Edges take a one-sided difference: `span` is 2 where both neighbours exist and 1 where the
 * grid runs out, which is the only place this and `computeVertexNormals` genuinely differ.
 */
function gridNormal(
  heights: Float32Array,
  width: number,
  height: number,
  x: number,
  y: number,
  i: number,
  step: number,
  heightScale: number,
  out: Float32Array,
  o: number,
): void {
  const hasLeft = x > 0;
  const hasRight = x < width - 1;
  const spanX = (hasLeft ? 1 : 0) + (hasRight ? 1 : 0);
  const dhx = spanX
    ? (heights[hasRight ? i + 1 : i] - heights[hasLeft ? i - 1 : i]) / spanX
    : 0;

  const hasUp = y > 0;
  const hasDown = y < height - 1;
  const spanY = (hasUp ? 1 : 0) + (hasDown ? 1 : 0);
  const dhy = spanY
    ? (heights[hasDown ? i + width : i] - heights[hasUp ? i - width : i]) / spanY
    : 0;

  const nx = heightScale * dhx;
  const ny = step;
  const nz = -heightScale * dhy;
  const len = Math.sqrt(nx * nx + ny * ny + nz * nz) || 1;
  out[o] = nx / len;
  out[o + 1] = ny / len;
  out[o + 2] = nz / len;
}

/**
 * Turn a height grid into a mesh.
 *
 * Written straight into typed arrays rather than through `PlaneGeometry` and a displacement
 * pass: the fine grid is 2048², four million vertices, and building it a `Vector3` at a time
 * is the difference between a view that appears and one that hitches on arrival.
 *
 * Normals are computed here too, from the height grid, rather than by
 * `computeVertexNormals` — see [`gridNormal`].
 *
 * The terrain is scaled to a fixed span so the camera framing holds for any track. Heights
 * are scaled by the same factor as the ground and then by [`RELIEF_EXAGGERATION`], so the
 * relief is proportionate everywhere — a flat supercross floor still looks flatter than a
 * hillside, it is just not drawn so shallow that nothing casts a shadow.
 */
/**
 * How world metres become view units.
 *
 * Everything drawn in the scene goes through this one function — the terrain grid, the
 * scenery standing on it, and the markers for what the track ships no mesh for. They are
 * placed in the same world frame by the track itself, and the only way they stay in it is by
 * being scaled and shifted by the same numbers.
 */
function viewFrame(terrain: TrackTerrain, lift = RELIEF_EXAGGERATION) {
  const { width, height, metresPerSample, minHeight, maxHeight } = terrain;
  // Metres across the widest edge, and the units-per-metre that fits it to the view.
  const spanMetres = Math.max(width - 1, height - 1) * metresPerSample;
  const unitsPerMetre = spanMetres > 0 ? VIEW_SPAN / spanMetres : 1;
  const step = metresPerSample * unitsPerMetre;
  return {
    unitsPerMetre,
    step,
    midHeight: (minHeight + maxHeight) / 2,
    // The Y scale heights go through, needed again to slope normals by the same amount.
    heightScale: unitsPerMetre * lift,
    originX: ((width - 1) * step) / 2,
    originZ: ((height - 1) * step) / 2,
  };
}

/** A standing ribbon over a stretch of track, for pointing at it from a list. */
function Highlight({
  terrain,
  at,
}: {
  terrain: TrackTerrain;
  at: { path: { x: number; z: number }[]; width: number };
}) {
  const lift = useContext(ReliefContext);
  const geometry = useMemo(() => {
    const frame = viewFrame(terrain, lift);
    // Tall enough to clear any relief the terrain has, and reaching below it too, so the
    // ribbon is visible whether the ground there is high or low.
    const tall = (terrain.maxHeight - terrain.minHeight) * frame.heightScale + 4;
    const verts: number[] = [];
    for (const p of at.path) {
      const [x, , z] = toView(frame, p.x, terrain.minHeight, p.z);
      verts.push(x, -tall / 2, z, x, tall / 2, z);
    }
    const index: number[] = [];
    for (let i = 0; i + 1 < at.path.length; i++) {
      const a = i * 2;
      index.push(a, a + 1, a + 2, a + 1, a + 3, a + 2);
    }
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(verts, 3));
    g.setIndex(index);
    return g;
  }, [terrain, at, lift]);

  if (at.path.length < 2) return null;
  return (
    <mesh geometry={geometry} renderOrder={2}>
      <meshBasicMaterial
        color="#ffffff"
        transparent
        opacity={0.14}
        depthWrite={false}
        side={THREE.DoubleSide}
      />
    </mesh>
  );
}

/** A line through the scene in world metres, height included, e.g. a rider's lap. */
export interface ViewerLine {
  points: [number, number, number][];
  colour: string;
  /** Pixels. */
  width?: number;
}

/** Lines placed through the same frame as the terrain, so a lap sits on the ground it rode. */
function Lines({ terrain, lines }: { terrain: TrackTerrain; lines: ViewerLine[] }) {
  const lift = useContext(ReliefContext);
  const placed = useMemo(() => {
    const frame = viewFrame(terrain, lift);
    return lines
      .filter((l) => l.points.length > 1)
      .map((l) => ({ ...l, points: l.points.map(([x, y, z]) => toView(frame, x, y, z)) }));
  }, [terrain, lines, lift]);
  return (
    <>
      {placed.map((l, i) => (
        <Line key={i} points={l.points} color={l.colour} lineWidth={l.width ?? 2} renderOrder={3} />
      ))}
    </>
  );
}

/**
 * A bike to draw on the track: where it is, which way it points, how it is leaned.
 *
 * Everything is in the world frame [`ViewerLine`] points are in, so a replay hands the same
 * numbers to both and the machine rides the line it is drawn beside.
 */
export interface ViewerActor {
  nodes: EdfNode[];
  /** The joints to pose about. Null draws the bike rigid, as an unassembled one always is. */
  rig: BikeRig | null;
  /** The model's own sheets, the way `BikeModel.base` carries them. */
  textures?: PaintTexture[];
  /** World metres. The ground under the tyres, not the middle of the machine. */
  at: [number, number, number];
  /** Heading, degrees: zero faces world +Z and positive turns towards world +X. */
  yaw: number;
  /** Lean, degrees, positive over to the RIDER'S RIGHT. */
  roll: number;
  /** Pitch, degrees, positive NOSE DOWN. */
  pitch: number;
  /**
   * Steering, fork, shock and wheel spin, as an OFFSET from where the bike settles.
   *
   * The same bargain `ModelViewer`'s `bikePoseOffset` makes, and for the same reason: the
   * settled pose is what stands the bike on both wheels, so replacing it outright leaves the
   * parts in the frame the model was authored in with the shock apparently collapsed.
   */
  pose?: Partial<BikePose> | null;
  /**
   * The rider's own body and kit, sat on the machine. Absent draws the bike alone.
   *
   * The same list `ModelViewer`'s `riderParts` takes, and seated the same way — a bike whose
   * `.geom` names no seat, or a body that brought no rig, rides riderless rather than wearing
   * a guess. See {@link canSeatRider}.
   */
  riderParts?: RiderPart[] | null;
  /** The rider's pose, a turn per bone — what `riderPoseFrom` makes of a replay sample. */
  riderPose?: RiderPose;
}

/** A bike wearing nothing but its own model. One instance, so an untextured bike settles. */
const NO_SHEETS: PaintTexture[] = [];

/**
 * One bike and its rider, standing on the track in the frame the lap lines are drawn in.
 *
 * The anchor goes through [`toView`] — the same call every line point makes — so the bike
 * cannot be anywhere but on the ground its own line is over, whatever [`RELIEF_EXAGGERATION`]
 * is doing to that ground. Its own metres are then scaled by `unitsPerMetre` rather than by
 * the taller `heightScale`, because a bike is one rigid machine: stretching it upright would
 * shear it the moment it leans. While relief is drawn true the two are the same number.
 *
 * What stands there is `ModelViewer`'s own `BikeOnGround` — joints, seating, lean and all —
 * so the machine out on the hillside is the machine the studio draws. This only says where.
 */
function TrackActor({ terrain, actor }: { terrain: TrackTerrain; actor: ViewerActor }) {
  const lift = useContext(ReliefContext);
  const { nodes, rig, yaw, roll, pitch, riderParts = null, riderPose } = actor;
  const [ax, ay, az] = actor.at;
  const tex = useTextureMap(actor.textures ?? NO_SHEETS);

  const settled = useMemo(() => settledPose(rig, nodes), [rig, nodes]);
  const pose = useMemo<BikePose>(() => {
    const o = actor.pose;
    if (!o) return settled;
    return {
      rearDrop: settled.rearDrop + (o.rearDrop ?? 0),
      forkUp: settled.forkUp + (o.forkUp ?? 0),
      steer: settled.steer + (o.steer ?? 0),
      spin: settled.spin + (o.spin ?? 0),
      spinRear: settled.spinRear + (o.spinRear ?? 0),
    };
  }, [settled, actor.pose]);

  const place = useMemo(() => {
    const frame = viewFrame(terrain, lift);
    return { at: toView(frame, ax, ay, az), scale: frame.unitsPerMetre };
  }, [terrain, lift, ax, ay, az]);
  const attitude = useMemo(() => ({ roll, pitch }), [roll, pitch]);
  // Only a pair that can actually be seated is offered a seat; the rest ride riderless rather
  // than with a body dropped on the swingarm pivot.
  const seat = canSeatRider(rig, riderParts) ? rig!.seat : null;

  // The canvas only draws when asked, and a bike that moves while nothing else does is
  // exactly the case that would otherwise sit still on screen.
  const invalidate = useThree((s) => s.invalidate);
  useEffect(
    () => invalidate(),
    [place, pose, attitude, yaw, tex, riderParts, riderPose, invalidate],
  );

  if (nodes.length === 0) return null;
  return (
    // Turned the other way about Y than the heading says, because `toView` negates X: a rider
    // bearing towards world +X is bearing towards -X here.
    <group
      position={place.at}
      rotation={[0, -yaw * THREE.MathUtils.DEG2RAD, 0]}
      scale={place.scale}
    >
      <BikeOnGround
        nodes={nodes}
        textures={tex}
        rig={rig}
        pose={pose}
        attitude={attitude}
        riderParts={riderParts}
        riderPose={riderPose}
        seat={seat}
      />
    </group>
  );
}

/**
 * Bring the camera to a point on the track.
 *
 * Sets the orbit target and pulls the camera back along the direction it was already
 * looking, so flying to a jump keeps whatever angle you had rather than snapping to a
 * canned one. Close enough to read a single face — the point of going there at all.
 */
/**
 * Keeps the camera pointed at something that is moving, without taking it over.
 *
 * `FocusCamera` places the camera: it picks a distance and an angle, which is right for landing
 * on a corner and wrong for following a bike, because it would undo the rider's own orbiting
 * and zoom on every frame. This only ever moves the target, and slides the camera by the same
 * amount, so where the rider put themselves is exactly where they stay.
 */
function FollowCamera({ terrain, at }: { terrain: TrackTerrain; at: { x: number; z: number } | null }) {
  const { camera, controls, invalidate } = useThree();
  const lift = useContext(ReliefContext);
  const was = useRef<THREE.Vector3 | null>(null);
  useEffect(() => {
    if (!at) {
      was.current = null;
      return;
    }
    const frame = viewFrame(terrain, lift);
    const [x, , z] = toView(frame, at.x, terrain.minHeight, at.z);
    const now = new THREE.Vector3(x, 0, z);
    const orbit0 = controls as unknown as { target?: THREE.Vector3 } | null;
    const orbit = controls as unknown as {
      target?: THREE.Vector3 & { set: (x: number, y: number, z: number) => void };
      update?: () => void;
    } | null;
    // Landing on a bike for the first time comes in close with it: a track is hundreds of
    // metres across and a bike is two, so the view that frames a whole circuit shows a rider a
    // dot. Only on arrival — after that the rider's own zoom is theirs to keep, and every frame
    // moves the camera by what the bike moved rather than placing it again.
    if (!was.current) {
      const from = camera.position.clone().sub(orbit0?.target ?? now);
      const back = from.lengthSq() > 1e-6 ? from.normalize() : new THREE.Vector3(0.5, 0.45, 0.5).normalize();
      const near = viewFrame(terrain, lift).unitsPerMetre * WATCH_M;
      camera.position.set(now.x + back.x * near, now.y + Math.max(back.y * near, near * 0.35), now.z + back.z * near);
    } else {
      camera.position.add(now.clone().sub(was.current));
    }
    was.current = now;
    orbit?.target?.set(now.x, now.y, now.z);
    orbit?.update?.();
    invalidate();
  }, [at, terrain, camera, controls, invalidate, lift]);
  return null;
}

function FocusCamera({
  terrain,
  focus,
}: {
  terrain: TrackTerrain;
  focus: { x: number; z: number } | null;
}) {
  const { camera, controls, invalidate } = useThree();
  const lift = useContext(ReliefContext);
  useEffect(() => {
    if (!focus) return;
    const frame = viewFrame(terrain, lift);
    const [x, , z] = toView(frame, focus.x, terrain.minHeight, focus.z);
    // The ground's own height at that point isn't known here, so aim at the middle of the
    // terrain's range: a jump is a metre of relief on ground that spans tens.
    const y = 0;
    const orbit = controls as unknown as {
      target?: { set: (x: number, y: number, z: number) => void };
      update?: () => void;
    } | null;
    const from = camera.position.clone().sub(new THREE.Vector3(x, y, z));
    // Whatever direction the camera was at, five view-units away — about a jump and a half.
    const back = from.lengthSq() > 1e-6 ? from.normalize() : new THREE.Vector3(0.6, 0.5, 0.6).normalize();
    camera.position.set(x + back.x * 5, y + Math.max(back.y * 5, 1.6), z + back.z * 5);
    orbit?.target?.set(x, y, z);
    camera.lookAt(x, y, z);
    orbit?.update?.();
    invalidate();
  }, [focus, terrain, camera, controls, invalidate, lift]);
  return null;
}

/**
 * Put one world-metre point where the terrain would put it.
 *
 * X is negated, the same conversion every model in the app goes through
 * (`edf::to_right_handed`): the game's frame is left-handed and three.js's is not.
 */
function toView(
  frame: ReturnType<typeof viewFrame>,
  wx: number,
  wy: number,
  wz: number,
): [number, number, number] {
  return [
    frame.originX - wx * frame.unitsPerMetre,
    (wy - frame.midHeight) * frame.heightScale,
    wz * frame.unitsPerMetre - frame.originZ,
  ];
}

/**
 * The same journey backwards: a point in the scene, in world metres.
 *
 * Read straight off [`toView`] rather than worked out again, so the two can't drift apart —
 * X is mirrored about the origin and Z is only shifted, and the scale is the one number both
 * axes go through. Height is left out: a place on a track is named by where it is on the
 * ground, and every caller matches it against a line that carries its own.
 */
function toWorld(
  frame: ReturnType<typeof viewFrame>,
  vx: number,
  vz: number,
): { x: number; z: number } {
  return {
    x: (frame.originX - vx) / frame.unitsPerMetre,
    z: (vz + frame.originZ) / frame.unitsPerMetre,
  };
}

/**
 * The sky overhead and the land beyond the track.
 *
 * Both are centred on the terrain rather than on their own origin: a track states them about
 * its middle, and the viewer puts the middle of the terrain at the middle of the scene.
 *
 * Drawn unlit and behind everything. A dome is a picture of a sky, not a surface with one —
 * shading it by a light the sky itself is supposed to be casting reads as a grey lid.
 */
/** A track's own colour, or a fallback, as something three.js can take. */
function colourOf(c: [number, number, number] | null, fallback: string): THREE.Color {
  return c ? new THREE.Color(c[0], c[1], c[2]) : new THREE.Color(fallback);
}

/**
 * The haze a track states, thinned to something you can see a track through.
 *
 * A track's own density is written for a rider looking a few hundred metres down a straight;
 * the viewer looks at all 550 m of the place at once, and at face value the far side vanishes.
 * The colour and the *relative* thickness are the track's — a misty circuit still reads
 * mistier than a dry one — while the depth it acts over is the view's.
 */
function TrackHaze({
  backdrop,
  terrain,
}: {
  backdrop: TrackBackdrop;
  terrain: TrackTerrain;
}) {
  const scene = useThree((s) => s.scene);
  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => {
    const density = backdrop.fogDensity ?? 0;
    if (density <= 0) {
      scene.fog = null;
      invalidate();
      return;
    }
    const span = Math.max(terrain.width - 1, terrain.height - 1) * terrain.metresPerSample;
    // Where the track's own haze would have swallowed a view this wide, hold it to a quarter
    // of the way — enough to sit the far edge into the sky without hiding what is on it.
    const swallow = 3 / Math.max(density, 1e-6); // metres to near-opaque at the stated density
    const strength = Math.min(1, span / Math.max(swallow, 1));
    const colour = colourOf(backdrop.fogColour ?? backdrop.skyColour, "#9fb0c4");
    // Held to the far edge. Anything nearer and the haze sits on the track itself, which
    // washes out the ground the view is about — the whole terrain is only ten units across,
    // and the camera watches it from about thirteen.
    scene.fog = new THREE.Fog(
      colour,
      VIEW_SPAN * 1.5,
      VIEW_SPAN * (6.5 - strength * 2.0),
    );
    invalidate();
    return () => {
      scene.fog = null;
    };
  }, [backdrop, terrain, scene, invalidate]);
  return null;
}

function Surrounds({
  backdrop,
  terrain,
}: {
  backdrop: TrackBackdrop;
  terrain: TrackTerrain;
}) {
  const lift = useContext(ReliefContext);
  const geometries = useMemo(() => {
    const frame = viewFrame(terrain, lift);
    // The middle of the terrain in world metres, which is where a track centres its sky.
    const midX = ((terrain.width - 1) * terrain.metresPerSample) / 2;
    const midZ = ((terrain.height - 1) * terrain.metresPerSample) / 2;
    const build = (m: TrackMeshArrays, minReach = 0) => {
      if (m.indices.length === 0) return null;
      // Some domes are authored as a unit sphere for the game to scale, and drawn at their
      // stated size they sit inside the terrain. Anything smaller than the track it is meant
      // to enclose is blown up to enclose it.
      let reach = 0;
      for (let i = 0; i < m.positions.length; i += 3) {
        const r = Math.hypot(m.positions[i], m.positions[i + 1], m.positions[i + 2]);
        if (r > reach) reach = r;
      }
      const scale = minReach > 0 && reach > 1e-3 && reach < minReach ? minReach / reach : 1;
      const out = new Float32Array(m.positions.length);
      for (let i = 0; i < m.positions.length; i += 3) {
        const [x, y, z] = toView(
          frame,
          m.positions[i] * scale + midX,
          m.positions[i + 1] * scale,
          m.positions[i + 2] * scale + midZ,
        );
        out[i] = x;
        out[i + 1] = y;
        out[i + 2] = z;
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(out, 3));
      g.setAttribute("uv", new THREE.BufferAttribute(m.uvs, 2));
      const idx = new Uint32Array(m.indices.length);
      for (let t = 0; t < m.indices.length; t += 3) {
        idx[t] = m.indices[t];
        idx[t + 1] = m.indices[t + 2];
        idx[t + 2] = m.indices[t + 1];
      }
      g.setIndex(new THREE.BufferAttribute(idx, 1));
      g.computeBoundingSphere();
      return g;
    };
    // The sky has to clear the terrain it covers; the backdrop is stated in real metres.
    const span = Math.max(terrain.width - 1, terrain.height - 1) * terrain.metresPerSample;
    const picture = (m: TrackMeshArrays) => {
      if (!m.picture) return null;
      const t = new THREE.DataTexture(
        m.picture.pixels,
        m.picture.width,
        m.picture.height,
        THREE.RGBAFormat,
      );
      t.colorSpace = THREE.SRGBColorSpace;
      t.wrapS = THREE.RepeatWrapping;
      t.wrapT = THREE.ClampToEdgeWrapping;
      t.minFilter = THREE.LinearMipmapLinearFilter;
      t.magFilter = THREE.LinearFilter;
      t.generateMipmaps = true;
      t.needsUpdate = true;
      return t;
    };
    return {
      sky: build(backdrop.sky, span * 1.6),
      land: build(backdrop.backdrop),
      skyMap: picture(backdrop.sky),
      landMap: picture(backdrop.backdrop),
    };
  }, [backdrop, terrain, lift]);

  useEffect(
    () => () => {
      geometries.sky?.dispose();
      geometries.land?.dispose();
      geometries.skyMap?.dispose();
      geometries.landMap?.dispose();
    },
    [geometries],
  );
  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => invalidate(), [geometries, invalidate]);

  const skyHex = colourOf(backdrop.skyColour, "#8fa6c4");
  const landHex = colourOf(backdrop.fogColour, "#93a2ad").lerp(
    new THREE.Color("#ffffff"),
    0.15,
  );

  return (
    <group>
      {geometries.sky && (
        // Never occludes: the dome is the far wall of the scene, so it draws first and takes
        // no part in depth.
        <mesh geometry={geometries.sky} renderOrder={-2}>
          <meshBasicMaterial
            key={geometries.skyMap ? "sky-picture" : "sky-plain"}
            map={geometries.skyMap ?? undefined}
            color={geometries.skyMap ? "#ffffff" : skyHex}
            side={THREE.BackSide}
            depthWrite={false}
            toneMapped={false}
          />
        </mesh>
      )}
      {geometries.land && (
        // Behind everything, like the dome: a backdrop is what a track puts at its horizon,
        // and some are authored as a shell that encloses the whole site rather than a ring
        // around it. Writing depth, such a shell sits between the camera and the track and
        // hides it — Sand Point opened as an empty blue sphere with the circuit inside it.
        <mesh geometry={geometries.land} renderOrder={-1}>
          <meshBasicMaterial
            key={geometries.landMap ? "land-picture" : "land-plain"}
            map={geometries.landMap ?? undefined}
            color={geometries.landMap ? "#ffffff" : landHex}
            side={THREE.DoubleSide}
            depthWrite={false}
            toneMapped={false}
          />
        </mesh>
      )}
    </group>
  );
}

function buildGeometry(
  terrain: TrackTerrain,
  textured: boolean,
  lift: number,
): THREE.BufferGeometry {
  const { width, height, heights } = terrain;

  const frame = viewFrame(terrain, lift);
  const { step, midHeight } = frame;

  // The ramp is spread over where the ground actually is, not over its extremes. A track's
  // full range is set by whatever sits at its edges — a boundary wall, a quarry face, one
  // stray sample — and keying colour to that leaves the entire riding area inside a single
  // band of the ramp, which is what made every track read as one flat brown.
  const [rampLow, rampHigh] = reliefBand(heights);
  const relief = rampHigh - rampLow;
  const cavity = cavityShade(heights, width, height);

  const count = width * height;
  const positions = new Float32Array(count * 3);
  const colors = new Float32Array(count * 3);
  const normals = new Float32Array(count * 3);
  // The overview map is drawn to the track's own footprint, so it lays across the grid
  // corner to corner — the same ground, at a different resolution.
  const uvs = new Float32Array(count * 2);
  const colour = new THREE.Color();

  const { heightScale, originX, originZ } = frame;

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = y * width + x;
      const metres = heights[i];
      const o = i * 3;
      // X is negated, the same conversion every model in the app goes through
      // (`edf::to_right_handed`): the game's frame is left-handed and three.js's is not.
      // Without it the whole track is mirrored — every left-hander rides as a right-hander.
      positions[o] = originX - x * step;
      positions[o + 1] = (metres - midHeight) * heightScale;
      positions[o + 2] = y * step - originZ;

      gridNormal(heights, width, height, x, y, i, step, heightScale, normals, o);

      // Vertex colour carries the cavity shading whether or not there is a texture: with
      // one, three.js multiplies the two, so the surface keeps its own colours and gains the
      // depth; without one, it darkens the elevation ramp the same way.
      const shade = cavity[i];
      if (textured) {
        // Neutral: the surface picture already says what colour the ground is, and tinting
        // it by elevation on top would report a height as a change of material.
        colors[o] = shade;
        colors[o + 1] = shade;
        colors[o + 2] = shade;
      } else {
        rampAt(relief > 0 ? (metres - rampLow) / relief : 0.5, colour);
        colors[o] = colour.r * shade;
        colors[o + 1] = colour.g * shade;
        colors[o + 2] = colour.b * shade;
      }

      const u = i * 2;
      uvs[u] = width > 1 ? x / (width - 1) : 0;
      // Not flipped. A `DataTexture` is uploaded as it arrives (`flipY` is false on it,
      // unlike every other texture three.js makes), so the first row of pixels is V zero —
      // and the decoder hands back the top row first, which is the row the grid starts at.
      uvs[u + 1] = height > 1 ? y / (height - 1) : 0;
    }
  }

  // Two triangles per cell, wound so their faces point up *after* X is negated — mirroring
  // an axis reverses winding, and the same order that faced upward before would now have the
  // terrain lit from underneath. 32-bit indices throughout: a 256² grid already needs more
  // than 65 536 vertices, so the 16-bit array would silently wrap.
  const cells = (width - 1) * (height - 1);
  const indices = new Uint32Array(cells * 6);
  let at = 0;
  for (let y = 0; y < height - 1; y += 1) {
    for (let x = 0; x < width - 1; x += 1) {
      const a = y * width + x;
      const b = a + 1;
      const c = a + width;
      const d = c + 1;
      indices[at] = a;
      indices[at + 1] = b;
      indices[at + 2] = c;
      indices[at + 3] = b;
      indices[at + 4] = d;
      indices[at + 5] = c;
      at += 6;
    }
  }

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
  geometry.setAttribute("normal", new THREE.BufferAttribute(normals, 3));
  geometry.setAttribute("uv", new THREE.BufferAttribute(uvs, 2));
  geometry.setIndex(new THREE.BufferAttribute(indices, 1));
  geometry.computeBoundingSphere();
  return geometry;
}

/**
 * Where a ray meets the ground, without walking the mesh.
 *
 * three.js's own intersection tests every triangle, and the fine grid is eight million of
 * them — measured here at 0.41 s a ray, and the event system casts one on every press and
 * every release as well as on the click itself. So the ground is intersected as what it is:
 * a heightfield, marched half a cell at a time until the ray first drops under the surface
 * and then halved down to the crossing. That costs a few thousand height lookups whatever
 * the grid's size, and it is the mesh's own heights it looks them up in.
 *
 * The grid is built already placed, and nothing above it is moved or scaled, so the ray
 * arrives in the same units the positions are in and needs no transform.
 */
function groundRaycast(
  self: { current: THREE.Mesh | null },
  terrain: TrackTerrain,
  frame: ReturnType<typeof viewFrame>,
): THREE.Mesh["raycast"] {
  const { width, height, heights, minHeight, maxHeight } = terrain;
  const { step, midHeight, heightScale, originX, originZ } = frame;
  // The surface between four samples, in view units, at a fractional place on the grid.
  const surfaceAt = (gx: number, gy: number): number => {
    const x0 = Math.min(Math.max(Math.floor(gx), 0), width - 1);
    const y0 = Math.min(Math.max(Math.floor(gy), 0), height - 1);
    const x1 = Math.min(x0 + 1, width - 1);
    const y1 = Math.min(y0 + 1, height - 1);
    const fx = Math.min(Math.max(gx - x0, 0), 1);
    const fy = Math.min(Math.max(gy - y0, 0), 1);
    const h =
      heights[y0 * width + x0] * (1 - fx) * (1 - fy) +
      heights[y0 * width + x1] * fx * (1 - fy) +
      heights[y1 * width + x0] * (1 - fx) * fy +
      heights[y1 * width + x1] * fx * fy;
    return (h - midHeight) * heightScale;
  };
  const lo = [-originX, (minHeight - midHeight) * heightScale, -originZ];
  const hi = [originX, (maxHeight - midHeight) * heightScale, originZ];
  return (raycaster, intersects) => {
    const mesh = self.current;
    if (!mesh) return;
    const { origin, direction } = raycaster.ray;
    const o = [origin.x, origin.y, origin.z];
    const d = [direction.x, direction.y, direction.z];
    // The stretch of the ray that is inside the box the grid occupies. Outside it there is
    // no ground to meet, and marching the whole ray instead would be mostly empty sky.
    let enter = Math.max(raycaster.near, 0);
    let leave = raycaster.far;
    for (let a = 0; a < 3; a += 1) {
      if (Math.abs(d[a]) < 1e-9) {
        if (o[a] < lo[a] || o[a] > hi[a]) return;
        continue;
      }
      const a0 = (lo[a] - o[a]) / d[a];
      const a1 = (hi[a] - o[a]) / d[a];
      enter = Math.max(enter, Math.min(a0, a1));
      leave = Math.min(leave, Math.max(a0, a1));
    }
    if (leave < enter) return;
    // How far the ray is above the ground under it, at a distance along it.
    const gap = (t: number): number =>
      o[1] +
      d[1] * t -
      surfaceAt((originX - (o[0] + d[0] * t)) / step, (o[2] + d[2] * t + originZ) / step);
    if (gap(enter) <= 0) return;
    const march = step / 2;
    const steps = Math.ceil((leave - enter) / march);
    let last = enter;
    for (let i = 1; i <= steps; i += 1) {
      const t = Math.min(enter + i * march, leave);
      if (gap(t) > 0) {
        last = t;
        continue;
      }
      let above = last;
      let below = t;
      // Sixteen halvings put the crossing well inside a millimetre of a cell.
      for (let k = 0; k < 16; k += 1) {
        const mid = (above + below) / 2;
        if (gap(mid) > 0) above = mid;
        else below = mid;
      }
      intersects.push({
        distance: below,
        point: raycaster.ray.at(below, new THREE.Vector3()),
        object: mesh,
      });
      return;
    }
  };
}

/** Metres one tile of the ground sheet covers. Small enough to read as grain up close,
 *  large enough not to shimmer when the whole track is in frame. */
const GROUND_TILE_METRES = 4;

/** How far the grain is allowed to swing the ground's brightness. */
const GROUND_STRENGTH = 0.5;


/** Heights are drawn this much taller than they are; 1 in the game view. */
const ReliefContext = createContext(RELIEF_EXAGGERATION);

/** One layer of the ground stack, as the GPU takes it. */
interface StackLayer {
  sheet: THREE.DataTexture;
  mask: THREE.DataTexture | null;
  tileU: number;
  tileV: number;
  bump: THREE.DataTexture | null;
  bumpTileU: number;
  bumpTileV: number;
}

/**
 * The ground the way the game's own shader draws it (GLSL in `mxbikes.exe`).
 *
 * Each layer is lit on its own, `sheet * clamp(ambient + sun * diffuse, 0, 1)`, and mixed in
 * by its mask. Diffuse comes from the mesh normal per vertex, or per pixel from the layer's
 * bump map where it has one (tangent space, `rgb * 2 - 1`). One sun and the ambient from the
 * track's `.amb`, exponential fog, and no gamma or tone mapping: textures and light go
 * straight to the screen. Left out: the game's projected shadows and the bump layers'
 * specular, whose strength the `.map` records don't give us yet.
 *
 * With `aids` the hollows are darkened too (the view's cavity term, carried in the vertex
 * colour), which the game doesn't do. It is the only difference from the game view.
 */
function gameGroundMaterial(
  stack: StackLayer[],
  backdrop: TrackBackdrop | null,
  metresPerUnit: number,
  maxTextures: number,
  aids: boolean,
): THREE.ShaderMaterial {
  const sun = backdrop?.sun ?? [8, 6, 4];
  // Into the view's frame: X is mirrored, like every vertex.
  const sunDir = new THREE.Vector3(-sun[0], sun[1], sun[2]).normalize();
  const rgb = (c: [number, number, number] | null, d: [number, number, number]) =>
    new THREE.Vector3(...(c ?? d));
  const uniforms: Record<string, THREE.IUniform> = {
    uSunDir: { value: sunDir },
    uAmbient: { value: rgb(backdrop?.ambientColour ?? null, [0.4, 0.45, 0.55]) },
    uSun: { value: rgb(backdrop?.sunColour ?? null, [1, 1, 1]) },
    uFogColour: { value: rgb(backdrop?.fogColour ?? backdrop?.skyColour ?? null, [0.7, 0.7, 0.85]) },
    uFogDensity: { value: backdrop?.fogDensity ?? 0 },
    uMetresPerUnit: { value: metresPerUnit },
  };
  // Samplers are the budget: most GPUs give a fragment shader 16, and seven layers with masks
  // and bump maps want twenty. A whole-ground bump map is kept first, since it carries the
  // ruts, then tiling ones from the top layer down while any are left.
  let spare = maxTextures - stack.length - stack.filter((l) => l.mask).length;
  const keep = stack.map((l) => !!l.bump && l.bumpTileU <= 1.001 && l.bumpTileV <= 1.001);
  spare -= keep.filter(Boolean).length;
  for (let i = stack.length - 1; i >= 0 && spare > 0; i -= 1) {
    if (stack[i].bump && !keep[i]) {
      keep[i] = true;
      spare -= 1;
    }
  }
  const decls: string[] = [];
  const blend: string[] = [];
  stack.forEach((l, i) => {
    uniforms[`uSheet${i}`] = { value: l.sheet };
    uniforms[`uTile${i}`] = { value: new THREE.Vector2(l.tileU, l.tileV) };
    decls.push(`uniform sampler2D uSheet${i};`, `uniform vec2 uTile${i};`);
    if (l.mask) {
      uniforms[`uMask${i}`] = { value: l.mask };
      decls.push(`uniform sampler2D uMask${i};`);
    }
    if (l.bump && keep[i]) {
      uniforms[`uBump${i}`] = { value: l.bump };
      uniforms[`uBumpTile${i}`] = { value: new THREE.Vector2(l.bumpTileU, l.bumpTileV) };
      decls.push(`uniform sampler2D uBump${i};`, `uniform vec2 uBumpTile${i};`);
    }
    const diffuse = l.bump && keep[i] ? `bumped(texture2D(uBump${i}, vUv * uBumpTile${i}))` : "vDiffuse";
    const lit = `lit(texture2D(uSheet${i}, vUv * uTile${i}).rgb, ${diffuse})`;
    blend.push(
      i === 0 || !l.mask ? `  c = ${lit};` : `  c = mix(c, ${lit}, texture2D(uMask${i}, vUv).r);`,
    );
  });
  return new THREE.ShaderMaterial({
    uniforms,
    fog: false,
    lights: false,
    toneMapped: false,
    vertexColors: aids,
    vertexShader: `
      uniform vec3 uSunDir;
      uniform float uMetresPerUnit;
      varying vec2 vUv;
      varying vec3 vNormalW;
      varying float vDiffuse;
      varying float vDepth;
      varying float vCavity;
      void main() {
        vUv = uv;
        #ifdef USE_COLOR
          vCavity = color.r;
        #else
          vCavity = 1.0;
        #endif
        vec3 n = normalize(mat3(modelMatrix) * normal);
        vNormalW = n;
        // Plain layers are lit per vertex, as the game does, and interpolated.
        vDiffuse = max(dot(uSunDir, n), 0.0);
        vec4 mv = modelViewMatrix * vec4(position, 1.0);
        vDepth = -mv.z * uMetresPerUnit;
        gl_Position = projectionMatrix * mv;
      }`,
    fragmentShader: `
      uniform vec3 uSunDir;
      uniform vec3 uAmbient;
      uniform vec3 uSun;
      uniform vec3 uFogColour;
      uniform float uFogDensity;
      varying vec2 vUv;
      varying vec3 vNormalW;
      varying float vDiffuse;
      varying float vDepth;
      varying float vCavity;
      ${decls.join("\n      ")}
      vec3 lit(vec3 col, float d) {
        return col * clamp(uAmbient + uSun * d, 0.0, 1.0);
      }
      float bumped(vec4 tex) {
        vec3 t = normalize(tex.xyz * 2.0 - 1.0);
        vec3 N = normalize(vNormalW);
        // Tangent along +u, which runs toward -X because the mesh is mirrored; bitangent +v.
        vec3 T = normalize(vec3(-1.0, 0.0, 0.0) - N * dot(N, vec3(-1.0, 0.0, 0.0)));
        vec3 B = cross(N, T);
        return max(dot(uSunDir, normalize(T * t.x + B * t.y + N * t.z)), 0.0);
      }
      void main() {
        vec3 c = vec3(0.0);
      ${blend.join("\n      ")}
        c *= vCavity;
        c = mix(uFogColour, c, exp(-uFogDensity * vDepth));
        gl_FragColor = vec4(c, 1.0);
      }`,
  });
}

function TerrainMesh({
  terrain,
  overview,
  ground,
  layers,
  game,
  backdrop,
  onGroundClick,
}: {
  terrain: TrackTerrain;
  overview: TrackOverview | null;
  /** A tiling sheet of the track's own ground, multiplied in for close-up detail. */
  ground: TrackGround | null;
  /** The ground the game draws: the track's own sheets, through the track's own masks. */
  layers: TrackGroundLayer[];
  /** Draw the ground the way the game's own shader does. */
  game: boolean;
  backdrop: TrackBackdrop | null;
  /** Told where a click landed on the ground, in world metres. */
  onGroundClick?: (at: { x: number; z: number }) => void;
}) {
  // The stack is the ground when a track states one. Everything below — the surface picture
  // built from the physics masks, the single sheet tiled everywhere, the elevation ramp — is
  // what to draw when it doesn't.
  const stacked = layers.length > 0;
  // A track with no surface data of its own still has ground: its sheet says what colour that
  // is, so the elevation ramp is only reached for when a track states neither.
  const tinted = overview != null || ground != null || stacked;
  const lift = useContext(ReliefContext);
  const geometry = useMemo(
    () => buildGeometry(terrain, tinted, lift),
    [terrain, tinted, lift],
  );

  // Its own intersection, for the same reason the mesh is built by hand: see [`groundRaycast`].
  const self = useRef<THREE.Mesh | null>(null);
  const picker = useMemo(
    () => groundRaycast(self, terrain, viewFrame(terrain, lift)),
    [terrain, lift],
  );

  // Built once per picture and handed to the GPU as-is. `sRGB` because it's artwork rather
  // than measurements: skipping that draws the whole track washed out.
  const texture = useMemo(() => {
    if (!overview) return null;
    const t = new THREE.DataTexture(
      overview.pixels,
      overview.width,
      overview.height,
      THREE.RGBAFormat,
    );
    t.colorSpace = THREE.SRGBColorSpace;
    t.minFilter = THREE.LinearMipmapLinearFilter;
    t.magFilter = THREE.LinearFilter;
    t.generateMipmaps = true;
    t.anisotropy = 4;
    t.needsUpdate = true;
    return t;
  }, [overview]);

  // The detail sheet, tiled. The surface picture says what the ground *is* at a third of a
  // metre; this says what it is made of, at whatever resolution you care to look.
  // How many times the sheet repeats across the whole grid.
  const repeat = useMemo(() => {
    const span = Math.max(terrain.width - 1, terrain.height - 1) * terrain.metresPerSample;
    return Math.max(1, Math.round(span / GROUND_TILE_METRES));
  }, [terrain]);

  const tile = (
    sheet: TrackSceneryTexture | null,
    srgb: boolean,
    repeats: number,
  ): THREE.DataTexture | null => {
    if (!sheet) return null;
    const t = new THREE.DataTexture(
      sheet.pixels,
      sheet.width,
      sheet.height,
      THREE.RGBAFormat,
    );
    // A normal map is direction data, not a picture — read it linearly or the relief is wrong.
    if (srgb) t.colorSpace = THREE.SRGBColorSpace;
    t.wrapS = THREE.RepeatWrapping;
    t.wrapT = THREE.RepeatWrapping;
    t.minFilter = THREE.LinearMipmapLinearFilter;
    t.magFilter = THREE.LinearFilter;
    t.generateMipmaps = true;
    t.anisotropy = 8;
    t.repeat.set(repeats, repeats);
    t.needsUpdate = true;
    return t;
  };

  const { detail, relief } = useMemo(
    () => ({
      detail: tile(ground?.colour ?? null, true, repeat),
      relief: tile(ground?.normal ?? null, false, repeat),
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ground, repeat],
  );

  // The stack, as the GPU takes it: a sheet per layer wrapped for tiling, and its mask laid
  // once across the whole ground. Masks are read as a single channel — they are coverage, not
  // colour — which is a quarter of the memory of the same map as RGBA.
  const stack = useMemo(() => {
    return layers.map((l) => {
      const sheet = new THREE.DataTexture(
        l.sheet.pixels,
        l.sheet.width,
        l.sheet.height,
        THREE.RGBAFormat,
      );
      sheet.wrapS = THREE.RepeatWrapping;
      sheet.wrapT = THREE.RepeatWrapping;
      sheet.minFilter = THREE.LinearMipmapLinearFilter;
      sheet.magFilter = THREE.LinearFilter;
      sheet.generateMipmaps = true;
      sheet.anisotropy = 8;
      sheet.needsUpdate = true;
      let mask: THREE.DataTexture | null = null;
      if (l.mask) {
        mask = new THREE.DataTexture(
          l.mask.coverage,
          l.mask.width,
          l.mask.height,
          THREE.RedFormat,
        );
        // Clamped, never wrapped: a mask covers the ground once, and a repeat would tile the
        // riding line across the whole map.
        mask.wrapS = THREE.ClampToEdgeWrapping;
        mask.wrapT = THREE.ClampToEdgeWrapping;
        mask.minFilter = THREE.LinearMipmapLinearFilter;
        mask.magFilter = THREE.LinearFilter;
        mask.generateMipmaps = true;
        mask.needsUpdate = true;
      }
      let bump: THREE.DataTexture | null = null;
      if (l.bump) {
        bump = new THREE.DataTexture(l.bump.pixels, l.bump.width, l.bump.height, THREE.RGBAFormat);
        // Direction data, read linearly. A whole-ground map is laid once, so it is clamped.
        const once = l.bumpTileU <= 1.001 && l.bumpTileV <= 1.001;
        bump.wrapS = once ? THREE.ClampToEdgeWrapping : THREE.RepeatWrapping;
        bump.wrapT = bump.wrapS;
        bump.minFilter = THREE.LinearMipmapLinearFilter;
        bump.magFilter = THREE.LinearFilter;
        bump.generateMipmaps = true;
        bump.anisotropy = 8;
        bump.needsUpdate = true;
      }
      const layer: StackLayer = {
        sheet,
        mask,
        tileU: l.tileU,
        tileV: l.tileV,
        bump,
        bumpTileU: l.bumpTileU,
        bumpTileV: l.bumpTileV,
      };
      return layer;
    });
  }, [layers]);


  useEffect(
    () => () =>
      stack.forEach((l) => {
        l.sheet.dispose();
        l.mask?.dispose();
        l.bump?.dispose();
      }),
    [stack],
  );

  const maxTextures = useThree((s) => s.gl.capabilities.maxTextures);
  const gameMaterial = useMemo(() => {
    if (stack.length === 0) return null;
    return gameGroundMaterial(
      stack,
      backdrop,
      1 / viewFrame(terrain, lift).unitsPerMetre,
      maxTextures,
      !game,
    );
  }, [game, stack, backdrop, terrain, lift, maxTextures]);
  useEffect(() => () => gameMaterial?.dispose(), [gameMaterial]);

  // The sheet's own average brightness. Dividing by it is what makes this a *detail* layer:
  // the grain then averages to no change at all, so a dark sheet adds texture instead of
  // dragging the whole track darker — which is what tiling a dirt or grass sheet raw did.
  // Its average colour too, which is what a track with no surface data of its own is drawn
  // in: the ramp reported height as if it were material, so a sand circuit came out banded
  // green-to-white. One honest colour beats a legend nobody asked for.
  const { meanLuma, meanHex } = useMemo(() => {
    if (!ground) return { meanLuma: 1, meanHex: "#ffffff" };
    const px = ground.colour.pixels;
    let total = 0;
    const rgb = [0, 0, 0];
    // Every sixteenth pixel: an average over a quarter of a million samples does not need
    // all of them, and this runs on the main thread while a track is opening.
    let n = 0;
    for (let i = 0; i < px.length; i += 64) {
      total += (px[i] * 0.299 + px[i + 1] * 0.587 + px[i + 2] * 0.114) / 255;
      for (let k = 0; k < 3; k += 1) rgb[k] += px[i + k];
      n += 1;
    }
    if (n === 0) return { meanLuma: 1, meanHex: "#ffffff" };
    // Lifted towards white: the sheet's average is the colour of ground in shade, and the
    // terrain is lit on top of it, so handing over the average raw draws the track too dark.
    const c = new THREE.Color(
      rgb[0] / n / 255,
      rgb[1] / n / 255,
      rgb[2] / n / 255,
    ).convertSRGBToLinear();
    return {
      meanLuma: Math.max(total / n, 0.02),
      meanHex: `#${c.lerp(new THREE.Color("#ffffff"), 0.35).getHexString()}`,
    };
  }, [ground]);
  const tint = overview ? "#ffffff" : meanHex;

  // A grid this size is megabytes of GPU buffers, and the viewer replaces it every time the
  // detail level changes — without this each one would be leaked.
  useEffect(() => () => geometry.dispose(), [geometry]);
  useEffect(
    () => () => {
      detail?.dispose();
      relief?.dispose();
    },
    [detail, relief],
  );
  useEffect(() => () => texture?.dispose(), [texture]);

  // The canvas only draws when asked, and the map arrives well after the terrain settled —
  // so without this the texture sits on a material nothing ever repaints.
  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => invalidate(), [texture, geometry, gameMaterial, invalidate]);

  return (
    // Both cast and receive: the terrain is the only thing in the scene, so every shadow it
    // shows is its own — a jump face darkening the ground in front of it, a berm shading its
    // own inside. That self-shadowing is most of what makes the relief read as ground.
    <mesh
      ref={self}
      geometry={geometry}
      castShadow
      receiveShadow
      raycast={picker}
      onClick={
        onGroundClick &&
        ((e) => {
          // A press that ends where it began. Orbiting ends in a click too — the browser
          // sends one whenever a drag starts and finishes on the same element — and two
          // pixels is what the event system itself treats as having stood still.
          if (e.delta > 2) return;
          e.stopPropagation();
          onGroundClick(toWorld(viewFrame(terrain, lift), e.point.x, e.point.z));
        })
      }
    >
      {/* Flat-ish and unshiny: dirt, and it keeps the relief legible rather than glared out.
          Vertex colours stay on with a texture, because three.js multiplies the two: the
          surface keeps the colours the track states while the cavity shading underneath gives
          its hollows depth. Without a texture the same vertex colours carry the elevation
          ramp instead. */}
      {/* Keyed on whether there's a texture, so the material is rebuilt rather than mutated
          when one arrives. Both taking a `map` and dropping `vertexColors` change the shader
          three.js compiles, and assigning them to a live material leaves it running the
          program it was built with — the terrain keeps its elevation ramp and never shows the
          picture at all. */}
      {gameMaterial ? (
        <primitive key="game" object={gameMaterial} attach="material" />
      ) : (
      <meshStandardMaterial
        key={`${texture ? "textured" : "plain"}-${detail ? "grain" : "flat"}-${
          relief ? "relief" : "smooth"
        }-${repeat}-stack${stack.length}`}
        color={stacked ? "#ffffff" : tint}
        map={stacked ? undefined : (texture ?? undefined)}
        normalMap={relief ?? undefined}
        // Gentle: this is one ground sheet standing in for every surface a track has, so it
        // should suggest a texture underfoot rather than emboss the whole place.
        normalScale={new THREE.Vector2(0.45, 0.45)}
        vertexColors
        roughness={0.95}
        metalness={0}
        onBeforeCompile={(shader) => {
          if (!detail) return;
          shader.uniforms.groundMap = { value: detail };
          shader.uniforms.groundRepeat = { value: repeat };
          shader.uniforms.groundMean = { value: meanLuma };
          shader.uniforms.groundStrength = { value: GROUND_STRENGTH };
          // Its own varying rather than the map's. `vMapUv` exists only where three.js
          // compiled in a `map`, so on a track that states no surfaces — no picture, so no
          // map — this read a name the shader had never declared, the program failed to
          // build, and the terrain drew nothing at all while everything around it drew fine.
          shader.vertexShader = shader.vertexShader
            .replace(
              "#include <common>",
              `#include <common>
               varying vec2 vGroundUv;`,
            )
            .replace(
              "#include <begin_vertex>",
              `#include <begin_vertex>
               vGroundUv = uv;`,
            );
          shader.fragmentShader = shader.fragmentShader
            .replace(
              "#include <common>",
              `#include <common>
               varying vec2 vGroundUv;
               uniform sampler2D groundMap;
               uniform float groundRepeat;
               uniform float groundMean;
               uniform float groundStrength;`,
            )
            // After the base colour is settled, modulate it about its own mean so the sheet
            // adds grain without shifting the ground's colour towards the sheet's.
            .replace(
              "#include <color_fragment>",
              `#include <color_fragment>
               {
                 vec3 grain = texture2D(groundMap, vGroundUv * groundRepeat).rgb;
                 float lum = dot(grain, vec3(0.299, 0.587, 0.114));
                 // Around one, so the grain varies the ground without darkening it. Only the
                 // sheet's luminance is used — its hue is its own surface's, not this one's.
                 float f = clamp(lum / groundMean, 0.45, 1.9);
                 diffuseColor.rgb *= mix(1.0, f, groundStrength);
               }`,
            );
        }}
      />
      )}
    </mesh>
  );
}

/**
 * The scenery, moved from world metres into the terrain's frame.
 *
 * Mirroring X reverses handedness, so two things have to follow it or the whole mesh is lit
 * from inside: every normal's X flips with the positions, and every triangle is rewound.
 * Rewinding happens within each triangle, never across them, so the material groups keep
 * pointing at the triangles they were cut for.
 *
 * Heights go through the terrain's own exaggeration rather than true scale. A tent drawn at
 * 1.5× is the price of a tent that stands on the ground instead of hovering over it or
 * sinking into it, and the ground is what the view is about.
 */
function buildSceneryGeometry(
  scenery: TrackScenery,
  terrain: TrackTerrain,
  slotOf: Map<number, number>,
  lift: number,
): THREE.BufferGeometry {
  const frame = viewFrame(terrain, lift);
  const src = scenery.positions;
  const count = src.length / 3;

  const positions = new Float32Array(src.length);
  const normals = new Float32Array(src.length);
  for (let i = 0; i < count; i += 1) {
    const o = i * 3;
    const [x, y, z] = toView(frame, src[o], src[o + 1], src[o + 2]);
    positions[o] = x;
    positions[o + 1] = y;
    positions[o + 2] = z;
    normals[o] = -scenery.normals[o];
    normals[o + 1] = scenery.normals[o + 1];
    normals[o + 2] = scenery.normals[o + 2];
  }

  // Kept as the map states it. `toView` mirrors X, and a mirror already reverses which side
  // of a triangle you are looking at — so the game's own winding lands the right way round
  // here without a swap, and swapping as well turns every face back to front. Drawn
  // double-sided that shows up not as holes but as light: three.js flips a back face's
  // normal, and a lit floor an acre across renders as if the sun were under it.
  const indices = scenery.indices;

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute("normal", new THREE.BufferAttribute(normals, 3));
  geometry.setAttribute("uv", new THREE.BufferAttribute(scenery.uvs, 2));
  geometry.setIndex(new THREE.BufferAttribute(indices, 1));
  // One draw range per material. The backend already sorted the triangles so each
  // material's sit together, which is what keeps this to a few dozen groups.
  for (const g of scenery.groups) {
    const slot = slotOf.get(g.material);
    if (slot == null) continue;
    geometry.addGroup(g.triStart * 3, g.triCount * 3, slot);
  }
  geometry.computeBoundingSphere();
  return geometry;
}

/** How opaque a cut-out's alpha has to be to be drawn at all. */
const CUTOUT_THRESHOLD = 0.5;

/** What a click on the scenery landed on. */
export interface PickedPiece {
  id: number;
  triangles: number;
  /** Metres. */
  size: [number, number, number];
}

function SceneryMesh({
  scenery,
  surfaces,
  terrain,
  onPick,
  clickThrough = false,
}: {
  scenery: TrackScenery;
  surfaces: TrackSceneryTexture[];
  terrain: TrackTerrain;
  onPick?: (piece: PickedPiece | null) => void;
  /** Let clicks fall through to the ground instead of selecting a piece. */
  clickThrough?: boolean;
}) {
  const [picked, setPicked] = useState<number | null>(null);
  const lift = useContext(ReliefContext);
  // A material slot per surface, plus one plain slot at the end for the groups no surface
  // covers — the `.scr` props, whose own sheets aren't read.
  const { materials, slotOf } = useMemo(() => {
    const slots = new Map<number, number>();
    // A cut-out with no colour in it is a shadow, not a surface. The game multiplies one onto
    // what it falls across; drawn here as geometry with an alpha test it is a solid black
    // silhouette standing up out of the ground. Indiana ships 24,465 triangles of
    // `tunnel_shadow_c_a`, forty metres tall, and they are the black trees.
    const shadow = (t: TrackSceneryTexture): boolean => {
      if (!t.alpha) return false;
      let sum = 0;
      let n = 0;
      for (let i = 0; i < t.pixels.length; i += 4) {
        // Only what the alpha test would keep — the rest is the black behind the cut.
        if (t.pixels[i + 3] < 128) continue;
        sum += t.pixels[i] * 0.299 + t.pixels[i + 1] * 0.587 + t.pixels[i + 2] * 0.114;
        n += 1;
      }
      return n > 0 && sum / n < 4;
    };
    const list: THREE.Material[] = surfaces.map((t, i) => {
      if (!shadow(t)) slots.set(t.material, i);
      const map = new THREE.DataTexture(t.pixels, t.width, t.height, THREE.RGBAFormat);
      map.colorSpace = THREE.SRGBColorSpace;
      // The surfaces tile — a fence sheet repeats along its run — so anything but repeat
      // wrapping smears the last pixel of the sheet across the whole length of it.
      map.wrapS = THREE.RepeatWrapping;
      map.wrapT = THREE.RepeatWrapping;
      map.minFilter = THREE.LinearMipmapLinearFilter;
      map.magFilter = THREE.LinearFilter;
      map.generateMipmaps = true;
      map.anisotropy = 4;
      map.needsUpdate = true;
      return new THREE.MeshStandardMaterial({
        map,
        roughness: 0.9,
        metalness: 0,
        // Cut-outs are drawn with an alpha test rather than blending: the shapes are
        // foliage and crowd, which need to occlude each other correctly at any angle, and
        // that is what a test gives and sorting-dependent blending does not.
        alphaTest: t.alpha ? CUTOUT_THRESHOLD : 0,
        // Much of this is single-sided cloth and card — banners, tent walls, leaf sheets —
        // and culling back faces makes half of it vanish from one side.
        side: THREE.DoubleSide,
      });
    });
    const plain = new THREE.MeshStandardMaterial({
      color: "#9a9384",
      roughness: 0.9,
      metalness: 0,
      side: THREE.DoubleSide,
    });
    const fallback = list.length;
    list.push(plain);
    // Materials the map never bound get the plain slot — but a shadow keeps none, so its
    // triangles are left out of the geometry entirely rather than drawn in grey instead.
    const shadows = new Set(
      surfaces.filter((t) => shadow(t)).map((t) => t.material),
    );
    for (const g of scenery.groups) {
      if (!slots.has(g.material) && !shadows.has(g.material)) slots.set(g.material, fallback);
    }
    return { materials: list, slotOf: slots };
  }, [scenery, surfaces]);

  const geometry = useMemo(
    () => buildSceneryGeometry(scenery, terrain, slotOf, lift),
    [scenery, terrain, slotOf, lift],
  );

  // Tens of megabytes of GPU buffers and surfaces, replaced whenever the terrain's detail
  // level changes — without this each pass would leak the last one's.
  useEffect(() => () => geometry.dispose(), [geometry]);
  useEffect(
    () => () => {
      for (const m of materials) {
        const mat = m as THREE.MeshStandardMaterial;
        mat.map?.dispose();
        mat.dispose();
      }
    },
    [materials],
  );

  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => invalidate(), [geometry, materials, invalidate]);

  // The picked piece, as its own geometry — the triangles that share its id.
  const outline = useMemo(() => {
    if (picked == null || scenery.pieceOfTriangle.length === 0) return null;
    const src = geometry.getIndex();
    if (!src) return null;
    const keep: number[] = [];
    for (let t = 0; t < scenery.pieceOfTriangle.length; t += 1) {
      if (scenery.pieceOfTriangle[t] !== picked) continue;
      keep.push(src.getX(t * 3), src.getX(t * 3 + 1), src.getX(t * 3 + 2));
    }
    if (keep.length === 0) return null;
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", geometry.getAttribute("position"));
    g.setIndex(keep);
    g.computeBoundingSphere();
    return g;
  }, [picked, geometry, scenery.pieceOfTriangle]);

  useEffect(() => () => outline?.dispose(), [outline]);

  // Report what was picked, in metres, from the world positions rather than the view units.
  useEffect(() => {
    if (!onPick) return;
    if (picked == null) {
      onPick(null);
      return;
    }
    let count = 0;
    const lo = [Infinity, Infinity, Infinity];
    const hi = [-Infinity, -Infinity, -Infinity];
    for (let t = 0; t < scenery.pieceOfTriangle.length; t += 1) {
      if (scenery.pieceOfTriangle[t] !== picked) continue;
      count += 1;
      for (let k = 0; k < 3; k += 1) {
        const v = scenery.indices[t * 3 + k] * 3;
        for (let axis = 0; axis < 3; axis += 1) {
          const p = scenery.positions[v + axis];
          if (p < lo[axis]) lo[axis] = p;
          if (p > hi[axis]) hi[axis] = p;
        }
      }
    }
    onPick({
      id: picked,
      triangles: count,
      size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]],
    });
  }, [picked, scenery, onPick]);

  return (
    <>
      {/* Casting but not receiving: these are small things on a big ground, and their shadows
          are what place them on it, while shadows landing *on* them would cost a second pass
          over the whole mesh to darken pixels a few metres across. */}
      <mesh
        geometry={geometry}
        material={materials}
        castShadow
        onClick={(e) => {
          // When the viewer is being used to point at places on the track, the ground is
          // what a click means: a tent or a tree drawn over a corner is in front of the
          // rider's own line, not the thing they were aiming at.
          if (clickThrough) return;
          e.stopPropagation();
          const face = e.faceIndex;
          if (face == null || scenery.pieceOfTriangle.length === 0) return;
          const id = scenery.pieceOfTriangle[face];
          setPicked((was) => (was === id ? null : id));
        }}
      />
      {outline && (
        <mesh geometry={outline} renderOrder={2}>
          {/* Drawn over everything so a piece inside a crowd of others still reads. */}
          <meshBasicMaterial
            color="#ffb648"
            wireframe
            depthTest={false}
            transparent
            opacity={0.9}
            toneMapped={false}
          />
        </mesh>
      )}
    </>
  );
}

/** What each kind of fixture is drawn in. Distinct hues rather than a ramp: these are
 *  categories, not a quantity. */
const MARKER_COLOURS: Record<TrackPlacement["kind"], string> = {
  marshal: "#f0a63c",
  camera: "#5fb2f0",
  sound: "#b98cf0",
  prop: "#8de08a",
};

/** View units. Fixed rather than scaled from metres so a marker stays legible on a 200 m
 *  supercross floor and a 1 km circuit alike. */
const MARKER_HEIGHT = 0.11;
const MARKER_RADIUS = 0.016;

/**
 * Pins for what the track places but ships no mesh for — marshal posts, TV cameras, crowd
 * sound. Props are left out: their meshes are in the scenery, so a pin would double them.
 */
function PlacementMarkers({
  placements,
  terrain,
}: {
  placements: TrackPlacement[];
  terrain: TrackTerrain;
}) {
  const lift = useContext(ReliefContext);
  const pins = useMemo(() => {
    const frame = viewFrame(terrain, lift);
    return placements
      .filter((p) => p.kind !== "prop")
      .map((p, i) => ({
        key: `${p.kind}-${i}`,
        colour: MARKER_COLOURS[p.kind] ?? MARKER_COLOURS.prop,
        at: toView(frame, p.pos[0], p.pos[1], p.pos[2]),
      }));
  }, [placements, terrain, lift]);

  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => invalidate(), [pins, invalidate]);

  return (
    <group>
      {pins.map(({ key, colour, at }) => (
        // The stated position is where the thing sits on the ground, so the pin is raised by
        // half its length to stand on that point rather than be centred through it.
        <mesh key={key} position={[at[0], at[1] + MARKER_HEIGHT / 2, at[2]]}>
          <cylinderGeometry args={[MARKER_RADIUS, MARKER_RADIUS, MARKER_HEIGHT, 6]} />
          {/* Unlit: a marker is an annotation, not part of the scene, and shading it would
              let it disappear into a hillside at the wrong sun angle. */}
          <meshBasicMaterial color={colour} toneMapped={false} />
        </mesh>
      ))}
    </group>
  );
}

/** Legend for the orbit gestures — the canvas gives no other clue that it can be moved.
 *  Same wording and placement as the model viewer's, so the two read as one control. */
function ControlsHint() {
  const t = useT();
  const items = [
    { Icon: Rotate3d, label: t("viewer.dragToRotate") },
    { Icon: ZoomIn, label: t("viewer.scrollToZoom") },
    { Icon: Move, label: t("viewer.rightDragToPan") },
  ];
  return (
    <div className="pointer-events-none absolute bottom-2 left-2 flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md bg-white/[0.06] px-2 py-1 text-[11px] leading-none text-white/45">
      {items.map(({ Icon, label }) => (
        <span key={label} className="flex items-center gap-1">
          <Icon className="h-3.5 w-3.5" />
          {label}
        </span>
      ))}
    </div>
  );
}

interface TrackViewerProps {
  terrain: TrackTerrain | null;
  /** The track's overview map, when it ships one that covers the same ground. */
  overview?: TrackOverview | null;
  /** What stands on the ground, when the track's `.map` carries any. */
  scenery?: TrackScenery | null;
  /** The surfaces that paint it. Arrives after the mesh — until then it draws plain. */
  surfaces?: TrackSceneryTexture[];
  /** Marshal posts, TV cameras and sound sources — pinned points with no mesh. */
  placements?: TrackPlacement[];
  /** Whether to draw either of the two above. */
  showObjects?: boolean;
  /** Draw the ground the way the game's own shader does, at its true height. */
  gameView?: boolean;
  /** The sky and land a track wraps itself in, and the light it states. */
  backdrop?: TrackBackdrop | null;
  /** A tiling sheet of the track's own ground, for detail closer than its data carries. */
  ground?: TrackGround | null;
  /**
   * The ground the game draws: the track's own sheets through the track's own masks.
   *
   * When a track states a stack this replaces the surface picture entirely — that picture is
   * built from the `.trh` coverage masks, which are the physics surfaces rather than the paint.
   */
  groundLayers?: TrackGroundLayer[];
  /** Told what a click on the scenery landed on, and when the selection clears. */
  onPick?: (piece: PickedPiece | null) => void;
  /**
   * Told where a click landed on the ground, in world metres — the frame `lines`, `focus`
   * and the actor are given in, so what comes back can be matched straight against a lap.
   *
   * Given one, the scenery stops taking clicks for itself: the place under the tent is what
   * was meant. Absent, nothing about the view changes.
   */
  onGroundClick?: (at: { x: number; z: number }) => void;
  /**
   * A point in world metres to bring the camera to, or null to leave it alone.
   *
   * The identity matters as much as the value: passing a fresh object with the same
   * coordinates moves the camera again, which is what makes clicking the same row twice
   * bring you back to it after you have panned away.
   */
  focus?: { x: number; z: number } | null;
  /**
   * Somewhere moving to keep the camera pointed at, in world metres — a bike being replayed.
   *
   * Unlike `focus` this never chooses a distance or an angle: it slides the camera and its
   * target together, so the rider keeps whatever view they had and the subject stays in it.
   */
  follow?: { x: number; z: number } | null;
  /**
   * A stretch of track to light up: world-metre points along it, and how wide it is.
   *
   * A path rather than a point, because a straight is two hundred metres long and marking
   * only where it begins says almost nothing about which one it is. Drawn as a standing
   * ribbon — a highlight painted flat on the ground disappears the moment the camera is low,
   * which is exactly when you are looking at a jump.
   */
  highlight?: { path: { x: number; z: number }[]; width: number } | null;
  /** Lines to draw over the ground, in world metres with height. */
  lines?: ViewerLine[];
  /**
   * A bike to stand on the track — the rider's own machine, on the line they rode.
   *
   * Null or absent draws the scene the viewer has always drawn. See {@link ViewerActor}.
   */
  actor?: ViewerActor | null;
  className?: string;
}

export function TrackViewer({
  terrain,
  overview = null,
  scenery = null,
  surfaces = [],
  placements = [],
  showObjects = true,
  gameView = false,
  backdrop = null,
  ground = null,
  groundLayers = [],
  onPick,
  onGroundClick,
  focus = null,
  follow = null,
  highlight = null,
  lines = [],
  actor = null,
  className,
}: TrackViewerProps) {
  const lift = RELIEF_EXAGGERATION;
  return (
    <div className={cn("relative", className)}>
      <ErrorBoundary compact label="track-viewer">
        <Canvas
          className="h-full w-full"
          // Soft (PCF): a hard shadow map on ground this flat reads as speckle, because one
          // of its texels covers about one triangle of a grid this fine.
          shadows="soft"
          // Nothing in the scene animates, so a parked terrain costs no frames at all.
          frameloop="demand"
          dpr={[1, 1.5]}
          camera={{ position: [0, 7.5, 11], fov: 45, near: 0.01, far: 200 }}
          onCreated={({ gl, invalidate }) => {
            reportRenderer(gl, "track-viewer");
            gl.domElement.addEventListener(
              "webglcontextlost",
              (e) => {
                e.preventDefault();
                console.warn("[TrackViewer] WebGL context lost — awaiting restore");
              },
              false,
            );
            // Restoring doesn't touch React state, so on demand nothing would redraw.
            gl.domElement.addEventListener("webglcontextrestored", () => invalidate(), false);
          }}
        >
          {/* The track's own sky behind everything; the viewer's own dark ground only when a
              track ships none. */}
          <color
            attach="background"
            args={[
              backdrop?.skyColour
                ? `rgb(${backdrop.skyColour.map((c) => Math.round(c * 255)).join(",")})`
                : "#0e0f13",
            ]}
          />
          <ReliefContext.Provider value={lift}>
          {/* Keyed on the height scale, so everything placed through it is rebuilt. */}
          <group key={`lift-${lift}`}>
          {terrain && backdrop && <Surrounds backdrop={backdrop} terrain={terrain} />}
          {terrain && backdrop && <TrackHaze backdrop={backdrop} terrain={terrain} />}
          <ambientLight
            intensity={0.5}
            color={colourOf(backdrop?.ambientColour ?? null, "#ffffff")}
          />
          {/* Sky above, warm bounce below — enough to keep hollows from going solid black. */}
          <hemisphereLight args={[0xdfe8ff, 0x4a4133, 0.8]} />
          {/* Low and to one side: relief reads by its shadows, and an overhead key flattens
              it. This is the only light that casts — a second caster would double the cost to
              soften shadows the fill light below already softens for free.
              The shadow camera is bounded to the terrain's own span, which is fixed however
              big the real track is, so the whole map fits one map at full precision. */}
          <directionalLight
            position={
              backdrop?.sun
                ? [backdrop.sun[0], Math.max(backdrop.sun[1], 1), backdrop.sun[2]]
                : [8, 6, 4]
            }
            intensity={1.15}
            color={colourOf(backdrop?.sunColour ?? null, "#ffffff")}
            castShadow
            // Four times the map over a box barely wider than the terrain: the tighter the
            // camera and the denser the map, the smaller a shadow texel is against the
            // triangles it has to resolve, which is what decides whether ground shadows
            // itself into speckle.
            shadow-mapSize={[4096, 4096]}
            shadow-camera-left={-5.6}
            shadow-camera-right={5.6}
            shadow-camera-top={5.6}
            shadow-camera-bottom={-5.6}
            shadow-camera-near={0.5}
            shadow-camera-far={40}
            // Offsetting along the normal is what actually cures acne on a heightfield —
            // there is no back face to push the comparison onto, so the sample has to be
            // moved off the surface it is testing. About three triangles' worth: enough to
            // clear a shadow texel several times over, and small enough that a jump's shadow
            // still starts at the jump instead of floating clear of it.
            shadow-normalBias={0.02}
            shadow-bias={-0.0006}
          />
          <directionalLight position={[-6, 3, -5]} intensity={0.4} />
          {terrain && (
            <TerrainMesh
              terrain={terrain}
              overview={overview}
              ground={ground}
              layers={groundLayers}
              game={gameView}
              backdrop={backdrop}
              onGroundClick={onGroundClick}
            />
          )}
          {terrain && showObjects && scenery && (
            <SceneryMesh
              scenery={scenery}
              surfaces={surfaces}
              terrain={terrain}
              onPick={onPick}
              clickThrough={onGroundClick != null}
            />
          )}
          {terrain && showObjects && placements.length > 0 && (
            <PlacementMarkers placements={placements} terrain={terrain} />
          )}
          {terrain && <FocusCamera terrain={terrain} focus={focus} />}
          {terrain && <FollowCamera terrain={terrain} at={follow} />}
          {terrain && highlight && <Highlight terrain={terrain} at={highlight} />}
          {terrain && lines.length > 0 && <Lines terrain={terrain} lines={lines} />}
          {terrain && actor && <TrackActor terrain={terrain} actor={actor} />}
          </group>
          </ReliefContext.Provider>
          <OrbitControls
            makeDefault
            enablePan
            screenSpacePanning={false}
            zoomToCursor
            // Close enough to put the camera on the dirt and read a single jump face, far
            // enough to hold a 1 km circuit in frame.
            minDistance={0.15}
            maxDistance={60}
            // Stop the camera going under the ground, where the terrain is an unlit shell.
            maxPolarAngle={Math.PI / 2.05}
            target={[0, 0, 0]}
          />
        </Canvas>
      </ErrorBoundary>
      {terrain && <ControlsHint />}
    </div>
  );
}
