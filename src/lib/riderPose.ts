import * as THREE from "three";
import type { Bone } from "../types";

/**
 * Posing a rider.
 *
 * A pose is a turn per bone, in that bone's own frame, on top of the rest the model was
 * authored in — so an empty pose is the model exactly as it came, and every control here
 * starts at zero. Angles are degrees, because that is what the sliders show and what a saved
 * pose should still read as in a year.
 */
export type RiderPose = Record<string, [number, number, number]>;

export const NO_POSE: RiderPose = {};

/** Is this pose the model as authored? */
export function isRestPose(pose: RiderPose): boolean {
  return Object.values(pose).every((t) => t[0] === 0 && t[1] === 0 && t[2] === 0);
}

/** Drop the bones that were turned back to zero, so a saved pose stays small and readable. */
export function trimPose(pose: RiderPose): RiderPose {
  const out: RiderPose = {};
  for (const [bone, t] of Object.entries(pose)) {
    if (t[0] || t[1] || t[2]) out[bone] = t;
  }
  return out;
}

/** The bone's turn, or three zeros. Never returns the caller a shared array to mutate. */
export function turnOf(pose: RiderPose, bone: string): [number, number, number] {
  const t = pose[bone];
  return t ? [t[0], t[1], t[2]] : [0, 0, 0];
}

export function withTurn(
  pose: RiderPose,
  bone: string,
  turn: [number, number, number],
): RiderPose {
  return trimPose({ ...pose, [bone]: turn });
}

// ── The rig, as three.js wants it ────────────────────────────────────────────

const DEG = Math.PI / 180;

/** A row-major `number[16]` from the backend as a three.js matrix. */
export function toMatrix(m: number[]): THREE.Matrix4 {
  const out = new THREE.Matrix4();
  // `set` takes its arguments row-major, which is how the file stores them.
  out.set(
    m[0], m[1], m[2], m[3],
    m[4], m[5], m[6], m[7],
    m[8], m[9], m[10], m[11],
    m[12], m[13], m[14], m[15],
  );
  return out;
}

/**
 * A three.js bone tree matching `bones`, plus the skeleton that binds a mesh to it.
 *
 * Each bone's *local* transform is its rest placement seen from its parent, so the tree at
 * rest reproduces the bind matrices exactly and an unposed body draws as it always did.
 *
 * A bone is only ever hung off one earlier in the list — the rig comes back depth-first, so
 * that holds of every real one. It has to be a tree: a bone inside a cycle hangs off no root,
 * nothing ever works out where it is, and every vertex it holds is pulled into the origin.
 */
export function buildSkeleton(bones: Bone[]): {
  roots: THREE.Bone[];
  order: THREE.Bone[];
  skeleton: THREE.Skeleton;
} {
  const made = bones.map(() => new THREE.Bone());
  const roots: THREE.Bone[] = [];
  bones.forEach((b, i) => {
    const bone = made[i];
    bone.name = b.name;
    const parent = b.parent !== null && b.parent !== undefined && b.parent < i ? b.parent : null;
    const world = toMatrix(b.bind);
    const local =
      parent === null ? world : toMatrix(bones[parent].bind).invert().multiply(world);
    local.decompose(bone.position, bone.quaternion, bone.scale);
    // Kept so a pose can turn the bone without losing the rest it turns from.
    bone.userData.restQuaternion = bone.quaternion.clone();
    if (parent === null) roots.push(bone);
    else made[parent].add(bone);
  });
  const skeleton = new THREE.Skeleton(
    made,
    bones.map((b) => toMatrix(b.invBind)),
  );
  return { roots, order: made, skeleton };
}

/**
 * Turn the tree to `pose`.
 *
 * Every bone is reset to its rest first, so removing a bone from the pose puts it back rather
 * than leaving the last turn applied. The turn is composed after the rest, which is what makes
 * an elbow bend about the forearm's own axes instead of the model's.
 */
export function applyPose(order: THREE.Bone[], pose: RiderPose): void {
  const turn = new THREE.Quaternion();
  const euler = new THREE.Euler();
  for (const bone of order) {
    const rest = bone.userData.restQuaternion as THREE.Quaternion | undefined;
    if (!rest) continue;
    const t = pose[bone.name];
    if (!t) {
      bone.quaternion.copy(rest);
      continue;
    }
    euler.set(t[0] * DEG, t[1] * DEG, t[2] * DEG, "XYZ");
    turn.setFromEuler(euler);
    bone.quaternion.copy(rest).multiply(turn);
  }
  for (const bone of order) {
    if (!bone.parent) bone.updateMatrixWorld(true);
  }
}

/**
 * How far a bone has moved from where it was authored: posed world × rest world⁻¹.
 *
 * Gear is placed against the body's proportions rather than its rig, and that placement is
 * tuned. Rather than redo it, each piece is moved by this — identity at rest, so an unposed
 * rider wears its kit exactly where it did before, and a posed one carries it along.
 */
export function boneDelta(
  order: THREE.Bone[],
  bones: Bone[],
  name: string,
): THREE.Matrix4 | null {
  const at = bones.findIndex((b) => b.name === name);
  if (at < 0) return null;
  const rest = toMatrix(bones[at].bind);
  return order[at].matrixWorld.clone().multiply(rest.invert());
}

// ── Grabbing the rider ───────────────────────────────────────────────────────

/**
 * A grab point on the rider.
 *
 * `on` is the bone the dot rides, `turns` is the bone a drag swings — the joint above it. That
 * is what makes a drag read the way it does in Pivot: you take hold of the wrist and the
 * forearm rotates about the elbow, so the thing under the cursor is the thing that moves.
 */
export interface PoseHandle {
  /** The bone whose turn a drag writes. */
  turns: string;
  /** The bone the dot rides. */
  on: string;
  /** Sit at the far end of `on`'s own box rather than at its joint — see [[boneTip]]. */
  tip?: boolean;
}

const SIDES = ["Left", "Right"] as const;

/**
 * Where the dots go.
 *
 * Fourteen of them, at the joints somebody moving a rider actually reaches for. A chain's last
 * bone — the hand, the shin, the head — has no joint below it to grab, so its dot sits at the
 * end of its own box instead. Everything the sliders cover and this doesn't (the collars, twist
 * about a bone's own axis) is still a slider away.
 */
export const POSE_HANDLES: PoseHandle[] = [
  { turns: "riderRIG_Head", on: "riderRIG_Head", tip: true },
  { turns: "riderRIG_Neck1", on: "riderRIG_Head" },
  { turns: "riderRIG_Spine3", on: "riderRIG_Spine4" },
  { turns: "riderRIG_Spine1", on: "riderRIG_Spine2" },
  ...SIDES.flatMap((s) => [
    { turns: `riderRIG_${s}Shoulder`, on: `riderRIG_${s}Elbow` },
    { turns: `riderRIG_${s}Elbow`, on: `riderRIG_${s}Wrist` },
    { turns: `riderRIG_${s}Wrist`, on: `riderRIG_${s}Wrist`, tip: true },
    { turns: `riderRIG_${s}Hip`, on: `riderRIG_${s}Knee` },
    { turns: `riderRIG_${s}Knee`, on: `riderRIG_${s}Knee`, tip: true },
  ]),
];

/**
 * The far end of a bone, in its own space.
 *
 * Every bone carries a box covering the slice of mesh it moves, so the end of the limb is the
 * centre of whichever face of that box sits furthest from the joint the bone hangs off — the
 * foot on a shin, the fingers on a hand. Read off the model rather than written down, so it
 * lands in the right place on a rig with different proportions.
 */
export function boneTip(bones: Bone[], at: number): [number, number, number] {
  const bone = bones[at];
  const { aabbLo: lo, aabbHi: hi } = bone;
  const mid: [number, number, number] = [
    (lo[0] + hi[0]) / 2,
    (lo[1] + hi[1]) / 2,
    (lo[2] + hi[2]) / 2,
  ];
  const parent = bone.parent === null || bone.parent === undefined ? at : bone.parent;
  const from = new THREE.Vector3().setFromMatrixPosition(toMatrix(bones[parent].bind));
  const bind = toMatrix(bone.bind);
  let best = mid;
  let far = -1;
  for (let axis = 0; axis < 3; axis++) {
    for (const end of [lo[axis], hi[axis]]) {
      const face: [number, number, number] = [...mid];
      face[axis] = end;
      const d = new THREE.Vector3(...face).applyMatrix4(bind).distanceTo(from);
      if (d > far) {
        far = d;
        best = face;
      }
    }
  }
  return best;
}

/**
 * The pose that swings `turns` so the point at `from` lands on `to`.
 *
 * Both points are in world space, which is where a pointer lands. The turn is measured from
 * the model as authored rather than from wherever the last move left the bone: composing one
 * short turn onto another is path-dependent, and a limb dragged out and back would come home
 * pointing the right way but rolled about its own length. Measured from rest, the same place
 * on screen always means the same pose, and dragging back to where you started is rest again.
 *
 * Worked out as matrices rather than quaternions because the rig is mirrored on the way in —
 * a bone's world matrix is left-handed, and its "rotation" alone is not the whole story. What
 * comes back is the same `[bend, twist, splay]` in degrees the sliders write, so a drag and a
 * slider are two ways of saying one thing.
 */
export function turnToward(
  order: THREE.Bone[],
  bones: Bone[],
  pose: RiderPose,
  turns: string,
  from: THREE.Vector3,
  to: THREE.Vector3,
): RiderPose {
  const at = bones.findIndex((b) => b.name === turns);
  if (at < 0) return pose;
  const bone = order[at];
  const rest = bone.userData.restQuaternion as THREE.Quaternion | undefined;
  if (!rest) return pose;
  // Where the dot sits in the turned bone's own frame — the same whatever this bone's turn is,
  // which is what lets the swing below be measured from rest.
  const held = from
    .clone()
    .applyMatrix4(new THREE.Matrix4().copy(bone.matrixWorld).invert());
  // This bone as authored, with every other bone left where the pose has put it.
  const asAuthored = new THREE.Matrix4().compose(bone.position, rest, bone.scale);
  if (bone.parent) asAuthored.premultiply(bone.parent.matrixWorld);
  const pivot = new THREE.Vector3().setFromMatrixPosition(asAuthored);
  const was = held.clone().applyMatrix4(asAuthored).sub(pivot);
  const wants = to.clone().sub(pivot);
  // A drag that lands on the joint itself says nothing about which way to point.
  if (was.lengthSq() < 1e-8 || wants.lengthSq() < 1e-8) return pose;
  const swing = new THREE.Quaternion().setFromUnitVectors(was.normalize(), wants.normalize());
  // About the joint, so the bone stays where it is and only turns.
  const about = new THREE.Matrix4()
    .makeTranslation(pivot.x, pivot.y, pivot.z)
    .multiply(new THREE.Matrix4().makeRotationFromQuaternion(swing))
    .multiply(new THREE.Matrix4().makeTranslation(-pivot.x, -pivot.y, -pivot.z));
  const local = new THREE.Matrix4();
  if (bone.parent) local.copy(bone.parent.matrixWorld).invert();
  local.multiply(about).multiply(asAuthored);
  const turned = new THREE.Quaternion();
  local.decompose(new THREE.Vector3(), turned, new THREE.Vector3());
  // `applyPose` composes the turn after the rest, so this is what it has to be handed.
  return withTurn(pose, turns, shortenToLimit(rest.clone().invert().multiply(turned)));
}

/**
 * A turn as three degrees, cut back along its own arc until every axis is inside the stop.
 *
 * Not one axis at a time: clipping bend and splay separately points the limb somewhere nobody
 * asked for, and a drag that goes too far should stop short on the way to the cursor rather
 * than fly off. Bisection because the three Euler angles don't grow evenly along the arc, so
 * there is no closed form to scale by.
 */
function shortenToLimit(turn: THREE.Quaternion): [number, number, number] {
  const scratch = new THREE.Euler();
  const q = new THREE.Quaternion();
  const worst = (t: number) => {
    scratch.setFromQuaternion(q.identity().slerp(turn, t), "XYZ");
    return Math.max(Math.abs(scratch.x), Math.abs(scratch.y), Math.abs(scratch.z)) / DEG;
  };
  let far = 1;
  if (worst(1) > TURN_LIMIT) {
    let near = 0;
    for (let i = 0; i < 16; i++) {
      const mid = (near + far) / 2;
      if (worst(mid) > TURN_LIMIT) far = mid;
      else near = mid;
    }
    far = near;
  }
  worst(far);
  return [clampTurn(scratch.x / DEG), clampTurn(scratch.y / DEG), clampTurn(scratch.z / DEG)];
}

// ── Which bones a person would want to move ──────────────────────────────────

export type BoneGroupId = "torso" | "arms" | "hands" | "legs";

export interface BoneGroup {
  id: BoneGroupId;
  /** Rig names, in the order they should be listed. Any the model lacks are skipped. */
  bones: string[];
}

/**
 * The rig has 65 bones and most of them are knuckles. These are the ones worth a control,
 * grouped the way someone thinks about a rider rather than the way the file lists them.
 */
export const BONE_GROUPS: BoneGroup[] = [
  {
    id: "torso",
    bones: [
      "riderRIG_Pelvis",
      "riderRIG_Spine1",
      "riderRIG_Spine2",
      "riderRIG_Spine3",
      "riderRIG_Spine4",
      "riderRIG_Neck1",
      "riderRIG_Head",
    ],
  },
  {
    id: "arms",
    bones: [
      "riderRIG_LeftCollar",
      "riderRIG_LeftShoulder",
      "riderRIG_LeftElbow",
      "riderRIG_RightCollar",
      "riderRIG_RightShoulder",
      "riderRIG_RightElbow",
    ],
  },
  { id: "hands", bones: ["riderRIG_LeftWrist", "riderRIG_RightWrist"] },
  {
    id: "legs",
    bones: [
      "riderRIG_LeftHip",
      "riderRIG_LeftKnee",
      "riderRIG_RightHip",
      "riderRIG_RightKnee",
    ],
  },
];

/** A short label for a bone: `riderRIG_LeftElbow` → `Left elbow`. */
export function boneLabel(name: string): string {
  const stem = name.replace(/^.*RIG_/, "");
  const spaced = stem
    .replace(/([a-z])([A-Z0-9])/g, "$1 $2")
    .replace(/_end$/, " tip")
    .replace(/_/g, " ");
  return spaced.charAt(0).toUpperCase() + spaced.slice(1).toLowerCase();
}

// ── The rider's own axes ─────────────────────────────────────────────────────

/**
 * Which way is up, left and forward *on this rider*, and how long a thigh is.
 *
 * Read off the model rather than written down, because there is nothing to write down: the
 * rigs the game ships aren't even in the same orientation as each other (`default_mx_c` is
 * turned half a turn about its up axis relative to `default_sm`), a rig may reach the viewer
 * mirrored, and a bone's own axes are whatever the author left them as. Every ready-made move
 * below is stated in these, so the same move means the same thing on every model.
 */
export interface RiderFrame {
  up: THREE.Vector3;
  /** The rider's own left, not the screen's. */
  left: THREE.Vector3;
  forward: THREE.Vector3;
  /** Hip to knee, in metres — the unit the moves are measured in, so a tall rig moves further. */
  leg: number;
}

/** A rider the ready-made moves can be stated against: its rig, and the axes read off it. */
export interface PosableRig {
  bones: Bone[];
  frame: RiderFrame;
}

/** A bone's rest position in model space, or null if the model doesn't bind it. */
function originOf(bones: Bone[], name: string): THREE.Vector3 | null {
  const b = bones.find((x) => x.name === name);
  return b ? new THREE.Vector3(b.bind[3], b.bind[7], b.bind[11]) : null;
}

/** The first of `names` this model binds. */
function firstOrigin(bones: Bone[], names: string[]): THREE.Vector3 | null {
  for (const n of names) {
    const at = originOf(bones, n);
    if (at) return at;
  }
  return null;
}

/**
 * Which way the rider faces, as ±1 along `f`, or 0 when the body doesn't say.
 *
 * A foot is the one part of a standing body that is plainly asymmetric about its joint —
 * around 20 cm of it in front of the ankle and 6 cm behind — so the lowest slice of the mesh
 * settles the question on its own.
 */
function facingFromFeet(
  nodes: { positions: ArrayLike<number> }[],
  f: THREE.Vector3,
  up: THREE.Vector3,
  ankle: THREE.Vector3,
): number {
  let lo = Infinity;
  let hi = -Infinity;
  for (const n of nodes) {
    for (let i = 0; i < n.positions.length; i += 3) {
      const t = n.positions[i] * up.x + n.positions[i + 1] * up.y + n.positions[i + 2] * up.z;
      if (t < lo) lo = t;
      if (t > hi) hi = t;
    }
  }
  if (!(hi > lo)) return 0;
  const cut = lo + (hi - lo) * 0.06;
  const at = ankle.dot(f);
  let toe = 0;
  let heel = 0;
  for (const n of nodes) {
    for (let i = 0; i < n.positions.length; i += 3) {
      const t = n.positions[i] * up.x + n.positions[i + 1] * up.y + n.positions[i + 2] * up.z;
      if (t > cut) continue;
      const d = n.positions[i] * f.x + n.positions[i + 1] * f.y + n.positions[i + 2] * f.z - at;
      if (d > toe) toe = d;
      if (-d > heel) heel = -d;
    }
  }
  if (toe > heel * 1.35) return 1;
  if (heel > toe * 1.35) return -1;
  return 0;
}

/**
 * The same question from the rig alone: a thumb points the way its owner does.
 *
 * Only asked when the mesh didn't answer — a rider authored already sitting on a bike has its
 * feet under it, and then the hands are the better witness.
 */
function facingFromThumb(bones: Bone[], f: THREE.Vector3, up: THREE.Vector3): number {
  const wrist = originOf(bones, "riderRIG_LeftWrist");
  const thumb = firstOrigin(bones, [
    "riderRIG_LeftThumb3",
    "riderRIG_LeftThumb2",
    "riderRIG_LeftThumb1",
  ]);
  if (!wrist || !thumb) return 0;
  const d = thumb.clone().sub(wrist);
  d.addScaledVector(up, -d.dot(up));
  const len = d.length();
  if (len < 1e-4) return 0;
  const along = d.dot(f) / len;
  return Math.abs(along) > 0.4 ? Math.sign(along) : 0;
}

/**
 * Read {@link RiderFrame} off a rig, with the body mesh to settle which way it faces.
 *
 * Null when the model binds too little to say — a rig with no hips or no spine is one the
 * ready-made moves can't be stated in, and offering them anyway would move something at
 * random.
 */
export function riderFrame(
  bones: Bone[],
  body?: { positions: ArrayLike<number> }[] | null,
): RiderFrame | null {
  if (!bones.length) return null;
  const leftHip = originOf(bones, "riderRIG_LeftHip");
  const rightHip = originOf(bones, "riderRIG_RightHip");
  if (!leftHip || !rightHip) return null;
  const hips = leftHip.clone().add(rightHip).multiplyScalar(0.5);
  const pelvis = originOf(bones, "riderRIG_Pelvis") ?? hips;
  const head = firstOrigin(bones, [
    "riderRIG_Head",
    "riderRIG_Neck1",
    "riderRIG_Spine4",
    "riderRIG_LeftCollar",
  ]);
  if (!head) return null;
  const up = head.clone().sub(pelvis);
  if (up.lengthSq() < 1e-8) return null;
  up.normalize();
  const left = leftHip.clone().sub(rightHip);
  left.addScaledVector(up, -left.dot(up));
  if (left.lengthSq() < 1e-8) return null;
  left.normalize();
  const forward = new THREE.Vector3().crossVectors(up, left).normalize();

  const leftKnee = originOf(bones, "riderRIG_LeftKnee");
  const rightKnee = originOf(bones, "riderRIG_RightKnee");
  const thigh = leftKnee
    ? leftKnee.distanceTo(leftHip)
    : rightKnee
      ? rightKnee.distanceTo(rightHip)
      : head.distanceTo(pelvis) * 0.6;

  // The knee stands over the ankle on a standing rider, which is the reference the foot test
  // measures its toe and heel from.
  const ankle = leftKnee ?? rightKnee ?? hips;
  const sign =
    (body?.length ? facingFromFeet(body, forward, up, ankle) : 0) ||
    facingFromThumb(bones, forward, up);
  if (sign < 0) forward.negate();
  return { up, left, forward, leg: thigh || 0.36 };
}

// ── Ready-made moves ─────────────────────────────────────────────────────────

export type QuickMoveId =
  | "legsWide"
  | "legsNarrow"
  | "leftLegForward"
  | "elbowsUp"
  | "leanIn"
  | "sitOnBike";

/** One joint sent somewhere, and the bone whose turn takes it there. */
export interface QuickStep {
  /** The bone the turn is written on — the joint the limb swings about. */
  turns: string;
  /** The bone that has to travel. */
  moves: string;
  /** Take the far end of `moves`' own box rather than its joint — see {@link boneTip}. */
  tip?: boolean;
  /** How far it travels, in thigh-lengths, along the rider's own axes. */
  by: { up?: number; left?: number; forward?: number };
}

export interface QuickMove {
  id: QuickMoveId;
  steps: QuickStep[];
}

/**
 * The moves, as places to send a joint.
 *
 * Not as degrees: degrees are read in a bone's own frame, and nothing says that frame is
 * squared up with the body — which is how "legs wider" came to pull them in on some models
 * and shorten them on others. "Send the knee 22 cm to the rider's own left" only has the one
 * meaning, and {@link turnToward} — the same solver a drag uses — works out the turn.
 */
export const QUICK_MOVES: QuickMove[] = [
  {
    id: "legsWide",
    steps: [
      { turns: "riderRIG_LeftHip", moves: "riderRIG_LeftKnee", by: { left: 0.24 } },
      { turns: "riderRIG_RightHip", moves: "riderRIG_RightKnee", by: { left: -0.24 } },
    ],
  },
  {
    id: "legsNarrow",
    steps: [
      { turns: "riderRIG_LeftHip", moves: "riderRIG_LeftKnee", by: { left: -0.18 } },
      { turns: "riderRIG_RightHip", moves: "riderRIG_RightKnee", by: { left: 0.18 } },
    ],
  },
  {
    id: "leftLegForward",
    steps: [
      { turns: "riderRIG_LeftHip", moves: "riderRIG_LeftKnee", by: { forward: 0.3 } },
      { turns: "riderRIG_RightHip", moves: "riderRIG_RightKnee", by: { forward: -0.22 } },
    ],
  },
  {
    id: "elbowsUp",
    steps: [
      {
        turns: "riderRIG_LeftShoulder",
        moves: "riderRIG_LeftElbow",
        by: { up: 0.22, left: 0.14 },
      },
      {
        turns: "riderRIG_RightShoulder",
        moves: "riderRIG_RightElbow",
        by: { up: 0.22, left: -0.14 },
      },
    ],
  },
  {
    id: "leanIn",
    steps: [
      { turns: "riderRIG_Spine2", moves: "riderRIG_Neck1", by: { forward: 0.18 } },
      // Back the other way, so leaning in doesn't take the rider's eyes off the track.
      { turns: "riderRIG_Neck1", moves: "riderRIG_Head", by: { forward: -0.07 } },
    ],
  },
  {
    // Thighs forward and apart, shins folded back under: a straddle, which is what makes the
    // on-bike view worth looking at before anyone touches a slider.
    id: "sitOnBike",
    steps: [
      {
        turns: "riderRIG_LeftHip",
        moves: "riderRIG_LeftKnee",
        by: { forward: 0.6, left: 0.3, up: 0.12 },
      },
      {
        turns: "riderRIG_RightHip",
        moves: "riderRIG_RightKnee",
        by: { forward: 0.6, left: -0.3, up: 0.12 },
      },
      {
        turns: "riderRIG_LeftKnee",
        moves: "riderRIG_LeftKnee",
        tip: true,
        by: { forward: -0.6, left: 0.12 },
      },
      {
        turns: "riderRIG_RightKnee",
        moves: "riderRIG_RightKnee",
        tip: true,
        by: { forward: -0.6, left: -0.12 },
      },
      { turns: "riderRIG_Spine2", moves: "riderRIG_Neck1", by: { forward: 0.16 } },
    ],
  },
];

/**
 * Can this model make this move?
 *
 * A rig that binds no spine — `default_mx_c` is one — has nothing to lean, and a button that
 * silently does nothing is worse than one that isn't offered.
 */
export function canMove(move: QuickMove, bones: Bone[]): boolean {
  const has = (name: string) => bones.some((b) => b.name === name);
  return move.steps.some((s) => has(s.turns) && has(s.moves));
}

/**
 * Stack a ready-made move onto a pose.
 *
 * Each step is solved against the rig as the steps before it have left it, so a move that
 * folds a shin under a thigh reads the thigh where it now is. Bones the model doesn't bind
 * are skipped rather than guessed at, so a rig with no spine still gets its legs moved.
 */
export function applyQuickMove(
  pose: RiderPose,
  move: QuickMove,
  bones: Bone[],
  frame: RiderFrame,
): RiderPose {
  const { order } = buildSkeleton(bones);
  let out = pose;
  applyPose(order, out);
  for (const step of move.steps) {
    const on = bones.findIndex((b) => b.name === step.moves);
    if (on < 0 || bones.findIndex((b) => b.name === step.turns) < 0) continue;
    const local = step.tip
      ? new THREE.Vector3(...boneTip(bones, on))
      : new THREE.Vector3(0, 0, 0);
    order[on].updateWorldMatrix(true, false);
    const from = local.applyMatrix4(order[on].matrixWorld);
    const to = from
      .clone()
      .addScaledVector(frame.up, (step.by.up ?? 0) * frame.leg)
      .addScaledVector(frame.left, (step.by.left ?? 0) * frame.leg)
      .addScaledVector(frame.forward, (step.by.forward ?? 0) * frame.leg);
    out = turnToward(order, bones, out, step.turns, from, to);
    applyPose(order, out);
  }
  return out;
}

/** How far one bone may be turned. Past this a rider stops looking like one. */
export const TURN_LIMIT = 60;

export function clampTurn(deg: number): number {
  return Math.max(-TURN_LIMIT, Math.min(TURN_LIMIT, Math.round(deg)));
}
