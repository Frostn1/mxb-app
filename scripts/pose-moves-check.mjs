// Do the ready-made poses do what their labels say?
//
//   MXB_EDF_FILE=<rider.edf> cargo test rig_json -- --ignored --nocapture | grep '^\[' > sm.json
//   npx esbuild scripts/pose-moves-check.mjs --bundle --format=esm --platform=node \
//     --outfile=/tmp/check.mjs && node /tmp/check.mjs sm.json [more.json...]
//
// Every move used to be a fixed turn in degrees on a bone's own axes, and a bone's axes are
// whatever its author left them as — so "Legs wider" pulled `default_sm`'s legs in (27.6 cm
// apart to 12.9) while widening `default_mx_c`'s, and "Left leg forward" moved neither knee
// forward at all. A move is a place to send a joint now, which is why this can check it: run
// it against the rig and measure where the joint went. Bundled rather than run directly
// because the moves live in TypeScript.
//
import * as THREE from "three";
import { readFileSync } from "node:fs";
import {
  QUICK_MOVES,
  applyQuickMove,
  canMove,
  applyPose,
  boneDelta,
  boneTip,
  buildSkeleton,
  riderFrame,
} from "../src/lib/riderPose.ts";

let failed = 0;
const ok = (cond, what) => {
  if (!cond) failed++;
  console.log(`  ${cond ? "ok  " : "FAIL"}  ${what}`);
};

/** Where a bone's joint is, with `pose` applied. */
function at(bones, pose, name, tip = false) {
  const { order } = buildSkeleton(bones);
  applyPose(order, pose);
  const i = bones.findIndex((b) => b.name === name);
  if (i < 0) return null;
  order[i].updateWorldMatrix(true, false);
  const local = tip ? new THREE.Vector3(...boneTip(bones, i)) : new THREE.Vector3();
  return local.applyMatrix4(order[i].matrixWorld);
}

/** How far a point travelled between two poses, along `axis`. */
const along = (a, b, axis) => b.clone().sub(a).dot(axis);

for (const path of process.argv.slice(2)) {
  const bones = JSON.parse(readFileSync(path, "utf8"));
  console.log(`\n== ${path} — ${bones.length} bones ==`);
  const frame = riderFrame(bones);
  if (!frame) {
    console.log("  FAIL  no frame");
    failed++;
    continue;
  }
  const v = (x) => `(${x.x.toFixed(2)}, ${x.y.toFixed(2)}, ${x.z.toFixed(2)})`;
  console.log(`  up ${v(frame.up)}  left ${v(frame.left)}  fwd ${v(frame.forward)}  thigh ${frame.leg.toFixed(3)}`);
  ok(frame.up.y > 0.9, "up is the scene's up — the viewer draws a rider standing");
  ok(Math.abs(frame.forward.y) < 0.2, "forward is level");
  ok(Math.abs(frame.left.dot(frame.forward)) < 0.01, "left and forward are square");

  // The feet are the primary witness for facing; the thumb is the fallback. Build a foot
  // cloud in front of the knee and check the two agree.
  const knee = at(bones, {}, "riderRIG_LeftKnee");
  const foot = [];
  for (let i = 0; i < 60; i++) {
    const p = knee
      .clone()
      .addScaledVector(frame.up, -0.42)
      .addScaledVector(frame.forward, -0.06 + (i / 60) * 0.26);
    foot.push(p.x, p.y, p.z);
  }
  const withFeet = riderFrame(bones, [{ positions: foot }]);
  ok(
    withFeet.forward.dot(frame.forward) > 0.99,
    "the toes and the thumb say the same way round",
  );

  const rest = {};
  for (const move of QUICK_MOVES) {
    const posed = applyQuickMove(rest, move, bones, frame);
    const turned = Object.keys(posed);
    const lk = [at(bones, rest, "riderRIG_LeftKnee"), at(bones, posed, "riderRIG_LeftKnee")];
    const rk = [at(bones, rest, "riderRIG_RightKnee"), at(bones, posed, "riderRIG_RightKnee")];
    const gap = [lk[0].distanceTo(rk[0]), lk[1].distanceTo(rk[1])];
    const offered = canMove(move, bones);
    console.log(`\n  ${move.id}: ${turned.length} bones turned, knee gap ${gap[0].toFixed(3)} → ${gap[1].toFixed(3)}${offered ? "" : " (not offered on this rig)"}`);
    ok(offered ? turned.length > 0 : turned.length === 0, "it turns what it says it turns");
    if (!offered) continue;

    if (move.id === "legsWide") ok(gap[1] > gap[0] + 0.05, "the legs go wider");
    if (move.id === "legsNarrow") ok(gap[1] < gap[0] - 0.03, "the legs come together");
    if (move.id === "leftLegForward") {
      ok(along(lk[0], lk[1], frame.forward) > 0.05, "the left knee goes forward");
      ok(along(rk[0], rk[1], frame.forward) < -0.02, "the right knee goes back");
    }
    if (move.id === "elbowsUp") {
      const le = [at(bones, rest, "riderRIG_LeftElbow"), at(bones, posed, "riderRIG_LeftElbow")];
      ok(along(le[0], le[1], frame.up) > 0.03, "the elbow comes up");
      ok(along(le[0], le[1], frame.left) > 0.01, "and out");
    }
    if (move.id === "leanIn") {
      const neck =
        at(bones, rest, "riderRIG_Neck1") ?? at(bones, rest, "riderRIG_LeftCollar");
      if (neck) {
        const now = at(bones, posed, "riderRIG_Neck1") ?? at(bones, posed, "riderRIG_LeftCollar");
        ok(along(neck, now, frame.forward) > 0.02, "the chest goes forward");
      } else {
        console.log("  --    no spine on this model, nothing to lean");
      }
    }
    if (move.id === "sitOnBike") {
      ok(along(lk[0], lk[1], frame.forward) > 0.1, "the thighs come forward");
      // Relative to the knee, which the thigh has already carried forward.
      const shin = [
        at(bones, rest, "riderRIG_LeftKnee", true).sub(lk[0]),
        at(bones, posed, "riderRIG_LeftKnee", true).sub(lk[1]),
      ];
      const fold = shin[1].dot(frame.forward) - shin[0].dot(frame.forward);
      ok(fold < -0.05, `and the shin folds back under the knee (${fold.toFixed(3)} m)`);
    }

    // The legs are a pair, so a move on one must not carry the other with it.
    if (move.id === "leftLegForward") {
      const solo = applyQuickMove(rest, { id: "x", steps: [move.steps[0]] }, bones, frame);
      const other = [at(bones, rest, "riderRIG_RightKnee"), at(bones, solo, "riderRIG_RightKnee")];
      ok(other[0].distanceTo(other[1]) < 1e-4, "moving one leg leaves the other where it was");
    }

    // Boots ride the knee twist; if its delta doesn't travel with the knee, they stay behind.
    for (const [side, k] of [["Left", lk], ["Right", rk]]) {
      const { order } = buildSkeleton(bones);
      applyPose(order, posed);
      const d =
        boneDelta(order, bones, `riderRIG_${side}KneeTwist`) ??
        boneDelta(order, bones, `riderRIG_${side}Knee`);
      if (!d) continue;
      const moved = k[0].clone().applyMatrix4(d);
      const drift = moved.distanceTo(k[1]);
      ok(drift < 0.02, `the ${side.toLowerCase()} boot follows its knee (off by ${drift.toFixed(4)} m)`);
    }
  }
}
console.log(failed ? `\n${failed} FAILED` : "\nall ok");
process.exit(failed ? 1 : 0);
