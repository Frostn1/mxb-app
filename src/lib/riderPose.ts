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

// ── Ready-made moves ─────────────────────────────────────────────────────────

export type QuickMoveId = "legsWide" | "legsNarrow" | "leftLegForward" | "elbowsUp" | "leanIn";

export interface QuickMove {
  id: QuickMoveId;
  /** Added to whatever the pose already holds, so two moves can be stacked. */
  turns: Record<string, [number, number, number]>;
}

/**
 * The moves the rig can actually make, as turns on real bones.
 *
 * Deliberately small: a hip swings out on one axis and forward on another, and naming those
 * two "wider" and "one leg forward" is most of what anyone wants from a leg. Everything finer
 * is a slider away in the bone list.
 */
export const QUICK_MOVES: QuickMove[] = [
  {
    id: "legsWide",
    turns: { riderRIG_LeftHip: [0, 0, 12], riderRIG_RightHip: [0, 0, -12] },
  },
  {
    id: "legsNarrow",
    turns: { riderRIG_LeftHip: [0, 0, -8], riderRIG_RightHip: [0, 0, 8] },
  },
  {
    id: "leftLegForward",
    turns: { riderRIG_LeftHip: [-14, 0, 0], riderRIG_RightHip: [10, 0, 0] },
  },
  {
    id: "elbowsUp",
    turns: {
      riderRIG_LeftShoulder: [0, 0, -14],
      riderRIG_RightShoulder: [0, 0, 14],
      riderRIG_LeftElbow: [0, 0, -8],
      riderRIG_RightElbow: [0, 0, 8],
    },
  },
  {
    id: "leanIn",
    turns: { riderRIG_Spine2: [-8, 0, 0], riderRIG_Spine3: [-6, 0, 0], riderRIG_Neck1: [8, 0, 0] },
  },
];

/** Stack a ready-made move onto a pose, clamped to the same limits the sliders use. */
export function applyQuickMove(pose: RiderPose, move: QuickMove): RiderPose {
  const out: RiderPose = { ...pose };
  for (const [bone, turn] of Object.entries(move.turns)) {
    const at = turnOf(out, bone);
    out[bone] = [
      clampTurn(at[0] + turn[0]),
      clampTurn(at[1] + turn[1]),
      clampTurn(at[2] + turn[2]),
    ];
  }
  return trimPose(out);
}

/** How far one bone may be turned. Past this a rider stops looking like one. */
export const TURN_LIMIT = 60;

export function clampTurn(deg: number): number {
  return Math.max(-TURN_LIMIT, Math.min(TURN_LIMIT, Math.round(deg)));
}
