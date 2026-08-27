import * as THREE from "three";
import type { BikeRig, Bone, EdfNode, Vec3 } from "../types";

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
  return withTurn(
    pose,
    turns,
    shortenToLimit(rest.clone().invert().multiply(turned), turnLimit(turns)),
  );
}

/**
 * A turn as three degrees, cut back along its own arc until every axis is inside the stop.
 *
 * Not one axis at a time: clipping bend and splay separately points the limb somewhere nobody
 * asked for, and a drag that goes too far should stop short on the way to the cursor rather
 * than fly off. Bisection because the three Euler angles don't grow evenly along the arc, so
 * there is no closed form to scale by.
 */
function shortenToLimit(turn: THREE.Quaternion, limit: number): [number, number, number] {
  const scratch = new THREE.Euler();
  const q = new THREE.Quaternion();
  const worst = (t: number) => {
    scratch.setFromQuaternion(q.identity().slerp(turn, t), "XYZ");
    return Math.max(Math.abs(scratch.x), Math.abs(scratch.y), Math.abs(scratch.z)) / DEG;
  };
  let far = 1;
  if (worst(1) > limit) {
    let near = 0;
    for (let i = 0; i < 16; i++) {
      const mid = (near + far) / 2;
      if (worst(mid) > limit) far = mid;
      else near = mid;
    }
    far = near;
  }
  worst(far);
  return [
    clampTurn(scratch.x / DEG, limit),
    clampTurn(scratch.y / DEG, limit),
    clampTurn(scratch.z / DEG, limit),
  ];
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
  /**
   * The bike under him — where its grips and pegs are, in his own frame. Null whenever he
   * isn't sitting on one, which is what makes the riding position offered only when it means
   * something.
   */
  mount?: RiderMount | null;
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
 * From the hand: an arm hanging at rest has its palm to the thigh, which puts the index
 * knuckle at the front of the body and the little finger at the back — 8 cm apart, and the
 * same 8 cm on `default_mx`, `default_sm` and Rider+, because the rig is the game's own.
 *
 * The body mesh is not asked, and can't be: it has no feet. The ankles and toes are dropped
 * from the rig — they belong to the boots, not the body — and the mesh stops at the sock, so
 * a toe-and-heel reading of its lowest slice measures the bottom of a shin and answers
 * "forward" whichever way the rider is really pointing. That reading used to be taken first,
 * and it is why the rider sat on the bike backwards.
 */
function facingFromKnuckles(bones: Bone[], f: THREE.Vector3, up: THREE.Vector3): number {
  for (const side of SIDES) {
    const index = originOf(bones, `riderRIG_${side}Index1`);
    const pink = firstOrigin(bones, [`riderRIG_${side}Pink1`, `riderRIG_${side}Pinky1`]);
    if (!index || !pink) continue;
    const d = index.clone().sub(pink);
    d.addScaledVector(up, -d.dot(up));
    const len = d.length();
    if (len < 1e-4) continue;
    const along = d.dot(f) / len;
    if (Math.abs(along) > 0.4) return Math.sign(along);
  }
  return 0;
}

/**
 * The last resort: a body carries more of itself behind its hip joints than in front.
 *
 * The pelvis bone's box covers the seat of the rider, which runs back from the joints it
 * hangs off; the front of the hip barely does. Coarser than the hand, and only asked of a rig
 * that binds no fingers at all.
 */
function facingFromSeat(bones: Bone[], f: THREE.Vector3): number {
  const at = bones.findIndex((b) => b.name === "riderRIG_Pelvis");
  if (at < 0) return 0;
  const bone = bones[at];
  const bind = toMatrix(bone.bind);
  const origin = new THREE.Vector3().setFromMatrixPosition(bind);
  const corner = new THREE.Vector3();
  let along = 0;
  let against = 0;
  for (const x of [bone.aabbLo[0], bone.aabbHi[0]]) {
    for (const y of [bone.aabbLo[1], bone.aabbHi[1]]) {
      for (const z of [bone.aabbLo[2], bone.aabbHi[2]]) {
        const d = corner.set(x, y, z).applyMatrix4(bind).sub(origin).dot(f);
        along = Math.max(along, d);
        against = Math.max(against, -d);
      }
    }
  }
  if (against > along * 1.2) return 1;
  if (along > against * 1.2) return -1;
  return 0;
}

/** The same question, from the thumb: it points the way its owner does. */
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
 * Read {@link RiderFrame} off a rig.
 *
 * Null when the model binds too little to say — a rig with no hips or no spine is one the
 * ready-made moves can't be stated in, and offering them anyway would move something at
 * random.
 */
export function riderFrame(bones: Bone[]): RiderFrame | null {
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
  // left × up, so this already points the way the rider does on a rig read the way round
  // the game writes them; the witnesses below only ever have to confirm it.
  const forward = new THREE.Vector3().crossVectors(left, up).normalize();

  const leftKnee = originOf(bones, "riderRIG_LeftKnee");
  const rightKnee = originOf(bones, "riderRIG_RightKnee");
  const thigh = leftKnee
    ? leftKnee.distanceTo(leftHip)
    : rightKnee
      ? rightKnee.distanceTo(rightHip)
      : head.distanceTo(pelvis) * 0.6;

  const sign =
    facingFromKnuckles(bones, forward, up) ||
    facingFromThumb(bones, forward, up) ||
    facingFromSeat(bones, forward);
  if (sign < 0) forward.negate();
  return { up, left, forward, leg: thigh || 0.36 };
}

// ── Sitting the rider on the bike ────────────────────────────────────────────

/**
 * Where the rider's weight goes: the underside of the pelvis, in the rider's own frame.
 *
 * The pelvis bone carries a box covering the slice of body it moves, so the bottom of that
 * box is where a seat would touch. Read off the model, because a rider is whatever height its
 * author made it.
 */
function seatContact(bones: Bone[], up: THREE.Vector3): THREE.Vector3 | null {
  const pelvis =
    bones.find((b) => b.name === "riderRIG_Pelvis") ??
    bones.find((b) => b.name === "riderRIG_LeftHip");
  if (!pelvis) return null;
  const bind = toMatrix(pelvis.bind);
  const at = new THREE.Vector3(pelvis.bind[3], pelvis.bind[7], pelvis.bind[11]);
  const { aabbLo: lo, aabbHi: hi } = pelvis;
  let drop = 0;
  const corner = new THREE.Vector3();
  for (const x of [lo[0], hi[0]]) {
    for (const y of [lo[1], hi[1]]) {
      for (const z of [lo[2], hi[2]]) {
        const d = corner.set(x, y, z).applyMatrix4(bind).sub(at).dot(up);
        if (d < drop) drop = d;
      }
    }
  }
  return at.addScaledVector(up, drop);
}

/** How far into the seat the rider settles, in metres. */
const SEAT_SINK = -0.02;

/**
 * How far up the seat the rider sits, as a share of the way from the seat to the steering
 * head.
 *
 * `seat_height_ref` is a setup reference in the middle of the seat, and nobody rides there:
 * from that point the bars are about 0.56 m away and an arm is 0.46 m long, so a rider left
 * on the reference cannot reach his own handlebars. Sitting up the seat — with the hunch that
 * goes with it — is what closes the gap.
 */
const SEAT_FORWARD = 0.25;

/**
 * The rider sat on the bike's seat, facing the way it does.
 *
 * Worked out rather than eyeballed: the bike's `.geom` names `seat_height_ref`, and the
 * rider's own up and forward come off its rig, so the two only have to be brought into one
 * frame. Null when either half won't say — a bike whose `.geom` names no seat, or a rig with
 * no hips — and then the pair falls back to standing side by side, which is honest about not
 * knowing.
 */
export function seatTransform(
  parts: { part: string; nodes: unknown[]; skeleton?: Bone[] | null }[],
  seat: Vec3,
  rig: BikeRig | null,
): THREE.Matrix4 | null {
  const bones = parts.find((p) => p.part === "body" && p.nodes.length)?.skeleton;
  if (!bones?.length) return null;
  const rf = riderFrame(bones);
  if (!rf) return null;
  const contact = seatContact(bones, rf.up);
  if (!contact) return null;
  // The rider's (forward, up) onto the bike's (+Z, +Y). Both triples are built by a cross
  // product, so both turn the same way round and what comes out is a rotation, not a mirror.
  const b3 = new THREE.Vector3().crossVectors(rf.forward, rf.up);
  const from = new THREE.Matrix4().makeBasis(rf.forward, rf.up, b3);
  const to = new THREE.Matrix4().makeBasis(
    new THREE.Vector3(0, 0, 1),
    new THREE.Vector3(0, 1, 0),
    new THREE.Vector3(0, 0, 1).cross(new THREE.Vector3(0, 1, 0)),
  );
  const turn = to.multiply(from.transpose());
  // Sit *on* the seat rather than with the hip joint in it — the reference is the top of the
  // seat, and a rider's weight is carried a little way into it — and up the seat towards the
  // bars, which is where a rider actually sits.
  const at = new THREE.Vector3(seat[0], seat[1] + SEAT_SINK, seat[2] + seatShift(seat, rig));
  return new THREE.Matrix4()
    .makeTranslation(at.x, at.y, at.z)
    .multiply(turn)
    .multiply(new THREE.Matrix4().makeTranslation(-contact.x, -contact.y, -contact.z));
}

/** How far forward of `seat_height_ref` the rider sits, in metres. */
function seatShift(seat: Vec3, rig: BikeRig | null): number {
  if (!rig) return 0;
  return (rig.steerHead[2] - seat[2]) * SEAT_FORWARD;
}

/**
 * Where the rider's hands and feet go on this bike: a grip and a peg per side.
 *
 * Stated in the rider's own rest frame rather than the bike's, so a move can send a wrist to
 * a grip with the same solver that sends a knee 22 cm to the left, and nothing downstream has
 * to know a bike is involved.
 */
export interface RiderMount {
  /** Left, right — the rider's own, matching the bone names. */
  grips: [THREE.Vector3, THREE.Vector3];
  pegs: [THREE.Vector3, THREE.Vector3];
}

/**
 * The grips, off the bike's own mesh.
 *
 * The `.geom` names no handlebar, but the assembled `steer` part is the bars: the widest
 * thing on it by a distance, so its outermost vertices on each side are the bar ends and the
 * levers. A hand goes a little inboard of that, where the grip is.
 *
 * `null` for a bike whose parts didn't come back named — the same `steer` prefix the
 * assembler keys on, so if this can't find them neither could that, and the bike is
 * unassembled anyway.
 */
function barGrips(nodes: EdfNode[]): [THREE.Vector3, THREE.Vector3] | null {
  const steer = nodes.filter((n) => n.name.toLowerCase().startsWith("steer"));
  if (!steer.length) return null;
  let wide = 0;
  for (const n of steer) {
    for (let i = 0; i < n.positions.length; i += 3) wide = Math.max(wide, Math.abs(n.positions[i]));
  }
  // A bar half-narrower than a rider's shoulders is not a bar; something else was named steer.
  if (wide < 0.2) return null;
  const band = wide - 0.1;
  const ends = [new THREE.Vector3(), new THREE.Vector3()];
  const seen = [0, 0];
  for (const n of steer) {
    for (let i = 0; i < n.positions.length; i += 3) {
      const x = n.positions[i];
      if (Math.abs(x) < band) continue;
      // The rider's left is +x, and so is the bar end his left hand takes.
      const side = x > 0 ? 0 : 1;
      ends[side].add(new THREE.Vector3(x, n.positions[i + 1], n.positions[i + 2]));
      seen[side]++;
    }
  }
  if (!seen[0] || !seen[1]) return null;
  ends[0].divideScalar(seen[0]);
  ends[1].divideScalar(seen[1]);
  // Inboard of the bar end by half a grip, so a hand lands on the rubber rather than the plug.
  ends[0].x = wide - GRIP_INSET;
  ends[1].x = -(wide - GRIP_INSET);
  return [ends[0], ends[1]];
}

/** How far inboard of the widest point on the bars a hand sits, in metres. */
const GRIP_INSET = 0.05;

/**
 * Where the footpegs are — an estimate, and the only one here.
 *
 * Unlike the seat and the bars, nothing in the bike says: the `.geom` names no footrest and
 * the mesh hides them among the frame rails. What is true of every motocross bike ever built
 * is that the pegs sit at about rear-axle height and a little way up the frame from the seat
 * reference, about 27 cm either side of the centreline — which is what this is.
 */
function footPegs(rig: BikeRig, seat: Vec3): [THREE.Vector3, THREE.Vector3] {
  // Peg height is rear-axle height, near enough to the centimetre on every bike; and a peg
  // sits just ahead of the swingarm pivot, which is the one point down there the .geom does
  // name.
  const y = rig.rearAxle ? rig.rearAxle[1] : seat[1] - 0.5;
  const z = rig.pivot[2] + PEG_AHEAD_OF_PIVOT;
  return [new THREE.Vector3(PEG_WIDTH, y, z), new THREE.Vector3(-PEG_WIDTH, y, z)];
}

/** How far the pegs sit either side of the centreline, and ahead of the swingarm pivot. */
const PEG_WIDTH = 0.27;
const PEG_AHEAD_OF_PIVOT = 0.09;

/**
 * The grips and pegs of `rig`, brought into the rider's own rest frame.
 *
 * Null when the rider can't be sat on the bike in the first place — there is nowhere to
 * measure from then, and a move that reached for a bar the rider isn't sitting behind would
 * pull him inside out.
 */
export function riderMount(
  parts: { part: string; nodes: unknown[]; skeleton?: Bone[] | null }[],
  bike: EdfNode[],
  rig: BikeRig | null,
): RiderMount | null {
  if (!rig?.seat) return null;
  const seated = seatTransform(parts, rig.seat, rig);
  if (!seated) return null;
  const grips = barGrips(bike);
  if (!grips) return null;
  const intoRider = seated.clone().invert();
  const pegs = footPegs(rig, rig.seat);
  return {
    grips: [grips[0].applyMatrix4(intoRider), grips[1].applyMatrix4(intoRider)],
    pegs: [pegs[0].applyMatrix4(intoRider), pegs[1].applyMatrix4(intoRider)],
  };
}

// ── Ready-made moves ─────────────────────────────────────────────────────────

export type QuickMoveId =
  | "legsWide"
  | "legsNarrow"
  | "leftLegForward"
  | "elbowsUp"
  | "leanIn"
  | "ride";

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
  /**
   * Also put his hands on the bars and his boots on the pegs — which needs the bike under
   * him, so a move with this set is offered only once {@link riderMount} has answered.
   */
  mounted?: boolean;
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
    // Sat on the machine: weight forward over the tank, hands on the bars, knees round it and
    // boots on the pegs. The hunch is written here; the limbs are solved against the bike,
    // because where a grip and a peg are is the bike's business, not the rider's.
    id: "ride",
    mounted: true,
    steps: [
      { turns: "riderRIG_Spine1", moves: "riderRIG_Spine3", by: { forward: 0.1 } },
      { turns: "riderRIG_Spine2", moves: "riderRIG_Neck1", by: { forward: 0.3 } },
      // Back the other way, so leaning in doesn't take the rider's eyes off the track.
      { turns: "riderRIG_Neck1", moves: "riderRIG_Head", by: { forward: -0.1, up: 0.05 } },
    ],
  },
];

// ── Reaching for the bike ────────────────────────────────────────────────────

/**
 * How long a shin is, as a share of the thigh above it.
 *
 * An estimate, and it has to be: the rig carries no ankle — the ankles and toes belong to the
 * boots, so the body binds neither — and the body mesh stops at the sock. Measured off the
 * riders the game ships, where the knee stands 0.50 m up and the ankle 0.09 m.
 */
const SHIN_OVER_THIGH = 1.12;

/** How far behind the grip the wrist sits: the width of a hand. */
function handLength(bones: Bone[], side: string): number {
  const wrist = originOf(bones, `riderRIG_${side}Wrist`);
  const knuckle = originOf(bones, `riderRIG_${side}Index1`);
  return wrist && knuckle ? wrist.distanceTo(knuckle) : 0.09;
}

/**
 * Where the middle joint of a two-bone limb lands when its end is put on `target`.
 *
 * The triangle root–middle–end has all three sides known, so the middle sits on a circle
 * about the root-to-target line and `pole` picks the point on it — which way the joint bends.
 * A target further off than the limb is long is drawn in until it is reachable, so a limb
 * asked for too much straightens towards it instead of tearing off.
 */
function midJoint(
  root: THREE.Vector3,
  target: THREE.Vector3,
  near: number,
  far: number,
  pole: THREE.Vector3,
): THREE.Vector3 {
  const axis = target.clone().sub(root);
  let d = axis.length();
  if (d < 1e-6) return root.clone().addScaledVector(pole.clone().normalize(), near);
  d = Math.min(Math.max(d, Math.abs(near - far) + 1e-4), near + far - 1e-4);
  axis.normalize();
  const along = (near * near - far * far + d * d) / (2 * d);
  const out = Math.sqrt(Math.max(0, near * near - along * along));
  const bend = pole.clone();
  bend.addScaledVector(axis, -bend.dot(axis));
  if (bend.lengthSq() < 1e-8) return root.clone().addScaledVector(axis, along);
  bend.normalize();
  return root.clone().addScaledVector(axis, along).addScaledVector(bend, out);
}

/** A two-bone limb, and where it is being sent. */
interface Reach {
  /** The joint the whole limb swings about, and the bone that turn is written on. */
  root: string;
  /** The joint between the two bones — and the bone the second turn is written on. */
  mid: string;
  /** The joint at the end, the one put on the target. */
  end: string;
  /**
   * Aim the far end of `mid`'s own box instead of `end`'s joint. For a shin, whose ankle the
   * rig doesn't carry: only the direction to the target is used, which is all it takes to
   * point a boot at a peg.
   */
  tip?: boolean;
  /** The length of the second bone, when `tip` means it can't be read off the rig. */
  far?: number;
}

/**
 * Swing a two-bone limb so its end lands on `to`, bending the way `pole` points.
 *
 * Solved rather than nudged towards it: the middle joint is placed by triangle, then each
 * bone is turned to point at where it now has to with {@link turnToward} — the same solver a
 * drag uses, so an arm put on a handlebar and an arm dragged there by the wrist come out
 * saying the same thing.
 */
function reach(
  order: THREE.Bone[],
  bones: Bone[],
  pose: RiderPose,
  limb: Reach,
  to: THREE.Vector3,
  pole: THREE.Vector3,
): RiderPose {
  const at = (name: string) => bones.findIndex((b) => b.name === name);
  const [r, m, e] = [at(limb.root), at(limb.mid), at(limb.end)];
  if (r < 0 || m < 0 || (e < 0 && !limb.tip)) return pose;
  const rest = (i: number) => new THREE.Vector3().setFromMatrixPosition(toMatrix(bones[i].bind));
  const near = rest(r).distanceTo(rest(m));
  const far = limb.far ?? rest(m).distanceTo(rest(e));
  if (near < 1e-4 || far < 1e-4) return pose;

  let out = pose;
  const world = (i: number) => {
    order[i].updateWorldMatrix(true, false);
    return new THREE.Vector3().setFromMatrixPosition(order[i].matrixWorld);
  };
  // Swing the whole limb so its middle joint lands where the triangle puts it.
  const mid = midJoint(world(r), to, near, far, pole);
  out = turnToward(order, bones, out, limb.root, world(m), mid);
  applyPose(order, out);
  // Then fold it, so the end lands on the target.
  const tail = limb.tip
    ? new THREE.Vector3(...boneTip(bones, m)).applyMatrix4(
        (order[m].updateWorldMatrix(true, false), order[m].matrixWorld),
      )
    : world(e);
  out = turnToward(order, bones, out, limb.mid, tail, to);
  applyPose(order, out);
  return out;
}

/**
 * Hands on the bars, boots on the pegs.
 *
 * Run after the hunch, not before: leaning the torso carries the shoulders forward, and where
 * the shoulder is decides how much elbow the reach leaves.
 */
function reachTheBike(
  order: THREE.Bone[],
  bones: Bone[],
  pose: RiderPose,
  frame: RiderFrame,
  mount: RiderMount,
): RiderPose {
  let out = pose;
  SIDES.forEach((side, i) => {
    // Which way is out for this side: +1 on his left, -1 on his right.
    const away = i === 0 ? 1 : -1;
    // Elbows out and up, the way a rider carries them.
    const elbow = frame.left
      .clone()
      .multiplyScalar(away)
      .addScaledVector(frame.up, 0.5)
      .addScaledVector(frame.forward, -0.4);
    const shoulderAt = bones.findIndex((b) => b.name === `riderRIG_${side}Shoulder`);
    const grip = mount.grips[i];
    if (shoulderAt >= 0) {
      order[shoulderAt].updateWorldMatrix(true, false);
      const shoulder = new THREE.Vector3().setFromMatrixPosition(order[shoulderAt].matrixWorld);
      // The hand grips the bar, so the wrist stops a hand's width short of it.
      const wristAt = grip
        .clone()
        .addScaledVector(
          grip.clone().sub(shoulder).normalize(),
          -handLength(bones, side),
        );
      out = reach(
        order,
        bones,
        out,
        {
          root: `riderRIG_${side}Shoulder`,
          mid: `riderRIG_${side}Elbow`,
          end: `riderRIG_${side}Wrist`,
        },
        wristAt,
        elbow,
      );
      // The forearm puts the wrist a hand's width off the bar; this closes the hand on it.
      // A hand hangs off the arm at its own angle, so where the knuckles ended up is not
      // where the reach was aimed.
      const knuckle = bones.findIndex((b) => b.name === `riderRIG_${side}Index1`);
      if (knuckle >= 0) {
        order[knuckle].updateWorldMatrix(true, false);
        const held = new THREE.Vector3().setFromMatrixPosition(order[knuckle].matrixWorld);
        out = turnToward(order, bones, out, `riderRIG_${side}Wrist`, held, grip);
        applyPose(order, out);
      }
    }
    // Knees forward and out around the tank — but inside the pegs, so the shin tapers back
    // in towards the machine rather than the rider riding bow-legged.
    const knee = frame.forward
      .clone()
      .addScaledVector(frame.left, 0.34 * away)
      .addScaledVector(frame.up, 0.15);
    out = reach(
      order,
      bones,
      out,
      {
        root: `riderRIG_${side}Hip`,
        mid: `riderRIG_${side}Knee`,
        end: `riderRIG_${side}Knee`,
        tip: true,
        far: frame.leg * SHIN_OVER_THIGH,
      },
      mount.pegs[i],
      knee,
    );
  });
  return out;
}

/**
 * Can this model make this move?
 *
 * A rig that binds no spine — `default_mx_c` is one — has nothing to lean, and a button that
 * silently does nothing is worse than one that isn't offered.
 */
export function canMove(move: QuickMove, bones: Bone[], mount?: RiderMount | null): boolean {
  const has = (name: string) => bones.some((b) => b.name === name);
  // A move that reaches for the bike needs the bike, and a limb to reach with.
  if (move.mounted) {
    return !!mount && bones.some((b) => /(?:Hip|Shoulder)$/.test(b.name));
  }
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
  mount?: RiderMount | null,
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
  if (move.mounted && mount) out = reachTheBike(order, bones, out, frame, mount);
  return out;
}

/**
 * How far a bone nothing below names may be turned. Past this a rider stops looking like one.
 */
export const TURN_LIMIT = 60;

/**
 * How far each joint may be turned, in degrees.
 *
 * One number can't do it. A rider sitting on a bike folds a knee past 90° and swings a hip
 * around 65°, while a neck that went either way would be a broken one — and a single 60° stop
 * was quietly cutting every seated pose back: {@link shortenToLimit} shortens the whole turn
 * until its worst axis fits, so a leg meant to fold under the machine came out half folded.
 */
const JOINT_LIMIT: { at: RegExp; deg: number }[] = [
  // The joints a riding position is made of.
  { at: /(Hip|Knee|Shoulder|Elbow)$/, deg: 135 },
  { at: /(Wrist|Collar)$/, deg: 70 },
  { at: /(Neck\d*|Head)$/, deg: 45 },
];

/** How far `bone` may be turned. */
export function turnLimit(bone: string): number {
  return JOINT_LIMIT.find((r) => r.at.test(bone))?.deg ?? TURN_LIMIT;
}

export function clampTurn(deg: number, limit: number = TURN_LIMIT): number {
  return Math.max(-limit, Math.min(limit, Math.round(deg)));
}
