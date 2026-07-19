import { useEffect, useMemo, useRef, useState } from "react";
import { Canvas, useThree } from "@react-three/fiber";
import { OrbitControls, Center, ContactShadows } from "@react-three/drei";
import * as THREE from "three";
import type { EdfNode, PaintTexture, RiderPart } from "../../types";
import { ErrorBoundary } from "../ErrorBoundary";

export type ViewerMode = "bike" | "rider";

/** Load a set of paint/bike textures into three.js textures keyed by lowercase
 * name (e.g. `livery`, `bike_parts`, `chain`). Disposed on change/unmount. */
function useTextureMap(textures: PaintTexture[]): Map<string, THREE.Texture> {
  const [map, setMap] = useState<Map<string, THREE.Texture>>(new Map());
  useEffect(() => {
    if (!textures.length) {
      setMap(new Map());
      return;
    }
    let alive = true;
    const loaded = new Map<string, THREE.Texture>();
    let pending = textures.length;
    const loader = new THREE.TextureLoader();
    textures.forEach((t) =>
      loader.load(t.png, (tex) => {
        tex.colorSpace = THREE.SRGBColorSpace;
        // MX Bikes is a DirectX game: its paints use a TOP-left UV origin, so the
        // three.js default (flipY = bottom-left) mirrors them vertically — which
        // made the torso sample the pants region and vice versa.
        tex.flipY = false;
        // UV **wrapping**, not clamping. Two real cases need it: groups whose island
        // runs slightly outside 0–1 (the number plates reach u -0.60…1.15), and the
        // Honda exhaust, authored wholly on tile 1 (u 1.001…2.000) so that it
        // selects a second texture — sampling it needs u-1, which is what repeat
        // gives for free. Clamping instead smears the texture's edge pixels.
        tex.wrapS = THREE.RepeatWrapping;
        tex.wrapT = THREE.RepeatWrapping;
        tex.anisotropy = 4;
        loaded.set(t.name.toLowerCase(), tex);
        pending -= 1;
        if (pending === 0 && alive) setMap(new Map(loaded));
      }),
    );
    return () => {
      alive = false;
      loaded.forEach((tex) => tex.dispose());
    };
  }, [textures]);
  return map;
}

/**
 * The texture a bike submesh binds to. Rust resolved this from the bike's OWN
 * files — `gfx.cfg`'s `texture = …` overrides, the model's packed texture names,
 * and the group's UV tile — so `sm.texture` is a real texture name and all this
 * has to do is look it up.
 *
 * There is deliberately **no** name-based fallback chain here. The old one guessed
 * from the mesh-group name (`chain`→chain, `plate`→w_plate, everything
 * else→`plastics`), which cannot work: the Honda CRF450R's entire body — frame,
 * engine, fenders, exhaust — is one group named `Frame`. Worse, forcing every
 * group to the paint's `plastics` smears a paint's map over parts it was never
 * drawn for, which is exactly the mis-placed-livery bug this replaced.
 *
 * `null` → render the group in a neutral colour rather than an arbitrary texture.
 */
function submeshTexture(
  texture: string | null | undefined,
  tex: Map<string, THREE.Texture>,
): THREE.Texture | null {
  return (texture && tex.get(texture.toLowerCase())) || null;
}

/**
 * Load a `data:` URI (from `unpackPaint`) into a three.js texture, disposing the
 * previous one on change/unmount. Returns `null` until ready / when no URI.
 */
function useDataTexture(uri: string | null | undefined): THREE.Texture | null {
  const [tex, setTex] = useState<THREE.Texture | null>(null);
  const current = useRef<THREE.Texture | null>(null);

  useEffect(() => {
    if (!uri) {
      current.current?.dispose();
      current.current = null;
      setTex(null);
      return;
    }
    let disposed = false;
    new THREE.TextureLoader().load(uri, (t) => {
      if (disposed) {
        t.dispose();
        return;
      }
      t.colorSpace = THREE.SRGBColorSpace;
      t.flipY = false; // top-left UV origin — see `useTextureMap`.
      t.anisotropy = 4;
      current.current?.dispose();
      current.current = t;
      setTex(t);
    });
    return () => {
      disposed = true;
    };
  }, [uri]);

  useEffect(
    () => () => {
      current.current?.dispose();
      current.current = null;
    },
    [],
  );

  return tex;
}

/** Shared material: the paint texture when present, else a neutral base colour. */
function bodyMaterial(map: THREE.Texture | null, color: string) {
  return map ? (
    <meshStandardMaterial map={map} metalness={0.15} roughness={0.55} />
  ) : (
    <meshStandardMaterial color={color} metalness={0.2} roughness={0.5} />
  );
}

/**
 * Placeholder bike stand-in built from primitives. Swapped for the real
 * decoded `model.edf` geometry once the `.edf` loader lands; the paint texture
 * wiring is already in place so that drop-in changes nothing here.
 */
function BikeStandIn({ map }: { map: THREE.Texture | null }) {
  return (
    <group rotation={[0, Math.PI / 6, 0]}>
      {/* wheels */}
      {[-0.9, 0.9].map((x) => (
        <mesh key={x} position={[x, 0.45, 0]} rotation={[Math.PI / 2, 0, 0]}>
          <torusGeometry args={[0.45, 0.14, 16, 40]} />
          <meshStandardMaterial color="#1a1a1a" roughness={0.8} />
        </mesh>
      ))}
      {/* frame / tank / seat (takes the livery) */}
      <mesh position={[0, 0.9, 0]}>
        <boxGeometry args={[1.5, 0.42, 0.34]} />
        {bodyMaterial(map, "#c0392b")}
      </mesh>
      <mesh position={[-0.45, 1.18, 0]} rotation={[0, 0, 0.25]}>
        <boxGeometry args={[0.7, 0.26, 0.3]} />
        {bodyMaterial(map, "#c0392b")}
      </mesh>
      {/* fork */}
      <mesh position={[0.85, 0.85, 0]} rotation={[0, 0, -0.35]}>
        <cylinderGeometry args={[0.05, 0.05, 0.9, 12]} />
        <meshStandardMaterial color="#888" metalness={0.7} roughness={0.3} />
      </mesh>
      {/* handlebars */}
      <mesh position={[1.0, 1.3, 0]} rotation={[Math.PI / 2, 0, 0]}>
        <cylinderGeometry args={[0.03, 0.03, 0.5, 10]} />
        <meshStandardMaterial color="#333" />
      </mesh>
    </group>
  );
}

/**
 * Rider **body** stand-in built from primitives. Only the body/suit + gloved
 * hands are placeholders — real helmet/boots/protection meshes (when installed)
 * are seated on top of it by {@link RiderComposite}. `showHead` hides the helmet
 * sphere when a real helmet mesh takes its place. The official rider-template mesh
 * drops in here later without touching the gear-seating logic.
 */
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
      {/* torso (suit) */}
      <mesh position={[0, 1.15, 0]}>
        <capsuleGeometry args={[0.22, 0.45, 8, 16]} />
        {bodyMaterial(suit, "#2c3e50")}
      </mesh>
      {/* arms + gloved hands */}
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
      {/* legs */}
      {[-0.13, 0.13].map((x) => (
        <mesh key={x} position={[x, 0.5, 0]}>
          <capsuleGeometry args={[0.1, 0.6, 6, 12]} />
          <meshStandardMaterial color="#34495e" roughness={0.6} />
        </mesh>
      ))}
    </group>
  );
}

/** First matching texture (by internal name) as a `data:` URI, else the first. */
function partPng(part: RiderPart | undefined, ...names: string[]): string | null {
  if (!part?.textures.length) return null;
  const hit = part.textures.find((t) => names.includes(t.name.toLowerCase()));
  return (hit ?? part.textures[0]).png;
}

/** Helmet/protection are authored **X-up**: a helmet's bbox runs X[-0.04, 0.21] —
 * it extends *up* from an origin at the neck, with Y = left-right and Z = front-back
 * (verified by rendering it down each axis; only X-vertical is an upright helmet). It
 * needs a 90° roll about **Z** to reach three.js' Y-up.
 *
 * The decoder runs `to_right_handed` on gear (negating X) so the artwork isn't
 * mirrored — that flips the up-axis to **−X**, so the roll is **−90°** (a +90° roll
 * would send −X to −Y and stand the helmet on its head). The rider body is Y-up
 * already. Boots don't share this up-axis — see {@link BOOT_ROT}. */
const GEAR_ROT: [number, number, number] = [0, 0, -Math.PI / 2];

/** Boots share the gear frame but their worn-up axis is the **opposite** of the
 * helmet's. After `to_right_handed` negates X, a boot's leg-opening sits at ≈-0.07
 * and its sole at ≈-0.50 (verified from the real Fox Instinct mesh), so "up" points
 * toward **+X** — the reverse of the helmet, whose crown is at -X. Hence boots take
 * the +90° roll (+X→+Y) while the helmet takes {@link GEAR_ROT}'s -90°. A boots
 * `.edf` also ships both feet as separate nodes (`boot_l`/`boot_r`) authored
 * coincident at the ankle, split left/right by {@link bootSides}. */
const BOOT_ROT: [number, number, number] = [0, 0, Math.PI / 2];

/** Protection (chest/neck armour) is authored **Y-up**, in the rider body's own
 * frame — not X-up like the helmet — so it needs **no** roll. Reusing the helmet's
 * {@link GEAR_ROT} tips it onto its side (a 90° roll), which is wrong. */
const PROT_ROT: [number, number, number] = [0, 0, 0];

/** A small downward nod so the helmet gazes ahead / slightly down rather than
 * skyward (the raw fit leaves the visor tipped up). */
const HELMET_PITCH = 0.25;

/** Tip the boots' toes forward into a riding stance instead of hanging straight
 * down off the ankle. */
const BOOT_PITCH = 0.2;

/** Splay each boot outward (toes out) about world Y, so the pair reads as a natural
 * stance rather than two parallel boots. Applied per side (±) on the full body only;
 * the solo/single preview stays straight. */
const BOOT_SPLAY = 0.48;

/** One gear material in the shared "product-preview" look (paint lifted with its
 * own colour as self-illumination so it reads true against the dark background). */
function makeGearMaterial(
  base: string | null | undefined,
  tex: Map<string, THREE.Texture>,
  fallbackFirst: THREE.Texture | null,
) {
  const map = submeshTexture(base, tex) ?? fallbackFirst;
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

/** Per-submesh materials for a gear piece, one array per node (matching the node's
 * geometry groups). A helmet's `goggles` submesh binds a different texture than its
 * shell — Rust resolved each submesh's `texture` from the helmet + goggle paints — so
 * they can't share one material. Nodes with no submesh table take a single material
 * from whatever texture the paint carries. */
function useGearMaterials(part: RiderPart, tex: Map<string, THREE.Texture>) {
  const mats = useMemo(() => {
    const first = (tex.values().next().value as THREE.Texture | undefined) ?? null;
    return part.nodes.map((n) =>
      n.submeshes.length
        ? n.submeshes.map((sm) => makeGearMaterial(sm.texture, tex, first))
        : [makeGearMaterial(n.texture ?? part.part, tex, first)],
    );
  }, [part, tex]);
  useEffect(
    () => () => mats.forEach((a) => a.forEach((m) => m.dispose())),
    [mats],
  );
  return mats;
}

/**
 * A real gear mesh (helmet/boots/protection) decoded from `.edf`, textured per
 * submesh, then **fitted** onto the body: uniformly scaled so its largest
 * dimension is `target` and translated so its centre lands on `anchor` (body
 * coords). The game rigs gear to skeleton bones (`helmetlinkobj = riderRIG_Head`);
 * we don't parse the rig, so this bbox fit is the stand-in for that — good enough
 * for a preview, and purely geometry-derived so it's stable.
 */
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
  /** Up-axis correction. Helmet/protection use {@link GEAR_ROT}; boots, whose
   * worn-up is the opposite axis, use {@link BOOT_ROT}. */
  rot?: [number, number, number];
  /** Facing correction about world Y, applied *after* the up-axis {@link rot}. The
   * bbox fit has no notion of front/back, so a helmet authored facing away needs a
   * 180° yaw to look the same way as the rider. */
  yaw?: number;
  /** How the piece meets the body vertically — which edge of its bbox lands on
   * `anchor[1]`. Gear is worn edge-to-body, not centred: a helmet hangs its
   * **bottom** on the neck, boots hang their **top** on the leg-bottom. */
  alignY?: "center" | "top" | "bottom";
  /** Nod about world X, applied *after* {@link yaw} — tips a helmet's gaze down or a
   * boot's toe forward. `+` nods the front (the +Z face) downward. */
  pitch?: number;
}) {
  const texMap = useTextureMap(part.textures);
  const geoms = useBodyGeometries(part.nodes);
  const mats = useGearMaterials(part, texMap);

  // Gear is authored around its own origin (a helmet's bbox is centred on 0, not
  // up at head height), so it's fitted onto the body rather than dropped in place.
  // Measure in the fully-oriented frame (up-axis rot, then yaw, then pitch), since
  // the anchor is in body coords and the same transform drives the rendered mesh.
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
  // Vertical edge alignment: shift so the requested bbox edge (not just the centre)
  // lands on anchor[1]. `bottom` seats the piece's underside on the anchor (helmet
  // on the neck); `top` hangs it below the anchor (boots off the leg-bottom).
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

/** A boots `.edf` ships both feet as separate nodes (`boot_l`/`boot_r`) authored
 * nearly coincident at the ankle origin — the game's rig pulls them apart onto the
 * two foot bones. We don't parse the rig, so split them ourselves: order the nodes
 * by their native left-right (Y) centre and push one to each side. */
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

/** Axis-aligned bounds of a rider part's raw geometry (native, un-rotated).
 * Used to seat gear onto the real body via fractions of its bbox. */
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

/** Yaw (about world Y) that turns a boot's heel→toe axis to point straight along
 * +Z, cancelling the mesh's built-in toe-in/out splay. Measured in the already
 * up-righted (rotated) frame, from the centroids of the front and back 20% of the
 * boot along Z (robust to a stray toe/heel vertex). Returns 0 for degenerate input. */
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

/** Build plain geometries (one material) for the rider body — its `.edf` has no
 * submesh table, so a single suit material covers the whole mesh. */
function useBodyGeometries(nodes: EdfNode[]) {
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
      // Material groups so a multi-submesh gear node (a helmet's shell + goggles)
      // can wear a different texture per submesh. The body has no submesh table, so
      // no groups are added and it keeps a single material.
      n.submeshes.forEach((sm, i) => g.addGroup(sm.triStart * 3, sm.triCount * 3, i));
      g.computeBoundingBox();
      g.computeBoundingSphere();
      return g;
    });
  }, [nodes]);
  useEffect(() => () => geoms.forEach((g) => g.dispose()), [geoms]);
  return geoms;
}

/** One rider-body material in the shared product-preview look. The backend tags each
 * submesh: `rider` (suit) / `gloves` (hands) bind their paint; `face` renders as bare
 * skin (no kit on the head/neck); `hide` is invisible (the number/name decal planes we
 * don't texture). Unknown/untagged falls back to the suit. */
function makeBodyMaterial(name: string | null | undefined, tex: Map<string, THREE.Texture>) {
  const key = name?.toLowerCase();
  // Number/name decal planes: render nothing (no color, no depth) rather than smear
  // the whole suit texture across a flat quad.
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
    // Keep the normal for surface detail but at a subtle scale — full strength
    // over-shadows the paint. Skip the roughness map (ambiguous semantics).
    normalMap,
    normalScale: new THREE.Vector2(0.45, 0.45),
    color: map ? 0xffffff : 0x8a929c,
    metalness: 0.0,
    roughness: 0.62,
    // Lift the paint with its own colour as self-illumination so the whites read
    // white and the reds read red even in shadow (a product-preview look).
    emissive: map ? 0xffffff : 0x000000,
    emissiveMap: map,
    emissiveIntensity: map ? 0.32 : 0.0,
    // Rider meshes aren't reliably wound/closed; render both faces so the body
    // reads as solid instead of see-through.
    side: THREE.DoubleSide,
  });
}

/** The real rider **body** mesh (from the game's `rider.pkz`). The body is one skinned
 * mesh with a per-material submesh table — the **hands** are their own material bound to
 * the `gloves` paint, the rest to the `rider` suit — so a glove paint lands on the hands
 * without touching the suit. Authored Y-up, so no Z-up flip (unlike the bike). */
function RiderBodyMesh({ part }: { part: RiderPart }) {
  const tex = useTextureMap(part.textures);
  const geoms = useBodyGeometries(part.nodes);
  // One material per submesh (matching the geometry groups from `useBodyGeometries`),
  // each bound to its submesh's texture. A node with no submesh table takes a single
  // suit material.
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

/**
 * A single gear item shown on its own — previewing a helmet/boots mod from the
 * Library, where you want to see *that piece*, not a whole rider wearing it.
 * Scaled to fill the frame like the rider does.
 */
function RiderGearSolo({ part }: { part: RiderPart }) {
  const tex = useTextureMap(part.textures);
  const geoms = useBodyGeometries(part.nodes);
  const mats = useGearMaterials(part, tex);
  const rot =
    part.part === "boots" ? BOOT_ROT : part.part === "protection" ? PROT_ROT : GEAR_ROT;
  // Measure in the ROTATED frame, and — for a two-node boots pair authored
  // coincident at the ankle — push each foot to its own side by ~half a boot's
  // width so the preview shows a pair rather than one boot inside the other, then
  // straighten each toe. The arrangement is scaled to a consistent size and
  // recentred on the origin ourselves (the up-righted boot's bbox sits well below
  // its origin, and relying on <Center> for that left the pair under the camera).
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
      // Each foot goes to its correct side of the frame. The boots are mirror
      // images authored coincident, so the one whose rotated-X centre is more +X
      // belongs on screen-left here (camera looks from +X); place it there.
      const firstIsPlusX =
        boxes[0].min.x + boxes[0].max.x >= boxes[1].min.x + boxes[1].max.x;
      offsets[0] = (firstIsPlusX ? -1 : 1) * w * 0.55;
      offsets[1] = (firstIsPlusX ? 1 : -1) * w * 0.55;
    }
    // Straighten each foot so its toe points forward instead of splaying in.
    const yaws = geoms.map((g) => (pair ? straightenYaw(g, rotM) : 0));
    // Final arranged bounds: each foot as T(offset)·RotY(yaw)·rot, so the fit and
    // the recentre account for the real on-screen placement.
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

/**
 * The rider preview. When the real body mesh is available (the game's `rider.pkz`
 * is configured) it's rendered with the outfit skinned on and each installed gear
 * piece seated onto the body's bounds; otherwise it falls back to the primitive
 * stand-in with the outfit as a tint. Unset gear slots keep their placeholder.
 *
 * With gear but **no body**, it's a single-item preview (Library → a helmet), so
 * the piece is shown on its own rather than on a stand-in figure.
 */
function RiderComposite({ parts }: { parts: RiderPart[] }) {
  const byPart = (p: RiderPart["part"]) => parts.find((x) => x.part === p);
  const body = byPart("body");
  const helmet = byPart("helmet");
  const boots = byPart("boots");
  const protection = byPart("protection");
  const suit = useDataTexture(partPng(byPart("suit"), "rider", "suit"));
  const gloves = useDataTexture(partPng(byPart("gloves"), "gloves"));
  const hasBody = !!body?.nodes.length;
  const hasHelmet = !!helmet?.nodes.length;

  // Previewing a single gear item (no body): show just that piece.
  const solo = !hasBody
    ? [helmet, boots, protection].find((p) => p?.nodes.length)
    : undefined;

  // Gear anchors: fractions of the real body's bounds when present, else the
  // fixed stand-in positions.
  const b = hasBody ? partBounds(body!.nodes) : null;
  const cx = b ? (b.lo[0] + b.hi[0]) / 2 : 0;
  const cz = b ? (b.lo[2] + b.hi[2]) / 2 : 0;
  const h = b ? b.hi[1] - b.lo[1] : 1;
  const depth = b ? b.hi[2] - b.lo[2] : 1;
  // Half the gap between the legs, so each boot sits under its own leg (not bunched
  // at the centre-line).
  const legX = b ? 0.265 * (b.hi[0] - b.lo[0]) : 0.13;
  // Helmet hangs its bottom edge low on the neck (so little neck shows), nudged
  // forward in Z over the face — see the `alignY="bottom"` seat below.
  const helmetAnchor: [number, number, number] = b
    ? [cx, b.hi[1] - 0.09 * h, cz + 0.08 * depth]
    : [0, 1.62, 0];
  // Boots hang their top edge on the body's floor (where the legs end), extending
  // down to the ground — `alignY="top"` below. Nudged slightly forward in Z (and
  // pitched, see BOOT_PITCH) so they read as a riding stance, not hanging straight down.
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
          target={hasBody ? 0.3 * h : 0.46}
          yaw={hasBody ? Math.PI : 0}
          pitch={hasBody ? HELMET_PITCH : 0}
          alignY={hasBody ? "bottom" : "center"}
        />
      )}
      {!!protection?.nodes.length && (
        <RiderGearMesh part={protection!} anchor={protAnchor} target={hasBody ? 0.42 * h : 0.62} rot={PROT_ROT} />
      )}
      {/* A boots `.edf` ships both feet as separate nodes authored coincident at
          the ankle origin — split them left/right rather than stacking them. A
          single-node boot (whole pair in one mesh) just renders centred. */}
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

/** Build three.js geometry + per-submesh materials for each `.edf` node
 * (memoized, disposed on change). Submeshes become material groups so each part
 * (plastics, frame, chain, …) gets its own texture. */
function useEdfMeshes(
  nodes: EdfNode[] | null | undefined,
  tex: Map<string, THREE.Texture>,
) {
  const built = useMemo(() => {
    if (!nodes?.length) return [];
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
      g.computeBoundingBox();
      g.computeBoundingSphere();

      const makeMat = (t: THREE.Texture | null) =>
        new THREE.MeshStandardMaterial({
          map: t ?? undefined,
          color: t ? 0xffffff : 0xb7bcc4,
          metalness: 0.2,
          roughness: 0.55,
          // MX Bikes meshes have inconsistent winding, so single-sided rendering
          // back-face-culls chunks and the bike looks hollow/see-through. Render
          // both sides so it's solid.
          side: THREE.DoubleSide,
        });

      let materials: THREE.Material[];
      if (n.submeshes.length) {
        n.submeshes.forEach((sm, i) =>
          g.addGroup(sm.triStart * 3, sm.triCount * 3, i),
        );
        materials = n.submeshes.map((sm) => makeMat(submeshTexture(sm.texture, tex)));
      } else {
        // No submesh table → no groups to bind individually, so the node takes the
        // whole-node binding Rust resolved (the model's primary body texture).
        materials = [makeMat(submeshTexture(n.texture, tex))];
      }
      return { g, materials };
    });
  }, [nodes, tex]);

  useEffect(
    () => () => {
      built.forEach(({ g, materials }) => {
        g.dispose();
        materials.forEach((m) => m.dispose());
      });
    },
    [built],
  );

  return built;
}

/** Real bike geometry decoded from `.edf`, textured per submesh. MX Bikes meshes
 * are authored **Y-up, +Z forward** (validated from part positions: bars at +Y,
 * front suspension at +Z), which is three.js's convention — so no rotation. */
function EdfMesh({
  nodes,
  textures,
}: {
  nodes: EdfNode[];
  textures: Map<string, THREE.Texture>;
}) {
  const built = useEdfMeshes(nodes, textures);
  return (
    <group>
      {built.map(({ g, materials }, i) => (
        <mesh
          key={i}
          geometry={g}
          material={materials.length === 1 ? materials[0] : materials}
          castShadow
          receiveShadow
        />
      ))}
    </group>
  );
}

/** Place the camera for what's on screen. The default sits high (y=1.8) to look
 * down over a bike/rider; a single gear item is centred on the origin, so that
 * angle stares down at its crown — a level, closer view reads far better. */
function CameraRig({ solo }: { solo: boolean }) {
  const camera = useThree((s) => s.camera);
  // OrbitControls (`makeDefault`) owns the camera and re-applies its own orbit
  // state every frame, so moving the camera alone gets reverted — the move has to
  // go through the controls and be committed with `update()`.
  const controls = useThree((s) => s.controls) as
    | { target: THREE.Vector3; update: () => void }
    | null;
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
  }, [solo, camera, controls]);
  return null;
}

export interface ModelViewerProps {
  mode: ViewerMode;
  /** Paint texture as a `data:` URI (stand-in models only). */
  texture?: string | null;
  /** All available bike/paint textures, bound per-submesh on the real mesh. */
  textures?: PaintTexture[];
  /** Real bike geometry (from `loadBikeModel`). When present in bike mode, it's
   * rendered instead of the placeholder stand-in. */
  nodes?: EdfNode[] | null;
  /** Real rider gear + paints (from `loadRiderModel`). When present in rider mode,
   * installed gear meshes are seated on the body stand-in. */
  riderParts?: RiderPart[] | null;
  /** While true (and no real model yet) render nothing — the caller shows a
   * spinner — instead of flashing the placeholder stand-in. */
  loading?: boolean;
  /** Never render the primitive stand-in (used for real bikes: if the geometry
   * didn't load, the caller shows a "can't load" message instead of a fake). */
  noStandIn?: boolean;
  className?: string;
}

/** Reusable 3D canvas: orbit, local lighting, contact shadow, and a stand-in
 * model for the chosen mode. No external/CDN assets (works offline in the
 * Tauri webview). */
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
    <ErrorBoundary compact label="model-viewer">
      <Canvas
        className={className}
        shadows
        dpr={[1, 2]}
        camera={{ position: [2.6, 1.8, 3.2], fov: 42 }}
        onCreated={({ gl }) => {
          // A lost GPU context (driver hiccup, background throttling) otherwise
          // leaves a permanently black canvas. Preventing the default lets the
          // browser restore it automatically instead of killing the WebGL view.
          gl.domElement.addEventListener(
            "webglcontextlost",
            (e) => {
              e.preventDefault();
              console.warn("[ModelViewer] WebGL context lost — awaiting restore");
            },
            false,
          );
        }}
      >
        <color attach="background" args={["#0e0f13"]} />
        <CameraRig solo={gearSolo} />
        <ambientLight intensity={0.75} />
        {/* Even sky/ground fill so matte paint reads its true colour instead of
          going grey against the dark background. */}
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
  );
}
