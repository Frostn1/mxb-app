import * as THREE from "three";
import { GLTFExporter } from "three/examples/jsm/exporters/GLTFExporter.js";
import { writePsd, type Layer as PsdLayer } from "ag-psd";
import type { EdfNode } from "@frost/shared/types";
import { exportPaintProxy, psdSave, type PaintProxyResult } from "@frost/shared/api/mods";
import { uvParts, uvWireframe } from "./uv";

/**
 * A painting proxy, written out: the cut-down `.obj` from the backend, then per sheet a `.png`
 * template (what the `.obj`'s materials load) and a layered `.psd` to paint on, and one `.glb`
 * with the templates already on the model — a single file to drag into Blender.
 *
 * Templates face the way painters see a sheet. The game keeps a sheet's rows the other way up
 * (see `pnt-sheets-are-stored-upside-down`), so a template drawn here in stored order is flipped
 * before it's written; the `.obj` and the `.glb` carry UVs that account for it.
 */

/** Edge of a template — the size sheets are usually painted at. */
export const TEMPLATE_SIZE = 2048;

export type ProxyTarget = {
  nodes: EdfNode[];
  /** What the files are named after: the bike, the helmet, the file the part came from. */
  name: string;
  /** Whether the nodes sit in one frame, so the template can wash each flank its own colour. */
  assembled: boolean;
  /** The model's own look for a texture, in stored row order — for the optional shading layer. */
  shading?: (texture: string) => Promise<ImageBitmap | null>;
};

function canvasOf(size: number): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = size;
  c.height = size;
  return c;
}

/** `src` drawn onto a fresh square, turned the way painters see a sheet. */
function flipped(src: CanvasImageSource | null, size: number, ground?: string): HTMLCanvasElement {
  const c = canvasOf(size);
  const ctx = c.getContext("2d");
  if (!ctx) return c;
  if (ground) {
    ctx.fillStyle = ground;
    ctx.fillRect(0, 0, size, size);
  }
  if (src) {
    ctx.translate(0, size);
    ctx.scale(1, -1);
    ctx.drawImage(src, 0, 0, size, size);
  }
  return c;
}

function pngOf(canvas: HTMLCanvasElement): Promise<ArrayBuffer | null> {
  return new Promise((ok) => canvas.toBlob(ok, "image/png")).then((b) =>
    b ? (b as Blob).arrayBuffer() : null,
  );
}

function layer(name: string, canvas: HTMLCanvasElement, locked = false): PsdLayer {
  const { width, height } = canvas;
  return { name, top: 0, left: 0, bottom: height, right: width, canvas, protected: locked ? { transparency: true, composite: true, position: true } : undefined };
}

/** The proxy's meshes as a three.js scene, each sheet wearing its template. */
function sceneOf(proxy: EdfNode[], maps: Map<string, THREE.Texture>): THREE.Scene {
  const scene = new THREE.Scene();
  const plain = new THREE.MeshStandardMaterial({ color: 0xcccccc, roughness: 1, metalness: 0 });
  const byTexture = new Map<string, THREE.MeshStandardMaterial>();
  const materialFor = (texture: string | null) => {
    const map = texture ? maps.get(texture) : undefined;
    if (!texture || !map) return plain;
    let m = byTexture.get(texture);
    if (!m) {
      m = new THREE.MeshStandardMaterial({ map, roughness: 1, metalness: 0, name: texture });
      byTexture.set(texture, m);
    }
    return m;
  };
  for (const node of proxy) {
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.BufferAttribute(node.positions, 3));
    // As stored: three flips the image on upload (`flipY`), which is the same turn the
    // template was given, so the two meet the way the game samples the sheet.
    g.setAttribute("uv", new THREE.BufferAttribute(node.uvs, 2));
    g.setIndex(new THREE.BufferAttribute(node.indices, 1));
    const materials: THREE.Material[] = [];
    node.submeshes.forEach((sm, i) => {
      g.addGroup(sm.triStart * 3, sm.triCount * 3, i);
      materials.push(materialFor(sm.texture ?? null));
    });
    const mesh = new THREE.Mesh(g, materials.length ? materials : plain);
    mesh.name = node.name;
    scene.add(mesh);
  }
  return scene;
}

/**
 * Write the whole proxy into `dir`. `withShading` puts the model's own look under each
 * template as a reference layer — the creator's artwork, so it is theirs to include.
 */
export async function writePaintProxy(
  target: ProxyTarget,
  dir: string,
  withShading: boolean,
): Promise<PaintProxyResult> {
  const sep = dir.includes("\\") ? "\\" : "/";
  const size = TEMPLATE_SIZE;
  const res = await exportPaintProxy(target.nodes, dir, target.name);
  const maps = new Map<string, THREE.Texture>();

  for (const { texture, file } of res.templates) {
    const parts = uvParts(target.nodes, texture, { assembled: target.assembled });
    const opts = { cap: size, template: true, ground: false };
    const wash = uvWireframe(parts, size, size, { ...opts, wire: false });
    const wire = uvWireframe(parts, size, size, { ...opts, wash: false });
    const shade = withShading && target.shading ? await target.shading(texture) : null;

    // Stored order first, turned once at the end — the same turn for every layer.
    const stored = canvasOf(size);
    const ctx = stored.getContext("2d");
    if (ctx) {
      ctx.fillStyle = "#fff";
      ctx.fillRect(0, 0, size, size);
      if (shade) ctx.drawImage(shade, 0, 0, size, size);
      if (wash) ctx.drawImage(wash, 0, 0);
      if (wire) ctx.drawImage(wire, 0, 0);
    }
    const flat = flipped(stored, size);
    const png = await pngOf(flat);
    if (png) await psdSave(`${dir}${sep}${file}`, png);

    // Bottom-first. The wire on top and locked, so painting never lands on the guide.
    const children: PsdLayer[] = [layer("Background", flipped(null, size, "#fff"))];
    if (shade) children.push(layer("Model shading", flipped(shade, size)));
    if (wash) children.push(layer("Left / right", flipped(wash, size)));
    children.push(layer("Paint here", canvasOf(size)));
    if (wire) children.push(layer("UV layout", flipped(wire, size), true));
    const psd = writePsd({ width: size, height: size, canvas: flat, children }, { generateThumbnail: true });
    await psdSave(`${dir}${sep}${file.replace(/\.png$/i, ".psd")}`, psd);

    const map = new THREE.CanvasTexture(flat);
    map.colorSpace = THREE.SRGBColorSpace;
    maps.set(texture, map);
  }

  const scene = sceneOf(res.proxy, maps);
  const glb = (await new GLTFExporter().parseAsync(scene, { binary: true })) as ArrayBuffer;
  await psdSave(res.obj.replace(/\.obj$/i, ".glb"), glb);
  scene.traverse((o) => {
    if (o instanceof THREE.Mesh) o.geometry.dispose();
  });
  maps.forEach((m) => m.dispose());
  return res;
}
