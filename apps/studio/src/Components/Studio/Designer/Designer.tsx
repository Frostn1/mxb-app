import {
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Bike,
  ClipboardPaste,
  Copy,
  CopyPlus,
  Eye,
  EyeOff,
  FileImage,
  FilePlus2,
  FlipHorizontal2,
  FlipVertical2,
  Grid3x3,
  GripVertical,
  Group,
  Layers as LayersIcon,
  Link2,
  Link2Off,
  Loader2,
  PaintBucket,
  Plus,
  Save,
  Shirt,
  Trash2,
  Ungroup,
} from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import * as THREE from "three";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Progress } from "@frost/shared/Components/ui/progress";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@frost/shared/Components/ui/card";
import { ContextBarLeft, ShellChrome, UnsavedRegistry } from "../../Shell/ContextBar";
import {
  paintStudioExtract,
  paintStudioPixels,
  paintStudioSave,
  paintStudioStage,
  paintStudioTarget,
  designerRecentNote,
  psdRead,
  psdUnwatch,
  psdWatch,
  psdSave,
  textureBytes,
} from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import StartScreen from "./StartScreen";
import { IMAGE_EXTS, isBikeKind, usePaintDest } from "../paintDest";
const PREVIEW_OPEN_KEY = "mxb:designer:preview:v1";

import { CanvasStage } from "./CanvasStage";
import { Slider } from "./controls";
import { PreviewPanel } from "./PreviewPanel";
import { LayerInspector } from "./LayerInspector";
import { PaintTools } from "./PaintTools";
import { bitmapFromRgba, composite, hasInk, sheetTexture, toPng } from "./composite";
import { EMPTY_GHOST, ghostShows, type Ghost } from "./ghost";
import {
  islandAt,
  partAt,
  partBox,
  partPath,
  triangleAt,
  uvParts,
  uvWireframe,
  type UvPart,
} from "./uv";
import {
  blankSheet,
  cloneLayer,
  groupOf,
  imageLayer,
  isCompanionMap,
  isNormalMap,
  FLAT_NORMAL,
  layerExtent,
  newId,
  paintLayer,
  regroup,
  shapeLayer,
  textLayer,
  unionRegion,
  type Layer,
  type PaintLayer,
  type ShapeLayer,
  type Region,
  type Sheet,
} from "./layers";
import { buildMirror, derive, mirrorLayer, type MirrorIndex } from "./mirror";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@frost/shared/Components/ui/dropdown-menu";
import {
  DEFAULT_PAINT,
  PaintHistory,
  SHAPE_TOOLS,
  Stroke,
  TOOL_KEYS,
  constrained,
  type PaintSettings,
  type PaintTool,
  type Point,
} from "./paint";
import type { EdfNode, PaintTexture } from "@frost/shared/types";

/**
 * The paint designer: layers on a sheet, the sheet on the model, and a `.pnt` at the end.
 *
 * MX Bikes paints are drawn as flat texture sheets and worn on curved geometry, which is why
 * doing this in an image editor is guesswork — you move a logo, save, pack, launch, look, and
 * move it again. Everything here exists to close that loop: the sheet you're drawing *is* the
 * texture on the model beside it, and Save writes the packed file the game reads rather than
 * an export somebody else's tool has to convert.
 *
 * There's a brush too, and a gradient, and shapes — see `paint.ts`. They paint into a layer of
 * their own rather than into the sheet, so the template underneath survives every stroke and
 * the whole tool kit inherits opacity, blending and stacking from the layer system for free.
 *
 * What it deliberately still isn't: a general image editor. No selections, no filters, no
 * masks. It draws liveries, and it knows where they go.
 */

/** A blank sheet's edge. Powers of two only — the backend would resize anything else. */
const BLANK_SIZE = 2048;

/**
 * A stock texture's pixels, decoded, or null if they can't be had.
 *
 * Null covers both ends of it: the store evicts, and a rejected read is the same nothing to
 * draw as a name that matched no texture at all.
 */
/** A square of one colour, for a companion map with no stock texture to copy. */
async function flatBitmap(color: string, size: number): Promise<ImageBitmap> {
  const canvas = new OffscreenCanvas(size, size);
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = color;
  ctx.fillRect(0, 0, size, size);
  return createImageBitmap(canvas);
}

function stockBitmap(tex: PaintTexture): Promise<ImageBitmap | null> {
  return textureBytes(tex.token)
    .then((buf) => bitmapFromRgba(buf, tex.width, tex.height))
    .catch(() => null);
}

/** A sheet of flat normal — the value that says "no relief of my own", drawn once. */
async function flatNormalBitmap(size: number): Promise<ImageBitmap | null> {
  try {
    const c = document.createElement("canvas");
    c.width = size;
    c.height = size;
    const ctx = c.getContext("2d");
    if (!ctx) return null;
    ctx.fillStyle = FLAT_NORMAL;
    ctx.fillRect(0, 0, size, size);
    return await createImageBitmap(c);
  } catch {
    return null;
  }
}

/**
 * What was last copied, and the sheet it was cut from.
 *
 * Outside the component because the Studio unmounts this pane when another tab is opened, and
 * a clipboard that emptied when you went to look at something is a clipboard nobody uses. The
 * sheet's size travels with it: a layer's position is in sheet pixels, so pasting across sheets
 * of different sizes has to bring the artwork with it rather than leave it off the edge.
 */
let clipboard: { width: number; height: number; layers: Layer[] } | null = null;

/** One row of the canvas menu, so a dozen of them don't each spell out the same classes. */
function MenuRow({
  icon: Icon,
  label,
  disabled,
  onPick,
}: {
  icon: typeof Copy;
  label: string;
  disabled?: boolean;
  onPick: () => void;
}) {
  return (
    <DropdownMenuItem disabled={disabled} onSelect={onPick}>
      <Icon className="size-3.5" />
      {label}
    </DropdownMenuItem>
  );
}

interface DesignerProps {
  /**
   * Sheets handed over from Paint Studio, by path — drawn on rather than replaced.
   *
   * Consumed once and cleared by `onIncomingLoaded`, so coming back to this tab later doesn't
   * silently throw away whatever has been drawn since.
   */
  incoming?: string[] | null;
  onIncomingLoaded?: () => void;
}

export default function Designer({ incoming, onIncomingLoaded }: DesignerProps) {
  const t = useT();
  const [sheets, setSheets] = useState<Sheet[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [selection, setSelection] = useState<string[]>([]);
  // Where the canvas's right-click menu is, in client coordinates, or null for closed.
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const [name, setName] = useState("");
  /**
   * Whether the user has started something.
   *
   * The tab used to open straight into a bike: a destination is picked for you, and the
   * effect below fills in that model's sheet names as soon as it knows them, so there was
   * never a moment where nothing was open. That is why the empty state was effectively
   * unreachable — and why there was nowhere to put "carry on with what you were doing".
   * Nothing is filled in until this is true.
   */
  const [started, setStarted] = useState(
    // A development escape hatch, off unless it is asked for: set
    // `VITE_DESIGNER_AUTOSTART=1` in `apps/studio/.env.local` and the tab opens straight into
    // the editor on whatever destination is chosen by default, rather than the start screen.
    // Working on the Designer otherwise means clicking through the front door every reload.
    () => import.meta.env.VITE_DESIGNER_AUTOSTART === "1",
  );
  const { setBare } = useContext(ShellChrome);
  // Set by every edit, cleared by a save that lands. `pristine` cannot answer this: it stays
  // false after a save, and a paint you have just written is not work you would lose.
  const [unsaved, setUnsaved] = useState(false);
  useEffect(() => {
    setBare(!started);
    return () => setBare(false);
  }, [setBare, started]);

  const { register } = useContext(UnsavedRegistry);
  const saveRef = useRef<(() => Promise<boolean>) | null>(null);
  const unsavedRef = useRef(false);
  unsavedRef.current = unsaved && started;
  useEffect(() => {
    register({
      dirty: () => unsavedRef.current,
      save: async () => saveRef.current?.() ?? false,
    });
    return () => register(null);
  }, [register]);
  // The title is editable in place: open when it is clicked, or when a save needs a name.
  const [naming, setNaming] = useState(false);
  const [draftName, setDraftName] = useState("");
  const nameRef = useRef<HTMLInputElement>(null);
  // Set when Save asked for the name, so Enter in the title finishes that save.
  const saveAfterName = useRef(false);
  const [busy, setBusy] = useState(false);
  // The sheets/layers rail folds away, because once a paint is set up the thing worth the
  // width is the canvas and the model — not the list of what you already chose.
  // Remembered: someone who paints with it hidden wants it hidden next session too.
  const [previewOpen, setPreviewOpen] = useState(
    () => localStorage.getItem(PREVIEW_OPEN_KEY) !== "0",
  );
  const togglePreview = useCallback(
    () =>
      setPreviewOpen((open) => {
        localStorage.setItem(PREVIEW_OPEN_KEY, open ? "0" : "1");
        return !open;
      }),
    [],
  );
  // One bump per change to any sheet's pixels. The canvas stage and the 3D preview both
  // follow it rather than trying to work out for themselves what a "change" is.
  const [version, setVersion] = useState(0);
  const [paint, setPaint] = useState<PaintSettings>(DEFAULT_PAINT);
  // Painting history, and a counter to bring its undo/redo buttons back into a render — the
  // stack itself is a mutable object, so nothing about it would reach React on its own.
  const history = useRef(new PaintHistory<Sheet[]>());
  const [historyRev, setHistoryRev] = useState(0);
  // The stroke in progress. A ref because it changes on every pointer sample and no render
  // depends on it — the pixels it writes are what reach the screen.
  const stroke = useRef<Stroke | null>(null);
  // The pane stays mounted while another Studio tab is on screen, so the keyboard shortcuts
  // need a way to tell whether they're the ones being typed at.
  const rootRef = useRef<HTMLDivElement>(null);

  /**
   * Reference underlays, by sheet id — deliberately *beside* the sheets rather than inside them.
   *
   * A ghost is something to look at while drawing, not part of what is drawn, and keeping it
   * out of `Sheet` means two things at once: the save path has nothing to filter out, and
   * fading one in and out doesn't count as a change to the sheet, so it never triggers the
   * recomposite that every real edit does.
   */
  const [ghosts, setGhosts] = useState<Map<string, Ghost>>(new Map());
  // The mesh the preview is showing, reported back by it. Null until one loads, and null again
  // if it fails — a UV map drawn from a model that isn't on screen would be a confident lie.
  const [geometry, setGeometry] = useState<EdfNode[] | null>(null);
  // The same mesh, reachable without waiting for a render — see `onGeometry` for why it exists
  // at all, and `ensureMirror` for why it has to be readable from this far up the file.
  const geometryRef = useRef<EdfNode[] | null>(null);
  // Whether that mesh was assembled about the bike's mirror plane. Without it a position is a
  // number in some part's own frame, and the sides and facings read off it would be invented.
  const [assembled, setAssembled] = useState(false);
  // That same model's own textures — the look it ships with. Empty for anything that can't
  // say which of its textures are its own, which is every model but a bike.
  const [stockTextures, setStockTextures] = useState<PaintTexture[]>([]);

  const destState = usePaintDest();
  const { dest, hints, hintsFor } = destState;

  // Composites, one per sheet, owned here and reused: they're the size of the sheet, and
  // reallocating a 4096² canvas on every pointer move is not a thing to do.
  const canvases = useRef(new Map<string, HTMLCanvasElement>());
  // The textures wrapping those canvases, handed to the viewer. Same lifetime, same owner.
  const textures = useRef(new Map<string, THREE.DataTexture>());
  // The sheet object each canvas was last drawn from, so an untouched sheet isn't redrawn.
  const drawn = useRef(new Map<string, Sheet>());
  /**
   * What has changed on each stale sheet since its canvas was last drawn.
   *
   * An entry of `null` means "somewhere, unspecified" and buys nothing — the recomposite falls
   * back to the whole sheet, which is what it always did. A region is a promise that nothing
   * outside it moved, and only a stroke is in a position to make that promise, because only a
   * stroke knows where its own pixels went. Everything else goes through `patchSheet`, which
   * says `null` on the way past.
   *
   * Cleared by the recomposite that consumes it: a region held over from a redraw that already
   * happened would describe the wrong sheet by the time the next one came round.
   */
  const dirty = useRef(new Map<string, Region | null>());
  // The live map the viewer reads, plus the only thing allowed to change its identity.
  const overridesRef = useRef(new Map<string, THREE.Texture>());
  const [overrideNames, setOverrideNames] = useState("");
  // What was last published as `overrideNames`. Held beside the state so the recomposite below
  // can tell whether it has anything to say *before* saying it — see the note there.
  const publishedNames = useRef("");

  // The sheets as they stand, for the undo stack to snapshot on its way past an edit. A mirror
  // rather than a read of `sheets`: the callbacks that mutate are memoised on their own
  // dependencies, and one holding a stale array would remember a state that had already moved on.
  const sheetsRef = useRef(sheets);
  sheetsRef.current = sheets;

  const active = sheets.find((s) => s.id === activeId) ?? null;
  const bump = useCallback(() => {
    setVersion((v) => v + 1);
    setUnsaved(true);
  }, []);

  /**
   * The one selected layer, where "one" is what the question means.
   *
   * The paint target, the part picker and the fit all act on a single layer, and null is the
   * honest answer for a selection of three — better than picking the first and acting on a
   * layer nobody pointed at.
   */
  const selectedId = selection.length === 1 ? selection[0] : null;
  const chosen = useMemo(
    () => active?.layers.filter((l) => selection.includes(l.id)) ?? [],
    [active, selection],
  );

  /**
   * The sheets that exist, as a string.
   *
   * A stroke replaces the sheet it touches on every pointer sample, so `sheets` is a new array
   * a hundred times a second while the brush is down — and an effect that follows it re-runs
   * just as often. The housekeeping below only cares about *which sheets there are*, which a
   * stroke never changes, so it follows this instead.
   */
  const sheetIdKey = useMemo(() => sheets.map((s) => s.id).join(" "), [sheets]);

  const canvasFor = useCallback((sheet: Sheet) => {
    let canvas = canvases.current.get(sheet.id);
    if (!canvas) {
      canvas = document.createElement("canvas");
      canvases.current.set(sheet.id, canvas);
    }
    return canvas;
  }, []);

  /**
   * Recomposite what changed and republish the override map.
   *
   * A **layout** effect, not a passive one, and that isn't a detail: React runs a child's
   * effects before its parent's, so with `useEffect` the canvas stage would blit the composite
   * *before* this redrew it — every drag would render one frame behind the pointer. Layout
   * effects run parent-last but ahead of every passive effect, which puts the redraw back in
   * front of the blit.
   *
   * Sheets are compared by identity: an edit replaces exactly the sheet it touched, so this
   * redraws one 2048² canvas per pointer move rather than all of them. The texture behind it
   * follows the same test rather than being rebuilt every pass — reading a canvas back is the
   * most expensive thing in here, and re-reading the three sheets a stroke did not touch was
   * paying that price for pixels that were identical to the ones already uploaded.
   *
   * What did change is redrawn and read back across the region the stroke reported, which is
   * the difference between a stamp's worth of work per sample and a sheet's worth.
   */
  useLayoutEffect(() => {
    const next = new Map<string, THREE.Texture>();
    for (const sheet of sheets) {
      const canvas = canvasFor(sheet);
      let tex = textures.current.get(sheet.id) ?? null;
      if (drawn.current.get(sheet.id) !== sheet) {
        const area = composite(canvas, sheet, dirty.current.get(sheet.id) ?? null);
        drawn.current.set(sheet.id, sheet);
        // Built the same way the viewer builds an installed paint's texture, so the drawing
        // lands on the mesh exactly where the `.pnt` would have. `needsUpdate` inside carries
        // the new pixels without any React work. A sheet that turned out to have no pixels to
        // redraw has none to upload either.
        if (area) tex = sheetTexture(canvas, tex, area);
      }
      if (!tex) tex = sheetTexture(canvas, null);
      if (!tex) continue;
      textures.current.set(sheet.id, tex);
      if (sheet.name.trim()) next.set(sheet.name.trim().toLowerCase(), tex);
    }
    dirty.current.clear();
    // Identity changes only when the *names* do. The viewer memoises one material per submesh
    // on this map, so handing it a fresh one per pointer move rebuilt every material in the
    // model on every frame of a drag — which is exactly as slow as it sounds. The textures
    // inside are the same objects either way; `needsUpdate` above is what carries the pixels.
    overridesRef.current = next;
    // Compared against a ref, and only *then* set. An updater that returns its own argument
    // still counts as an update: React re-renders this component to find out that nothing
    // moved, and this effect runs on every frame of a stroke — so guarding inside the updater
    // bought a wasted render of the whole editor per frame of every drag.
    const names = [...next.keys()].sort().join(" ");
    if (publishedNames.current !== names) {
      publishedNames.current = names;
      setOverrideNames(names);
    }
  }, [sheets, version, canvasFor]);

  const overrides = useMemo(
    () => new Map(overridesRef.current),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [overrideNames],
  );

  // Drop the canvases and textures of sheets that are gone, and everything on unmount.
  useEffect(() => {
    const live = new Set(sheets.map((s) => s.id));
    for (const [id, tex] of textures.current) {
      if (!live.has(id)) {
        tex.dispose();
        textures.current.delete(id);
        canvases.current.delete(id);
        drawn.current.delete(id);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sheetIdKey]);

  useEffect(() => {
    const held = textures.current;
    return () => {
      held.forEach((tex) => tex.dispose());
      held.clear();
    };
  }, []);

  /**
   * Record the document as it stands, so the edit about to happen can be taken back.
   *
   * Before the mutation, never after: what undo needs is the state that existed a moment ago,
   * and once `setSheets` has run there is nothing left holding it. `key` collapses a run of
   * edits that are one gesture — see `PaintHistory.pushDoc`.
   */
  const remember = useCallback((key: string | null = null) => {
    history.current.pushDoc(sheetsRef.current, key);
    setHistoryRev((v) => v + 1);
  }, []);

  /* ── Mirroring ─────────────────────────────────────────────────────────────────────────
     A mirrored layer is a *follower*: it holds no placement of its own worth keeping, and is
     re-derived from the layer it reflects on every edit. That derivation hangs off `patchSheet`
     below, which is the one road every edit but a stroke takes. ──────────────────────────── */

  /**
   * What the mirror needs, in a ref rather than a memo, and built only when something asks.
   *
   * `patchSheet` is defined here — long before the model's parts are — and every edit has to
   * go through it, so the geometry arrives sideways rather than in a dependency list. Lazily,
   * too: most paints mirror nothing, and a memo would build an index for every sheet anyone
   * opened to answer a question that was never asked.
   *
   * `ready` is the model's own statement that its axes can be trusted. Without it there is no
   * left and right to reflect between — see `uvParts`.
   */
  const mirrorRef = useRef<{
    sheetId: string | null;
    parts: UvPart[];
    ready: boolean;
    index: MirrorIndex | null;
  }>({ sheetId: null, parts: [], ready: false, index: null });

  const ensureMirror = useCallback((): MirrorIndex | null => {
    const held = mirrorRef.current;
    if (!held.ready) return null;
    if (!held.index) held.index = buildMirror(held.parts, geometryRef.current ?? []);
    return held.index;
  }, []);

  /**
   * Bring every follower on a sheet back into step with the layer it reflects.
   *
   * Strokes don't come through here and don't need to: a paint layer is the sheet, so it can
   * never be a source or a follower, and that is what keeps this off the one path in the
   * editor that runs a hundred times a second.
   */
  const syncMirrors = useCallback(
    (sheet: Sheet): Sheet => {
      if (!sheet.layers.some((l) => l.mirror)) return sheet;
      const held = mirrorRef.current;
      const index = held.sheetId === sheet.id ? ensureMirror() : null;
      const by = new Map(sheet.layers.map((l) => [l.id, l]));
      return {
        ...sheet,
        layers: sheet.layers.map((layer) => {
          if (!layer.mirror) return layer;
          const from = by.get(layer.mirror.of);
          // The source has gone — deleted, or undone away — or is something that can't be a
          // source. The follower stops following rather than going with it: what it holds is
          // still somebody's artwork, and it is already on the bike.
          if (!from || from.id === layer.id || from.kind === "paint") {
            return { ...layer, mirror: null };
          }
          // A null placement leaves it where it is. That is the case where the model isn't
          // loaded, and a follower that jumped to a guess would look placed rather than stale.
          const placed = index ? mirrorLayer(index, held.parts, from, sheet) : null;
          return derive(layer, from, placed?.ok ? placed : null, held.parts, sheet);
        }),
      };
    },
    [ensureMirror],
  );

  const patchSheet = useCallback(
    /**
     * `undoKey` names the gesture for coalescing, or `false` for an edit the history must not
     * record — one whose other half lives outside the document, where an undo would restore the
     * sheet and leave that half where it was.
     */
    (id: string, fn: (s: Sheet) => Sheet, undoKey: string | null | false = null) => {
      // Every route into a sheet but a stroke comes through here, and none of them says where it
      // drew. Marking the sheet wholly dirty on the way past is what lets the recomposite treat a
      // region as a promise rather than a hint: if one is there, a stroke put it there.
      if (undoKey !== false) remember(undoKey);
      dirty.current.set(id, null);
      // Followers re-derived on the way out, so no caller has to remember they exist. Dragging
      // a logo and hiding one are the same kind of edit as far as the far side is concerned.
      setSheets((prev) => prev.map((s) => (s.id === id ? syncMirrors(fn(s)) : s)));
    },
    [remember, syncMirrors],
  );

  const patchLayer = useCallback(
    (layerId: string, fn: (l: Layer) => Layer, undoKey: string | null | false = null) => {
      if (!activeId) return;
      patchSheet(
        activeId,
        (s) => ({
          ...s,
          layers: s.layers.map((l) => (l.id === layerId ? fn(l) : l)),
        }),
        undoKey,
      );
    },
    [activeId, patchSheet],
  );

  /* ── Painting ──────────────────────────────────────────────────────────────────────────
     A stroke writes straight into its layer's canvas, which React cannot see. `touchPaint`
     is what makes the change exist as far as the editor is concerned: a fresh layer with a
     higher `rev` inside a fresh sheet, which is exactly the signal the recomposite above
     watches for. ─────────────────────────────────────────────────────────────────────── */

  /**
   * Record where a stroke drew, without telling React anything.
   *
   * Split from the notification below because the two want different rates. Pixels have to be
   * accounted for the instant they land — miss one sample's region and that part of the stroke
   * never reaches the composite — while the render they add up to is worth doing once a frame.
   */
  const markPaint = useCallback((sheetId: string, region: Region | null) => {
    const held = dirty.current.get(sheetId);
    // Once a sheet is wholly dirty it stays that way until the redraw: a region unioned onto
    // "everything" would narrow the redraw to less than is actually stale.
    if (region && held !== null) dirty.current.set(sheetId, unionRegion(held ?? null, region));
    else dirty.current.set(sheetId, null);
  }, []);

  /** Make the pixels a stroke has already written exist as far as React is concerned. */
  const notifyPaint = useCallback(
    (sheetId: string, layerId: string) => {
      setSheets((prev) =>
        prev.map((s) =>
          s.id === sheetId
            ? {
                ...s,
                layers: s.layers.map((l) =>
                  l.id === layerId && l.kind === "paint" ? { ...l, rev: l.rev + 1 } : l,
                ),
              }
            : s,
        ),
      );
      bump();
    },
    [bump],
  );

  const touchPaint = useCallback(
    (sheetId: string, layerId: string, region?: Region | null) => {
      markPaint(sheetId, region ?? null);
      notifyPaint(sheetId, layerId);
    },
    [markPaint, notifyPaint],
  );

  /**
   * One render per frame, however fast the pointer reports.
   *
   * Pointer samples are not paced by the display. A high-rate mouse on a webview that doesn't
   * align them to the frame delivers a dozen or more between two paints, and each one used to
   * drive a full React commit, a recomposite, a texture upload and a redraw of both views — a
   * dozen renders where the screen could show one. The stroke itself still takes every sample
   * the moment it arrives, because that is what the mark is made of; only the telling-everyone
   * waits, and it waits at most until the next frame, which is the soonest anyone could see it.
   */
  // The shape layer a drag is currently rewriting, and where the press landed. Null except
  // between the press and the release of a shape tool — the stroke ref's opposite number.
  const shaping = useRef<{ id: string; from: Point } | null>(null);

  const queued = useRef<{ sheetId: string; layerId: string } | null>(null);
  const frame = useRef(0);

  const flushPaint = useCallback(() => {
    if (frame.current) {
      cancelAnimationFrame(frame.current);
      frame.current = 0;
    }
    const q = queued.current;
    queued.current = null;
    if (q) notifyPaint(q.sheetId, q.layerId);
  }, [notifyPaint]);

  const schedulePaint = useCallback(
    (sheetId: string, layerId: string) => {
      queued.current = { sheetId, layerId };
      if (!frame.current) frame.current = requestAnimationFrame(flushPaint);
    },
    [flushPaint],
  );

  // A stroke abandoned by an unmount has already written its pixels; the frame that would have
  // announced them has nowhere left to land.
  useEffect(
    () => () => {
      if (frame.current) cancelAnimationFrame(frame.current);
    },
    [],
  );

  /** The paint layer a stroke would land on: the selected one, when it is one. */
  const target = useMemo<PaintLayer | null>(() => {
    const layer = active?.layers.find((l) => l.id === selectedId);
    return layer?.kind === "paint" ? layer : null;
  }, [active, selectedId]);

  const addPaintLayer = useCallback(() => {
    if (!active) return;
    const layer = paintLayer(t("designer.paintLayerName"), active);
    patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, layer] }));
    setSelection([layer.id]);
    bump();
  }, [active, bump, patchSheet, t]);

  /**
   * Choose a tool, and make sure it has somewhere to paint.
   *
   * Picking up a brush with no paint layer selected and having nothing happen is
   * indistinguishable from a broken brush, so this finds the paint layer already on the sheet
   * or starts one. The template is never a candidate — it is what's being drawn on top of.
   */
  const pickTool = useCallback(
    (tool: PaintTool) => {
      setPaint((p) => ({ ...p, tool }));
      // A shape makes its own layer on the press, so it needs nothing selected to land on.
      if (tool === "move" || SHAPE_TOOLS.has(tool) || !active) return;
      const selected = active.layers.find((l) => l.id === selectedId);
      if (selected?.kind === "paint") return;
      const existing = [...active.layers].reverse().find((l) => l.kind === "paint");
      if (existing) setSelection([existing.id]);
      else addPaintLayer();
    },
    [active, addPaintLayer, selectedId],
  );


  const movePaint = useCallback(
    (points: Point[], constrain: boolean) => {
      // A shape in progress is a layer, not a stroke: the drag rewrites its box and the
      // composite redraws it, so what is on screen mid-drag is the shape itself rather than a
      // preview of one. See `startPaint`.
      const drawing = shaping.current;
      if (drawing) {
        const raw = points[points.length - 1];
        if (!raw) return;
        const to = constrain ? constrained(drawing.from, raw, paint.tool) : raw;
        patchLayer(
          drawing.id,
          (l) =>
            l.kind === "shape"
              ? {
                  ...l,
                  x: (drawing.from.x + to.x) / 2,
                  y: (drawing.from.y + to.y) / 2,
                  w: to.x - drawing.from.x,
                  h: -(to.y - drawing.from.y),
                }
              : l,
          `shape:${drawing.id}`,
        );
        bump();
        return;
      }
      const live = stroke.current;
      if (!live || !target || !activeId) return;
      live.move(points, constrain);
      if (!live.dirty) return;
      markPaint(activeId, live.dirty);
      schedulePaint(activeId, target.id);
    },
    [activeId, bump, markPaint, paint.tool, patchLayer, schedulePaint, target],
  );

  const endPaint = useCallback(() => {
    const drawing = shaping.current;
    if (drawing) {
      shaping.current = null;
      // A click with a shape tool selected is not a shape. Left in, it would be an invisible
      // layer in the list with handles too small to grab and nothing to see.
      if (active) {
        const made = active.layers.find((l) => l.id === drawing.id);
        if (made?.kind === "shape" && Math.abs(made.w) < 2 && Math.abs(made.h) < 2) {
          patchSheet(active.id, (sh) => ({
            ...sh,
            layers: sh.layers.filter((l) => l.id !== drawing.id),
          }));
          setSelection([]);
          bump();
        }
      }
      return;
    }
    const done = stroke.current;
    stroke.current = null;
    // Whatever the last frame didn't get to, now — a stroke that ended between two frames would
    // otherwise leave its final samples drawn on the layer and missing from the composite.
    flushPaint();
    // A press that put nothing down isn't a step to undo — clicking to check the tool would
    // otherwise fill the history with states identical to the one before them.
    if (!done?.end() || !target || !activeId) return;
    history.current.push(activeId, target.id, done.before);
    setHistoryRev((v) => v + 1);
  }, [active, activeId, bump, flushPaint, patchSheet, target]);

  /** The live canvas behind a layer id, wherever it lives — history spans every sheet. */
  const paintCanvas = useCallback(
    (layerId: string) => {
      for (const sheet of sheets) {
        const layer = sheet.layers.find((l) => l.id === layerId);
        if (layer?.kind === "paint") return layer.canvas;
      }
      return null;
    },
    [sheets],
  );

  // The stack is a mutable object, so React has no way to notice it moved. `historyRev` is the
  // notice, and reading it here is what makes these two follow it.
  const { canUndo, canRedo } = useMemo(
    () => ({ canUndo: history.current.canUndo, canRedo: history.current.canRedo }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [historyRev],
  );

  /**
   * Reading and replacing the document for the history — plus putting the cursor somewhere real
   * afterwards.
   *
   * Undoing the sheet you were looking at leaves `activeId` naming something that no longer
   * exists, and the editor would come back empty rather than showing what it just restored. The
   * same for a selected layer. Neither is part of the document, so neither is restored by it.
   */
  const docAccess = useMemo(
    () => ({
      read: () => sheetsRef.current,
      write: (next: Sheet[]) => {
        setSheets(next);
        setActiveId((cur) => (cur && next.some((s) => s.id === cur) ? cur : next[0]?.id ?? null));
        setSelection((cur) => {
          const alive = cur.filter((id) => next.some((s) => s.layers.some((l) => l.id === id)));
          return alive.length === cur.length ? cur : alive;
        });
      },
    }),
    [],
  );

  // Not while the pointer is still down: the stroke redraws its layer from its own snapshot on
  // the next sample, so an undo mid-drag would be silently taken back a moment later.
  const undo = useCallback(() => {
    if (stroke.current) return;
    const entry = history.current.undo(paintCanvas, docAccess);
    setHistoryRev((v) => v + 1);
    if (!entry) return;
    // A document step replaces sheet objects wholesale, and the recomposite already follows
    // sheet identity — so it needs no region, only to be told the drawing moved.
    if (entry.kind === "pixels") touchPaint(entry.sheetId, entry.layerId);
    else bump();
  }, [bump, docAccess, paintCanvas, touchPaint]);

  const redo = useCallback(() => {
    if (stroke.current) return;
    const entry = history.current.redo(paintCanvas, docAccess);
    setHistoryRev((v) => v + 1);
    if (!entry) return;
    if (entry.kind === "pixels") touchPaint(entry.sheetId, entry.layerId);
    else bump();
  }, [bump, docAccess, paintCanvas, touchPaint]);

  // Which paint layers exist, as a string. `sheets` changes on every pointer sample of a
  // stroke; this changes only when one is added or deleted, which is the only time the
  // history below could be holding something that no longer has a canvas to go back onto.
  const paintLayerKey = useMemo(
    () =>
      sheets
        .map((s) => `${s.id}:${s.layers.filter((l) => l.kind === "paint").map((l) => l.id)}`)
        .join("|"),
    [sheets],
  );

  useEffect(() => {
    const alive = new Map(
      sheets.map((s) => [
        s.id,
        new Set(s.layers.filter((l) => l.kind === "paint").map((l) => l.id)),
      ]),
    );
    if (history.current.keepOnly((sid, lid) => alive.get(sid)?.has(lid) ?? false)) {
      setHistoryRev((v) => v + 1);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paintLayerKey]);


  /**
   * Pixels for a file the user picked, at the sheet's own resolution.
   *
   * The rows arrive in the order the file holds them, which is the order the mesh samples
   * them — and that is upside down from the template a painter works in. Nothing is flipped
   * here: the sheet, the composite and the save all stay in the file's own row order, and the
   * 2D stage turns it the right way up for display alone (see `CanvasStage`). Flipping the
   * pixels instead would put the editor's opinion about orientation inside the saved paint.
   */
  const readImage = useCallback(async (path: string) => {
    const tex = await paintStudioPixels(path);
    const buf = await textureBytes(tex.token);
    return { name: tex.name, bitmap: await bitmapFromRgba(buf, tex.width, tex.height) };
  }, []);

  /**
   * Which destination the sheets on screen belong to, and which one has already been asked
   * about — see the effect below `destKey` for what each of them stops from happening twice.
   *
   * `destKeyRef` is the same string as `destKey`, readable from up here: `installSheets` is
   * defined long before the destination is, and every caller of it means "these are the sheets
   * for wherever this is aimed right now".
   */
  const filled = useRef<string | null>(null);
  const warned = useRef<string | null>(null);
  const destKeyRef = useRef("");

  /**
   * Make `next` the sheets, whatever they were made out of.
   *
   * Every way into the editor ends here — an unpacked `.pnt`, a Photoshop file, the sheets a
   * model asks for — because they all mean the same thing: what was open is gone and this is
   * open instead. One undo step covers it, which is what makes a wrong choice of starting
   * point cost a keystroke rather than the work.
   *
   * Whatever arrives is recorded against the destination that is chosen. A paint opened for
   * this bike is *for* this bike, and without that the warning below would fire on the way in
   * — sheets handed over by Paint Studio land here before the model's own list has been
   * fetched, which is indistinguishable from having switched models unless somebody says so.
   */
  const installSheets = useCallback(
    (next: Sheet[], nameHint?: string) => {
      remember();
      setSheets(next);
      setActiveId(next[0]?.id ?? null);
      setSelection([]);
      if (nameHint) setName((n) => n || nameHint);
      filled.current = destKeyRef.current;
      bump();
    },
    [bump, remember],
  );

  /**
   * Start from an installed paint.
   *
   * This is the template step, and it matters more than it looks: the sheets come back named
   * the way the model binds them, so a livery drawn on top lands on the right bodywork. A
   * blank sheet has to be named by hand, and a wrong name is a paint that loads and shows
   * nothing.
   */
  const loadSheets = useCallback(
    async (paths: string[], nameHint?: string) => {
      // Not a silent return: a paint that unpacks to nothing looks exactly like a button that
      // doesn't work, and the difference matters — one is a broken app, the other is a file
      // this can't read.
      if (!paths.length) {
        toast.error(t("designer.noSheetsFound"));
        return;
      }
      const loaded = await Promise.all(paths.map((f) => readImage(f)));
      installSheets(
        loaded.map(({ name: sheetName, bitmap }) => ({
          id: newId("sheet"),
          name: sheetName,
          width: bitmap.width,
          height: bitmap.height,
          base: bitmap,
          layers: [],
        })),
        nameHint,
      );
      toast.success(t("designer.loadedSheets", { count: String(paths.length) }));
    },
    [installSheets, readImage, t],
  );

  const openPaint = useCallback(
    async (path: string) => {
    // Busy from here, not from inside `loadSheets`: unpacking the `.pnt` is the slow half —
    // it reads the file, inflates every sheet and writes them out — and leaving it outside the
    // spinner is why picking a paint looked like nothing had happened.
    setBusy(true);
    try {
      const template = await paintStudioExtract(path);
      await loadSheets(
        template.files,
        (path.replace(/\\/g, "/").split("/").pop() ?? "").replace(/\.pnt$/i, ""),
      );
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
    },
    [loadSheets],
  );

  /** The same act with the path still to be chosen. */
  const startFromPaint = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "MX Bikes paint", extensions: ["pnt"] }],
    });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    setStarted(true);
    await openPaint(path);
  }, [openPaint]);

  /**
   * Start from a Photoshop file — one sheet per document.
   *
   * The layers survive the crossing, which is the whole point: a livery that arrives flattened
   * is a picture of somebody's work rather than the work, and every adjustment after that is
   * repainting. Sizes and names come from the file, so a `plastics.psd` lands as a sheet called
   * `plastics` at whatever the artist drew it at — the save resizes to an edge the game takes.
   *
   * One at a time rather than in parallel: a 4096² document with two dozen layers is decoded
   * into as many bitmaps, and several of those in flight at once is how a webview runs out of
   * memory partway through.
   */
  const startFromPsd = useCallback(async () => {
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Photoshop", extensions: ["psd", "psb"] }],
    });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (!paths.length) return;
    setStarted(true);
    setBusy(true);
    try {
      // Loaded on demand, here and in the export below. The PSD codec is a quarter of a
      // megabyte and most sessions never open one — see `psd.ts`.
      const { psdToSheet } = await import("./psd");
      const next: Sheet[] = [];
      for (const path of paths) {
        const stem = (path.replace(/\\/g, "/").split("/").pop() ?? "").replace(/\.ps[db]$/i, "");
        next.push(await psdToSheet(await psdRead(path), stem));
      }
      installSheets(next, next[0]?.name);
      toast.success(t("designer.loadedSheets", { count: String(next.length) }));
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [installSheets, t]);

  /**
   * Write every sheet out as a `.psd`, into a folder the picker chose.
   *
   * A folder and not a file, because a sheet is a document: they have their own sizes, and a
   * PSD has one canvas — so a paint with a 4096² `plastics` and a 1024² `number` cannot be one
   * file without resampling one of them. One `.psd` each says what is true.
   *
   * Composited on the way out for the same reason the save does it: the layers only exist as
   * canvases until something asks, and asking here is what stops an export shipping a frame
   * older than the screen.
   */
  /**
   * The `.psd` files this session exported, and which sheet each came from.
   *
   * A ref because the listener below is registered once and must see the current map — and
   * because nothing on screen depends on it until a file actually changes.
   */
  const watched = useRef(new Map<string, string>());

  useEffect(() => {
    const un = listen<{ path: string }>("psd-changed", async (e) => {
      const sheetId = watched.current.get(e.payload.path);
      if (!sheetId) return;
      try {
        const { psdToSheet } = await import("./psd");
        const stem = (e.payload.path.replace(/\\/g, "/").split("/").pop() ?? "").replace(
          /\.psb?d$/i,
          "",
        );
        const made = await psdToSheet(await psdRead(e.payload.path), stem);
        // The sheet keeps its name and its place: it is the same sheet, redrawn. Replacing
        // the name would rebind it to different bodywork on the next save.
        remember();
        patchSheet(sheetId, (sheet) => ({ ...sheet, ...made, id: sheet.id, name: sheet.name }));
        bump();
        toast.success(t("designer.psdReloaded", { name: stem }));
      } catch (err) {
        toast.error(String(err).replace(/^Error:\s*/, ""));
      }
    });
    return () => {
      void un.then((f) => f());
      void psdUnwatch().catch(() => {});
    };
  }, [bump, patchSheet, remember, t]);

  const exportPsd = useCallback(async () => {
    if (!sheets.length) return;
    const picked = await openDialog({ directory: true });
    const dir = Array.isArray(picked) ? picked[0] : picked;
    if (!dir) return;
    setBusy(true);
    try {
      const { sheetToPsd } = await import("./psd");
      // The picker answers in the platform's own separator, and a path with both in it is a
      // path some Windows API will refuse. Follow whatever it handed back.
      const sep = dir.includes("\\") ? "\\" : "/";
      const prefix = name.trim() ? `${name.trim()} - ` : "";
      let written = 0;
      const exported = new Map<string, string>();
      for (const sheet of sheets) {
        const canvas = canvasFor(sheet);
        composite(canvas, sheet);
        // A sheet is named after a texture, and a texture name is not obliged to be a legal
        // file name. Nothing is renamed on the sheet — only on the file it is written to.
        const label = (sheet.name.trim() || `sheet-${written + 1}`).replace(/[\\/:*?"<>|]/g, "_");
        const at = `${dir}${sep}${prefix}${label}.psd`;
        await psdSave(at, sheetToPsd(sheet, canvas));
        exported.set(at, sheet.id);
        written += 1;
      }
      // Watched from here, which is what makes this a round trip rather than an export:
      // save in Photoshop and the sheet it came from is replaced in place.
      watched.current = exported;
      await psdWatch([...exported.keys()]).catch(() => {});
      toast.success(t("designer.exportedPsd", { count: written, dir }), {
        description: t("designer.psdWatching"),
      });
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [canvasFor, name, sheets, t]);

  // Sheets sent over from Paint Studio. Same path as unpacking a paint here, because it is the
  // same thing — that tab has already done the unpacking.
  useEffect(() => {
    if (!incoming?.length) return;
    // Sheets handed over by Paint Studio are a start like any other.
    setStarted(true);
    setBusy(true);
    void loadSheets(incoming)
      .catch((e) => toast.error(String(e).replace(/^Error:\s*/, "")))
      .finally(() => {
        setBusy(false);
        onIncomingLoaded?.();
      });
  }, [incoming, loadSheets, onIncomingLoaded]);

  /**
   * Texture names this model wants that have no sheet yet — what the create button offers,
   * and the name a new blank sheet is given.
   *
   * Color sheets only. The hint line lists the companion maps too, because knowing the bike
   * has a `plastics_n` is worth knowing, but an *empty* one is worse than none: a paint
   * replaces textures by name, so saving a blank normal map strips the bike's real one.
   */
  const missingHints = useMemo(() => {
    const taken = new Set(sheets.map((s) => s.name.trim().toLowerCase()));
    return hints.filter((h) => !taken.has(h.trim().toLowerCase()) && !isCompanionMap(h));
  }, [hints, sheets]);

  /** The companion maps the model asks for that aren't on the list — normals, mostly. */
  const missingCompanions = useMemo(() => {
    const taken = new Set(sheets.map((s) => s.name.trim().toLowerCase()));
    return hints.filter((h) => !taken.has(h.trim().toLowerCase()) && isCompanionMap(h));
  }, [hints, sheets]);

  const addBlankSheet = useCallback(() => {
    // Name it after a texture the chosen model actually asks for, when we know one — that's
    // the difference between a paint that shows and a paint that doesn't.
    const suggested = missingHints[0] ?? "";
    const sheet = blankSheet(suggested, BLANK_SIZE);
    remember();
    setSheets((prev) => [...prev, sheet]);
    setActiveId(sheet.id);
    setSelection([]);
    bump();
  }, [bump, missingHints, remember]);

  /**
   * Start a new paint for the destination that is chosen.
   *
   * The sheets themselves come from the effect above, which knows what the model binds. When
   * a model offers no names — a loose folder, mostly — one blank sheet is a better editor to
   * be dropped into than none at all.
   */
  const beginNew = useCallback(() => {
    setStarted(true);
    if (!hints.some((h) => !isCompanionMap(h))) addBlankSheet();
  }, [addBlankSheet, hints]);

  /**
   * Add a sheet from an image on disk.
   *
   * Not the same act as "Start from a paint", which is the template step and *replaces*
   * everything. This adds one sheet beside the others — the answer to having a `.tga` for one
   * piece of bodywork and nothing else, which until now meant making a blank sheet, adding the
   * image as a layer and resizing the sheet to match by hand.
   *
   * The file name becomes the sheet name, because the name is the entire binding and the file
   * is usually already called what the model asks for. The image lands as the sheet's base
   * rather than as a layer: it is the sheet, not something placed on it.
   */
  const addSheetFromImage = useCallback(async () => {
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Images", extensions: IMAGE_EXTS }],
    });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (!paths.length) return;
    setBusy(true);
    try {
      const made: Sheet[] = [];
      for (const path of paths) {
        const { name: fileName, bitmap } = await readImage(path);
        const stem = (fileName.replace(/\\/g, "/").split("/").pop() ?? fileName).replace(
          /\.[^.]+$/,
          "",
        );
        made.push({
          ...blankSheet(stem, bitmap.width),
          width: bitmap.width,
          height: bitmap.height,
          base: bitmap,
        });
      }
      remember();
      setSheets((prev) => [...prev, ...made]);
      setActiveId(made[0].id);
      setSelection([]);
      bump();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [bump, readImage, remember]);

  /**
   * One sheet per color texture the model asks for that isn't on the list yet.
   *
   * The names are the whole binding — a sheet called anything else paints nothing — and until
   * now the only way to get them right without an installed paint to start from was to read
   * them off the hint line and type each one back in.
   *
   * Only the missing ones, so pressing it twice doesn't leave two sheets fighting over a name.
   */
  const addHintSheets = useCallback(() => {
    const made = missingHints.map((h) => blankSheet(h, BLANK_SIZE));
    if (!made.length) return;
    remember();
    setSheets((prev) => [...prev, ...made]);
    setActiveId(made[0].id);
    setSelection([]);
    bump();
  }, [bump, missingHints, remember]);

  /**
   * The destination as one string, for comparing one against another.
   *
   * A model, not a folder path, is what decides which sheets a paint is made of — but a paint
   * aimed at a folder of the player's own has no model behind it and no hints either, so the
   * two share a key and the empty answer takes care of the difference.
   */
  const destKey = useMemo(
    () => (dest ? (dest.kind === "mods" ? dest.rel : dest.path) : ""),
    [dest],
  );
  destKeyRef.current = destKey;

  /**
   * Whether there is anything here worth not throwing away.
   *
   * "Blank sheets with the right names" is the state this editor now opens in, and replacing
   * those costs nothing — they are a fact about the model, not somebody's work. A base or a
   * layer is the first thing that isn't.
   */
  const pristine = useMemo(() => sheets.every((s) => !s.base && !s.layers.length), [sheets]);

  /**
   * Keep the sheet list on the model that's chosen.
   *
   * The names are the entire binding — a sheet called anything else paints nothing — and until
   * now they were on screen as a hint line with a button beside it, which made the first move
   * in every session the same move. So the sheets a model wants are simply *there* when it is
   * picked, and they follow when it is changed.
   *
   * Two refs, not one. `filled` is which destination the sheets on screen belong to, and stops
   * this from refilling a list the user has since emptied on purpose. `warned` is which one has
   * already been asked about, and stops the warning below from being raised again by the next
   * unrelated render — this effect follows the sheets, so there are many.
   */
  useEffect(() => {
    // Not until the hints are about the destination that's actually chosen: they're fetched,
    // and acting on the last model's list would fill a KTM with a Yamaha's sheet names.
    if (!started) return;
    if (!destKey || hintsFor !== destKey || filled.current === destKey) return;
    const wanted = hints.filter((h) => !isCompanionMap(h));
    if (!wanted.length) return;
    const make = () => wanted.map((h) => blankSheet(h, BLANK_SIZE));
    if (pristine) {
      const switching = sheets.length > 0;
      warned.current = destKey;
      installSheets(make());
      // Said out loud when it replaces something, silent when it fills an empty editor. A
      // list that changed under you is worth a line; one that arrived is what was asked for.
      if (switching) toast.info(t("designer.sheetsSwitched", { dest: destKey }));
      return;
    }
    if (warned.current === destKey) return;
    warned.current = destKey;
    // Asked rather than done. Everything on screen would go, and a model picker is not a
    // control anybody expects to lose an afternoon's drawing to.
    toast.warning(t("designer.switchSheetsTitle"), {
      description: t("designer.switchSheetsBody", { dest: destKey, names: wanted.join(", ") }),
      action: {
        label: t("designer.switchSheets"),
        onClick: () => installSheets(make()),
      },
    });
  }, [destKey, hints, hintsFor, installSheets, pristine, sheets.length, started, t]);

  const addImage = useCallback(async () => {
    if (!active) return;
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Images", extensions: IMAGE_EXTS }],
    });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (!paths.length) return;
    setBusy(true);
    try {
      const loaded = await Promise.all(paths.map((p) => readImage(p)));
      const added = loaded.map(({ name: layerName, bitmap }) =>
        imageLayer(layerName, bitmap, active),
      );
      patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, ...added] }));
      setSelection(added.length ? [added[added.length - 1].id] : []);
      // Straight into the pointer, with the new layer selected: what you do with an image you
      // have just placed is move it, and any paint tool would have painted over it instead.
      setPaint((p) => ({ ...p, tool: "move" }));
      bump();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [active, bump, patchSheet, readImage]);

  const addText = useCallback(() => {
    if (!active) return;
    const layer = textLayer(t("designer.newTextValue"), active);
    patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, layer] }));
    setSelection([layer.id]);
    setPaint((p) => ({ ...p, tool: "move" }));
    bump();
  }, [active, bump, patchSheet, t]);

  const removeLayers = useCallback(
    (ids: string[]) => {
      if (!activeId || !ids.length) return;
      const gone = new Set(ids);
      patchSheet(activeId, (s) => ({ ...s, layers: s.layers.filter((l) => !gone.has(l.id)) }));
      setSelection((cur) => cur.filter((id) => !gone.has(id)));
      bump();
    },
    [activeId, bump, patchSheet],
  );

  /**
   * Apply a change to every selected layer.
   *
   * Followers need no special case here, which is worth saying because it looks like they
   * should. `patchSheet` re-derives them on the way out, so a change to something a follower
   * takes from its source is simply put back — that *is* the lock, and it costs nothing.
   * What a follower owns for itself, its name and its group, sticks.
   */
  const patchSelection = useCallback(
    (fn: (l: Layer) => Layer, undoKey: string | null | false = null) => {
      if (!activeId || !selection.length) return;
      const ids = new Set(selection);
      patchSheet(
        activeId,
        (s) => ({ ...s, layers: s.layers.map((l) => (ids.has(l.id) ? fn(l) : l)) }),
        undoKey,
      );
      bump();
    },
    [activeId, bump, patchSheet, selection],
  );

  const moveSelection = useCallback(
    (dx: number, dy: number) => {
      if (!dx && !dy) return;
      // One undo step per run of the same gesture, however many samples it took — see
      // `PaintHistory.pushDoc`. Keyed on what is moving, so picking up a different layer
      // starts a new step rather than folding into the last one.
      patchSelection(
        (l) => (l.kind === "paint" ? l : { ...l, x: l.x + dx, y: l.y + dy }),
        `move:${selection.join(",")}`,
      );
    },
    [patchSelection, selection],
  );

  /** A corner drag, as where each dragged layer ends up. Clamped by the stage to the range. */
  const scaleSelection = useCallback(
    (next: { id: string; x: number; y: number; scale: number }[]) => {
      if (!activeId || !next.length) return;
      const by = new Map(next.map((n) => [n.id, n]));
      patchSheet(
        activeId,
        (s) => ({
          ...s,
          layers: s.layers.map((l) => {
            const to = by.get(l.id);
            return to ? { ...l, x: to.x, y: to.y, scale: to.scale } : l;
          }),
        }),
        `scale:${next.map((n) => n.id).join(",")}`,
      );
      bump();
    },
    [activeId, bump, patchSheet],
  );

  /* ── The reference underlay ────────────────────────────────────────────────────────────
     None of this touches a sheet, with one exception: turning tracing on *moves* the template
     out of `Sheet.base`, which is a real edit to what would be saved and is meant to be — it
     is the whole difference between drawing over a paint and drawing from one. ───────── */

  /**
   * The model's bodywork for the active sheet.
   *
   * Cheap — no rasterising, just a walk over the mesh — so it is derived rather than cached
   * behind a toggle the way the wireframe is. Clipping and fitting need it whether or not
   * anyone has asked to *see* the islands, and keying it on the name rather than the sheet
   * keeps a brush stroke, which replaces the sheet object, from recomputing it.
   */
  const activeName = active?.name ?? "";
  // The rest of what the UV map is built from, pulled out for the same reason: a stroke
  // replaces the sheet object without changing any of these.
  const activeWidth = active?.width ?? 0;
  const activeHeight = active?.height ?? 0;
  // Left and right are asked for on bikes only: a bike arrives assembled about its mirror
  // plane, where gear is a single piece whose up-axis the viewer has to work out per mod.
  // And only when it *did* arrive that way — a bike that loaded without its `.geom` is a heap
  // of parts in their own frames, and a side named from that is worse than no side at all.
  const bike = isBikeKind(destState.kind);
  // The island the pointer is over, for the 3D view to light up. Held here rather than in the
  // stage because it crosses from the 2D half of the editor to the 3D one, and this is the
  // only thing that owns both.
  const [hoverIsland, setHoverIsland] = useState<Int32Array | null>(null);
  const parts = useMemo<UvPart[]>(
    () => (geometry ? uvParts(geometry, activeName, { assembled: bike && assembled }) : []),
    [geometry, activeName, bike, assembled],
  );

  const startPaint = useCallback(
    (at: Point, whole: boolean) => {
      // A rectangle, ellipse or line becomes a layer, not pixels. Made on the press and
      // rewritten as the drag goes, so the thing being dragged out *is* the finished object —
      // there is no separate preview to disagree with the result, and on release it is
      // already selected with handles on it.
      if (SHAPE_TOOLS.has(paint.tool) && active) {
        const shape = paint.tool as ShapeLayer["shape"];
        const layer = shapeLayer(
          t(`designer.tool.${shape}`),
          shape,
          at,
          at,
          paint.shape,
          paint.colorA,
          paint.strokeWidth,
        );
        remember();
        patchSheet(active.id, (sh) => ({ ...sh, layers: [...sh.layers, layer] }));
        setSelection([layer.id]);
        shaping.current = { id: layer.id, from: at };
        bump();
        return;
      }
      if (!target || !activeId) return;
      // The bucket fills the uv triangle under the press, not the sheet and not the whole mesh
      // group. Worked out here rather than inside `Stroke`, because the parts are the editor's
      // knowledge of the model and paint.ts is deliberately ignorant of it — it puts pixels
      // down, wherever it is told to.
      //
      // The group is only the first cut: `shroud` is both flanks and often several islands, so
      // stopping there floods panels the press never pointed at. Left button takes the one
      // triangle under the pointer; right button takes the island it belongs to.
      let fillTo: { path: Path2D; box: Region } | null = null;
      if (paint.tool === "fill" && active && parts.length) {
        const u = at.x / active.width;
        const v = at.y / active.height;
        const under = partAt(parts, u, v);
        const pick = whole ? islandAt : triangleAt;
        const region = under ? (pick(under, u, v) ?? under) : null;
        if (region) {
          fillTo = {
            path: partPath(region, active.width, active.height),
            box: partBox(region, active.width, active.height),
          };
        }
      }
      const next = new Stroke(target.canvas, paint, at, fillTo);
      stroke.current = next;
      // Straight through rather than queued: the press is the one sample nobody would forgive a
      // frame's wait on, and a tool that puts nothing down on the press has nothing to show.
      if (next.dirty) touchPaint(activeId, target.id, next.dirty);
    },
    [active, activeId, bump, paint, parts, patchSheet, remember, t, target, touchPaint],
  );

  /** Pin the selection to a piece of bodywork, or let it cover the sheet again. */
  const clipLayer = useCallback(
    (label: string | null) => {
      if (!active) return;
      const part = label ? parts.find((p) => p.label === label) : null;
      patchSelection((l) => ({
        ...l,
        // Built here, at this sheet's size, so the composite never has to. Re-picking the
        // part is what rebuilds it, which is also the answer to a resized sheet.
        clip: part
          ? { label: part.label, path: partPath(part, active.width, active.height) }
          : null,
      }));
    },
    [active, parts, patchSelection],
  );

  /**
   * Place and scale the selected layer to cover a part.
   *
   * Cover, not contain: a photo meant for a shroud should reach every edge of it, and the
   * clip is what trims the overspill. Contain would leave the sheet showing through at two
   * sides of anything whose shape didn't happen to match the panel's.
   */
  const fitLayer = useCallback(
    (label: string) => {
      const layer = active?.layers.find((l) => l.id === selectedId);
      const part = parts.find((p) => p.label === label);
      if (!active || !layer || !part || layer.kind === "paint") return;
      const bw = (part.maxU - part.minU) * active.width;
      const bh = (part.maxV - part.minV) * active.height;
      const { w, h } = layerExtent(layer);
      if (!w || !h || !bw || !bh) return;
      const scale = Math.min(4, Math.max(0.05, Math.max(bw / w, bh / h)));
      patchLayer(layer.id, (l) => ({
        ...l,
        x: (part.minU + part.maxU) * 0.5 * active.width,
        y: (part.minV + part.maxV) * 0.5 * active.height,
        scale,
      }));
      bump();
    },
    [active, bump, parts, patchLayer, selectedId],
  );

  /**
   * Hand the mirror the model it should answer from, and drop the index built from the last one.
   *
   * Dropped rather than rebuilt. Most sheets are never mirrored on, and the rebuild is a walk
   * over the whole of the bodywork — `ensureMirror` does it the moment something actually asks.
   *
   * During the render rather than in an effect, the same way `sheetsRef` is kept: the button
   * that offers a mirror is rendered from `ready`, and an effect would leave it a render behind
   * the model — disabled, with nothing on screen to explain why.
   */
  if (mirrorRef.current.sheetId !== activeId || mirrorRef.current.parts !== parts) {
    mirrorRef.current = {
      sheetId: activeId,
      parts,
      // The flank codes are the model's own statement that its axes mean something; `uvParts`
      // only produces them for a bike that arrived assembled.
      ready: parts.some((p) => p.flanks),
      index: null,
    };
  }

  /* ── The selection, and what can be done to it ─────────────────────────────────────────
     A group is a tag on its members rather than a container (see `layers.ts`), so "select the
     group" is a question asked of the layer list right here — everything downstream, the stage
     and the inspector both, only ever sees a list of ids. ─────────────────────────────── */

  const select = useCallback(
    (ids: string[], mode: "replace" | "toggle" | "isolate") => {
      const layers = active?.layers ?? [];
      // Alt reaches inside a group. Anything else takes the whole block a layer belongs to,
      // which is the entire point of having grouped it.
      const want =
        mode === "isolate" ? ids : [...new Set(ids.flatMap((id) => groupOf(layers, id)))];
      setSelection((cur) => {
        if (mode !== "toggle") return want;
        const next = new Set(cur);
        // A group toggles as a block: if any of it is out, all of it comes in.
        const add = want.some((id) => !next.has(id));
        for (const id of want) {
          if (add) next.add(id);
          else next.delete(id);
        }
        // Kept in stacking order, so anything reading the selection reads it bottom-first.
        return layers.filter((l) => next.has(l.id)).map((l) => l.id);
      });
    },
    [active],
  );

  /** How far a duplicate lands from what it was copied from, as a fraction of the sheet. */
  const offset = active ? Math.max(4, Math.round(Math.min(active.width, active.height) * 0.02)) : 0;

  const duplicateSelection = useCallback(() => {
    if (!active || !chosen.length) return;
    // A duplicate of several layers is a group of its own, or the first drag takes it apart.
    const tag = chosen.length > 1 ? newId("group") : null;
    const copies = chosen.map((l) => ({
      ...cloneLayer(l, t("designer.copyName", { name: l.name })),
      group: tag,
      // Down and to the right on screen — the sheet's y runs the other way. A copy landing
      // exactly on its original looks like nothing happened.
      x: l.x + offset,
      y: l.y - offset,
    }));
    patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, ...copies] }));
    setSelection(copies.map((c) => c.id));
    bump();
  }, [active, bump, chosen, offset, patchSheet, t]);

  const copySelection = useCallback(() => {
    if (!active || !chosen.length) return;
    // Cloned on the way in rather than on the way out: the layers left behind go on being
    // edited, and a clipboard holding the live ones would paste whatever they had become.
    clipboard = {
      width: active.width,
      height: active.height,
      layers: chosen.map((l) => cloneLayer(l, l.name)),
    };
    toast.success(t("designer.copied", { count: String(chosen.length) }));
  }, [active, chosen, t]);

  const pasteClipboard = useCallback(() => {
    if (!active || !clipboard?.layers.length) return;
    const kx = active.width / clipboard.width;
    const ky = active.height / clipboard.height;
    const sized = kx === 1 && ky === 1;
    // A paint layer is the sheet, and its raster is the size of the sheet it was cut from.
    // Onto a sheet of another size there is nothing sensible to do with it.
    const keep = sized ? clipboard.layers : clipboard.layers.filter((l) => l.kind !== "paint");
    const dropped = clipboard.layers.length - keep.length;
    if (!keep.length) {
      toast.error(t("designer.pasteWrongSize"));
      return;
    }
    const tag = keep.length > 1 ? newId("group") : null;
    const copies = keep.map((l) => ({
      ...cloneLayer(l, l.name),
      group: tag,
      // Positions are in sheet pixels, so a decal copied off a 2048² sheet has to be brought
      // with it or half of what was copied lands off the edge of a 1024² one.
      x: l.x * kx,
      y: l.y * ky,
      scale: l.scale * Math.min(kx, ky),
    }));
    patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, ...copies] }));
    setSelection(copies.map((c) => c.id));
    if (dropped) toast.warning(t("designer.pasteDropped", { count: String(dropped) }));
    bump();
  }, [active, bump, patchSheet, t]);

  const groupSelection = useCallback(() => {
    if (!active || chosen.length < 2) return;
    const tag = newId("group");
    const ids = new Set(chosen.map((l) => l.id));
    patchSheet(active.id, (s) => {
      const tagged = s.layers.map((l) => (ids.has(l.id) ? { ...l, group: tag } : l));
      // Gathered as well as tagged, so raising the group later takes it past what it sits on
      // rather than past its own members — see `regroup`.
      return { ...s, layers: regroup(tagged, tag) };
    });
    bump();
  }, [active, bump, chosen, patchSheet]);

  const ungroupSelection = useCallback(() => {
    const tags = new Set(chosen.map((l) => l.group).filter((g): g is string => !!g));
    if (!tags.size) return;
    patchSelection((l) => (l.group && tags.has(l.group) ? { ...l, group: null } : l));
  }, [chosen, patchSelection]);

  const unlinkSelection = useCallback(() => {
    patchSelection((l) => (l.mirror ? { ...l, mirror: null } : l));
  }, [patchSelection]);

  /**
   * Put a copy of the selected layer where it lands on the far flank, and keep it there.
   *
   * The copy is appended with no placement worked out here on purpose: `patchSheet` derives
   * every follower on the way through, so the same code that puts it down is the code that
   * will keep it in step afterwards. Asking first is only to have something to say when the
   * answer is no.
   */
  const mirrorSelected = useCallback(() => {
    const layer = chosen.length === 1 ? chosen[0] : null;
    if (!active || !layer) return;
    if (layer.kind === "paint") {
      toast.error(t("designer.fitNotForPaint"));
      return;
    }
    const result = mirrorLayer(ensureMirror(), mirrorRef.current.parts, layer, active);
    if (!result.ok) {
      toast.error(t(`designer.mirrorWhy.${result.why}` as "designer.mirrorWhy.no-model"));
      return;
    }
    const follower: Layer = {
      ...cloneLayer(layer, t("designer.mirrorName", { name: layer.name })),
      mirror: { of: layer.id },
    };
    patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, follower] }));
    setSelection([follower.id]);
    if (result.approximate) toast.warning(t("designer.mirrorRough"));
    bump();
  }, [active, bump, chosen, ensureMirror, patchSheet, t]);

  /**
   * Tool keys, and undo.
   *
   * On the window rather than on the canvas, because a brush should be one key away wherever
   * the focus happens to be — but the Studio keeps this pane mounted behind whichever tab is
   * open, so an invisible Designer would otherwise steal every `b` typed into Paint Studio.
   */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!rootRef.current?.offsetParent) return;
      // A modal over the editor — the fullscreen preview — owns the keyboard: a tool letter
      // pressed while looking at the model would swap a brush nobody can see.
      if (document.querySelector('[role="dialog"][data-state="open"]')) return;
      const el = e.target as HTMLElement | null;
      if (el && (/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) || el.isContentEditable)) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
        return;
      }
      const key = e.key.toLowerCase();
      // Everything on a modifier, because every unmodified letter is already a tool.
      if (e.metaKey || e.ctrlKey) {
        const run: Record<string, (() => void) | undefined> = {
          c: copySelection,
          v: pasteClipboard,
          d: duplicateSelection,
          g: e.shiftKey ? ungroupSelection : groupSelection,
          a: () => setSelection((sheetsRef.current.find((s) => s.id === activeId)?.layers ?? []).map((l) => l.id)),
        };
        const act = run[key];
        if (act) {
          e.preventDefault();
          act();
        }
        return;
      }
      if (e.altKey) return;

      if ((e.key === "Delete" || e.key === "Backspace") && selection.length) {
        e.preventDefault();
        removeLayers(selection);
        return;
      }

      const step = e.shiftKey ? 10 : 1;
      // Up on the keyboard is up on the *picture*. The sheet's rows run the other way (see
      // `CanvasStage`), so the arrow that agrees with what's on screen is the one that
      // disagrees with the array, and this is where that gets turned round.
      const arrows: Record<string, [number, number] | undefined> = {
        ArrowLeft: [-step, 0],
        ArrowRight: [step, 0],
        ArrowUp: [0, step],
        ArrowDown: [0, -step],
      };
      const nudge = arrows[e.key];
      if (nudge && selection.length) {
        e.preventDefault();
        moveSelection(nudge[0], nudge[1]);
        return;
      }

      const tool = (Object.keys(TOOL_KEYS) as PaintTool[]).find((k) => TOOL_KEYS[k] === key);
      if (tool) {
        e.preventDefault();
        pickTool(tool);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    activeId,
    copySelection,
    duplicateSelection,
    groupSelection,
    moveSelection,
    pasteClipboard,
    pickTool,
    redo,
    removeLayers,
    selection,
    undo,
    ungroupSelection,
  ]);

  const ghostOf = useCallback(
    (id: string | null | undefined) => (id && ghosts.get(id)) || EMPTY_GHOST,
    [ghosts],
  );

  const patchGhost = useCallback((id: string, fn: (g: Ghost) => Ghost) => {
    setGhosts((prev) => {
      const next = new Map(prev);
      next.set(id, fn(prev.get(id) ?? EMPTY_GHOST));
      return next;
    });
  }, []);

  /**
   * Take the model the preview loaded, and drop every wireframe built from the last one.
   *
   * Switching bikes keeps the sheet names, so without this a `livery` map rasterised from the
   * previous model would look perfectly valid over the new one while describing bodywork that
   * isn't there — the worst kind of wrong for a guide.
   */
  const onGeometry = useCallback((nodes: EdfNode[] | null, assembled: boolean) => {
    // Compared against a ref rather than inside a `setState` updater: an updater has to be
    // pure, and this has to invalidate the wires as well as record the mesh.
    if (geometryRef.current === nodes) return;
    geometryRef.current = nodes;
    setGeometry(nodes);
    setAssembled(assembled);
    setGhosts((gs) =>
      gs.size ? new Map([...gs].map(([id, g]) => [id, { ...g, wire: null, wireFor: null }])) : gs,
    );
  }, []);

  /** The same, for the model's own textures: a new bike, a new set of stock sheets. */
  const stockRef = useRef<PaintTexture[]>([]);
  const onStock = useCallback((textures: PaintTexture[]) => {
    if (stockRef.current === textures) return;
    stockRef.current = textures;
    setStockTextures(textures);
    setGhosts((gs) =>
      gs.size ? new Map([...gs].map(([id, g]) => [id, { ...g, stock: null, stockFor: null }])) : gs,
    );
  }, []);

  /**
   * Move the template between the sheet and the ghost.
   *
   * Moved, never copied. A template that stayed as `base` while also showing as a ghost would
   * be saved into the paint, which is the thing somebody asking to trace is trying to avoid;
   * and keeping the bitmap on the other side is what lets this be undone by pressing it again.
   */
  /**
   * Build the active sheet's UV map, once the user has asked for one.
   *
   * Lazily, and keyed on the sheet's *name*, because the name is the entire binding — rename a
   * sheet from `livery` to `plate` and it describes different triangles. Rasterising eagerly
   * would spend the work on sheets nobody looks at, and rasterising on every render would spend
   * it again on every brush stroke, since a stroke replaces the sheet object.
   *
   * Which is also why this takes the sheet apart rather than depending on it: every field it
   * reads survives a stroke untouched, so following the sheet itself would re-run this — and
   * re-check a guard that has already been satisfied — once per pointer sample of every drag.
   */
  useEffect(() => {
    if (!activeId || !geometry) return;
    // A half-typed name is not a name yet. Without this, every keystroke of "livery" would be
    // asked of the mesh and answered "nothing binds that", which is true and useless.
    if (!activeName.trim()) return;
    const ghost = ghosts.get(activeId) ?? EMPTY_GHOST;
    if (!ghost.showWire || ghost.wireFor === activeName) return;
    // `wireFor` records the attempt whether or not it found anything, so a name that matches
    // nothing is asked once rather than on every render. The panel reads the pair to say so —
    // out loud, because an empty overlay is indistinguishable from one still being built.
    const wire = uvWireframe(parts, activeWidth, activeHeight);
    patchGhost(activeId, (g) => ({ ...g, wire, wireFor: activeName }));
  }, [activeId, activeName, activeWidth, activeHeight, geometry, ghosts, parts, patchGhost]);

  /**
   * The model's own texture for a sheet name, or null for a name it carries none of.
   *
   * The name is the whole binding here exactly as it is for the triangles — call a sheet
   * `plastics` and it is the plastics, call it anything else and the model has nothing of its
   * own to answer with.
   */
  const stockFor = useCallback(
    (name: string): PaintTexture | null => {
      const want = name.trim().toLowerCase();
      return stockTextures.find((t) => t.name.trim().toLowerCase() === want) ?? null;
    },
    [stockTextures],
  );

  /**
   * Give a companion sheet something correct to start from.
   *
   * A color sheet starts blank because blank is where a livery starts. A companion map is the
   * opposite: a paint replaces the model's textures *by name*, so saving an empty `plastics_n`
   * does not add a normal map — it throws the bike's real one away and puts black in its
   * place, which decodes to a surface pointing back into itself. That is why these are kept
   * off the suggestion list, and it left anyone who did want to author one starting from the
   * worst possible sheet.
   *
   * So: the model's own map if it has one — the actual base, the vents and the seat weave
   * already in it, ready to be drawn over. Failing that, and only for a normal map, flat
   * (128, 128, 255), which is the one companion with an honest neutral value. Other companions
   * are left alone rather than given a constant that would be a claim about the material.
   *
   * Only ever onto a pristine sheet, and once per name, so this cannot land on someone's work
   * or fight a template they loaded themselves.
   */
  const seeded = useRef(new Set<string>());
  useEffect(() => {
    const sheet = sheets.find(
      (s) =>
        !s.base &&
        !s.layers.length &&
        isCompanionMap(s.name) &&
        !seeded.current.has(`${s.id}:${s.name.trim().toLowerCase()}`),
    );
    if (!sheet) return;
    const key = `${sheet.id}:${sheet.name.trim().toLowerCase()}`;
    seeded.current.add(key);
    let alive = true;
    void (async () => {
      const stock = stockFor(sheet.name);
      const base = stock
        ? await stockBitmap(stock)
        : isNormalMap(sheet.name)
          ? await flatNormalBitmap(sheet.width)
          : null;
      if (!alive || !base) return;
      patchSheet(
        sheet.id,
        (sh) =>
          sh.base || sh.layers.length
            ? sh
            : { ...sh, base, width: base.width, height: base.height },
        false,
      );
      bump();
    })();
    return () => {
      alive = false;
    };
  }, [bump, patchSheet, sheets, stockFor]);

  /**
   * Fetch the model's own texture for the active sheet, the same way and for the same reasons.
   *
   * Lazy and keyed on the name again — the name is what picks the texture out of the model,
   * exactly as it picks the triangles — but this one crosses to the backend for pixels, so a
   * 2048² sheet is a few megabytes over IPC. Only the sheet on screen pays for it, and only
   * once: a paint with twenty sheets fetches the one being looked at.
   */
  const stockFetch = useRef<string | null>(null);
  useEffect(() => {
    if (!activeId || !activeName.trim() || !stockTextures.length) return;
    const ghost = ghosts.get(activeId) ?? EMPTY_GHOST;
    if (!ghost.showStock || ghost.stockFor === activeName) return;
    // One request in the air per sheet-and-name. This effect re-runs on every change to any
    // ghost — the opacity slider alone is dozens — and without this each of those would put
    // another copy of the same megabyte read on the wire while the first was still coming.
    const key = `${activeId}:${activeName}`;
    if (stockFetch.current === key) return;
    stockFetch.current = key;
    const tex = stockFor(activeName);
    void (async () => {
      const stock = tex ? await stockBitmap(tex) : null;
      // Written against the sheet it was asked *for*, not whichever is active now, and both
      // halves land together. So the worst a slow answer can do is describe a name the sheet
      // no longer has — which the guard above then notices, and asks again.
      patchGhost(activeId, (g) => ({ ...g, stock, stockFor: activeName }));
      if (stockFetch.current === key) stockFetch.current = null;
    })();
  }, [activeId, activeName, ghosts, patchGhost, stockFor, stockTextures]);

  /**
   * Paint the model's own texture into the sheet, at full strength.
   *
   * The second thing here that touches a sheet, and the opposite of tracing: the stock plastics
   * stop being something to draw *against* and become what is drawn. That is the whole of what
   * most people are after — the bike as it ships, with their number on it — and until now the
   * only route to it was to find the artwork outside the app, because a reference at 35% is
   * deliberately not it. So this one is saved, and it goes through the history like any other
   * edit to the sheet.
   *
   * The reference's copy is used when there is one, because it is the same texture read from
   * the same store; the fetch is for the sheet whose ghost is switched off, since the button
   * has to work without asking anyone to turn a guide on first.
   */
  /** Take the base back out. `stockAsBase` had no counterpart, so it was a one-way door. */
  const clearBase = useCallback(
    (sheetId: string) => {
      remember();
      patchSheet(sheetId, (sheet) => ({ ...sheet, base: null }));
      bump();
    },
    [bump, patchSheet, remember],
  );

  /**
   * Add the companion maps, seeded rather than blank.
   *
   * These are kept out of the automatic fill for a good reason: a `.pnt` replaces the model's
   * textures by name, so shipping an empty `plastics_n` throws away the bike's real normal
   * map. Offered separately, and never empty — each one starts as the model's own texture, or
   * for a normal map with no stock to copy, as the flat value that says "exactly as the mesh
   * says". Both are a correct sheet; a blank one is a hole in the bike.
   */
  const addCompanionSheets = useCallback(async () => {
    if (!missingCompanions.length) return;
    setBusy(true);
    try {
      const made: Sheet[] = [];
      for (const wanted of missingCompanions) {
        const tex = stockFor(wanted);
        const base = tex ? await stockBitmap(tex) : null;
        const size = tex?.width ?? BLANK_SIZE;
        const sheet = blankSheet(wanted, size);
        made.push(
          base
            ? { ...sheet, width: tex?.width ?? size, height: tex?.height ?? size, base }
            : { ...sheet, base: await flatBitmap(FLAT_NORMAL, size) },
        );
      }
      remember();
      setSheets((prev) => [...prev, ...made]);
      setActiveId(made[0].id);
      setSelection([]);
      bump();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [bump, missingCompanions, remember, stockFor]);

  const toggleTrace = useCallback(
    (sheetId: string) => {
      const sheet = sheets.find((s) => s.id === sheetId);
      if (!sheet) return;
      const ghost = ghostOf(sheetId);
      // Not recorded by the history, either way: the bitmap moves between the sheet and the
      // ghost, and the ghost isn't part of the document — an undo would put the template back
      // on the sheet while the ghost still held it, which is the copy this is careful not to
      // make. Pressing the button again is the way back, as it always was.
      if (sheet.base) {
        const template = sheet.base;
        patchSheet(sheetId, (s) => ({ ...s, base: null }), false);
        patchGhost(sheetId, (g) => ({ ...g, template, showTemplate: true }));
      } else if (ghost.template) {
        const base = ghost.template;
        patchSheet(sheetId, (s) => ({ ...s, base }), false);
        patchGhost(sheetId, (g) => ({ ...g, template: null }));
      }
      bump();
    },
    [bump, ghostOf, patchGhost, patchSheet, sheets],
  );

  const stockAsBase = useCallback(
    async (sheetId: string) => {
      const sheet = sheetsRef.current.find((s) => s.id === sheetId);
      if (!sheet) return;
      const tex = stockFor(sheet.name);
      if (!tex) {
        toast.error(t("designer.stockNoMatch", { name: sheet.name.trim() }));
        return;
      }
      remember();
      const held = ghostOf(sheetId);
      const ready = held.stockFor === sheet.name ? held.stock : null;
      setBusy(true);
      const base = ready ?? (await stockBitmap(tex));
      setBusy(false);
      if (!base) {
        toast.error(t("designer.stockReadFailed", { name: sheet.name.trim() }));
        return;
      }
      patchSheet(sheetId, (s) => ({
        ...s,
        // The texture's own size, but only for a sheet nothing has been done to yet: a stock
        // texture stretched onto a 2048 blank is the bike's artwork resampled for no reason,
        // and resizing a sheet already drawn on would take every layer's placement with it.
        ...(s.base || s.layers.length ? {} : { width: tex.width, height: tex.height }),
        base,
      }));
      // The reference is drawn underneath the sheet, and the sheet is now the same picture —
      // leaving it on would be showing the stock texture through the stock texture.
      patchGhost(sheetId, (g) => ({ ...g, showStock: false }));
      bump();
      toast.success(t("designer.stockAsBaseDone", { name: sheet.name.trim() }));
    },
    [bump, ghostOf, patchGhost, patchSheet, remember, stockFor, t],
  );

  // Ghosts of sheets that are gone. Each holds a decoded bitmap and a raster the size of the
  // sheet, so leaving them behind would keep a closed paint's pixels alive for the session.
  useEffect(() => {
    setGhosts((prev) => {
      if (!prev.size) return prev;
      const live = new Set(sheets.map((s) => s.id));
      if ([...prev.keys()].every((id) => live.has(id))) return prev;
      return new Map([...prev].filter(([id]) => live.has(id)));
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sheetIdKey]);

  /** Reorder within the stack. `delta` of -1 is one step down (further back). */
  /**
   * Drop `id` where `before` sits in the stack.
   *
   * The list is drawn top-of-stack first while the array is bottom-first, so both indices are
   * read from the array and the reversal in the panel takes care of itself.
   */
  const moveLayer = useCallback(
    (id: string, before: string) => {
      if (!activeId) return;
      remember(`layer-order:${id}`);
      patchSheet(activeId, (s) => {
        const from = s.layers.findIndex((l) => l.id === id);
        const to = s.layers.findIndex((l) => l.id === before);
        if (from < 0 || to < 0 || from === to) return s;
        const layers = [...s.layers];
        const [moved] = layers.splice(from, 1);
        layers.splice(to, 0, moved);
        return { ...s, layers };
      });
      bump();
    },
    [activeId, bump, patchSheet, remember],
  );

  /**
   * Drop `id` where `before` sits.
   *
   * The list order is the order `write` packs them in, so this is not cosmetic. It replaced a
   * pair of arrows on every row — six controls to move a list of five things, each disabled
   * at one end — with the gesture people already try first.
   */
  const moveSheet = useCallback(
    (id: string, before: string) => {
      remember(`sheet-order:${id}`);
      setSheets((prev) => {
        const from = prev.findIndex((s) => s.id === id);
        const to = prev.findIndex((s) => s.id === before);
        if (from < 0 || to < 0 || from === to) return prev;
        const next = [...prev];
        const [moved] = next.splice(from, 1);
        next.splice(to, 0, moved);
        return next;
      });
    },
    [remember],
  );

  const removeSheet = useCallback(
    (id: string) => {
      remember();
      setSheets((prev) => {
        const next = prev.filter((s) => s.id !== id);
        setActiveId((cur) => (cur === id ? next[0]?.id ?? null : cur));
        return next;
      });
      bump();
    },
    [bump, remember],
  );

  /** Why saving isn't possible yet, as a translated message — or null when it is, and on an
   *  empty canvas, where there is nothing to fix yet and Save is simply off. */
  const blocked = useMemo<string | null>(() => {
    if (!sheets.length) return null;
    if (!name.trim()) return t("paints.needName");
    if (sheets.some((s) => !s.name.trim())) return t("paints.needTextureNames");
    const seen = new Set<string>();
    for (const s of sheets) {
      const key = s.name.trim().toLowerCase();
      if (seen.has(key)) return t("paints.duplicateName", { name: s.name.trim() });
      seen.add(key);
    }
    if (!dest) return t("paints.needTarget");
    return null;
  }, [sheets, name, dest, t]);


  const write = useCallback(
    async (overwrite: boolean, as?: string) => {
      if (!dest) return;
      const title = (as ?? name).trim();
      setBusy(true);
      try {
        // Composite every sheet first: they only exist as canvases until now, and doing it on
        // the way out is what stops a save shipping a frame older than the screen.
        //
        // Then drop the ones with nothing on them. A `.pnt` replaces the model's textures by
        // name, so an empty sheet doesn't add a blank — it *removes* whatever the bike had
        // there. Offering to create every missing sheet at once made that easy to do by
        // accident: create twenty, paint two, and the other eighteen would have wiped the
        // bike's normal and roughness maps.
        const inked = sheets.filter((sheet) => {
          const canvas = canvasFor(sheet);
          composite(canvas, sheet);
          return hasInk(canvas);
        });
        const blank = sheets.length - inked.length;
        if (!inked.length) {
          toast.error(t("designer.nothingToSave"));
          return;
        }
        const staged = await Promise.all(
          inked.map(async (sheet) => {
            const path = await paintStudioStage(sheet.name.trim(), await toPng(canvasFor(sheet)));
            return { path, name: sheet.name.trim() };
          }),
        );
        const outcome = await paintStudioSave({
          name: title,
          fileName: title,
          textures: staged,
          dest,
          overwrite,
        });
        // Said out loud, not silently: a sheet that doesn't get written is a sheet that
        // won't be in the file, and finding that out later is worse than reading it now.
        toast.success(t("paints.saved", { path: outcome.path }), {
          description: blank ? t("designer.blankSheetsSkipped", { count: blank }) : undefined,
        });
        setUnsaved(false);
        // Best effort: failing to note a recent must never look like a failed save.
        void designerRecentNote({
          path: outcome.path,
          name: title,
          kind: t(destState.kind.label),
          model: destState.folder ?? destState.model,
          savedAt: Math.round(Date.now() / 1000),
        }).catch(() => {});
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setBusy(false);
      }
    },
    [canvasFor, dest, destState, name, sheets, t],
  );

  const save = useCallback(
    async (as?: string) => {
    const title = (as ?? name).trim();
    // `blocked` covers the name too, so a save started from the dialog has to be judged
    // against the name being handed in rather than the one in state.
    const stop = as ? blocked && blocked !== t("paints.needName") : blocked;
    if (!sheets.length || !dest || !title) {
      if (stop) toast.error(stop);
      return;
    }
    if (stop) {
      toast.error(stop);
      return;
    }
    try {
      const target = await paintStudioTarget(title, dest);
      // Overwriting is the normal case here — you save, look, adjust, save again — so this
      // asks with a toast action rather than a modal that would interrupt that rhythm.
      if (target.exists) {
        toast.warning(t("paints.replaceTitle"), {
          description: t("paints.replaceBody", { path: target.path }),
          action: { label: t("paints.replace"), onClick: () => void write(true, title) },
        });
        return;
      }
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
      return;
    }
    await write(false, title);
    },
    [blocked, dest, name, sheets.length, t, write],
  );

  saveRef.current = async () => {
    if (!name.trim()) {
      setDraftName(name);
      saveAfterName.current = true;
      setNaming(true);
      return false;
    }
    await save();
    return !unsavedRef.current;
  };

  /**
   * The menu, routed to the same handlers the buttons use.
   *
   * Every item is a request rather than an action of its own: a menu entry and the control
   * beside the canvas end up in one place, so they cannot drift. Registered once — the
   * handlers are read off refs so this never has to re-subscribe.
   */
  const menuRef = useRef<Record<string, () => void>>({});
  menuRef.current = {
    "new-paint": () => setStarted(false),
    "open-paint": () => void startFromPaint(),
    "open-psd": () => void startFromPsd(),
    "add-sheet": addBlankSheet,
    "sheet-from-image": () => void addSheetFromImage(),
    save: () => void saveRef.current?.(),
    "export-psd": () => void exportPsd(),
    "toggle-model": () => togglePreview(),
  };
  useEffect(() => {
    const un = listen<string>("menu", (e) => menuRef.current[e.payload]?.());
    return () => {
      void un.then((f) => f());
    };
  }, []);

  // Whether the model can say where the far flank is at all, for the controls that need it.
  const mirrorReady = mirrorRef.current.ready && mirrorRef.current.sheetId === activeId;
  const canGroup = chosen.length > 1;
  const canUngroup = chosen.some((l) => l.group);
  const canUnlink = chosen.some((l) => l.mirror);

  if (!started) {
    return (
      <div ref={rootRef} className="relative min-h-0 flex-1 overflow-hidden">
        <StartScreen
          dest={destState}
          busy={busy}
          onBlank={beginNew}
          onFromPaint={() => void startFromPaint()}
          onFromPsd={() => void startFromPsd()}
          onOpenRecent={(r) => {
            setName(r.name);
            setStarted(true);
            void openPaint(r.path);
          }}
        />
      </div>
    );
  }

  return (
    <div ref={rootRef} className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
      {/* Something on screen while it works: unpacking a `.pnt` reads the file, inflates
          every sheet and writes them out, and before this the window simply sat there. */}
      {busy && (
        <Progress className="absolute inset-x-0 top-0 z-30 h-[2px] rounded-none bg-transparent" />
      )}
      {/* The decisions made once — where it goes, what it's called, save — go in the shell's
          own strip rather than a row of their own, so the sheet and the model get the whole
          window below it. */}
      <ContextBarLeft>
        <span className="flex min-w-0 items-baseline gap-2 pl-1 text-[12.5px] text-faint">
          <span className="flex-none font-medium">{t(destState.kind.label)}</span>
          <span className="min-w-0 truncate">{destState.folder ?? destState.model}</span>
        </span>
        {/* The paint's title, in the middle of the window and not in either group of
            controls — it names what is on screen rather than doing anything to it. It was a
            text box in the toolbar from the moment the tab opened: a question asked before
            there was anything to name. */}
        <div className="pointer-events-none absolute left-1/2 top-1/2 z-10 w-[240px] -translate-x-1/2 -translate-y-1/2 text-center">
          {naming ? (
            <Input
              ref={nameRef}
              autoFocus
              value={draftName}
              placeholder={t("paints.namePlaceholder")}
              className="pointer-events-auto h-8 text-center text-[13px]"
              onChange={(e) => setDraftName(e.target.value)}
              onBlur={() => {
                setNaming(false);
                if (draftName.trim()) setName(draftName.trim());
              }}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setNaming(false);
                  return;
                }
                if (e.key !== "Enter" || !draftName.trim()) return;
                const next = draftName.trim();
                setName(next);
                setNaming(false);
                // Enter from the field a save asked for finishes the save it interrupted.
                if (saveAfterName.current) {
                  saveAfterName.current = false;
                  void save(next);
                }
              }}
            />
          ) : (
            <button
              type="button"
              onClick={() => {
                setDraftName(name);
                setNaming(true);
              }}
              className="pointer-events-auto max-w-full cursor-default truncate rounded-md px-2 py-1 text-[13px] transition-colors hover:bg-foreground/[0.06]"
              title={t("paints.nameTitle")}
            >
              {name.trim() ? (
                <span className="font-medium text-foreground">{name.trim()}</span>
              ) : (
                <span className="text-faint">{t("paints.untitled")}</span>
              )}
            </button>
          )}
        </div>
        {/* The two ways out of the screen, pushed to the far end away from the setup that
            precedes them. Export sits beside Save rather than in a menu: it is what somebody
            finishing a job in Photoshop is looking for. */}
        <div className="ml-auto flex shrink-0 items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            className="border-border text-muted-foreground hover:text-foreground"
            disabled={busy || !sheets.length}
            title={t("designer.exportPsdHint")}
            onClick={() => void exportPsd()}
          >
            {t("designer.exportPsd")}
          </Button>
          <Button
            size="sm"
            disabled={busy || !sheets.length}
            title={blocked ?? undefined}
            onClick={() => {
              // An unnamed paint puts the cursor in the title instead of refusing to save.
              // Enter there finishes what this click started.
              if (!name.trim()) {
                setDraftName(name);
                saveAfterName.current = true;
                setNaming(true);
                return;
              }
              void save();
            }}
          >
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
            {t("paints.save")}
          </Button>
        </div>
      </ContextBarLeft>

      {/* No dialog: naming is a two-word edit, and dimming the app to collect it is
          heavier than the thing being collected. The title edits where it sits. */}

      {/* The sheet is the window; everything else floats on it.
          Three columns side by side is still a dashboard — the thing being worked on is
          boxed in by chrome on both sides and never gets the room. An editor puts the work
          underneath and the tools on top of it, so the canvas is the full width of the app
          and the panels are things you can put away. */}
      <div className="relative min-h-0 flex-1 overflow-hidden">
        {/* ── Sheets, and the tracing ghost under them ─────────────────────────── */}
        {(
          <div data-dock="left" className="absolute inset-y-0 left-0 z-10 flex w-[264px] flex-col gap-3 overflow-y-auto p-3">
          <SheetList
            sheets={sheets}
            activeId={activeId}
            hints={hints}
            onPick={(id) => {
              setActiveId(id);
              setSelection([]);
            }}
            onRename={(id, value) => {
              patchSheet(id, (s) => ({ ...s, name: value }));
              bump();
            }}
            onRemove={removeSheet}
            onMove={moveSheet}
            missingHints={missingHints}
            onAddBlank={addBlankSheet}
            onAddHintSheets={addHintSheets}
            missingCompanions={missingCompanions}
            onAddCompanions={() => void addCompanionSheets()}
            onAddFromImage={() => void addSheetFromImage()}
          />
          {active && (
            <LayerList
              layers={active.layers}
              selection={selection}
              onSelect={select}
              onToggle={(id, visible) => {
                patchLayer(id, (l) => ({ ...l, visible }));
                bump();
              }}
              onRemove={(id) => removeLayers([id])}
              onMove={moveLayer}
              onAdd={addPaintLayer}
            />
          )}
          </div>
        )}

        {/* ── The sheet, the whole window ──────────────────────────────────────── */}
          {/* Held to the sheet's own area: the cluster positions itself from the top right,
              and the panes container runs under the docks, so without this it would sit on
              top of the tools column. */}
          {active && (
            <div className="pointer-events-none absolute inset-y-0 left-[264px] right-[312px] z-10">
            <GhostPanel
              ghost={ghostOf(active.id)}
              isBike={isBikeKind(destState.kind)}
              sheetName={active.name}
              hasBase={!!active.base}
              hasGeometry={!!geometry}
              hasStock={!!stockTextures.length}
              stockSheet={!!stockFor(active.name)}
              busy={busy}
              onTrace={() => toggleTrace(active.id)}
              onStockBase={() => void stockAsBase(active.id)}
              onClearBase={() => clearBase(active.id)}
              onChange={(fn) => patchGhost(active.id, fn)}
            />
            </div>
          )}
          {active ? (
            <CanvasStage
              className="absolute inset-y-0 left-[264px] right-[312px] rounded-none border-0 bg-canvas"
              sheet={active}
              source={canvases.current.get(active.id) ?? null}
              version={version}
              ghost={ghostOf(active.id)}
              parts={parts}
              onHoverSpot={setHoverIsland}
              selection={selection}
              onSelect={select}
              onMove={moveSelection}
              onScale={scaleSelection}
              onMenu={(x, y) => setMenuAt({ x, y })}
              tool={paint.tool}
              brushSize={paint.size}
              canPaint={!!target || SHAPE_TOOLS.has(paint.tool)}
              onPaintStart={startPaint}
              onPaintMove={movePaint}
              onPaintEnd={endPaint}
            />
          ) : null}

        {/* The canvas's own menu. Anchored to a point rather than to the canvas, because what
            it is about is whatever was under the pointer — and opened from the *release* of a
            right press, since the native `contextmenu` event fires on the press and can't tell
            a menu from the start of a pan. See `CanvasStage.onMenu`. */}
        <DropdownMenu open={!!menuAt} onOpenChange={(open) => !open && setMenuAt(null)}>
          <DropdownMenuTrigger asChild>
            <span
              aria-hidden
              className="pointer-events-none fixed size-0"
              style={{ left: menuAt?.x ?? 0, top: menuAt?.y ?? 0 }}
            />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-52">
            <MenuRow
              icon={FlipHorizontal2}
              label={t("designer.mirror")}
              disabled={chosen.length !== 1 || !mirrorReady || !!chosen[0]?.mirror}
              onPick={mirrorSelected}
            />
            {canUnlink && (
              <MenuRow icon={Link2Off} label={t("designer.unlink")} onPick={unlinkSelection} />
            )}
            <DropdownMenuSeparator />
            <MenuRow
              icon={CopyPlus}
              label={t("designer.duplicate")}
              disabled={!chosen.length}
              onPick={duplicateSelection}
            />
            <MenuRow
              icon={Copy}
              label={t("designer.copy")}
              disabled={!chosen.length}
              onPick={copySelection}
            />
            <MenuRow
              icon={ClipboardPaste}
              label={t("designer.paste")}
              disabled={!clipboard?.layers.length}
              onPick={pasteClipboard}
            />
            <DropdownMenuSeparator />
            <MenuRow
              icon={FlipHorizontal2}
              label={t("designer.flipX")}
              disabled={!chosen.length}
              onPick={() => patchSelection((l) => ({ ...l, flipX: !l.flipX }))}
            />
            <MenuRow
              icon={FlipVertical2}
              label={t("designer.flipY")}
              disabled={!chosen.length}
              onPick={() => patchSelection((l) => ({ ...l, flipY: !l.flipY }))}
            />
            <DropdownMenuSeparator />
            <MenuRow
              icon={Group}
              label={t("designer.group")}
              disabled={!canGroup}
              onPick={groupSelection}
            />
            <MenuRow
              icon={Ungroup}
              label={t("designer.ungroup")}
              disabled={!canUngroup}
              onPick={ungroupSelection}
            />
            <DropdownMenuSeparator />
            <MenuRow
              icon={Trash2}
              label={t("common.remove")}
              disabled={!chosen.length}
              onPick={() => removeLayers(selection)}
            />
          </DropdownMenuContent>
        </DropdownMenu>

        {/* ── The model and the tools, over the sheet they act on ──────────────── */}
        {(
          <div data-dock="right" className="absolute inset-y-0 right-0 z-10 flex w-[312px] flex-col px-3 pb-3 pt-1.5">
        {/* Hidden rather than unmounted. Unmounting dropped the WebGL context and the model
            with it, so putting the preview away and bringing it back re-read the mesh, re-framed
            the camera, and left the UV map and the stock textures unavailable in between. */}
        <div className={cn("relative h-[260px] flex-none", !previewOpen && "hidden")}>
            <PreviewPanel
              compact
              state={destState}
              overrides={overrides}
              frameToken={version}
              onGeometry={onGeometry}
              onStock={onStock}
              highlight={hoverIsland}
              className="h-full"
            />
            <button
              onClick={() => togglePreview()}
              className="absolute bottom-1.5 right-1.5 z-10 cursor-default rounded-md bg-black/30 px-1.5 py-0.5 text-[11px] text-white/80 backdrop-blur-[2px] transition-colors hover:bg-black/55 hover:text-white"
            >
              {t("designer.hideModel")}
            </button>
        </div>
        {!previewOpen && (
          <button
            onClick={() => togglePreview()}
            className="flex flex-none cursor-default items-center justify-center rounded-md py-1.5 text-[11.5px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
          >
            {t("designer.showModel")}
          </button>
        )}
          {/* The model stays put and everything under it scrolls. Scrolling the whole column
              meant the bike slid off the top the moment the tool panel grew — which it does
              every time you pick a brush. */}
          <div className="mt-10 flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
          {active && (
            <PaintTools
              settings={paint}
              onTool={pickTool}
              onChange={(patch) => setPaint((p) => ({ ...p, ...patch }))}
              canUndo={canUndo}
              canRedo={canRedo}
              onUndo={undo}
              onRedo={redo}
              onAddImage={() => void addImage()}
              onAddText={addText}
              busy={busy}
            />
          )}
          {!!chosen.length && active && (
            <LayerInspector
              layers={chosen}
              all={active.layers}
              width={active.width}
              height={active.height}
              parts={parts}
              mirrorReady={mirrorReady}
              onClip={clipLayer}
              onFit={fitLayer}
              onMirror={mirrorSelected}
              onUnlink={unlinkSelection}
              onSelect={(id) => select([id], "replace")}
              onGroup={groupSelection}
              onUngroup={ungroupSelection}
              onChange={(fn) => patchSelection(fn, `layer:${selection.join(",")}`)}
            />
          )}
          </div>
          </div>
        )}
      </div>
    </div>
  );
}

function SheetList({
  className,
  sheets,
  activeId,
  hints,
  missingHints,
  onPick,
  onRename,
  onRemove,
  onMove,
  onAddBlank,
  onAddHintSheets,
  missingCompanions,
  onAddCompanions,
  onAddFromImage,
}: {
  className?: string;
  sheets: Sheet[];
  activeId: string | null;
  /** Every texture name the model's paints use — shown in full, companion maps included. */
  hints: string[];
  /** The color sheets among them that don't exist yet: what the create button would make. */
  missingHints: string[];
  onPick: (id: string) => void;
  onRename: (id: string, value: string) => void;
  onRemove: (id: string) => void;
  onMove: (id: string, before: string) => void;
  onAddBlank: () => void;
  onAddHintSheets: () => void;
  /** The companion maps — normals, mostly — the model asks for that aren't on the list. */
  missingCompanions: string[];
  onAddCompanions: () => void;
  onAddFromImage: () => void;
}) {
  const t = useT();
  // Which row is being dragged. A ref, not state: it changes on every dragover and nothing
  // on screen depends on it until the drop.
  const drag = useRef<string | null>(null);
  // The names the model actually binds, for the mark on each row.
  const bound = useMemo(
    () => new Set(hints.map((h) => h.trim().toLowerCase())),
    [hints],
  );
  return (
    <Card className={cn("bg-card/40", className)}>
      <CardHeader>
        <CardTitle>{t("designer.sheets")}</CardTitle>
        <CardAction>
          {/* Two ways to add one, behind the one control that means "add one". */}
          <DropdownMenu>
            <DropdownMenuTrigger
              className="cursor-default rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground"
              title={t("designer.addSheet")}
            >
              <Plus className="size-4" />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <MenuRow icon={FilePlus2} label={t("designer.blankSheet")} onPick={onAddBlank} />
              <MenuRow
                icon={FileImage}
                label={t("designer.sheetFromImage")}
                onPick={onAddFromImage}
              />
            </DropdownMenuContent>
          </DropdownMenu>
        </CardAction>
      </CardHeader>
      <CardContent className="flex min-h-0 flex-col">
      {/* Scrolls rather than growing: a bike's paint runs to two dozen sheets, and a list that
          long pushed the hint line and every button below the fold of the rail. */}
      <div className="flex max-h-[46vh] flex-col gap-1.5 overflow-y-auto pr-0.5">
        {sheets.map((sheet) => (
          /* The row picks the sheet. It used to be the size label that did — a `2048²` that
             was secretly the button — while the name was a text field and reorder and delete
             sat beside it, so five sheets meant five text fields and fifteen buttons on a
             264px column. Reorder and delete are now on the row you are touching, and the
             size is what it always looked like: a label. */
          <div
            key={sheet.id}
            draggable
            onDragStart={(e) => {
              drag.current = sheet.id;
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragOver={(e) => {
              if (drag.current && drag.current !== sheet.id) e.preventDefault();
            }}
            onDrop={(e) => {
              e.preventDefault();
              if (drag.current && drag.current !== sheet.id) onMove(drag.current, sheet.id);
              drag.current = null;
            }}
            onDragEnd={() => (drag.current = null)}
            onClick={() => onPick(sheet.id)}
            className={cn(
              "group flex cursor-default items-center gap-1 rounded-md border px-1.5 py-1 transition-colors",
              sheet.id === activeId
                ? "border-primary bg-primary/10"
                : "border-transparent hover:bg-foreground/[0.04]",
            )}
          >
            <span
              className={cn(
                "flex-none cursor-grab text-faint transition-opacity",
                sheet.id === activeId
                  ? "opacity-100"
                  : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
              )}
              title={t("designer.dragToOrder")}
            >
              <GripVertical className="size-3.5" />
            </span>
            <Input
              value={sheet.name}
              placeholder={t("designer.sheetName")}
              title={
                bound.size && !bound.has(sheet.name.trim().toLowerCase())
                  ? t("designer.sheetUnbound")
                  : undefined
              }
              className={cn(
                "h-6 min-w-0 flex-1 border-0 bg-transparent px-1 text-[12px] shadow-none focus-visible:ring-0",
                bound.size &&
                  !bound.has(sheet.name.trim().toLowerCase()) &&
                  "text-warning decoration-warning/40 decoration-dotted underline-offset-4 [text-decoration-line:underline]",
              )}
              onFocus={() => onPick(sheet.id)}
              onChange={(e) => onRename(sheet.id, e.target.value)}
            />
            <span className="flex-none px-0.5 text-[10.5px] tabular-nums text-faint">
              {sheet.width}²
            </span>
            <div
              className={cn(
                "flex flex-none items-center transition-opacity",
                sheet.id === activeId
                  ? "opacity-100"
                  : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
              )}
            >
              <button
                type="button"
                className="px-0.5 text-muted-foreground hover:text-destructive"
                onClick={() => onRemove(sheet.id)}
                title={t("common.remove")}
              >
                <Trash2 className="size-3.5" />
              </button>
            </div>
          </div>
        ))}
      </div>

      {/* The names this model binds — what its mesh draws, plus whatever the paints already
          installed replace. A sheet named anything else binds to nothing, and this is the only
          place the right answer is visible — so it also offers to make them, rather than
          leaving the list to be copied out by hand. */}
      {!!hints.length && (
        <div className="mt-2 flex flex-col items-start gap-1.5">
          {/* Full width and clipping, not sized to its label: the rail is 224px, a bike can
              want two dozen sheets, and `Button` is `whitespace-nowrap` — so a count in the
              label, or a longer word for it in another language, ran straight out of the rail. */}
          {!!missingHints.length && (
            <Button
              variant="outline"
              size="sm"
              className="w-full min-w-0 justify-start"
              onClick={onAddHintSheets}
              title={missingHints.join(", ")}
            >
              <FilePlus2 className="size-3.5" />
              <span className="truncate">
                {t("designer.createExpected", { count: missingHints.length })}
              </span>
            </Button>
          )}
          {/* Separate from the colour sheets on purpose. Adding these is the less common job
              and the riskier one — see `addCompanionSheets`. */}
          {!!missingCompanions.length && (
            <Button
              variant="outline"
              size="sm"
              className="w-full min-w-0 justify-start"
              onClick={onAddCompanions}
              title={missingCompanions.join(", ")}
            >
              <FilePlus2 className="size-3.5" />
              <span className="truncate">
                {t("designer.createCompanions", { count: missingCompanions.length })}
              </span>
            </Button>
          )}
        </div>
      )}

      {/* The three ways to start used to sit here as well, offered while the editor was
          still pristine. The start screen asks that question now, and asking it twice — once
          at the front door and again in a panel beside the work — is how you end up throwing
          away a sheet you had just made. Adding another sheet is the ＋ above. */}
      </CardContent>
    </Card>
  );
}

/**
 * The reference underlay's controls.
 *
 * Sits under the sheet list rather than in the layer panel, because a ghost belongs to the
 * sheet and not to the stack — it can't be reordered, selected, painted on or saved, and
 * putting it among things that can would be four wrong promises at once.
 */
function GhostPanel({
  ghost,
  isBike,
  sheetName,
  hasBase,
  hasGeometry,
  hasStock,
  stockSheet,
  busy,
  onTrace,
  onStockBase,
  onClearBase,
  onChange,
}: {
  ghost: Ghost;
  /** What is being painted, for the glyph on the stock toggle. */
  isBike: boolean;
  sheetName: string;
  /** Whether the sheet still holds a template that tracing could lift out of it. */
  hasBase: boolean;
  hasGeometry: boolean;
  /** Whether the model on screen can say which of its textures are its own — bikes can. */
  hasStock: boolean;
  /** Whether it carries one under *this* sheet's name — what makes it something to paint on. */
  stockSheet: boolean;
  busy: boolean;
  onTrace: () => void;
  /** Put that texture into the sheet for real, rather than faintly underneath it. */
  onStockBase: () => void;
  onClearBase: () => void;
  onChange: (fn: (g: Ghost) => Ghost) => void;
}) {
  const t = useT();
  const tracing = !!ghost.template;
  // A map was built for this name and came back with nothing on it. Distinct from "not built
  // yet" (`wireFor` still null), which is why both halves are checked.
  const noMatch = ghost.showWire && ghost.wireFor === sheetName && !ghost.wire;
  // The same pair, asked of the model's textures rather than of its triangles.
  const noStock = hasStock && ghost.showStock && ghost.stockFor === sheetName && !ghost.stock;
  // Nothing to trace: a blank sheet never had a template, and one that did has already had it
  // lifted. The UV map is the guide that still applies, so the button says so rather than
  // sitting there dead with no explanation.
  const canTrace = hasBase || tracing;
  const showing = ghostShows(ghost);
  // Something to show, and an opaque template still in the sheet sitting on top of it. The
  // reference draws underneath, so this is showing nothing until the template is lifted out.
  const buried = showing && hasBase;

  // Nothing to reference and nothing to put in the sheet — no cluster at all rather than a
  // row of controls that can only be greyed out.
  if (!canTrace && !hasStock && !hasGeometry) return null;

  return (
    /* On the picture rather than in a panel. What these switch is drawn on the sheet you are
       looking at, and a control for that belongs on the thing it changes — the left column was
       an inch of dead panel that existed to hold three toggles and a slider.
       The opacity comes out on hover: it is the adjustment you make after deciding *what* to
       show, and having it up permanently made a four-control cluster out of a three-control
       decision. */
    <div className="group pointer-events-auto absolute right-4 top-4 z-10 flex flex-col items-end gap-1">
      <div className="flex items-center gap-1 rounded-lg bg-background/55 p-1 opacity-80 shadow-sm backdrop-blur transition-opacity hover:opacity-100 group-hover:opacity-100">
        {canTrace && (
          <GhostToggle
            icon={<LayersIcon className="size-3.5" />}
            label={t("designer.traceTemplate")}
            title={t("designer.traceHint")}
            on={tracing && ghost.showTemplate}
            onClick={() => {
              // Already lifted and visible — this press is asking to see it in the paint
              // again, so put it back. Otherwise lift it, or show what has been lifted.
              if (!tracing || ghost.showTemplate) onTrace();
              else onChange((g) => ({ ...g, showTemplate: true }));
            }}
          />
        )}
        <GhostToggle
          icon={isBike ? <Bike className="size-3.5" /> : <Shirt className="size-3.5" />}
          label={t("designer.stockTexture")}
          title={t(hasStock ? "designer.stockHint" : "designer.noStock")}
          on={ghost.showStock}
          disabled={!hasStock}
          onClick={() => onChange((g) => ({ ...g, showStock: !g.showStock }))}
        />
        <GhostToggle
          icon={<Grid3x3 className="size-3.5" />}
          label={t("designer.uvMap")}
          title={t(hasGeometry ? "designer.uvHint" : "designer.noGeometry")}
          on={ghost.showWire}
          disabled={!hasGeometry}
          onClick={() => onChange((g) => ({ ...g, showWire: !g.showWire }))}
        />
        {hasStock && (
          <button
            type="button"
            disabled={(!stockSheet && !hasBase) || busy}
            title={t(
              hasBase
                ? "designer.clearBase"
                : stockSheet
                  ? "designer.stockAsBaseHint"
                  : "designer.stockNoMatch",
              { name: sheetName.trim() },
            )}
            onClick={hasBase ? onClearBase : onStockBase}
            className={cn(
              "ml-0.5 cursor-default rounded-md px-1.5 py-1 transition-colors disabled:opacity-30",
              hasBase
                ? "bg-primary/15 text-foreground"
                : "text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
            )}
          >
            {busy ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <PaintBucket className="size-3.5" />
            )}
          </button>
        )}
      </div>

      {showing && (
        <div className="flex w-[190px] items-center gap-2 rounded-lg bg-background/85 px-2.5 py-1.5 opacity-0 shadow-sm backdrop-blur transition-opacity group-hover:opacity-100">
          <span className="flex-none text-[10.5px] uppercase tracking-[0.09em] text-faint">
            {t("designer.opacity")}
          </span>
          <Slider
            value={ghost.opacity}
            min={0}
            max={1}
            step={0.01}
            onChange={(v) => onChange((g) => ({ ...g, opacity: v }))}
            format={(v) => `${Math.round(v * 100)}%`}
          />
        </div>
      )}

      {/* The three ways this shows nothing, said where the thing itself is. */}
      {(buried || noMatch || (noStock && !noMatch)) && (
        <p
          className={cn(
            "max-w-[260px] rounded-lg bg-background/85 px-2.5 py-1.5 text-right text-[11px] leading-snug shadow-sm backdrop-blur",
            buried ? "text-warning" : noMatch ? "text-destructive" : "text-faint",
          )}
        >
          {buried
            ? t("designer.ghostBuried")
            : noMatch
              ? t("designer.uvNoMatch", { name: sheetName.trim() })
              : t("designer.stockNoMatch", { name: sheetName.trim() })}
        </p>
      )}
    </div>
  );
}

function GhostToggle({
  icon,
  label,
  title,
  on,
  disabled,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  title: string;
  on: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium transition-colors disabled:opacity-35",
        on ? "border-primary/60 bg-primary/10 text-foreground" : "border-border text-faint",
      )}
    >
      {icon}
      {label}
    </button>
  );
}
/**
 * The layer stack, with grouped layers gathered under a heading.
 *
 * A group is a tag rather than a container (see `layers.ts`), so this doesn't recurse — it walks
 * the stack once and starts a block wherever the tag changes. That works because grouping also
 * *gathers*: `regroup` puts a group's members next to each other, and everything downstream,
 * this list included, gets to stay flat.
 */
function LayerList({
  className,
  layers,
  selection,
  onSelect,
  onToggle,
  onRemove,
  onMove,
  onAdd,
}: {
  className?: string;
  layers: Layer[];
  selection: string[];
  onSelect: (ids: string[], mode: "replace" | "toggle" | "isolate") => void;
  onToggle: (id: string, visible: boolean) => void;
  onRemove: (id: string) => void;
  onMove: (id: string, before: string) => void;
  onAdd: () => void;
}) {
  const t = useT();
  // Which row is being dragged. A ref, not state: it changes on every dragover and nothing
  // on screen depends on it until the drop.
  const drag = useRef<string | null>(null);
  // Top of the list is the top of the stack, which is how a layer panel reads — the array
  // itself is bottom-first because that's the order it's drawn in.
  const shown = [...layers].reverse();
  const rows: ({ tag: string; members: Layer[] } | { tag: null; members: [Layer] })[] = [];
  for (let i = 0; i < shown.length; i += 1) {
    const layer = shown[i];
    if (!layer.group) {
      rows.push({ tag: null, members: [layer] });
      continue;
    }
    // The block is emitted at its first member and skipped at the rest of them.
    if (i > 0 && shown[i - 1].group === layer.group) continue;
    rows.push({ tag: layer.group, members: shown.filter((l) => l.group === layer.group) });
  }

  const row = (layer: Layer) => (
    /* The same shape as a sheet row: grab it on the left, and what you can do to it on the
       right. Two arrows per row, each disabled at one end, was the old way of saying "move
       this" — and a layer stack is the one list where dragging is the obvious gesture. */
    <div
      key={layer.id}
      draggable
      onDragStart={(e) => {
        drag.current = layer.id;
        e.dataTransfer.effectAllowed = "move";
      }}
      onDragOver={(e) => {
        if (drag.current && drag.current !== layer.id) e.preventDefault();
      }}
      onDrop={(e) => {
        e.preventDefault();
        if (drag.current && drag.current !== layer.id) onMove(drag.current, layer.id);
        drag.current = null;
      }}
      onDragEnd={() => (drag.current = null)}
      className={cn(
        "group flex shrink-0 items-center gap-1.5 rounded-md border px-1.5 py-1 text-[11.5px] transition-colors",
        selection.includes(layer.id) ? "border-primary bg-primary/10" : "border-transparent",
      )}
    >
      <span
        className={cn(
          "flex-none cursor-grab text-faint transition-opacity",
          selection.includes(layer.id)
            ? "opacity-100"
            : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
        )}
        title={t("designer.dragToOrder")}
      >
        <GripVertical className="size-3.5" />
      </span>
      {/* A follower says so here as well as in the inspector: this is the list you scan when
          you can't work out why a layer won't move. */}
      {layer.mirror && (
        <Link2 className="size-3 flex-none text-primary" aria-label={t("designer.mirroredShort")} />
      )}
      <button
        type="button"
        className="min-w-0 flex-1 truncate text-left"
        // Alt reaches inside a group, shift adds to the selection — the same two modifiers the
        // canvas uses, because it is the same question being asked twice.
        onClick={(e) =>
          onSelect([layer.id], e.shiftKey ? "toggle" : e.altKey ? "isolate" : "replace")
        }
      >
        {layer.kind === "text" ? layer.text || layer.name : layer.name}
      </button>
      {/* Showing and removing, together at the end: both are about the layer as a whole
          rather than about what it is, and the rule keeps them out of the name. */}
      <span className="ml-1 h-3.5 w-px flex-none bg-border" />
      <button
        type="button"
        className="flex-none text-muted-foreground hover:text-foreground"
        onClick={() => onToggle(layer.id, !layer.visible)}
        title={t(layer.visible ? "designer.hide" : "designer.show")}
      >
        {layer.visible ? <Eye className="size-3.5" /> : <EyeOff className="size-3.5" />}
      </button>
      <button
        type="button"
        className="flex-none text-muted-foreground hover:text-destructive"
        onClick={() => onRemove(layer.id)}
        title={t("common.remove")}
      >
        <Trash2 className="size-3.5" />
      </button>
    </div>
  );

  return (
    <Card className={cn("bg-card/40", className)}>
      <CardHeader>
        <CardTitle>{t("designer.layers")}</CardTitle>
        <CardAction>
          <button
            type="button"
            className="text-muted-foreground transition-colors hover:text-foreground"
            onClick={onAdd}
            title={t("designer.addPaint")}
          >
            <Plus className="size-4" />
          </button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex min-h-0 flex-1 flex-col">
      {!layers.length ? (
        <p className="text-[11px] leading-snug text-faint">{t("designer.noLayers")}</p>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto pr-0.5">
          {rows.map((block) =>
            block.tag === null ? (
              row(block.members[0])
            ) : (
              <div
                key={block.tag}
                className="flex flex-col gap-1 rounded-md border border-dashed border-border/70 p-1"
              >
                <div className="flex items-center gap-1.5 px-0.5 text-[10.5px] text-muted-foreground">
                  <button
                    type="button"
                    className="flex-none hover:text-foreground"
                    // One toggle for the block: if any of it is showing, the whole thing hides.
                    onClick={() => {
                      const anyVisible = block.members.some((l) => l.visible);
                      for (const l of block.members) onToggle(l.id, !anyVisible);
                    }}
                    title={t("designer.hide")}
                  >
                    {block.members.some((l) => l.visible) ? (
                      <Eye className="size-3" />
                    ) : (
                      <EyeOff className="size-3" />
                    )}
                  </button>
                  <button
                    type="button"
                    className="flex min-w-0 flex-1 items-center gap-1 truncate text-left hover:text-foreground"
                    onClick={() => onSelect(block.members.map((l) => l.id), "replace")}
                  >
                    <Group className="size-3 flex-none" />
                    {t("designer.groupOf", { count: String(block.members.length) })}
                  </button>
                </div>
                {block.members.map(row)}
              </div>
            ),
          )}
        </div>
      )}
      </CardContent>
    </Card>
  );
}
