import {
  ArrowLeftRight,
  Blend,
  Brush,
  Circle,
  Eraser,
  Minus,
  MousePointer2,
  PaintBucket,
  Redo2,
  Square,
  Undo2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { useT } from "../../../i18n/context";
import type { TKey } from "../../../i18n/core";
import { Row, Slider } from "./controls";
import {
  PAINT_TOOLS,
  TOOL_KEYS,
  hasTip,
  type GradientMode,
  type PaintSettings,
  type PaintTool,
  type ShapeStyle,
} from "./paint";

const ICONS: Record<PaintTool, typeof Brush> = {
  move: MousePointer2,
  brush: Brush,
  eraser: Eraser,
  gradient: Blend,
  fill: PaintBucket,
  rect: Square,
  ellipse: Circle,
  line: Minus,
};

const SHAPES = new Set<PaintTool>(["rect", "ellipse", "line"]);

interface PaintToolsProps {
  settings: PaintSettings;
  /** Separate from `onChange`: picking a tool can also have to find something to paint into. */
  onTool: (tool: PaintTool) => void;
  onChange: (patch: Partial<PaintSettings>) => void;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
}

/**
 * The tool kit: what the pointer does on the sheet, and what it does it with.
 *
 * Rows appear with the tool that needs them rather than all at once — hardness means nothing to
 * a gradient and an end colour means nothing to a brush, and a panel of controls that don't
 * apply is a panel nobody reads.
 */
export function PaintTools({
  settings,
  onTool,
  onChange,
  canUndo,
  canRedo,
  onUndo,
  onRedo,
}: PaintToolsProps) {
  const t = useT();
  const { tool } = settings;
  const tipped = hasTip(tool);
  const shaped = SHAPES.has(tool);

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-card/40 p-3.5">
      <div className="flex items-center justify-between">
        <h2 className="text-[13px] font-semibold">{t("designer.paint")}</h2>
        <div className="flex items-center gap-0.5">
          <button
            type="button"
            disabled={!canUndo}
            onClick={onUndo}
            title={t("designer.undoStroke")}
            aria-label={t("designer.undoStroke")}
            className="rounded p-1 text-muted-foreground transition-colors hover:text-foreground disabled:opacity-30"
          >
            <Undo2 className="size-3.5" />
          </button>
          <button
            type="button"
            disabled={!canRedo}
            onClick={onRedo}
            title={t("designer.redoStroke")}
            aria-label={t("designer.redoStroke")}
            className="rounded p-1 text-muted-foreground transition-colors hover:text-foreground disabled:opacity-30"
          >
            <Redo2 className="size-3.5" />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-4 gap-1">
        {PAINT_TOOLS.map((id) => {
          const Icon = ICONS[id];
          const label = t(`designer.tool.${id}` as TKey);
          return (
            <button
              key={id}
              type="button"
              onClick={() => onTool(id)}
              title={`${label} (${TOOL_KEYS[id].toUpperCase()})`}
              aria-label={label}
              aria-pressed={id === tool}
              className={cn(
                "flex h-7 items-center justify-center rounded-md border transition-colors",
                id === tool
                  ? "border-primary bg-primary/15 text-foreground"
                  : "border-border text-muted-foreground hover:text-foreground",
              )}
            >
              <Icon className="size-3.5" />
            </button>
          );
        })}
      </div>

      {tool === "move" ? (
        <p className="text-[11px] leading-snug text-faint">{t("designer.moveHint")}</p>
      ) : (
        <>
          <Row label={t("designer.colour")}>
            <input
              type="color"
              value={settings.colorA}
              onChange={(e) => onChange({ colorA: e.target.value })}
              className="h-6 w-9 flex-none rounded border border-input bg-background"
              title={t("designer.colourFrom")}
            />
            {tool === "gradient" && (
              <>
                <button
                  type="button"
                  onClick={() => onChange({ colorA: settings.colorB, colorB: settings.colorA })}
                  title={t("designer.swapColours")}
                  aria-label={t("designer.swapColours")}
                  className="flex-none rounded p-1 text-muted-foreground transition-colors hover:text-foreground"
                >
                  <ArrowLeftRight className="size-3" />
                </button>
                <input
                  type="color"
                  value={settings.colorB}
                  onChange={(e) => onChange({ colorB: e.target.value })}
                  className="h-6 w-9 flex-none rounded border border-input bg-background"
                  title={t("designer.colourTo")}
                />
              </>
            )}
            {/* What the gradient will actually do, at a glance — the two swatches on their own
                don't show which way round it runs or where it fades out. */}
            {tool === "gradient" && (
              <span
                className="h-6 min-w-0 flex-1 rounded border border-input"
                style={{
                  backgroundImage: `linear-gradient(to right, ${settings.colorA}, ${
                    settings.fade ? `${settings.colorB}00` : settings.colorB
                  })`,
                }}
              />
            )}
          </Row>

          {tipped && (
            <Row label={t("designer.brushSize")}>
              <Slider
                value={settings.size}
                min={2}
                max={512}
                step={1}
                onChange={(v) => onChange({ size: v })}
                format={(v) => `${v}`}
              />
            </Row>
          )}

          {tipped && (
            <Row label={t("designer.hardness")}>
              <Slider
                value={settings.hardness}
                min={0}
                max={1}
                step={0.01}
                onChange={(v) => onChange({ hardness: v })}
                format={(v) => `${Math.round(v * 100)}%`}
              />
            </Row>
          )}

          <Row label={t("designer.strength")}>
            <Slider
              value={settings.opacity}
              min={0.02}
              max={1}
              step={0.01}
              onChange={(v) => onChange({ opacity: v })}
              format={(v) => `${Math.round(v * 100)}%`}
            />
          </Row>

          {tool === "gradient" && (
            <Row label={t("designer.gradient")}>
              <select
                value={settings.gradient}
                onChange={(e) => onChange({ gradient: e.target.value as GradientMode })}
                className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-[11.5px]"
              >
                <option value="linear">{t("designer.gradient.linear")}</option>
                <option value="radial">{t("designer.gradient.radial")}</option>
              </select>
              <label className="flex flex-none items-center gap-1 text-[11px] text-muted-foreground">
                <input
                  type="checkbox"
                  checked={settings.fade}
                  onChange={(e) => onChange({ fade: e.target.checked })}
                  className="size-3 accent-primary"
                />
                {t("designer.fadeOut")}
              </label>
            </Row>
          )}

          {shaped && tool !== "line" && (
            <Row label={t("designer.shape")}>
              <select
                value={settings.shape}
                onChange={(e) => onChange({ shape: e.target.value as ShapeStyle })}
                className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-[11.5px]"
              >
                <option value="fill">{t("designer.shape.fill")}</option>
                <option value="outline">{t("designer.shape.outline")}</option>
              </select>
            </Row>
          )}

          {shaped && (tool === "line" || settings.shape === "outline") && (
            <Row label={t("designer.lineWidth")}>
              <Slider
                value={settings.strokeWidth}
                min={1}
                max={128}
                step={1}
                onChange={(v) => onChange({ strokeWidth: v })}
                format={(v) => `${v}`}
              />
            </Row>
          )}

          {/* The gradient and the fill cover the whole layer, which is the one thing about
              them worth saying out loud — a brush stroke made first is underneath it. */}
          <p className="text-[11px] leading-snug text-faint">
            {t(
              tool === "fill"
                ? "designer.fillHint"
                : tool === "gradient"
                  ? "designer.gradientHint"
                  : "designer.paintHint",
            )}
          </p>
        </>
      )}
    </div>
  );
}
