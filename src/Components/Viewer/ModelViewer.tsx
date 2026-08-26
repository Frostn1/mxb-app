import { useEffect, useMemo, useRef, useState } from "react";
import { Canvas, useThree } from "@react-three/fiber";
import { OrbitControls, Center, ContactShadows } from "@react-three/drei";
import { ChevronDown, Move, Rotate3d, SlidersHorizontal, ZoomIn } from "lucide-react";
import * as THREE from "three";
import { cn } from "@/lib/utils";
import { Row, Slider } from "@/Components/ui/controls";
import type { BikeRig, EdfNode, PaintTexture, RiderPart, Vec3 } from "../../types";
import { textureBytes } from "../../api/mods";
import { ErrorBoundary } from "../ErrorBoundary";
import { useT } from "../../i18n/context";

/** `both` draws the bike and the rider in one scene — see {@link SideBySide}. */
export type ViewerMode = "bike" | "rider" | "both";

/**
 * Pull a texture's pixels over the binary IPC channel and wrap them in a `DataTexture`.
 *
 * The backend hands us raw RGBA rather than an encoded image, so there is no decode step
 * here — but that also means none of `TextureLoader`'s defaults come along, and every
 * sampler setting below has to be stated. `DataTexture` starts out nearest-filtered with no
 * mipmaps, which would read as a hard, aliased livery.
 */
async function loadTexture(t: PaintTexture): Promise<THREE.DataTexture | null> {
  let buf: ArrayBuffer;
  try {
    buf = await textureBytes(t.token);
  } catch (e) {
    console.warn(`[ModelViewer] texture '${t.name}' could not be fetched:`, e);
    return null;
  }
  const expected = t.width * t.height * 4;
  if (buf.byteLength !== expected) {
    // The store dropped it (or something is badly out of step) — leave the part untextured
    // rather than hand three.js a buffer it will read past the end of.
    console.warn(
      `[ModelViewer] texture '${t.name}' is ${buf.byteLength}B, expected ${expected}B`,
    );
    return null;
  }
  const rgba = new Uint8Array(buf);
  const tex = new THREE.DataTexture(rgba, t.width, t.height, THREE.RGBAFormat);
  tex.userData.maskedAlpha = hasMaskedAlpha(rgba);
  tex.colorSpace = THREE.SRGBColorSpace;
  // MX Bikes paints use a top-left UV origin, which is `DataTexture`'s own default.
  tex.flipY = false;
  // Wrap (not clamp): some islands run outside 0–1 (plates, tiled exhaust) and need it.
  tex.wrapS = THREE.RepeatWrapping;
  tex.wrapT = THREE.RepeatWrapping;
  tex.magFilter = THREE.LinearFilter;
  tex.minFilter = THREE.LinearMipmapLinearFilter;
  tex.generateMipmaps = true;
  tex.anisotropy = 4;
  tex.needsUpdate = true;
  return tex;
}

/**
 * Whether a sheet's alpha channel is a cut-out mask, or just a channel nobody filled in.
 *
 * A wheel's brake discs and its sprocket are flat quads wearing a masked square — two thirds
 * of `fdisc` is fully transparent — so drawn without a mask each one is a square sitting on
 * the wheel. A naive "does it have alpha" test can't be used, though: a bike's `w_plate` is
 * alpha-0 on *every* pixel, an unused channel, and masking on that erases the number plates
 * outright. So the channel only counts as a mask when it varies.
 *
 * Sampled, not scanned: a 4096² sheet is 16M pixels and this runs per texture per load,
 * while a real mask covers a third of the image or more and turns up in the first few
 * samples. A mask smaller than one part in 65536 is missed, and renders as it always did.
 */
function hasMaskedAlpha(rgba: Uint8Array): boolean {
  const pixels = rgba.length / 4;
  const step = Math.max(1, Math.floor(pixels / 65536));
  let clear = false;
  let solid = false;
  for (let i = 0; i < pixels; i += step) {
    if (rgba[i * 4 + 3] < 128) clear = true;
    else solid = true;
    if (clear && solid) return true;
  }
  return false;
}

/**
 * The map a mesh with nothing to wear gets — one shared instance, never written to.
 *
 * Shared rather than freshly built, because handing out a new empty `Map` is a *state change*
 * as far as React is concerned, and the effect below would schedule the render that re-runs it.
 */
const NO_TEXTURES: Map<string, THREE.Texture> = new Map();

/** The same, for the `textures` prop — a default of `[]` would be a new list per render. */
const NO_PAINT_TEXTURES: PaintTexture[] = [];

/**
 * What a list of textures is, as far as this hook is concerned: the names it binds under and
 * the pixels behind them.
 *
 * The effect below is keyed on this rather than on the array's identity, and that isn't a
 * micro-optimisation — it's what stops the hook looping. A caller that builds its list inline,
 * or leaves the prop off and takes the default, hands over a new array on every render; keyed
 * on identity, the effect would re-run each time, `setMap` would schedule another render, and
 * that render would re-run the effect. React ends that with "Maximum update depth exceeded".
 */
function texturesKey(textures: PaintTexture[]): string {
  return textures.map((t) => `${t.name} ${t.token} ${t.width}x${t.height}`).join("\n");
}

function useTextureMap(textures: PaintTexture[]): Map<string, THREE.Texture> {
  const [map, setMap] = useState<Map<string, THREE.Texture>>(NO_TEXTURES);
  const key = texturesKey(textures);
  useEffect(() => {
    if (!textures.length) {
      // The same map every time, so an untextured mesh settles instead of re-rendering
      // forever. `prev` is returned untouched when it is already empty, which is the bail-out
      // React needs to stop scheduling work.
      setMap((prev) => (prev.size ? NO_TEXTURES : prev));
      return;
    }
    let alive = true;
    // Disposal hangs off this rather than off each texture as it lands, so every texture
    // has exactly one owner no matter when the effect is torn down.
    let settled: Map<string, THREE.Texture> | null = null;

    void Promise.all(
      // `all`, not a counter: a texture that fails to load used to leave the tally short,
      // and the model stayed untextured forever instead of losing the one bad part.
      textures.map(async (t) => [t.name.toLowerCase(), await loadTexture(t)] as const),
    ).then((pairs) => {
      const loaded = new Map<string, THREE.Texture>();
      for (const [name, tex] of pairs) if (tex) loaded.set(name, tex);
      if (!alive) {
        loaded.forEach((tex) => tex.dispose());
        return;
      }
      settled = loaded;
      setMap(loaded);
    });

    return () => {
      alive = false;
      settled?.forEach((tex) => tex.dispose());
    };
    // `textures` is read through `key`: it changes exactly when the list's contents do, so the
    // closure is never stale — and never re-run for an array that only *looks* new.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return map;
}

/**
 * [`useTextureMap`], with caller-owned textures layered over it by name.
 *
 * The override is how pixels that only exist in this webview — the Designer's live composite
 * canvas — reach a mesh, which otherwise only ever sees textures fetched by token. Layered
 * last so a sheet being edited replaces the installed one it is named after, and never merged
 * into the disposal set below: these belong to whoever passed them in.
 */
function useTextureMapWith(
  textures: PaintTexture[],
  overrides?: Map<string, THREE.Texture>,
): Map<string, THREE.Texture> {
  const loaded = useTextureMap(textures);
  return useMemo(
    () => (overrides?.size ? new Map([...loaded, ...overrides]) : loaded),
    [loaded, overrides],
  );
}

function submeshTexture(
  texture: string | null | undefined,
  tex: Map<string, THREE.Texture>,
): THREE.Texture | null {
  return (texture && tex.get(texture.toLowerCase())) || null;
}

/** A single texture, for the stand-in body and the loose gear pieces. */
function useDataTexture(source: PaintTexture | null | undefined): THREE.Texture | null {
  const [tex, setTex] = useState<THREE.Texture | null>(null);
  const current = useRef<THREE.Texture | null>(null);
  const token = source?.token;

  useEffect(() => {
    if (!source) {
      current.current?.dispose();
      current.current = null;
      setTex(null);
      return;
    }
    let disposed = false;
    void loadTexture(source).then((t) => {
      if (!t) return;
      if (disposed) {
        t.dispose();
        return;
      }
      current.current?.dispose();
      current.current = t;
      setTex(t);
    });
    return () => {
      disposed = true;
    };
    // Keyed on the token: the same pixels never need re-fetching just because the object
    // identity around them changed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  useEffect(
    () => () => {
      current.current?.dispose();
      current.current = null;
    },
    [],
  );

  return tex;
}

function bodyMaterial(map: THREE.Texture | null, color: string) {
  return map ? (
    <meshStandardMaterial map={map} metalness={0.15} roughness={0.55} />
  ) : (
    <meshStandardMaterial color={color} metalness={0.2} roughness={0.5} />
  );
}

function BikeStandIn({ map }: { map: THREE.Texture | null }) {
  return (
    <group rotation={[0, Math.PI / 6, 0]}>
      {[-0.9, 0.9].map((x) => (
        <mesh key={x} position={[x, 0.45, 0]} rotation={[Math.PI / 2, 0, 0]}>
          <torusGeometry args={[0.45, 0.14, 16, 40]} />
          <meshStandardMaterial color="#1a1a1a" roughness={0.8} />
        </mesh>
      ))}
      <mesh position={[0, 0.9, 0]}>
        <boxGeometry args={[1.5, 0.42, 0.34]} />
        {bodyMaterial(map, "#c0392b")}
      </mesh>
      <mesh position={[-0.45, 1.18, 0]} rotation={[0, 0, 0.25]}>
        <boxGeometry args={[0.7, 0.26, 0.3]} />
        {bodyMaterial(map, "#c0392b")}
      </mesh>
      <mesh position={[0.85, 0.85, 0]} rotation={[0, 0, -0.35]}>
        <cylinderGeometry args={[0.05, 0.05, 0.9, 12]} />
        <meshStandardMaterial color="#888" metalness={0.7} roughness={0.3} />
      </mesh>
      <mesh position={[1.0, 1.3, 0]} rotation={[Math.PI / 2, 0, 0]}>
        <cylinderGeometry args={[0.03, 0.03, 0.5, 10]} />
        <meshStandardMaterial color="#333" />
      </mesh>
    </group>
  );
}

function RiderBody({
  suit,
  gloves,
  showHead,
}: {
  suit: THREE.Texture | null;
  gloves: THREE.Texture | null;
  showHead: boolean;
}) {
  return (
    <group>
      {showHead && (
        <mesh position={[0, 1.62, 0]}>
          <sphereGeometry args={[0.2, 24, 24]} />
          {bodyMaterial(suit, "#2c3e50")}
        </mesh>
      )}
      <mesh position={[0, 1.15, 0]}>
        <capsuleGeometry args={[0.22, 0.45, 8, 16]} />
        {bodyMaterial(suit, "#2c3e50")}
      </mesh>
      {[-0.32, 0.32].map((x) => (
        <group key={x}>
          <mesh position={[x, 1.15, 0]} rotation={[0, 0, x < 0 ? 0.3 : -0.3]}>
            <capsuleGeometry args={[0.08, 0.5, 6, 12]} />
            <meshStandardMaterial color="#34495e" roughness={0.6} />
          </mesh>
          <mesh position={[x * 1.28, 0.86, 0]}>
            <sphereGeometry args={[0.09, 16, 16]} />
            {bodyMaterial(gloves, "#222831")}
          </mesh>
        </group>
      ))}
      {[-0.13, 0.13].map((x) => (
        <mesh key={x} position={[x, 0.5, 0]}>
          <capsuleGeometry args={[0.1, 0.6, 6, 12]} />
          <meshStandardMaterial color="#34495e" roughness={0.6} />
        </mesh>
      ))}
    </group>
  );
}

function partTexture(part: RiderPart | undefined, ...names: string[]): PaintTexture | null {
  if (!part?.textures.length) return null;
  const hit = part.textures.find((t) => names.includes(t.name.toLowerCase()));
  return hit ?? part.textures[0];
}

// Helmet/protection are authored X-up; after to_right_handed negates X, up is −X,
// so a −90° roll about Z reaches three.js' Y-up. Boots differ — see BOOT_ROT.
const GEAR_ROT: [number, number, number] = [0, 0, -Math.PI / 2];

// Boots' worn-up is +X (opposite the helmet), so +90° roll. A boots `.edf` ships
// both feet as separate nodes (`boot_l`/`boot_r`) coincident at the ankle, split by bootSides.
const BOOT_ROT: [number, number, number] = [0, 0, Math.PI / 2];

// Protection shares the helmet's up-axis (GEAR_ROT) but is rolled a quarter turn about it:
// its left-right axis is Z where a helmet's is Y. So the piece stands up on the roll alone
// and then needs a −90° yaw to face the way the body does.
//
// Measured off the meshes rather than assumed: in the frame the loader hands over, every
// piece — the game's own chest protector and neck brace, a tactical vest, a Leatt, two
// chains — is mirror-symmetric about Z and stacks along X, and the Long Hair mod sits wholly
// at −Y, which is the rider's back. Front is +Y, and after the roll that lands on +Z, the
// way the body faces.
const PROT_YAW = -Math.PI / 2;

// Downward nod so the helmet gazes ahead / slightly down rather than skyward.
const HELMET_PITCH = 0.25;

// Tip the boots' toes forward into a riding stance.
const BOOT_PITCH = 0.2;

// Splay each boot outward (toes out) about world Y; applied per side on the full body only.
const BOOT_SPLAY = 0.48;

function makeGearMaterial(
  base: string | null | undefined,
  tex: Map<string, THREE.Texture>,
) {
  // Bind only to this submesh's own texture. A miss renders neutral grey (color below) —
  // never fall back to another texture, which would smear e.g. the goggle lens over the shell.
  const map = submeshTexture(base, tex);
  const normalMap = base ? tex.get(`${base.toLowerCase()}_n`) ?? null : null;
  return new THREE.MeshStandardMaterial({
    map: map ?? undefined,
    normalMap,
    color: map ? 0xffffff : 0x9aa2ad,
    metalness: 0.05,
    roughness: 0.55,
    emissive: map ? 0xffffff : 0x000000,
    emissiveMap: map,
    emissiveIntensity: map ? 0.28 : 0.0,
    side: THREE.DoubleSide,
  });
}

/**
 * One material per submesh, or a single material for a node with no submesh table.
 *
 * The shape has to match what `useNodeGeometries` builds: it adds a material group per
 * submesh and none at all for a node without them, and three.js draws an *array* material
 * strictly per group. So an array handed to a group-less node renders nothing whatever —
 * geometry, textures and all present, and not one triangle on screen. That is a real shape:
 * `bind_gear_submeshes` gives a single-piece helmet its texture on the node itself, which is
 * how the game's own stock helmet and any one-piece mod arrive. `RiderBodyMesh` unwraps the
 * same case at its call site; gear does it here, once, so both readers get it right.
 */
function useGearMaterials(
  part: RiderPart,
  tex: Map<string, THREE.Texture>,
): (THREE.Material | THREE.Material[])[] {
  const mats = useMemo(() => {
    return part.nodes.map((n) =>
      n.submeshes.length
        ? n.submeshes.map((sm) => makeGearMaterial(sm.texture, tex))
        : makeGearMaterial(n.texture, tex),
    );
  }, [part, tex]);
  useEffect(
    () => () => mats.flat().forEach((m) => m.dispose()),
    [mats],
  );
  return mats;
}

function RiderGearMesh({
  part,
  anchor,
  target = 1,
  rot = GEAR_ROT,
  yaw = 0,
  alignY = "center",
  pitch = 0,
  fit: fitMode = "box",
  overrides,
}: {
  part: RiderPart;
  anchor: [number, number, number];
  /** Longest edge the bounding box is scaled to. Ignored when `fit` is `"native"`. */
  target?: number;
  rot?: [number, number, number];
  yaw?: number;
  alignY?: "center" | "top" | "bottom";
  pitch?: number;
  /**
   * How the piece meets the body. `"box"` scales its bounding box to `target` and centres
   * it on the anchor — right for a helmet or a boot, each authored around its own origin
   * in its own frame. `"native"` hangs it off the anchor unscaled, at the size and offset
   * it was authored at, and ignores `target`.
   */
  fit?: "box" | "native";
  overrides?: Map<string, THREE.Texture>;
}) {
  const texMap = useTextureMapWith(part.textures, overrides);
  const geoms = useNodeGeometries(part.nodes);
  const mats = useGearMaterials(part, texMap);

  // Gear is authored around its own origin, so fit it onto the body. Measure in the
  // fully-oriented frame (up-axis rot, then yaw, then pitch) to match the rendered mesh.
  const fit = useMemo(() => {
    const rotM = new THREE.Matrix4().makeRotationFromEuler(new THREE.Euler(...rot));
    const orientM = new THREE.Matrix4()
      .makeRotationX(pitch)
      .multiply(new THREE.Matrix4().makeRotationY(yaw))
      .multiply(rotM);
    const box = new THREE.Box3();
    for (const g of geoms) {
      if (!g.boundingBox) g.computeBoundingBox();
      if (g.boundingBox) box.union(g.boundingBox.clone().applyMatrix4(orientM));
    }
    if (box.isEmpty()) return null;
    const size = new THREE.Vector3();
    const center = new THREE.Vector3();
    box.getSize(size);
    box.getCenter(center);
    const dim = Math.max(size.x, size.y, size.z) || 1;
    return { scale: target / dim, center, halfY: size.y / 2 };
  }, [geoms, target, rot, yaw, pitch]);

  if (!fit) return null;
  // Native: the mesh already knows its own size and where it hangs off the mount, so the
  // only thing left to do is put its origin on the anchor.
  const s = fitMode === "native" ? 1 : fit.scale;
  // Shift so the requested bbox edge (not just the centre) lands on anchor[1].
  const alignShift =
    alignY === "bottom" ? fit.halfY * s : alignY === "top" ? -fit.halfY * s : 0;
  const recentre = fitMode === "native" ? new THREE.Vector3() : fit.center;
  return (
    <group
      position={[
        anchor[0] - recentre.x * s,
        anchor[1] - recentre.y * s + alignShift,
        anchor[2] - recentre.z * s,
      ]}
      scale={s}
    >
      {/* Pitch (nod) ▷ yaw (facing) ▷ up-axis roll — matches the `orientM` above. */}
      <group rotation={[pitch, 0, 0]}>
        <group rotation={[0, yaw, 0]}>
          <group rotation={rot}>
            {geoms.map((g, i) => (
              <mesh key={i} geometry={g} material={mats[i]} castShadow receiveShadow />
            ))}
          </group>
        </group>
      </group>
    </group>
  );
}

/**
 * Which leg a boot node belongs on: −1 for the rider's right (−X), +1 for their left (+X).
 *
 * The rider faces +Z — the body's name and number planes, which sit on a rider's back, are
 * at its most negative Z — and in a right-handed Y-up frame a figure facing +Z wears its
 * left on +X.
 *
 * `null` where the mesh doesn't say, which the geometric split below then settles.
 */
function namedBootSide(node: EdfNode): number | null {
  // The node's own name first, then its groups': a pair comes as `boot_l`/`boot_r`, or as
  // one node holding `lboots`/`rboots`, and either alone identifies the foot.
  for (const name of [node.name, ...node.submeshes.map((s) => s.name)]) {
    // Drop the word the piece is named for, leaving the side marker to read on its own:
    // `boot_l` → `_l`, `lboots` → `l`, `Boot_Right` → `_right`. Whole-word only, so
    // `boot_lod0` and a group called `plastic` say nothing.
    const rest = name.toLowerCase().replace(/boots?/g, "");
    if (/(^|[^a-z])l(eft)?([^a-z]|$)/.test(rest)) return 1;
    if (/(^|[^a-z])r(ight)?([^a-z]|$)/.test(rest)) return -1;
  }
  return null;
}

/**
 * Boots ship both feet as separate nodes, near enough coincident at the ankle. Put each on
 * the leg the mesh says it belongs to.
 *
 * The two nodes differ laterally by around a centimetre and a half — that's each foot's own
 * asymmetry about the mirror plane it was copied across, not a left-right layout — so
 * splitting them on that centre was a coin toss decided by how an author happened to bias
 * the mesh, and came out mirrored on the mods that biased it the other way. Names are the
 * mesh's own statement of which foot is which; the centre only settles a pair that has none.
 */
function bootSideOf(part: RiderPart): number[] {
  const named = part.nodes.map(namedBootSide);
  // Both feet named, and named as opposites — a lone "left", or two nodes both reading
  // left, is a naming accident rather than an answer, and the geometry below is better
  // evidence than half a statement.
  if (named.length === 2 && named[0] !== null && named[1] !== null && named[0] !== named[1]) {
    return named as number[];
  }
  // Nothing to read: fall back to the pair's own lateral centres, lower to the right.
  const ys = part.nodes.map((node) => {
    let lo = Infinity;
    let hi = -Infinity;
    for (let i = 1; i < node.positions.length; i += 3) {
      const v = node.positions[i];
      if (v < lo) lo = v;
      if (v > hi) hi = v;
    }
    return (lo + hi) / 2;
  });
  const lowest = ys.reduce((best, y, i) => (y < ys[best] ? i : best), 0);
  return part.nodes.map((_, i) => (i === lowest ? -1 : 1));
}

/** The same split as [`bootSideOf`], paired back up with the nodes it describes. */
function bootSides(part: RiderPart): { node: EdfNode; side: number }[] {
  const sides = bootSideOf(part);
  return part.nodes.map((node, i) => ({ node, side: sides[i] }));
}

function partBounds(nodes: EdfNode[]) {
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (const n of nodes) {
    for (let i = 0; i < n.positions.length; i += 3) {
      for (let k = 0; k < 3; k++) {
        const v = n.positions[i + k];
        if (v < lo[k]) lo[k] = v;
        if (v > hi[k]) hi[k] = v;
      }
    }
  }
  return { lo, hi };
}

// Yaw about world Y that points a boot's heel→toe along +Z, from the centroids of
// the front and back 20% along Z (measured in the up-righted frame). 0 for degenerate input.
function straightenYaw(geom: THREE.BufferGeometry, rotM: THREE.Matrix4): number {
  const pos = geom.getAttribute("position") as THREE.BufferAttribute | undefined;
  if (!pos) return 0;
  const v = new THREE.Vector3();
  const pts: [number, number][] = [];
  let zmin = Infinity;
  let zmax = -Infinity;
  for (let i = 0; i < pos.count; i++) {
    v.fromBufferAttribute(pos, i).applyMatrix4(rotM);
    pts.push([v.x, v.z]);
    if (v.z < zmin) zmin = v.z;
    if (v.z > zmax) zmax = v.z;
  }
  const span = zmax - zmin;
  if (span < 1e-6) return 0;
  const loCut = zmin + span * 0.2;
  const hiCut = zmax - span * 0.2;
  let hx = 0;
  let hz = 0;
  let hn = 0;
  let tx = 0;
  let tz = 0;
  let tn = 0;
  for (const [x, z] of pts) {
    if (z <= loCut) {
      hx += x;
      hz += z;
      hn++;
    }
    if (z >= hiCut) {
      tx += x;
      tz += z;
      tn++;
    }
  }
  if (!hn || !tn) return 0;
  const dx = tx / tn - hx / hn;
  const dz = tz / tn - hz / hn;
  if (Math.abs(dz) < 1e-6) return 0;
  return -Math.atan2(dx, dz);
}

/**
 * Turn parsed nodes into `BufferGeometry`, keyed on the nodes alone.
 *
 * Deliberately separate from the materials that dress them: geometry only changes when the
 * model does, so a paint switch has no business rebuilding vertex buffers and re-uploading
 * a whole bike to the GPU.
 */
function useNodeGeometries(nodes: EdfNode[]) {
  const geoms = useMemo(() => {
    return nodes.map((n) => {
      const g = new THREE.BufferGeometry();
      g.setAttribute(
        "position",
        new THREE.Float32BufferAttribute(Float32Array.from(n.positions), 3),
      );
      if (n.uvs.length)
        g.setAttribute("uv", new THREE.Float32BufferAttribute(Float32Array.from(n.uvs), 2));
      if (n.normals.length)
        g.setAttribute(
          "normal",
          new THREE.Float32BufferAttribute(Float32Array.from(n.normals), 3),
        );
      g.setIndex(n.indices);
      if (!n.normals.length) g.computeVertexNormals();
      // Material groups so a multi-submesh node can wear one texture per submesh.
      n.submeshes.forEach((sm, i) => g.addGroup(sm.triStart * 3, sm.triCount * 3, i));
      g.computeBoundingBox();
      g.computeBoundingSphere();
      return g;
    });
  }, [nodes]);
  useEffect(() => () => geoms.forEach((g) => g.dispose()), [geoms]);
  return geoms;
}

function makeBodyMaterial(name: string | null | undefined, tex: Map<string, THREE.Texture>) {
  const key = name?.toLowerCase();
  // Decal planes: render nothing rather than smear the suit over a flat quad.
  if (key === "hide") {
    return new THREE.MeshBasicMaterial({ colorWrite: false, depthWrite: false });
  }
  // Head/neck: bare skin so the kit doesn't wrap onto it.
  if (key === "face") {
    return new THREE.MeshStandardMaterial({
      color: 0xc79a74,
      metalness: 0.0,
      roughness: 0.75,
      side: THREE.DoubleSide,
    });
  }
  const suit = tex.get("rider") ?? null;
  const map = (key && tex.get(key)) || suit;
  const normalMap = (key && tex.get(`${key}_n`)) || tex.get("rider_n") || null;
  return new THREE.MeshStandardMaterial({
    map: map ?? undefined,
    // Subtle normal scale — full strength over-shadows the paint.
    normalMap,
    normalScale: new THREE.Vector2(0.45, 0.45),
    color: map ? 0xffffff : 0x8a929c,
    metalness: 0.0,
    roughness: 0.62,
    // Self-illuminate with the paint's own colour so it reads true even in shadow.
    emissive: map ? 0xffffff : 0x000000,
    emissiveMap: map,
    emissiveIntensity: map ? 0.32 : 0.0,
    // Meshes aren't reliably wound/closed — render both faces so the body reads solid.
    side: THREE.DoubleSide,
  });
}

function RiderBodyMesh({
  part,
  overrides,
}: {
  part: RiderPart;
  overrides?: Map<string, THREE.Texture>;
}) {
  const tex = useTextureMapWith(part.textures, overrides);
  const geoms = useNodeGeometries(part.nodes);
  // One material per submesh; a node with no submesh table takes a single suit material.
  const mats = useMemo(
    () =>
      part.nodes.map((n) =>
        n.submeshes.length
          ? n.submeshes.map((sm) => makeBodyMaterial(sm.texture, tex))
          : [makeBodyMaterial("rider", tex)],
      ),
    [part, tex],
  );
  useEffect(() => () => mats.forEach((a) => a.forEach((m) => m.dispose())), [mats]);
  return (
    <group>
      {geoms.map((g, i) => (
        <mesh
          key={i}
          geometry={g}
          material={mats[i].length === 1 ? mats[i][0] : mats[i]}
          castShadow
          receiveShadow
        />
      ))}
    </group>
  );
}

function RiderGearSolo({
  part,
  overrides,
}: {
  part: RiderPart;
  overrides?: Map<string, THREE.Texture>;
}) {
  const tex = useTextureMapWith(part.textures, overrides);
  const geoms = useNodeGeometries(part.nodes);
  const mats = useGearMaterials(part, tex);
  const rot = part.part === "boots" ? BOOT_ROT : GEAR_ROT;
  // The quarter turn that faces a protection piece forward, the same one the on-body render
  // gives it — a preview that showed a chest protector edge-on was the piece being right and
  // only this view disagreeing.
  const baseYaw = part.part === "protection" ? PROT_YAW : 0;
  // Measure in the rotated frame; for a coincident two-node boots pair, push each foot
  // to its own side, straighten each toe, then scale and recentre on the origin ourselves.
  const layout = useMemo(() => {
    const rotM = new THREE.Matrix4().makeRotationFromEuler(new THREE.Euler(...rot));
    const boxes = geoms.map((g) => {
      if (!g.boundingBox) g.computeBoundingBox();
      return g.boundingBox
        ? g.boundingBox.clone().applyMatrix4(rotM)
        : new THREE.Box3();
    });
    const offsets = geoms.map(() => 0);
    const pair =
      part.part === "boots" && boxes.length === 2 && !boxes.some((b) => b.isEmpty());
    if (pair) {
      const w = Math.max(boxes[0].max.x - boxes[0].min.x, boxes[1].max.x - boxes[1].min.x);
      // Each foot to the side it belongs on — read off the mesh by the same rule the
      // on-body render uses, so the preview and the rider can't disagree about a boot.
      const sides = bootSideOf(part);
      offsets[0] = sides[0] * w * 0.55;
      offsets[1] = sides[1] * w * 0.55;
    }
    // Straighten each foot so its toe points forward instead of splaying in.
    const yaws = geoms.map((g) => (pair ? straightenYaw(g, rotM) : baseYaw));
    // Arranged bounds: each foot as T(offset)·RotY(yaw)·rot.
    const total = new THREE.Box3();
    geoms.forEach((g, i) => {
      if (!g.boundingBox) return;
      const m = new THREE.Matrix4()
        .makeTranslation(offsets[i], 0, 0)
        .multiply(new THREE.Matrix4().makeRotationY(yaws[i]))
        .multiply(rotM);
      total.union(g.boundingBox.clone().applyMatrix4(m));
    });
    if (total.isEmpty()) return null;
    const size = new THREE.Vector3();
    total.getSize(size);
    const center = new THREE.Vector3();
    total.getCenter(center);
    return { scale: 1.1 / (Math.max(size.x, size.y, size.z) || 1), offsets, yaws, center };
  }, [geoms, rot, baseYaw, part]);
  if (!layout) return null;
  return (
    <group scale={layout.scale}>
      <group position={[-layout.center.x, -layout.center.y, -layout.center.z]}>
        {geoms.map((g, i) => (
          <group key={i} position={[layout.offsets[i], 0, 0]} rotation={[0, layout.yaws[i], 0]}>
            <group rotation={rot}>
              <mesh geometry={g} material={mats[i]} castShadow receiveShadow />
            </group>
          </group>
        ))}
      </group>
    </group>
  );
}

function RiderComposite({
  parts,
  overrides,
}: {
  parts: RiderPart[];
  overrides?: Map<string, THREE.Texture>;
}) {
  const byPart = (p: RiderPart["part"]) => parts.find((x) => x.part === p);
  const body = byPart("body");
  const helmet = byPart("helmet");
  const boots = byPart("boots");
  const protection = byPart("protection");
  const suit = useDataTexture(partTexture(byPart("suit"), "rider", "suit"));
  const gloves = useDataTexture(partTexture(byPart("gloves"), "gloves"));
  const hasBody = !!body?.nodes.length;
  const hasHelmet = !!helmet?.nodes.length;

  // Previewing a single gear item (no body): show just that piece.
  const solo = !hasBody
    ? [helmet, boots, protection].find((p) => p?.nodes.length)
    : undefined;

  // Gear anchors: fractions of the real body's bounds when present, else fixed stand-in positions.
  const b = hasBody ? partBounds(body!.nodes) : null;
  const cx = b ? (b.lo[0] + b.hi[0]) / 2 : 0;
  const cz = b ? (b.lo[2] + b.hi[2]) / 2 : 0;
  const h = b ? b.hi[1] - b.lo[1] : 1;
  const depth = b ? b.hi[2] - b.lo[2] : 1;
  // Half the leg gap, so each boot sits under its own leg (not bunched at the centre-line).
  const legX = b ? 0.265 * (b.hi[0] - b.lo[0]) : 0.13;
  // Helmet hangs its bottom edge low on the neck, nudged forward in Z (alignY="bottom").
  const helmetAnchor: [number, number, number] = b
    ? [cx, b.hi[1] - 0.11 * h, cz + 0.08 * depth]
    : [0, 1.62, 0];
  // Boots hang their top edge on the body's floor (alignY="top"), nudged forward in Z.
  const footY = b ? b.lo[1] + 0.08 * h : 0.2;
  const bootZ = b ? cz + 0.16 * depth : cz;
  // Where a protection's own origin sits on the body. Unlike a helmet or a boot, the whole
  // slot shares one mount: a chest protector, a neck brace, a chain and a bib are all
  // authored around the same point in the rider's own frame, so this anchor places every
  // one of them and their own geometry does the rest.
  //
  // Mid-chest, which the game's own two pieces locate between them: the chest protector
  // reaches 21 cm above the mount and 12 cm below, and the neck brace sits 11–25 cm above it,
  // i.e. around the base of the neck.
  const protAnchor: [number, number, number] = b ? [cx, b.lo[1] + 0.74 * h, cz] : [0, 1.16, 0.03];
  const bootTarget = hasBody ? 0.44 * h : 0.32;

  if (solo) return <RiderGearSolo part={solo} overrides={overrides} />;

  return (
    <group>
      {hasBody ? (
        <RiderBodyMesh part={body!} overrides={overrides} />
      ) : (
        <RiderBody suit={suit} gloves={gloves} showHead={!hasHelmet} />
      )}
      {hasHelmet && (
        <RiderGearMesh
          overrides={overrides}
          part={helmet!}
          anchor={helmetAnchor}
          target={hasBody ? 0.38 * h : 0.52}
          yaw={hasBody ? Math.PI : 0}
          pitch={hasBody ? HELMET_PITCH : 0}
          alignY={hasBody ? "bottom" : "center"}
        />
      )}
      {/* Native, not fitted: the protection slot spans a full chest protector and a thin
          necklace, and scaling each to one size inflated the chain to vest proportions and
          threw away the offset a piece like a chain or a hood hangs at deliberately. */}
      {!!protection?.nodes.length && (
        <RiderGearMesh
          overrides={overrides}
          part={protection!}
          anchor={protAnchor}
          yaw={PROT_YAW}
          fit="native"
        />
      )}
      {/* Two feet as separate nodes → split left/right; a single-node boot renders centred. */}
      {!!boots?.nodes.length &&
        (boots!.nodes.length === 2 ? (
          bootSides(boots!).map(({ node, side }, i) => (
            <RiderGearMesh
          overrides={overrides}
              key={i}
              part={{ ...boots!, nodes: [node] }}
              anchor={[cx + side * legX, footY, bootZ]}
              target={bootTarget}
              rot={BOOT_ROT}
              pitch={hasBody ? BOOT_PITCH : 0}
              yaw={hasBody ? side * BOOT_SPLAY : 0}
              alignY={hasBody ? "top" : "center"}
            />
          ))
        ) : (
          <RiderGearMesh
          overrides={overrides}
            part={boots!}
            anchor={[cx, footY, bootZ]}
            target={bootTarget}
            rot={BOOT_ROT}
            pitch={hasBody ? BOOT_PITCH : 0}
            alignY={hasBody ? "top" : "center"}
          />
        ))}
    </group>
  );
}

function makeEdfMaterial(t: THREE.Texture | null) {
  return new THREE.MeshStandardMaterial({
    map: t ?? undefined,
    color: t ? 0xffffff : 0xb7bcc4,
    metalness: 0.2,
    roughness: 0.55,
    // Cut the mask out where a sheet carries one (see `hasMaskedAlpha`) — a brake disc and a
    // sprocket are a masked square on a flat quad, and the square is what shows otherwise.
    // Tested rather than blended: the mask is hard-edged, and `transparent` would drag the
    // quad into the sorted pass and let it disappear behind the wheel it sits on.
    alphaTest: t?.userData.maskedAlpha ? 0.5 : 0,
    // Inconsistent winding — render both sides so the bike isn't see-through.
    side: THREE.DoubleSide,
  });
}

function useEdfMeshes(
  nodes: EdfNode[] | null | undefined,
  tex: Map<string, THREE.Texture>,
) {
  const list = useMemo(() => nodes ?? [], [nodes]);
  const geoms = useNodeGeometries(list);

  // Only the materials know about the paint. Building them alongside the geometry meant
  // every livery switch tore down and rebuilt the bike's vertex buffers to change a map.
  const materials = useMemo(
    () =>
      list.map((n) =>
        n.submeshes.length
          ? n.submeshes.map((sm) => makeEdfMaterial(submeshTexture(sm.texture, tex)))
          : // No submesh table → whole-node binding (the model's primary body texture).
            [makeEdfMaterial(submeshTexture(n.texture, tex))],
      ),
    [list, tex],
  );
  useEffect(
    () => () => materials.forEach((a) => a.forEach((m) => m.dispose())),
    [materials],
  );

  return { geoms, materials };
}

/**
 * The piece of bodywork the pointer is over in the 2D editor, lit up on the bike.
 *
 * Triangles rather than a whole mesh group: a bike's bodywork is regularly one group with
 * both flanks in it, and highlighting the group would answer "which side is this" with the
 * whole bike. `tris` names the triangles of one uv island — node index, then triangle index.
 *
 * Drawn as its own geometry over the model with the depth test still on, so it is occluded by
 * the parts in front of it: a panel glowing *through* the bike would read as being on the near
 * side whichever flank it is actually on, which is the confusion this exists to settle.
 */
function HighlightMesh({
  nodes,
  tris,
  groups,
  want,
}: {
  nodes: EdfNode[];
  tris: Int32Array;
  /** Each node's pose group — see {@link poseGroup}. */
  groups: PoseGroup[];
  /** Draw only the triangles of nodes in this group, so the rest stay with their own part. */
  want: PoseGroup;
}) {
  const geom = useMemo(() => {
    const pos = new Float32Array((tris.length / 2) * 9);
    let o = 0;
    for (let i = 0; i < tris.length; i += 2) {
      const node = nodes[tris[i]];
      if (!node || groups[tris[i]] !== want) continue;
      const t = tris[i + 1];
      for (let c = 0; c < 3; c += 1) {
        const v = node.indices[t * 3 + c];
        pos[o] = node.positions[v * 3];
        pos[o + 1] = node.positions[v * 3 + 1];
        pos[o + 2] = node.positions[v * 3 + 2];
        o += 3;
      }
    }
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(pos.subarray(0, o), 3));
    g.computeBoundingSphere();
    return g;
  }, [nodes, tris, groups, want]);
  useEffect(() => () => geom.dispose(), [geom]);

  const mat = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: "#22d3ee",
        transparent: true,
        opacity: 0.55,
        side: THREE.DoubleSide,
        depthWrite: false,
        // Pulled towards the camera so it wins against the surface it is sitting on, which
        // is the same surface — without it the two z-fight and the highlight speckles.
        polygonOffset: true,
        polygonOffsetFactor: -4,
        polygonOffsetUnits: -4,
      }),
    [],
  );
  useEffect(() => () => mat.dispose(), [mat]);

  return <mesh geometry={geom} material={mat} renderOrder={2} />;
}

/**
 * How a bike is standing: the three joints its `.geom` gives, in the units the controls show.
 *
 * All zero is the bike exactly as the `.geom` assembled it — the frame it was *authored* in,
 * which is not a stance it ever holds on the ground. See {@link BikeRig}.
 */
export interface BikePose {
  /** How far the rear axle hangs below where it was authored, in mm. */
  rearDrop: number;
  /** How far the fork is pushed up its own axis, in mm. */
  forkUp: number;
  /** Steering angle, in degrees. */
  steer: number;
}

export const NEUTRAL_POSE: BikePose = { rearDrop: 0, forkUp: 0, steer: 0 };

/** Which part of the bike moves with which joint. */
type PoseGroup = "static" | "swing" | "steer" | "fork";

/**
 * Which pose group a node belongs to, read off the same name prefixes `assemble_bike` mounts
 * it by — so the two can only agree. `steer` is the triple clamps and bars; `fork` is the
 * sliding leg the wheel hangs off, which is why it moves inside the steering group.
 */
function poseGroup(name: string): PoseGroup {
  const n = name.toLowerCase();
  if (n.startsWith("rsusp") || n.startsWith("rwheel")) return "swing";
  if (n.startsWith("fsusp") || n.startsWith("fwheel")) return "fork";
  if (n.startsWith("steer")) return "steer";
  return "static";
}

const X_AXIS = new THREE.Vector3(1, 0, 0);

/** The fork/steering axis: straight up, tilted back by the rake. */
function forkAxis(rake: number): THREE.Vector3 {
  const r = rake * THREE.MathUtils.DEG2RAD;
  return new THREE.Vector3(0, Math.cos(r), Math.sin(r));
}

function wrapPi(a: number): number {
  return Math.atan2(Math.sin(a), Math.cos(a));
}

/**
 * The swingarm rotation that puts the rear axle at `targetY`, or null where it can't reach.
 *
 * The axle rides a circle about the pivot, so `y(θ) = pivotY + dy·cosθ − dz·sinθ` — a single
 * cosine of amplitude `hypot(dy, dz)`. Two rotations hit any height inside that; the one
 * nearest as-authored is the one meant, the other swings the wheel over the seat.
 */
function swingAngleForAxleY(rig: BikeRig, targetY: number): number | null {
  if (!rig.rearAxle) return null;
  const dy = rig.rearAxle[1] - rig.pivot[1];
  const dz = rig.rearAxle[2] - rig.pivot[2];
  const r = Math.hypot(dy, dz);
  const k = targetY - rig.pivot[1];
  if (r < 1e-6 || Math.abs(k) > r) return null;
  const phi = Math.atan2(dz, dy);
  const a = Math.acos(k / r);
  const both = [wrapPi(a - phi), wrapPi(-a - phi)];
  return both.reduce((best, x) => (Math.abs(x) < Math.abs(best) ? x : best));
}

/**
 * A wheel's radius, off the mesh rather than off a number we'd have to keep: the tyre is
 * authored about its own axle, so half its height is what it rolls on. Null when the bike
 * has no such wheel — a mod whose tyres aren't installed, or any bike before the wheels went
 * on at all.
 */
function wheelRadius(nodes: EdfNode[], prefix: string): number | null {
  let lo = Infinity;
  let hi = -Infinity;
  for (const n of nodes) {
    if (!n.name.toLowerCase().startsWith(prefix)) continue;
    for (let i = 1; i < n.positions.length; i += 3) {
      lo = Math.min(lo, n.positions[i]);
      hi = Math.max(hi, n.positions[i]);
    }
  }
  return hi > lo ? (hi - lo) / 2 : null;
}

/** As far as the rear may be dropped or squatted from as-authored, in mm. */
const REAR_LIMIT_MM = 140;

/**
 * The rear drop that stands the bike level — both tyres touching the same ground.
 *
 * Contact points, not axles: a 21" front and a 19" rear are not level when their axles are.
 * Null when there is nothing to solve against — no axles in the `.geom`, no wheel meshes, or
 * a bike whose swingarm can't reach that far — and the bike then stands as it was authored,
 * which is exactly how it stood before any of this.
 */
function levelRearDrop(
  rig: BikeRig | null | undefined,
  nodes: EdfNode[],
  forkUpM: number,
): number | null {
  if (!rig?.frontAxle || !rig.rearAxle) return null;
  const rf = wheelRadius(nodes, "fwheel");
  const rr = wheelRadius(nodes, "rwheel");
  if (rf == null || rr == null) return null;
  const frontY = rig.frontAxle[1] + forkAxis(rig.rake).y * forkUpM;
  const targetY = frontY - rf + rr;
  const angle = swingAngleForAxleY(rig, targetY);
  if (angle == null) return null;
  const drop = (rig.rearAxle[1] - targetY) * 1000;
  return Math.abs(drop) > REAR_LIMIT_MM ? null : drop;
}

/** The stance a bike is first drawn in: level if it can be solved, as-authored if not. */
function settledPose(rig: BikeRig | null | undefined, nodes: EdfNode[]): BikePose {
  if (!rig) return NEUTRAL_POSE;
  const drop = levelRearDrop(rig, nodes, 0);
  return drop == null ? NEUTRAL_POSE : { ...NEUTRAL_POSE, rearDrop: drop };
}

/** Turn a group about a point that isn't the origin. */
function About({
  at,
  q,
  children,
}: {
  at: Vec3;
  q: THREE.Quaternion;
  children: React.ReactNode;
}) {
  return (
    <group position={at}>
      <group quaternion={q}>
        <group position={[-at[0], -at[1], -at[2]]}>{children}</group>
      </group>
    </group>
  );
}

// MX Bikes meshes are authored Y-up, +Z forward (three.js' convention) — no rotation.
function EdfMesh({
  nodes,
  textures,
  highlight,
  rig,
  pose = NEUTRAL_POSE,
}: {
  nodes: EdfNode[];
  textures: Map<string, THREE.Texture>;
  highlight?: Int32Array | null;
  /** The joints to pose about. Absent (an unassembled bike, or gear) draws one rigid group. */
  rig?: BikeRig | null;
  pose?: BikePose;
}) {
  const { geoms, materials } = useEdfMeshes(nodes, textures);
  const groups = useMemo<PoseGroup[]>(
    () => nodes.map((n) => (rig ? poseGroup(n.name) : "static")),
    [nodes, rig],
  );

  const swingQ = useMemo(() => {
    const target = rig?.rearAxle ? rig.rearAxle[1] - pose.rearDrop / 1000 : null;
    const a = rig && target !== null ? swingAngleForAxleY(rig, target) : null;
    return new THREE.Quaternion().setFromAxisAngle(X_AXIS, a ?? 0);
  }, [rig, pose.rearDrop]);
  const axis = useMemo(() => forkAxis(rig?.rake ?? 0), [rig?.rake]);
  const steerQ = useMemo(
    () => new THREE.Quaternion().setFromAxisAngle(axis, pose.steer * THREE.MathUtils.DEG2RAD),
    [axis, pose.steer],
  );
  const forkAt = useMemo<Vec3>(() => {
    const v = axis.clone().multiplyScalar(pose.forkUp / 1000);
    return [v.x, v.y, v.z];
  }, [axis, pose.forkUp]);

  const part = (want: PoseGroup) => (
    <>
      {geoms.map((g, i) =>
        groups[i] === want ? (
          <mesh
            key={i}
            geometry={g}
            material={materials[i].length === 1 ? materials[i][0] : materials[i]}
            castShadow
            receiveShadow
          />
        ) : null,
      )}
      {!!highlight?.length && (
        <HighlightMesh nodes={nodes} tris={highlight} groups={groups} want={want} />
      )}
    </>
  );

  // No rig: every node is "static", so this is the one flat group it has always been.
  if (!rig) return <group>{part("static")}</group>;
  return (
    <group>
      {part("static")}
      <About at={rig.pivot} q={swingQ}>
        {part("swing")}
      </About>
      <About at={rig.steerHead} q={steerQ}>
        {part("steer")}
        {/* Inside the steering group: the fork slides along the axis the bars turn about. */}
        <group position={forkAt}>{part("fork")}</group>
      </About>
    </group>
  );
}

/** Clear air between the bike and the rider, in metres. */
const PAIR_GAP = 0.35;

/**
 * The rider composite's own extent.
 *
 * The body, when there is one: every other piece is anchored and scaled off the body's
 * bounds in `RiderComposite`, so their own boxes describe where they were authored rather
 * than where they end up. With no body it's a single gear item, and its box is all there is.
 */
function riderBounds(parts: RiderPart[]) {
  const body = parts.find((p) => p.part === "body" && p.nodes.length);
  if (body) return partBounds(body.nodes);
  const drawn = parts.filter((p) => p.nodes.length).flatMap((p) => p.nodes);
  return drawn.length ? partBounds(drawn) : { lo: [0, 0, 0], hi: [0, 0, 0] };
}

/**
 * Bike and rider in one scene, standing beside each other.
 *
 * Neither is scaled. Both meshes come out of the game in one frame — Y-up, metres — so the
 * bike really is that much longer than the rider is tall, and the pair reads as a garage
 * shot instead of two models fitted to the same box. All this adds is the offsets: shoulder
 * to shoulder along X with `PAIR_GAP` between their bounding boxes, and each dropped onto
 * y=0 so they share the ground `ContactShadows` is drawn against.
 *
 * The pair is centred by the `<Center>` above it, exactly as a lone model is.
 */
function SideBySide({
  nodes,
  textures,
  highlight,
  parts,
  overrides,
  rig,
  pose,
}: {
  nodes: EdfNode[];
  textures: Map<string, THREE.Texture>;
  highlight?: Int32Array | null;
  parts: RiderPart[];
  overrides?: Map<string, THREE.Texture>;
  rig?: BikeRig | null;
  pose?: BikePose;
}) {
  const at = useMemo(() => {
    const bike = partBounds(nodes);
    const rider = riderBounds(parts);
    return {
      // Bike's right edge lands on -GAP/2, rider's left edge on +GAP/2, so whatever their
      // sizes they never intersect.
      bike: [-PAIR_GAP / 2 - bike.hi[0], -bike.lo[1], 0] as [number, number, number],
      rider: [PAIR_GAP / 2 - rider.lo[0], -rider.lo[1], 0] as [number, number, number],
    };
  }, [nodes, parts]);

  return (
    <group>
      <group position={at.bike}>
        <EdfMesh
          nodes={nodes}
          textures={textures}
          highlight={highlight}
          rig={rig}
          pose={pose}
        />
      </group>
      <group position={at.rider}>
        <RiderComposite parts={parts} overrides={overrides} />
      </group>
    </group>
  );
}

/** How much of the scene the camera has to take in. */
type Framing = "default" | "solo" | "pair";

// Default camera looks down over a bike/rider; a solo gear item gets a level, closer view,
// and a bike-plus-rider pair is roughly twice as wide, so it starts further back.
function CameraRig({ frame }: { frame: Framing }) {
  const camera = useThree((s) => s.camera);
  // OrbitControls (`makeDefault`) owns the camera each frame, so moves must go through
  // the controls and be committed with `update()`.
  const controls = useThree((s) => s.controls) as
    | { target: THREE.Vector3; update: () => void }
    | null;
  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => {
    const [x, y, z] =
      frame === "solo"
        ? [1.25, 0.35, 1.7]
        : frame === "pair"
          ? [3.9, 2.2, 4.8]
          : [2.6, 1.8, 3.2];
    camera.position.set(x, y, z);
    camera.updateProjectionMatrix();
    if (controls) {
      controls.target.set(0, 0, 0);
      controls.update();
    } else {
      camera.lookAt(0, 0, 0);
    }
    // Moving the camera by hand changes no React state, so under `frameloop="demand"`
    // nothing would repaint and the reframe wouldn't show until the next interaction.
    invalidate();
  }, [frame, camera, controls, invalidate]);
  return null;
}

// Legend for the OrbitControls gestures below — the canvas gives no other clue
// that it's draggable. Kept muted so it reads as chrome, never competing with the model.
/**
 * The pose panel: the bike's own joints, on sliders.
 *
 * Sliders in millimetres of wheel movement rather than in joint angles — how far the rear
 * wheel hangs is a thing anyone can see, where "8.4° of swingarm" is a thing only the maths
 * knows. {@link swingAngleForAxleY} turns the one into the other.
 */
function PoseControls({
  pose,
  onChange,
  onLevel,
  onReset,
}: {
  pose: BikePose;
  onChange: (p: BikePose) => void;
  /** Absent when the bike gives nothing to level against — no axles, or no wheels on it. */
  onLevel?: () => void;
  onReset: () => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const mm = (v: number) => `${Math.round(v)}mm`;
  return (
    <div className="absolute bottom-2 right-2 w-[230px] rounded-md border border-white/10 bg-black/60 text-white/80 backdrop-blur-sm">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center gap-1.5 px-2 py-1.5 text-[11px] font-medium leading-none text-white/70 hover:text-white"
      >
        <SlidersHorizontal className="h-3.5 w-3.5" />
        {t("viewer.pose")}
        <ChevronDown
          className={cn("ml-auto h-3.5 w-3.5 transition-transform", open && "rotate-180")}
        />
      </button>
      {open && (
        <div className="flex flex-col gap-1.5 border-t border-white/10 px-2 py-2">
          <Row label={t("viewer.poseRear")}>
            <Slider
              value={pose.rearDrop}
              min={-REAR_LIMIT_MM}
              max={REAR_LIMIT_MM}
              step={1}
              onChange={(v) => onChange({ ...pose, rearDrop: v })}
              format={mm}
            />
          </Row>
          <Row label={t("viewer.poseFront")}>
            <Slider
              value={pose.forkUp}
              min={-60}
              max={180}
              step={1}
              onChange={(v) => onChange({ ...pose, forkUp: v })}
              format={mm}
            />
          </Row>
          <Row label={t("viewer.poseSteer")}>
            <Slider
              value={pose.steer}
              min={-40}
              max={40}
              step={1}
              onChange={(v) => onChange({ ...pose, steer: v })}
              format={(v) => `${Math.round(v)}°`}
            />
          </Row>
          <div className="mt-0.5 flex items-center gap-1.5">
            {onLevel && (
              <button
                type="button"
                onClick={onLevel}
                className="rounded border border-white/15 px-2 py-1 text-[11px] leading-none text-white/75 hover:bg-white/10 hover:text-white"
              >
                {t("viewer.poseLevel")}
              </button>
            )}
            <button
              type="button"
              onClick={onReset}
              className="rounded border border-white/15 px-2 py-1 text-[11px] leading-none text-white/75 hover:bg-white/10 hover:text-white"
            >
              {t("viewer.poseReset")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

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

/** Asks for one frame whenever `token` changes. Nothing else in the scene re-renders. */
function FrameOnChange({ token }: { token?: number }) {
  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => {
    invalidate();
  }, [token, invalidate]);
  return null;
}

export interface ModelViewerProps {
  mode: ViewerMode;
  texture?: PaintTexture | null;
  textures?: PaintTexture[];
  /**
   * Textures that win over `textures`, keyed by lowercase texture name.
   *
   * For a caller holding pixels the backend has never seen — the Designer's live composite,
   * which is a canvas in this webview. Everything else arrives by token and goes through
   * `textures`; this is the door for the one case that can't.
   *
   * The caller owns these and must dispose them: unlike `useTextureMap`'s, their lifetime is
   * the editor's, not this component's.
   */
  overrides?: Map<string, THREE.Texture>;
  /**
   * Draw a frame when this changes.
   *
   * `frameloop="demand"` only redraws on a React commit, and a caller mutating a texture in
   * place — the Designer, repainting its canvas — deliberately doesn't cause one: handing the
   * material map a new identity per frame is what made dragging crawl. This is the way to say
   * "the pixels moved" without saying "rebuild everything".
   */
  frameToken?: number;
  nodes?: EdfNode[] | null;
  /**
   * Triangles to light up on the model — node index and triangle index, two per triangle.
   *
   * The Designer's answer to "which panel is this region of the sheet": it hands over what
   * the pointer is on and the model shows where that lands, instead of naming a flank and
   * leaving the reader to work out whose left it meant.
   */
  highlight?: Int32Array | null;
  riderParts?: RiderPart[] | null;
  /**
   * The bike's joints, from the model that carried `nodes`.
   *
   * Absent leaves the bike rigid, exactly as it was drawn before it could be posed — which is
   * also what an unassembled bike gets, since there is nothing to pose a pile of loose parts.
   */
  rig?: BikeRig | null;
  /** Offer the pose panel. Off by default: a preview nobody is posing shouldn't grow chrome. */
  poseControls?: boolean;
  loading?: boolean;
  noStandIn?: boolean;
  className?: string;
}

export function ModelViewer({
  mode,
  texture,
  textures = NO_PAINT_TEXTURES,
  overrides,
  frameToken,
  nodes,
  highlight,
  riderParts,
  rig,
  poseControls = false,
  loading = false,
  noStandIn = false,
  className,
}: ModelViewerProps) {
  const map = useDataTexture(texture);
  const texMap = useTextureMapWith(textures, overrides);
  // The stance the bike is drawn in until someone moves a slider. Held as "no answer yet"
  // rather than as a copy of `settled`, so a new bike settles on its own instead of inheriting
  // the pose the last one was left in.
  const settled = useMemo(() => settledPose(rig, nodes ?? []), [rig, nodes]);
  const [posed, setPosed] = useState<BikePose | null>(null);
  useEffect(() => setPosed(null), [settled]);
  const pose = posed ?? settled;
  // Solved against the fork where it is now, so "level" still means level after the front
  // has been moved.
  const level = useMemo(
    () => levelRearDrop(rig, nodes ?? [], pose.forkUp / 1000),
    [rig, nodes, pose.forkUp],
  );
  // What `mode` asks for, narrowed to what actually arrived. In `both`, either half can be
  // missing — a bike that wouldn't resolve, a rider still loading — and the scene falls back
  // to whichever one is here rather than to a stand-in.
  const showBike = mode !== "rider" && !!nodes?.length;
  const showRider = mode !== "bike" && !!riderParts?.length;
  const pair = showBike && showRider;
  const hasReal = showBike && !showRider;
  const hasRider = showRider && !showBike;
  // A single gear item (no body) is a small centred object — frame it level.
  const gearSolo =
    hasRider && !riderParts!.some((p) => p.part === "body" && p.nodes.length);
  const frame: Framing = pair ? "pair" : gearSolo ? "solo" : "default";
  return (
    <div className={cn("relative", className)}>
      <ErrorBoundary compact label="model-viewer">
        <Canvas
          className="h-full w-full"
          shadows
          // Nothing in this scene animates, so drawing 60 shadowed frames a second at a
          // parked model was pure burn — and the Rider Studio panel is mounted the whole
          // time that page is open. On demand, a frame is drawn when React commits, when
          // OrbitControls moves, and when `CameraRig` reframes; otherwise the GPU idles.
          frameloop="demand"
          // 2× on a retina panel quadruples the pixels for a preview-sized model.
          dpr={[1, 1.5]}
          camera={{ position: [2.6, 1.8, 3.2], fov: 42 }}
          onCreated={({ gl, invalidate }) => {
            // A lost GPU context otherwise leaves a black canvas; preventDefault lets the browser restore it.
            gl.domElement.addEventListener(
              "webglcontextlost",
              (e) => {
                e.preventDefault();
                console.warn("[ModelViewer] WebGL context lost — awaiting restore");
              },
              false,
            );
            // Restoring doesn't touch React state, so on demand nothing would redraw and
            // the canvas would stay black until the next interaction.
            gl.domElement.addEventListener("webglcontextrestored", () => invalidate(), false);
          }}
        >
          <color attach="background" args={["#0e0f13"]} />
          <FrameOnChange token={frameToken} />
          <CameraRig frame={frame} />
          <ambientLight intensity={0.75} />
          {/* Even sky/ground fill so matte paint reads its true colour. */}
          <hemisphereLight args={[0xffffff, 0x555a66, 0.7]} />
          <directionalLight
            position={[4, 6, 3]}
            intensity={1.25}
            castShadow
            shadow-mapSize={[1024, 1024]}
          />
          <directionalLight position={[-4, 2, -3]} intensity={0.55} />
          {/* Front fill from the camera side so the front of the kit isn't in shadow. */}
          <directionalLight position={[0, 1.5, 5]} intensity={0.5} />
          <Center>
            {pair ? (
              <SideBySide
                nodes={nodes!}
                textures={texMap}
                highlight={highlight}
                parts={riderParts!}
                overrides={overrides}
                rig={rig}
                pose={pose}
              />
            ) : hasReal ? (
              <EdfMesh
                nodes={nodes!}
                textures={texMap}
                highlight={highlight}
                rig={rig}
                pose={pose}
              />
            ) : hasRider ? (
              <RiderComposite parts={riderParts!} overrides={overrides} />
            ) : loading || noStandIn ? null : mode === "bike" ? (
              <BikeStandIn map={map} />
            ) : (
              <RiderBody suit={map} gloves={null} showHead />
            )}
          </Center>
          <ContactShadows
            position={[0, -0.01, 0]}
            opacity={0.5}
            scale={8}
            blur={2.4}
            far={4}
          />
          <OrbitControls
            makeDefault
            enablePan
            screenSpacePanning
            zoomToCursor
            panSpeed={0.9}
            minDistance={0.4}
            maxDistance={20}
            target={[0, 0, 0]}
          />
        </Canvas>
      </ErrorBoundary>
      {!loading && <ControlsHint />}
      {poseControls && showBike && !!rig && !loading && (
        <PoseControls
          pose={pose}
          onChange={setPosed}
          onLevel={
            level === null ? undefined : () => setPosed({ ...pose, rearDrop: level })
          }
          onReset={() => setPosed(NEUTRAL_POSE)}
        />
      )}
    </div>
  );
}
