import type { EdfNode } from "../types";

/**
 * A neck protection the way it is worn, for the preview.
 *
 * A chain's `.edf` is drawn by the game in the frame of the bone its slot hangs from,
 * `riderRIG_Spine4`: X up the spine, Y the rider's front, Z the rider's right. A chain made for
 * the game is modelled there already (the stock neck brace sits 0.115 to 0.248 up that bone),
 * and fbx3edf's Convert writes it as it is. This turns that frame upright for the screen: the
 * rider facing +Z, the base of the neck on the origin, so the viewer's Top, Front and Right
 * read as Blender's do. The stock torso, read into the same bone frame, takes the same turn,
 * so a chain sits on it where the game puts it.
 *
 * Input is as the viewer reads a loose `.edf` (X negated): (−x, y, z) of the bone frame.
 */

/** The base of the neck in Spine4's frame on the stock rider: up the spine, then forward. */
export const NECK_IN_BONE: readonly [number, number] = [0.198, -0.0184];

/** One point or direction, bone frame as loaded -> worn. */
export function wornPoint(x: number, y: number, z: number, point = true): [number, number, number] {
  // Loaded (−bx, by, bz) -> worn (−bz, bx − neckUp, by − neckFront).
  return point ? [-z, -x - NECK_IN_BONE[0], y - NECK_IN_BONE[1]] : [-z, -x, y];
}

/** Nodes as loaded, turned to how they are worn. A proper turn: the winding stands. */
export function wornNodes(nodes: EdfNode[]): EdfNode[] {
  const turn = (a: Float32Array, point: boolean) => {
    const out = new Float32Array(a.length);
    for (let i = 0; i + 2 < a.length; i += 3) {
      const [x, y, z] = wornPoint(a[i], a[i + 1], a[i + 2], point);
      out[i] = x;
      out[i + 1] = y;
      out[i + 2] = z;
    }
    return out;
  };
  return nodes.map((n) => ({ ...n, positions: turn(n.positions, true), normals: turn(n.normals, false) }));
}
