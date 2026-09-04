import { useState } from "react";

/**
 * The small labelled controls a side panel is made of.
 *
 * Shared rather than copied so the Designer's rails and the viewer's pose panel stay identical
 * instead of merely similar — a row that lines up differently in each reads as a bug.
 */

/** A labelled row. The panels are nothing but these, so it earns its own component. */
export function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex items-center gap-2">
      <span className="w-[68px] flex-none text-[11px] text-muted-foreground">{label}</span>
      <span className="flex min-w-0 flex-1 items-center gap-1.5">{children}</span>
    </label>
  );
}

export function Slider({
  value,
  min,
  max,
  step,
  onChange,
  format,
  editable = false,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  format: (v: number) => string;
  /** Let the readout be typed into as well as dragged. Dragging is fine for finding a
   *  feel, but a rider copying a number off a setup sheet shouldn't have to hunt for it
   *  one pixel at a time. Off by default, so the panels that only drag are unchanged. */
  editable?: boolean;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="h-1 min-w-0 flex-1 accent-primary"
      />
      {editable ? (
        <input
          type="text"
          inputMode="decimal"
          value={draft ?? format(value)}
          // The draft holds whatever is typed until focus leaves. Rendering the clamped
          // value back on every keystroke would make an out-of-range number impossible to
          // type through — "250" on a 0-200 slider never gets past "2".
          onChange={(e) => {
            setDraft(e.target.value);
            const n = Number.parseFloat(e.target.value);
            if (Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n)));
          }}
          onBlur={() => setDraft(null)}
          className="w-[44px] flex-none rounded border border-input bg-transparent px-1 text-right text-[11px] tabular-nums text-muted-foreground outline-none focus:border-ring focus:text-foreground"
        />
      ) : (
        <span className="w-[44px] flex-none text-right text-[11px] tabular-nums text-muted-foreground">
          {format(value)}
        </span>
      )}
    </>
  );
}
