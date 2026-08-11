import { useEffect, useMemo } from "react";
import { Canvas, useThree } from "@react-three/fiber";
import { OrbitControls } from "@react-three/drei";
import { Move, Rotate3d, ZoomIn } from "lucide-react";
import * as THREE from "three";
import { cn } from "@/lib/utils";
import type { TrackOverview, TrackTerrain } from "../../types";
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
 * Turn a height grid into a mesh.
 *
 * Written straight into typed arrays rather than through `PlaneGeometry` and a displacement
 * pass: a 320² grid is a hundred thousand vertices, and building it a `Vector3` at a time is
 * the difference between a view that appears and one that hitches on arrival.
 *
 * The terrain is scaled to a fixed span so the camera framing holds for any track. Heights
 * are scaled by the same factor as the ground and then by [`RELIEF_EXAGGERATION`], so the
 * relief is proportionate everywhere — a flat supercross floor still looks flatter than a
 * hillside, it is just not drawn so shallow that nothing casts a shadow.
 */
function buildGeometry(terrain: TrackTerrain): THREE.BufferGeometry {
  const { width, height, heights, metresPerSample, minHeight, maxHeight } = terrain;

  // Metres across the widest edge, and the units-per-metre that fits it to the view.
  const spanMetres = Math.max(width - 1, height - 1) * metresPerSample;
  const unitsPerMetre = spanMetres > 0 ? VIEW_SPAN / spanMetres : 1;
  const step = metresPerSample * unitsPerMetre;

  const relief = maxHeight - minHeight;
  const midHeight = (minHeight + maxHeight) / 2;

  const count = width * height;
  const positions = new Float32Array(count * 3);
  const colors = new Float32Array(count * 3);
  // The overview map is drawn to the track's own footprint, so it lays across the grid
  // corner to corner — the same ground, at a different resolution.
  const uvs = new Float32Array(count * 2);
  const colour = new THREE.Color();

  const originX = ((width - 1) * step) / 2;
  const originZ = ((height - 1) * step) / 2;

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = y * width + x;
      const metres = heights[i];
      const o = i * 3;
      // X is negated, the same conversion every model in the app goes through
      // (`edf::to_right_handed`): the game's frame is left-handed and three.js's is not.
      // Without it the whole track is mirrored — every left-hander rides as a right-hander.
      positions[o] = originX - x * step;
      positions[o + 1] = (metres - midHeight) * unitsPerMetre * RELIEF_EXAGGERATION;
      positions[o + 2] = y * step - originZ;

      rampAt(relief > 0 ? (metres - minHeight) / relief : 0.5, colour);
      colors[o] = colour.r;
      colors[o + 1] = colour.g;
      colors[o + 2] = colour.b;

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
  geometry.setAttribute("uv", new THREE.BufferAttribute(uvs, 2));
  geometry.setIndex(new THREE.BufferAttribute(indices, 1));
  geometry.computeVertexNormals();
  geometry.computeBoundingSphere();
  return geometry;
}

function TerrainMesh({
  terrain,
  overview,
}: {
  terrain: TrackTerrain;
  overview: TrackOverview | null;
}) {
  const geometry = useMemo(() => buildGeometry(terrain), [terrain]);

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
    // No shadow casting: relief this size would need a huge shadow map to look like
    // anything, and the directional key below already reads the terrain's shape.
    <mesh geometry={geometry}>
      {/* Flat-ish and unshiny: dirt, and it keeps the relief legible rather than glared out.
          With a map the height ramp steps aside — the two together would tint the picture by
          elevation and misreport what the track's own artwork says. */}
      {/* Keyed on whether there's a texture, so the material is rebuilt rather than mutated
          when one arrives. Both taking a `map` and dropping `vertexColors` change the shader
          three.js compiles, and assigning them to a live material leaves it running the
          program it was built with — the terrain keeps its elevation ramp and never shows the
          picture at all. */}
      <meshStandardMaterial
        key={texture ? "textured" : "plain"}
        map={texture ?? undefined}
        vertexColors={!texture}
        roughness={0.95}
        metalness={0}
      />
    </mesh>
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
  className?: string;
}

export function TrackViewer({ terrain, overview = null, className }: TrackViewerProps) {
  return (
    <div className={cn("relative", className)}>
      <ErrorBoundary compact label="track-viewer">
        <Canvas
          className="h-full w-full"
          // Nothing in the scene animates, so a parked terrain costs no frames at all.
          frameloop="demand"
          dpr={[1, 1.5]}
          camera={{ position: [0, 7.5, 11], fov: 45, near: 0.05, far: 200 }}
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
          {/* Low and to one side: relief reads by its shadows, and an overhead key flattens it. */}
          <directionalLight position={[8, 6, 4]} intensity={1.15} />
          <directionalLight position={[-6, 3, -5]} intensity={0.4} />
          {terrain && <TerrainMesh terrain={terrain} overview={overview} />}
          <OrbitControls
            makeDefault
            enablePan
            screenSpacePanning={false}
            zoomToCursor
            minDistance={1.5}
            maxDistance={40}
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
