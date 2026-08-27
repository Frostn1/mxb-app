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
    const world = toMatrix(b.bind);
    const local =
      b.parent === null || b.parent === undefined
        ? world
        : toMatrix(bones[b.parent].bind).invert().multiply(world);
    local.decompose(bone.position, bone.quaternion, bone.scale);
    // Kept so a pose can turn the bone without losing the rest it turns from.
    bone.userData.restQuaternion = bone.quaternion.clone();
    if (b.parent === null || b.parent === undefined) roots.push(bone);
    else made[b.parent].add(bone);
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
