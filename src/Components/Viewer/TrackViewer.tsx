import { useEffect, useMemo, useState } from "react";
import { Canvas, useThree } from "@react-three/fiber";
import { OrbitControls } from "@react-three/drei";
import { Move, Rotate3d, ZoomIn } from "lucide-react";
import * as THREE from "three";
import { cn } from "@/lib/utils";
import type {
  TrackOverview,
  TrackPlacement,
  TrackScenery,
  TrackSceneryTexture,
  TrackTerrain,
} from "../../types";
import { ErrorBoundary } from "../ErrorBoundary";
import { useT } from "../../i18n/context";

/** The terrain is scaled to sit this many units across, whatever its real size. */
const VIEW_SPAN = 10;

/**
 * Relief is drawn this much taller than life.
 *
 * A motocross track is a few metres of relief across a few hundred of ground, so at true
 * scale the shapes that matter — faces, lips, berms — are a fraction of a degree of slope and
 * read as flat. This is enough to give them a shadow to be seen by, and little enough that
 * the track still looks like ground rather than a mountain range.
 */
const RELIEF_EXAGGERATION = 1.5;

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
function viewFrame(terrain: TrackTerrain) {
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
    heightScale: unitsPerMetre * RELIEF_EXAGGERATION,
    originX: ((width - 1) * step) / 2,
    originZ: ((height - 1) * step) / 2,
  };
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

/** The inverse of [`toView`] — a point picked in the scene, back in world metres. */
function fromView(
  frame: ReturnType<typeof viewFrame>,
  vx: number,
  vy: number,
  vz: number,
): [number, number, number] {
  return [
    (frame.originX - vx) / frame.unitsPerMetre,
    vy / frame.heightScale + frame.midHeight,
    (vz + frame.originZ) / frame.unitsPerMetre,
  ];
}

/** A prop put down on the track: a model, and where it goes in world metres. */
export interface PlacedProp {
  name: string;
  pos: [number, number, number];
  rot?: [number, number, number];
  mesh: TrackScenery;
}

/**
 * Props placed in the app, drawn where they will be written.
 *
 * Built in the terrain's frame like everything else, so what you see standing on the ground
 * is where the `.scr` will put it.
 */
function PlacedProps({
  props: placed,
  terrain,
}: {
  props: PlacedProp[];
  terrain: TrackTerrain;
}) {
  const geometries = useMemo(() => {
    const frame = viewFrame(terrain);
    return placed.map((p) => {
      const [rx, ry, rz] = (p.rot ?? [0, 0, 0]).map((d) => (d * Math.PI) / 180);
      const m = new THREE.Matrix4().makeRotationFromEuler(
        new THREE.Euler(rx, ry, rz, "ZYX"),
      );
      const src = p.mesh.positions;
      const out = new Float32Array(src.length);
      const v = new THREE.Vector3();
      for (let i = 0; i < src.length; i += 3) {
        v.set(src[i], src[i + 1], src[i + 2]).applyMatrix4(m);
        const [x, y, z] = toView(
          frame,
          v.x + p.pos[0],
          v.y + p.pos[1],
          v.z + p.pos[2],
        );
        out[i] = x;
        out[i + 1] = y;
        out[i + 2] = z;
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(out, 3));
      // Rewound for the mirrored X, same as the scenery.
      const src_i = p.mesh.indices;
      const idx = new Uint32Array(src_i.length);
      for (let t = 0; t < src_i.length; t += 3) {
        idx[t] = src_i[t];
        idx[t + 1] = src_i[t + 2];
        idx[t + 2] = src_i[t + 1];
      }
      g.setIndex(new THREE.BufferAttribute(idx, 1));
      g.computeVertexNormals();
      g.computeBoundingSphere();
      return g;
    });
  }, [placed, terrain]);

  useEffect(
    () => () => {
      for (const g of geometries) g.dispose();
    },
    [geometries],
  );
  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => invalidate(), [geometries, invalidate]);

  return (
    <group>
      {geometries.map((g, i) => (
        <mesh key={`${placed[i].name}-${i}`} geometry={g} castShadow>
          <meshStandardMaterial
            color="#7fc8f0"
            roughness={0.85}
            metalness={0}
            side={THREE.DoubleSide}
          />
        </mesh>
      ))}
    </group>
  );
}

function buildGeometry(terrain: TrackTerrain, textured: boolean): THREE.BufferGeometry {
  const { width, height, heights } = terrain;

  const frame = viewFrame(terrain);
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

function TerrainMesh({
  terrain,
  overview,
  onGround,
}: {
  terrain: TrackTerrain;
  overview: TrackOverview | null;
  /** Told where a click landed, in world metres, when the viewer is placing something. */
  onGround?: (at: [number, number, number]) => void;
}) {
  const geometry = useMemo(() => buildGeometry(terrain, overview != null), [terrain, overview]);

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

  // A grid this size is megabytes of GPU buffers, and the viewer replaces it every time the
  // detail level changes — without this each one would be leaked.
  useEffect(() => () => geometry.dispose(), [geometry]);
  useEffect(() => () => texture?.dispose(), [texture]);

  // The canvas only draws when asked, and the map arrives well after the terrain settled —
  // so without this the texture sits on a material nothing ever repaints.
  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => invalidate(), [texture, geometry, invalidate]);

  return (
    // Both cast and receive: the terrain is the only thing in the scene, so every shadow it
    // shows is its own — a jump face darkening the ground in front of it, a berm shading its
    // own inside. That self-shadowing is most of what makes the relief read as ground.
    <mesh
      geometry={geometry}
      castShadow
      receiveShadow
      onClick={
        onGround &&
        ((e) => {
          e.stopPropagation();
          const frame = viewFrame(terrain);
          onGround(fromView(frame, e.point.x, e.point.y, e.point.z));
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
      <meshStandardMaterial
        key={texture ? "textured" : "plain"}
        map={texture ?? undefined}
        vertexColors
        roughness={0.95}
        metalness={0}
      />
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
): THREE.BufferGeometry {
  const frame = viewFrame(terrain);
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

  const srcIndices = scenery.indices;
  const indices = new Uint32Array(srcIndices.length);
  for (let t = 0; t < srcIndices.length; t += 3) {
    indices[t] = srcIndices[t];
    indices[t + 1] = srcIndices[t + 2];
    indices[t + 2] = srcIndices[t + 1];
  }

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
}: {
  scenery: TrackScenery;
  surfaces: TrackSceneryTexture[];
  terrain: TrackTerrain;
  onPick?: (piece: PickedPiece | null) => void;
}) {
  const [picked, setPicked] = useState<number | null>(null);
  // A material slot per surface, plus one plain slot at the end for the groups no surface
  // covers — the `.scr` props, whose own sheets aren't read.
  const { materials, slotOf } = useMemo(() => {
    const slots = new Map<number, number>();
    const list: THREE.Material[] = surfaces.map((t, i) => {
      slots.set(t.material, i);
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
    for (const g of scenery.groups) {
      if (!slots.has(g.material)) slots.set(g.material, fallback);
    }
    return { materials: list, slotOf: slots };
  }, [scenery, surfaces]);

  const geometry = useMemo(
    () => buildSceneryGeometry(scenery, terrain, slotOf),
    [scenery, terrain, slotOf],
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
  const pins = useMemo(() => {
    const frame = viewFrame(terrain);
    return placements
      .filter((p) => p.kind !== "prop")
      .map((p, i) => ({
        key: `${p.kind}-${i}`,
        colour: MARKER_COLOURS[p.kind] ?? MARKER_COLOURS.prop,
        at: toView(frame, p.pos[0], p.pos[1], p.pos[2]),
      }));
  }, [placements, terrain]);

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
  /** Told what a click on the scenery landed on, and when the selection clears. */
  onPick?: (piece: PickedPiece | null) => void;
  /** Props put down in the app, drawn where they will be written. */
  placed?: PlacedProp[];
  /** Told where a click on the ground landed, in world metres. Set only while placing. */
  onGround?: (at: [number, number, number]) => void;
  className?: string;
}

export function TrackViewer({
  terrain,
  overview = null,
  scenery = null,
  surfaces = [],
  placements = [],
  showObjects = true,
  onPick,
  placed = [],
  onGround,
  className,
}: TrackViewerProps) {
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
          <color attach="background" args={["#0e0f13"]} />
          <ambientLight intensity={0.5} />
          {/* Sky above, warm bounce below — enough to keep hollows from going solid black. */}
          <hemisphereLight args={[0xdfe8ff, 0x4a4133, 0.8]} />
          {/* Low and to one side: relief reads by its shadows, and an overhead key flattens
              it. This is the only light that casts — a second caster would double the cost to
              soften shadows the fill light below already softens for free.
              The shadow camera is bounded to the terrain's own span, which is fixed however
              big the real track is, so the whole map fits one map at full precision. */}
          <directionalLight
            position={[8, 6, 4]}
            intensity={1.15}
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
            <TerrainMesh terrain={terrain} overview={overview} onGround={onGround} />
          )}
          {terrain && placed.length > 0 && (
            <PlacedProps props={placed} terrain={terrain} />
          )}
          {terrain && showObjects && scenery && (
            <SceneryMesh
              scenery={scenery}
              surfaces={surfaces}
              terrain={terrain}
              onPick={onPick}
            />
          )}
          {terrain && showObjects && placements.length > 0 && (
            <PlacementMarkers placements={placements} terrain={terrain} />
          )}
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
