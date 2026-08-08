import { useEffect, useMemo, useRef, useState } from "react";
import { Canvas, useThree } from "@react-three/fiber";
import { OrbitControls, Center, ContactShadows } from "@react-three/drei";
import { Move, Rotate3d, ZoomIn } from "lucide-react";
import * as THREE from "three";
import { cn } from "@/lib/utils";
import type { EdfNode, PaintTexture, RiderPart } from "../../types";
import { textureBytes } from "../../api/mods";
import { ErrorBoundary } from "../ErrorBoundary";
import { useT } from "../../i18n/context";

export type ViewerMode = "bike" | "rider";

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
  const tex = new THREE.DataTexture(
    new Uint8Array(buf),
    t.width,
    t.height,
    THREE.RGBAFormat,
  );
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

function useTextureMap(textures: PaintTexture[]): Map<string, THREE.Texture> {
  const [map, setMap] = useState<Map<string, THREE.Texture>>(new Map());
  useEffect(() => {
    if (!textures.length) {
      setMap(new Map());
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
  }, [textures]);
  return map;
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

// Protection is authored Y-up (the rider body's own frame), so no roll.
const PROT_ROT: [number, number, number] = [0, 0, 0];

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

function useGearMaterials(part: RiderPart, tex: Map<string, THREE.Texture>) {
  const mats = useMemo(() => {
    return part.nodes.map((n) =>
      n.submeshes.length
        ? n.submeshes.map((sm) => makeGearMaterial(sm.texture, tex))
        : [makeGearMaterial(n.texture, tex)],
    );
  }, [part, tex]);
  useEffect(
    () => () => mats.forEach((a) => a.forEach((m) => m.dispose())),
    [mats],
  );
  return mats;
}

function RiderGearMesh({
  part,
  anchor,
  target,
  rot = GEAR_ROT,
  yaw = 0,
  alignY = "center",
  pitch = 0,
}: {
  part: RiderPart;
  anchor: [number, number, number];
  target: number;
  rot?: [number, number, number];
  yaw?: number;
  alignY?: "center" | "top" | "bottom";
  pitch?: number;
}) {
  const texMap = useTextureMap(part.textures);
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
  const s = fit.scale;
  // Shift so the requested bbox edge (not just the centre) lands on anchor[1].
  const alignShift =
    alignY === "bottom" ? fit.halfY * s : alignY === "top" ? -fit.halfY * s : 0;
  return (
    <group
      position={[
        anchor[0] - fit.center.x * s,
        anchor[1] - fit.center.y * s + alignShift,
        anchor[2] - fit.center.z * s,
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

// Boots ship both feet as separate nodes coincident at the ankle; split by their
// native left-right (Y) centre, one to each side.
function bootSides(part: RiderPart): { node: EdfNode; side: number }[] {
  const withY = part.nodes.map((node) => {
    let lo = Infinity;
    let hi = -Infinity;
    for (let i = 1; i < node.positions.length; i += 3) {
      const v = node.positions[i];
      if (v < lo) lo = v;
      if (v > hi) hi = v;
    }
    return { node, y: (lo + hi) / 2 };
  });
  withY.sort((a, b) => a.y - b.y);
  return withY.map((w, i) => ({ node: w.node, side: i === 0 ? -1 : 1 }));
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

function RiderBodyMesh({ part }: { part: RiderPart }) {
  const tex = useTextureMap(part.textures);
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

function RiderGearSolo({ part }: { part: RiderPart }) {
  const tex = useTextureMap(part.textures);
  const geoms = useNodeGeometries(part.nodes);
  const mats = useGearMaterials(part, tex);
  const rot =
    part.part === "boots" ? BOOT_ROT : part.part === "protection" ? PROT_ROT : GEAR_ROT;
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
      // Place each foot on its correct side (camera looks from +X).
      const firstIsPlusX =
        boxes[0].min.x + boxes[0].max.x >= boxes[1].min.x + boxes[1].max.x;
      offsets[0] = (firstIsPlusX ? -1 : 1) * w * 0.55;
      offsets[1] = (firstIsPlusX ? 1 : -1) * w * 0.55;
    }
    // Straighten each foot so its toe points forward instead of splaying in.
    const yaws = geoms.map((g) => (pair ? straightenYaw(g, rotM) : 0));
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
  }, [geoms, rot, part.part]);
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

function RiderComposite({ parts }: { parts: RiderPart[] }) {
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
  const protAnchor: [number, number, number] = b ? [cx, b.lo[1] + 0.62 * h, cz] : [0, 1.16, 0.03];
  const bootTarget = hasBody ? 0.44 * h : 0.32;

  if (solo) return <RiderGearSolo part={solo} />;

  return (
    <group>
      {hasBody ? (
        <RiderBodyMesh part={body!} />
      ) : (
        <RiderBody suit={suit} gloves={gloves} showHead={!hasHelmet} />
      )}
      {hasHelmet && (
        <RiderGearMesh
          part={helmet!}
          anchor={helmetAnchor}
          target={hasBody ? 0.38 * h : 0.52}
          yaw={hasBody ? Math.PI : 0}
          pitch={hasBody ? HELMET_PITCH : 0}
          alignY={hasBody ? "bottom" : "center"}
        />
      )}
      {!!protection?.nodes.length && (
        <RiderGearMesh part={protection!} anchor={protAnchor} target={hasBody ? 0.42 * h : 0.62} rot={PROT_ROT} />
      )}
      {/* Two feet as separate nodes → split left/right; a single-node boot renders centred. */}
      {!!boots?.nodes.length &&
        (boots!.nodes.length === 2 ? (
          bootSides(boots!).map(({ node, side }, i) => (
            <RiderGearMesh
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

// MX Bikes meshes are authored Y-up, +Z forward (three.js' convention) — no rotation.
function EdfMesh({
  nodes,
  textures,
}: {
  nodes: EdfNode[];
  textures: Map<string, THREE.Texture>;
}) {
  const { geoms, materials } = useEdfMeshes(nodes, textures);
  return (
    <group>
      {geoms.map((g, i) => (
        <mesh
          key={i}
          geometry={g}
          material={materials[i].length === 1 ? materials[i][0] : materials[i]}
          castShadow
          receiveShadow
        />
      ))}
    </group>
  );
}

// Default camera looks down over a bike/rider; a solo gear item gets a level, closer view.
function CameraRig({ solo }: { solo: boolean }) {
  const camera = useThree((s) => s.camera);
  // OrbitControls (`makeDefault`) owns the camera each frame, so moves must go through
  // the controls and be committed with `update()`.
  const controls = useThree((s) => s.controls) as
    | { target: THREE.Vector3; update: () => void }
    | null;
  const invalidate = useThree((s) => s.invalidate);
  useEffect(() => {
    const [x, y, z] = solo ? [1.25, 0.35, 1.7] : [2.6, 1.8, 3.2];
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
  }, [solo, camera, controls, invalidate]);
  return null;
}

// Legend for the OrbitControls gestures below — the canvas gives no other clue
// that it's draggable. Kept muted so it reads as chrome, never competing with the model.
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

export interface ModelViewerProps {
  mode: ViewerMode;
  texture?: PaintTexture | null;
  textures?: PaintTexture[];
  nodes?: EdfNode[] | null;
  riderParts?: RiderPart[] | null;
  loading?: boolean;
  noStandIn?: boolean;
  className?: string;
}

export function ModelViewer({
  mode,
  texture,
  textures = [],
  nodes,
  riderParts,
  loading = false,
  noStandIn = false,
  className,
}: ModelViewerProps) {
  const map = useDataTexture(texture);
  const texMap = useTextureMap(textures);
  const hasReal = mode === "bike" && !!nodes?.length;
  const hasRider = mode === "rider" && !!riderParts?.length;
  // A single gear item (no body) is a small centred object — frame it level.
  const gearSolo =
    hasRider && !riderParts!.some((p) => p.part === "body" && p.nodes.length);
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
          <CameraRig solo={gearSolo} />
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
            {hasReal ? (
              <EdfMesh nodes={nodes!} textures={texMap} />
            ) : hasRider ? (
              <RiderComposite parts={riderParts!} />
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
    </div>
  );
}
