import { Crop, Maximize2 } from "lucide-react";
import { Input } from "../../ui/input";
import { useT } from "../../../i18n/context";
import { Row, Slider } from "./controls";
import { BLEND_MODES, FONTS, type BlendMode, type Layer } from "./layers";
import type { UvPart } from "./uv";

/**
 * Everything about the selected layer that isn't its position on the sheet.
 *
 * Position is dragged directly on the canvas — putting X and Y here as well would be a second
 * way to say the same thing, and the one that doesn't show you the result.
 */
export function LayerInspector({
  layer,
  parts,
  onClip,
  onFit,
  onChange,
}: {
  layer: Layer;
  /** The model's bodywork for this sheet, empty when no model is loaded. */
  parts: UvPart[];
  /** Pin the layer to a part, or unpin it with null. */
  onClip: (label: string | null) => void;
  /** Place and scale the layer to cover a part. */
  onFit: (label: string) => void;
  onChange: (fn: (l: Layer) => Layer) => void;
}) {
  const t = useT();
  const patch = (fn: (l: Layer) => Layer) => onChange(fn);
  // A paint layer is the sheet: strokes were put down in sheet pixels, and scaling or rotating
  // the canvas afterwards would move every one of them off the panel it was painted on.
  const movable = layer.kind !== "paint";

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-card/40 p-3.5">
      <h2 className="text-[13px] font-semibold">{t("designer.layerTitle")}</h2>

      {movable && (
        <>
          <Row label={t("designer.scale")}>
            <Slider
              value={layer.scale}
              min={0.05}
              max={4}
              step={0.01}
              onChange={(v) => patch((l) => ({ ...l, scale: v }))}
              format={(v) => `${Math.round(v * 100)}%`}
            />
          </Row>

          <Row label={t("designer.rotation")}>
            <Slider
              value={Math.round((layer.rotation * 180) / Math.PI)}
              min={-180}
              max={180}
              step={1}
              onChange={(v) => patch((l) => ({ ...l, rotation: (v * Math.PI) / 180 }))}
              format={(v) => `${v}°`}
            />
          </Row>
        </>
      )}

      {/* What the layer is *for*, rather than where it is: a photo dropped on a livery is
          almost always meant for one panel, and this is where that gets said. Offered for
          every layer kind — clipping a paint layer to a shroud is how you brush freely and
          still stop at the seam. */}
      {!!parts.length && (
        <>
          <Row label={t("designer.part")}>
            <select
              value={layer.clip?.label ?? ""}
              onChange={(e) => onClip(e.target.value || null)}
              className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-[11.5px]"
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
              disabled={!layer.clip || !movable}
              onClick={() => layer.clip && onFit(layer.clip.label)}
              title={t(movable ? "designer.fitToPartHint" : "designer.fitNotForPaint")}
              className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-[11px] font-medium transition-colors hover:border-primary/60 disabled:opacity-35"
            >
              <Maximize2 className="size-3.5" />
              {t("designer.fitToPart")}
            </button>
            {!!layer.clip && (
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
          value={layer.opacity}
          min={0}
          max={1}
          step={0.01}
          onChange={(v) => patch((l) => ({ ...l, opacity: v }))}
          format={(v) => `${Math.round(v * 100)}%`}
        />
      </Row>

      <Row label={t("designer.blend")}>
        <select
          value={layer.blend}
          onChange={(e) => patch((l) => ({ ...l, blend: e.target.value as BlendMode }))}
          className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-[11.5px]"
        >
          {BLEND_MODES.map((m) => (
            <option key={m} value={m}>
              {t(`designer.blend.${m}` as "designer.blend.normal")}
            </option>
          ))}
        </select>
      </Row>

      {layer.kind === "text" && (
        <>
          <Row label={t("designer.text")}>
            <Input
              value={layer.text}
              className="h-7 text-[11.5px]"
              onChange={(e) =>
                patch((l) => (l.kind === "text" ? { ...l, text: e.target.value } : l))
              }
            />
          </Row>

          <Row label={t("designer.font")}>
            <select
              value={layer.font}
              onChange={(e) =>
                patch((l) => (l.kind === "text" ? { ...l, font: e.target.value } : l))
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
              onChange={(v) => patch((l) => (l.kind === "text" ? { ...l, size: v } : l))}
              format={(v) => `${v}`}
            />
          </Row>

          <Row label={t("designer.colour")}>
            <input
              type="color"
              value={layer.color}
              onChange={(e) =>
                patch((l) => (l.kind === "text" ? { ...l, color: e.target.value } : l))
              }
              className="h-6 w-9 flex-none rounded border border-input bg-background"
            />
            <input
              type="color"
              value={layer.outline}
              onChange={(e) =>
                patch((l) => (l.kind === "text" ? { ...l, outline: e.target.value } : l))
              }
              className="h-6 w-9 flex-none rounded border border-input bg-background"
              title={t("designer.outline")}
            />
            <Slider
              value={layer.outlineWidth}
              min={0}
              max={24}
              step={1}
              onChange={(v) => patch((l) => (l.kind === "text" ? { ...l, outlineWidth: v } : l))}
              format={(v) => `${v}`}
            />
          </Row>
        </>
      )}
    </div>
  );
}
