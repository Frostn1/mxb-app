import { frameAt, lerp } from "@coach/Components/Review/useReplayClock";
import type { Ride } from "@/api/cinema";

/** One moment of a ride, read between the two samples either side of it. */
export interface Moment {
  t: number;
  x: number;
  y: number;
  z: number;
  yaw: number;
  pitch: number;
  roll: number;
  steer: number;
  used: [number, number];
  speed: number;
  spin: number;
  throttle: number;
  brake: number;
  gear: number;
  air: boolean;
  crashed: boolean;
  lean: [number | null, number | null];
  stance: number;
}

/** Headings wrap at ±180: read between two of them the short way round. */
function lerpAngle(a: number, b: number, mix: number): number {
  let d = b - a;
  while (d > 180) d -= 360;
  while (d < -180) d += 360;
  return a + d * mix;
}

const known = (v: number) => (Number.isFinite(v) ? v : null);

export function momentAt(r: Ride, t: number): Moment | null {
  if (r.count === 0) return null;
  const { i, mix } = frameAt(r.t as unknown as number[], t);
  const j = Math.min(r.count - 1, i + 1);
  const L = (c: Float32Array) => lerp(c[i], c[j], mix);
  return {
    t,
    x: L(r.x),
    y: L(r.y),
    z: L(r.z),
    yaw: lerpAngle(r.yaw[i], r.yaw[j], mix),
    pitch: L(r.pitch),
    roll: L(r.roll),
    steer: L(r.steer),
    used: [L(r.usedF), L(r.usedR)],
    speed: L(r.speed),
    spin: L(r.spin),
    throttle: L(r.throttle),
    brake: L(r.brake),
    // states, not quantities: the sample below the moment
    gear: r.gear[i],
    air: r.air[i] > 0.5,
    crashed: r.crashed[i] > 0.5,
    lean: [known(r.leanLR[i]), known(r.leanFB[i])],
    stance: r.stance[i],
  };
}

/** The ride's length, seconds. */
export function rideLength(r: Ride): number {
  return r.count ? r.t[r.count - 1] : 0;
}

/** `m:ss.cc`, the way a lap time reads. */
export function clock(s: number): string {
  const m = Math.floor(s / 60);
  const rest = s - m * 60;
  return `${m}:${rest.toFixed(2).padStart(5, "0")}`;
}

/** `2026-10-25T20:47:50` -> a local date and time in the active locale. */
export function when(started: string, locale: string): string {
  const d = new Date(started);
  if (Number.isNaN(d.getTime())) return started;
  return d.toLocaleString(locale, { dateStyle: "medium", timeStyle: "short" });
}

/** Seconds as `1:02` for a duration. */
export function duration(s: number): string {
  const m = Math.floor(s / 60);
  const sec = Math.round(s - m * 60);
  return `${m}:${String(sec).padStart(2, "0")}`;
}
