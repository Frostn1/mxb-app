import { Crop, FlipHorizontal2, FlipVertical2, Group, Link2Off, Maximize2, Ungroup } from "lucide-react";
import { Input } from "../../ui/input";
import { useT } from "../../../i18n/context";
import { NumberField, Row, Slider } from "./controls";
import { BLEND_MODES, FONTS, type BlendMode, type Layer } from "./layers";
import type { UvPart } from "./uv";

/** The size range a layer can be taken to, shared with the stage's corner drag. */
const MIN_SCALE = 0.05;
const MAX_SCALE = 4;

/**
 * Everything about the selection that isn't dragged directly on the sheet.
 *
 * Position *is* here as well as on the canvas, which reverses an earlier call. The objection
 * was that a typed X would be a second way to say the same thing and the one that doesn't show
 * you the result — but the boxes below track the drag live, so they are the same way of saying
 * it with a number attached. Placing a plate number by eye is not a thing the eye is good at.
 *
 * Rows that need one layer to mean anything appear only when one is selected; opacity, blend
 * and the flips apply across the lot.
 */
export function LayerInspector({
  layers,
  all,
  width,
  height,
  parts,
  mirrorReady,
  onClip,
  onFit,
  onMirror,
  onUnlink,
  onSelect,
  onGroup,
  onUngroup,
  onChange,
}: {
  /** The selection. Never empty — the rail leaves this out entirely when nothing is selected. */
  layers: Layer[];
  /** Every layer on the sheet, so a follower can name the layer it follows. */
  all: Layer[];
  width: number;
  height: number;
  /** The model's bodywork for this sheet, empty when no model is loaded. */
  parts: UvPart[];
  /** Whether the model can answer where the far flank is at all. */
  mirrorReady: boolean;
  /** Pin the selection to a part, or unpin it with null. */
  onClip: (label: string | null) => void;
  /** Place and scale the layer to cover a part. */
  onFit: (label: string) => void;
  onMirror: () => void;
  onUnlink: () => void;
  onSelect: (id: string) => void;
  onGroup: () => void;
  onUngroup: () => void;
  onChange: (fn: (l: Layer) => Layer) => void;
}) {
  const t = useT();
  const layer = layers.length === 1 ? layers[0] : null;
  // A paint layer is the sheet: strokes were put down in sheet pixels, and scaling or rotating
  // the canvas afterwards would move every one of them off the panel it was painted on.
  const movable = layers.every((l) => l.kind !== "paint");
  // A follower is derived from its source on every edit, so anything typed into it here would
  // be overwritten by the next sync. Unlink is one click away and does exactly what's wanted.
  const linked = layers.some((l) => l.mirror);
  const source = layer?.mirror ? all.find((l) => l.id === layer.mirror?.of) : null;
  const grouped = layers.some((l) => l.group);
  const editable = movable && !linked;

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-card/40 p-3.5">
      <h2 className="text-[13px] font-semibold">
        {layer ? t("designer.layerTitle") : t("designer.layersSelected", { count: String(layers.length) })}
      </h2>

      {/* A follower says what it is before anything else, because every disabled row below is
          explained by this one line. */}
      {linked && (
        <div className="flex flex-col gap-1.5 rounded-md border border-border bg-background/50 p-2">
          <p className="text-[11px] leading-snug text-muted-foreground">
            {source
              ? t("designer.mirroredFrom", { name: source.kind === "text" ? source.text || source.name : source.name })
              : t("designer.mirroredOrphan")}
          </p>
          {!mirrorReady && (
            <p className="text-[11px] leading-snug text-faint">{t("designer.mirrorPaused")}</p>
          )}
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={onUnlink}
              title={t("designer.unlinkHint")}
              className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60"
            >
              <Link2Off className="size-3.5" />
              {t("designer.unlink")}
            </button>
            {source && (
              <button
                type="button"
                onClick={() => onSelect(source.id)}
                className="rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60"
              >
                {t("designer.selectSource")}
              </button>
            )}
          </div>
        </div>
      )}

      {layer && editable && (
        <Row label={t("designer.position")}>
          <NumberField
            value={layer.x}
            min={-width}
            max={width * 2}
            title="X"
            onChange={(v) => onChange((l) => ({ ...l, x: v }))}
          />
          {/* Counted from the bottom, because that is where the stage counts from — the sheet
              is shown flipped and a Y that grew as the layer moved up the screen would be a
              number that disagrees with the thing it names. */}
          <NumberField
            value={height - layer.y}
            min={-height}
            max={height * 2}
            title="Y"
            onChange={(v) => onChange((l) => ({ ...l, y: height - v }))}
          />
        </Row>
      )}

      {movable && (
        <>
          <Row label={t("designer.scale")}>
            <Slider
              value={layer ? layer.scale : 1}
              min={MIN_SCALE}
              max={MAX_SCALE}
              step={0.01}
              onChange={(v) => onChange((l) => ({ ...l, scale: v }))}
              format={(v) => `${Math.round(v * 100)}%`}
            />
          </Row>

          <Row label={t("designer.rotation")}>
            <Slider
              value={layer ? Math.round((layer.rotation * 180) / Math.PI) : 0}
              min={-180}
              max={180}
              step={1}
              onChange={(v) => onChange((l) => ({ ...l, rotation: (v * Math.PI) / 180 }))}
              format={(v) => `${v}°`}
            />
          </Row>

          <Row label={t("designer.flip")}>
            <button
              type="button"
              disabled={!editable}
              onClick={() => onChange((l) => ({ ...l, flipX: !l.flipX }))}
              title={t("designer.flipX")}
              className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60 disabled:opacity-35"
            >
              <FlipHorizontal2 className="size-3.5" />
            </button>
            <button
              type="button"
              disabled={!editable}
              onClick={() => onChange((l) => ({ ...l, flipY: !l.flipY }))}
              title={t("designer.flipY")}
              className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60 disabled:opacity-35"
            >
              <FlipVertical2 className="size-3.5" />
            </button>
            {/* The mirror sits with the flips because that is where a hand goes looking for it,
                and half its job is to be found instead of the flip that isn't what was meant. */}
            <button
              type="button"
              disabled={!layer || !editable || !mirrorReady}
              onClick={onMirror}
              title={t(mirrorReady ? "designer.mirrorHint" : "designer.mirrorWhy.no-model")}
              className="ml-auto flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60 disabled:opacity-35"
            >
              <FlipHorizontal2 className="size-3.5" />
              {t("designer.mirror")}
            </button>
          </Row>
        </>
      )}

      {(layers.length > 1 || grouped) && (
        <Row label={t("designer.groupRow")}>
          <button
            type="button"
            disabled={layers.length < 2}
            onClick={onGroup}
            title={t("designer.groupHint")}
            className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60 disabled:opacity-35"
          >
            <Group className="size-3.5" />
            {t("designer.group")}
          </button>
          <button
            type="button"
            disabled={!grouped}
            onClick={onUngroup}
            className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60 disabled:opacity-35"
          >
            <Ungroup className="size-3.5" />
            {t("designer.ungroup")}
          </button>
        </Row>
      )}

      {/* What the layer is *for*, rather than where it is: a photo dropped on a livery is
          almost always meant for one panel, and this is where that gets said. Offered for
          every layer kind — clipping a paint layer to a shroud is how you brush freely and
          still stop at the seam. */}
      {!!parts.length && (
        <>
          <Row label={t("designer.part")}>
            <select
              value={layer?.clip?.label ?? ""}
              disabled={linked}
              onChange={(e) => onClip(e.target.value || null)}
              className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-[11.5px] disabled:opacity-35"
            >
              <option value="">{t("designer.wholeSheet")}</option>
              {parts.map((p) => (
                <option key={p.label} value={p.label}>
                  {/* The side is part of the name here rather than a column of its own: a
                      picker of one-word options is read as a list of names. */}
                  {p.side && p.side !== "centre"
                    ? `${p.label} — ${t(`designer.flank.${p.side}` as "designer.flank.left")}`
                    : p.label}
                </option>
              ))}
            </select>
          </Row>

          <Row label="">
            <button
              type="button"
              disabled={!layer?.clip || !editable}
              onClick={() => layer?.clip && onFit(layer.clip.label)}
              title={t(movable ? "designer.fitToPartHint" : "designer.fitNotForPaint")}
              className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60 disabled:opacity-35"
            >
              <Maximize2 className="size-3.5" />
              {t("designer.fitToPart")}
            </button>
            {!!layer?.clip && (
              <span
                className="flex items-center gap-1 text-[11px] text-faint"
                title={t("designer.clippedHint")}
              >
                <Crop className="size-3" />
                {t("designer.clipped")}
              </span>
            )}
          </Row>
        </>
      )}

      <Row label={t("designer.opacity")}>
        <Slider
          value={layer ? layer.opacity : 1}
          min={0}
          max={1}
          step={0.01}
          onChange={(v) => onChange((l) => ({ ...l, opacity: v }))}
          format={(v) => `${Math.round(v * 100)}%`}
        />
      </Row>

      <Row label={t("designer.blend")}>
        <select
          value={layer ? layer.blend : "normal"}
          onChange={(e) => onChange((l) => ({ ...l, blend: e.target.value as BlendMode }))}
          className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-[11.5px]"
        >
          {BLEND_MODES.map((m) => (
            <option key={m} value={m}>
              {t(`designer.blend.${m}` as "designer.blend.normal")}
            </option>
          ))}
        </select>
      </Row>

      {/* A shape is geometry, so it keeps its colour and its pen editable for as long as the
          paint is open — which is the whole point of it not being pixels. Size and angle are
          the shared controls above; only what it is drawn *with* is particular to it. */}
      {layer?.kind === "shape" && !linked && (
        <>
          <Row label={t("designer.colour")}>
            <input
              type="color"
              value={layer.color}
              onChange={(e) =>
                onChange((l) => (l.kind === "shape" ? { ...l, color: e.target.value } : l))
              }
              className="h-7 w-full cursor-pointer rounded-md border border-input bg-background"
            />
          </Row>

          {layer.shape !== "line" && (
            <Row label={t("designer.shape")}>
              <select
                value={layer.style}
                onChange={(e) =>
                  onChange((l) =>
                    l.kind === "shape" ? { ...l, style: e.target.value as "fill" | "outline" } : l,
                  )
                }
                className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-[11.5px]"
              >
                <option value="fill">{t("designer.shape.fill")}</option>
                <option value="outline">{t("designer.shape.outline")}</option>
              </select>
            </Row>
          )}

          {(layer.style === "outline" || layer.shape === "line") && (
            <Row label={t("designer.lineWidth")}>
              <Slider
                value={layer.strokeWidth}
                min={1}
                max={128}
                step={1}
                onChange={(v) => onChange((l) => (l.kind === "shape" ? { ...l, strokeWidth: v } : l))}
                format={(v) => `${v}px`}
              />
            </Row>
          )}
        </>
      )}

      {layer?.kind === "text" && !linked && (
        <>
          <Row label={t("designer.text")}>
            <Input
              value={layer.text}
              className="h-7 text-[11.5px]"
              onChange={(e) =>
                onChange((l) => (l.kind === "text" ? { ...l, text: e.target.value } : l))
              }
            />
          </Row>

          <Row label={t("designer.font")}>
            <select
              value={layer.font}
              onChange={(e) =>
                onChange((l) => (l.kind === "text" ? { ...l, font: e.target.value } : l))
              }
              className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-[11.5px]"
              style={{ fontFamily: layer.font }}
            >
              {FONTS.map((f) => (
                <option key={f} value={f} style={{ fontFamily: f }}>
                  {f.split(",")[0].replace(/'/g, "")}
                </option>
              ))}
            </select>
          </Row>

          <Row label={t("designer.size")}>
            <Slider
              value={layer.size}
              min={8}
              max={512}
              step={1}
              onChange={(v) => onChange((l) => (l.kind === "text" ? { ...l, size: v } : l))}
              format={(v) => `${v}`}
            />
          </Row>

          <Row label={t("designer.colour")}>
            <input
              type="color"
              value={layer.color}
              onChange={(e) =>
                onChange((l) => (l.kind === "text" ? { ...l, color: e.target.value } : l))
              }
              className="h-6 w-9 flex-none rounded border border-input bg-background"
            />
            <input
              type="color"
              value={layer.outline}
              onChange={(e) =>
                onChange((l) => (l.kind === "text" ? { ...l, outline: e.target.value } : l))
              }
              className="h-6 w-9 flex-none rounded border border-input bg-background"
              title={t("designer.outline")}
            />
            <Slider
              value={layer.outlineWidth}
              min={0}
              max={24}
              step={1}
              onChange={(v) => onChange((l) => (l.kind === "text" ? { ...l, outlineWidth: v } : l))}
              format={(v) => `${v}`}
            />
          </Row>
        </>
      )}
    </div>
  );
}
