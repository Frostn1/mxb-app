import { clampTurn, trimPose, turnLimit, type RiderPose } from "./riderPose";

/**
 * What a replay can show of the rider's body.
 *
 * The telemetry carries no body angle at all — `lean` is the rider's *input*, two stick axes
 * the bike's own rates and springs then answer however they like, and the riding aids can move
 * the body instead. So this turns a few bones far enough to read at replay size and nothing
 * more; it is a legible sketch of what the rider asked for, not a reconstruction of where the
 * body was.
 */

/** What the telemetry says the rider was doing, one sample's worth. */
export type RiderMotion = {
  /** Rider input, -1 (left) to +1 (right), NaN when it was never read. */
  leanLR: number;
  /** Rider input, -1 (back) to +1 (forward), NaN when it was never read. */
  leanFB: number;
  /** Sitting, standing, or not known. */
  stance: "sit" | "stand" | "unknown";
  /** The bike's own lean, degrees, positive to the rider's right. */
  bikeRoll: number;
};

/**
 * Degrees on one bone's own three axes, in the order the pose sliders name them: bend, twist,
 * splay.
 *
 * A bone's frame is its author's business and nothing says it is squared up with the body —
 * the ready-made moves in `riderPose` are stated as places to send a joint for exactly that
 * reason. Degrees are used here anyway because a replay has no rig to solve against and has to
 * pose whatever rider is installed, so the table below reads the game's own rigs: positive bend
 * folds a joint forward, positive splay takes a midline bone to the rider's left, and on a limb
 * the two sides mirror, so a symmetric move shares a sign and a lopsided one flips it.
 */
export type Turn = [bend: number, twist: number, splay: number];

/** One bone, and how far it turns per unit of each thing the telemetry knows. */
export interface MotionBone {
  /** Rig names, best first. The first one the model binds takes the turn, so a rig short of a
   * joint — and `default_mx_c` binds no spine at all — bends the next one along rather than
   * losing the pose. */
  bones: string[];
  /** Per unit of `leanLR`, +1 being the rider asking for weight to their right. */
  lr?: Turn;
  /** Per unit of `leanFB`, +1 being forward over the tank. */
  fb?: Turn;
  /** Per unit of bike roll — a unit is {@link ROLL_UNIT} degrees. */
  roll?: Turn;
  /** Added while standing. Sitting is the rest the model was authored in, so it adds nothing. */
  stand?: Turn;
}

/** The bike roll that counts as a full unit of counter-lean: a hard, committed corner. */
export const ROLL_UNIT = 45;

// Every angle below is a judgement call, chosen to read clearly at replay size rather than
// measured: `lean` is an input and no body angle was ever recorded, so there is nothing to
// measure against.
export const MOTION_BONES: MotionBone[] = [
  // Counter-lean is the thing worth showing: in a corner the bike is laid over and the rider's
  // upper body stays nearer upright, so the torso bends back against the roll. That is why
  // `bikeRoll` feeds the spine at all — the lean input alone would leave a rider welded to a
  // machine on its side, which is the one shape a replay makes obviously wrong.
  {
    bones: ["riderRIG_Spine1", "riderRIG_Spine2"],
    fb: [7, 0, 0],
    lr: [0, 0, -5],
    roll: [0, 0, 7],
  },
  {
    bones: ["riderRIG_Spine2", "riderRIG_Spine3"],
    fb: [6, 0, 0],
    lr: [0, 0, -4],
    roll: [0, 0, 6],
  },
  {
    // The head comes back the other way, so weight forward and a bike on its side don't take
    // the rider's eyes off the track.
    bones: ["riderRIG_Neck1", "riderRIG_Neck2", "riderRIG_Spine4"],
    fb: [-4, 0, 0],
    lr: [0, 0, 2],
    roll: [0, 0, 4],
  },
  {
    // A rig can be turned but never lifted, so standing "lifts the hips" by straightening the
    // legs under them; the pelvis only opens enough that the back doesn't stay folded.
    bones: ["riderRIG_Pelvis", "riderRIG_Spine1"],
    fb: [4, 0, 0],
    stand: [-6, 0, 0],
  },
  {
    bones: ["riderRIG_LeftHip"],
    stand: [-25, 0, 0],
  },
  {
    bones: ["riderRIG_RightHip"],
    stand: [-25, 0, 0],
  },
  {
    bones: ["riderRIG_LeftKnee"],
    stand: [-35, 0, 0],
  },
  {
    bones: ["riderRIG_RightKnee"],
    stand: [-35, 0, 0],
  },
  {
    bones: ["riderRIG_LeftShoulder", "riderRIG_LeftCollar"],
    fb: [6, 0, 0],
  },
  {
    bones: ["riderRIG_RightShoulder", "riderRIG_RightCollar"],
    fb: [6, 0, 0],
  },
  {
    // Weight to one side folds that elbow and straightens the other, which is what makes a
    // lean read as a lean rather than as a torso sliding sideways.
    bones: ["riderRIG_LeftElbow"],
    fb: [5, 0, 0],
    lr: [-8, 0, 0],
  },
  {
    bones: ["riderRIG_RightElbow"],
    fb: [5, 0, 0],
    lr: [8, 0, 0],
  },
];

/**
 * A lean axis as a number, or null where the recorder never read it.
 *
 * The same test the recorder applies: finite, and inside a stick's range. Null and zero are
 * different facts — zero is the rider asking to be centred, null is nobody knowing — and only
 * zero may turn a bone.
 */
export function leanUnit(v: number): number | null {
  if (!Number.isFinite(v) || v < -1.5 || v > 1.5) return null;
  return Math.max(-1, Math.min(1, v));
}

/** Bike roll as a share of {@link ROLL_UNIT}, or null when the sample carries no attitude. */
export function rollUnit(deg: number): number | null {
  if (!Number.isFinite(deg)) return null;
  return Math.max(-1, Math.min(1, deg / ROLL_UNIT));
}

function add(into: Turn, by: Turn | undefined, amount: number | null): void {
  if (!by || amount === null) return;
  into[0] += by[0] * amount;
  into[1] += by[1] * amount;
  into[2] += by[2] * amount;
}

/**
 * The pose one sample's worth of telemetry justifies.
 *
 * `bound` is the bones the model actually binds, which is what walks each entry's fallback
 * chain; without it the chains' first names are used, and a model missing one simply never
 * reads that turn. Anything the telemetry didn't record contributes nothing at all, so a
 * recording from before the recorder read the Sit control comes back as the rider was
 * authored rather than as a rider asserted to be sitting.
 */
export function riderPoseFrom(motion: RiderMotion, bound?: Iterable<string>): RiderPose {
  const has = bound ? new Set(bound) : null;
  const lr = leanUnit(motion.leanLR);
  const fb = leanUnit(motion.leanFB);
  const roll = rollUnit(motion.bikeRoll);
  // Sitting is the rest pose and unknown is no claim, so only standing moves the legs.
  const standing = motion.stance === "stand" ? 1 : null;

  const raw = new Map<string, Turn>();
  for (const entry of MOTION_BONES) {
    const bone = has ? entry.bones.find((b) => has.has(b)) : entry.bones[0];
    if (!bone) continue;
    const turn: Turn = raw.get(bone) ?? [0, 0, 0];
    add(turn, entry.lr, lr);
    add(turn, entry.fb, fb);
    add(turn, entry.roll, roll);
    add(turn, entry.stand, standing);
    raw.set(bone, turn);
  }

  // Clamped once, at the end: two entries can fall back onto the same bone, and rounding each
  // contribution on the way in would make the total depend on how the chains happened to land.
  const pose: RiderPose = {};
  for (const [bone, turn] of raw) {
    const limit = turnLimit(bone);
    pose[bone] = [
      clampTurn(turn[0], limit),
      clampTurn(turn[1], limit),
      clampTurn(turn[2], limit),
    ];
  }
  return trimPose(pose);
}
