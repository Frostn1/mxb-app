import * as THREE from "three";

/**
 * The backdrops the 3D preview can stand a model against.
 *
 * A pose is worth photographing, and the near-black studio the viewer has always drawn is a
 * poor frame for it. Each of these is a two-stop sky, a light rig to match, and a ground — all
 * of it written here rather than shipped as assets, so a backdrop costs nothing to download
 * and works with no network at all.
 */
export type SceneId = "studio" | "white" | "sky" | "sunset" | "dusk";

export interface ViewerScene {
  id: SceneId;
  /** Top and bottom of the backdrop. The same colour twice is a flat wall. */
  sky: [string, string];
  /** A ground under the model, or null to leave the contact shadow on its own. */
  ground: string | null;
  ambient: number;
  hemi: { sky: string; ground: string; intensity: number };
  /** The one light that casts. */
  key: { at: [number, number, number]; intensity: number };
  /** Rim from behind, and fill from the camera's side so the front of the kit isn't dark. */
  back: number;
  front: number;
  shadow: number;
}

export const SCENES: ViewerScene[] = [
  {
    // What the viewer has always drawn. Kept to the number, so nothing that doesn't ask for a
    // backdrop changes at all.
    id: "studio",
    sky: ["#0e0f13", "#0e0f13"],
    ground: null,
    ambient: 0.75,
    hemi: { sky: "#ffffff", ground: "#555a66", intensity: 0.7 },
    key: { at: [4, 6, 3], intensity: 1.25 },
    back: 0.55,
    front: 0.5,
    shadow: 0.5,
  },
  {
    id: "white",
    sky: ["#f7f8fa", "#dde2ea"],
    ground: "#e9ecf1",
    ambient: 0.9,
    hemi: { sky: "#ffffff", ground: "#c9cfd8", intensity: 0.85 },
    key: { at: [4, 6, 3], intensity: 1.1 },
    back: 0.5,
    front: 0.55,
    shadow: 0.32,
  },
  {
    id: "sky",
    sky: ["#5f9fdd", "#cfe2f2"],
    ground: "#8a6a4c",
    ambient: 0.7,
    hemi: { sky: "#bcd9f2", ground: "#8a6a4c", intensity: 0.9 },
    key: { at: [5, 7, 2], intensity: 1.35 },
    back: 0.4,
    front: 0.4,
    shadow: 0.45,
  },
  {
    id: "sunset",
    sky: ["#3a2350", "#ff9d5c"],
    ground: "#5c4634",
    ambient: 0.55,
    hemi: { sky: "#ffb27a", ground: "#4a3628", intensity: 0.8 },
    key: { at: [-5, 2.2, 3.5], intensity: 1.5 },
    back: 0.35,
    front: 0.3,
    shadow: 0.5,
  },
  {
    id: "dusk",
    sky: ["#0c1220", "#2b3550"],
    ground: "#242b3c",
    ambient: 0.45,
    hemi: { sky: "#8ea6d8", ground: "#1b2130", intensity: 0.6 },
    key: { at: [3, 5, -2], intensity: 1.1 },
    back: 0.7,
    front: 0.35,
    shadow: 0.55,
  },
];

export const DEFAULT_SCENE: SceneId = "studio";

export function sceneOf(id?: SceneId | null): ViewerScene {
  return SCENES.find((s) => s.id === id) ?? SCENES[0];
}

/**
 * The sky as a texture to hang behind the scene.
 *
 * A texture rather than a dome, deliberately: `ContactShadows` renders whatever is in the
 * scene, and a sphere around the model would be rendered into the shadow and black it out.
 * The background isn't part of the scene graph, so it can't be.
 */
export function skyTexture(scene: ViewerScene): THREE.Texture {
  const canvas = document.createElement("canvas");
  canvas.width = 4;
  canvas.height = 256;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    const grad = ctx.createLinearGradient(0, 0, 0, canvas.height);
    grad.addColorStop(0, scene.sky[0]);
    grad.addColorStop(1, scene.sky[1]);
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}
