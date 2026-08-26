import { useState } from "react";

/**
 * The controls the designer's side panels are made of.
 *
 * Shared rather than copied so the layer inspector and the paint tools stay identical instead
 * of merely similar — they sit one above the other in the same rail, and a row that lines up
 * differently in each reads as a bug.
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
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  format: (v: number) => string;
}) {
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
      <span className="w-[44px] flex-none text-right text-[11px] tabular-nums text-muted-foreground">
        {format(value)}
      </span>
    </>
  );
}

/**
 * A number typed rather than dragged.
 *
 * Held as text while it is being typed and read back as a number only on Enter or on leaving
 * the box. Committing per keystroke sounds simpler and isn't: "-" and "" are both states you
 * pass through on the way to `-40`, and each of them would snap the layer to zero and take the
 * caret with it.
 *
 * Shows the live value whenever it isn't being typed into, so dragging on the canvas moves the
 * number too — which is the answer to the old objection that a typed X was a second way of
 * saying the same thing without showing the result.
 */
export function NumberField({
  value,
  min,
  max,
  step = 1,
  title,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  title?: string;
  onChange: (v: number) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);

  const commit = (text: string) => {
    setDraft(null);
    const n = Number(text);
    if (text.trim() !== "" && Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n)));
  };

  return (
    <input
      type="text"
      inputMode="decimal"
      title={title}
      value={draft ?? String(Math.round(value / step) * step)}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={(e) => commit(e.target.value)}
      onKeyDown={(e) => {
        // Escape drops the draft and the live value comes back — nothing is committed.
        if (e.key === "Enter") e.currentTarget.blur();
        else if (e.key === "Escape") setDraft(null);
      }}
      className="h-6 min-w-0 flex-1 rounded-md border border-input bg-background px-1.5 text-center text-[11px] tabular-nums"
    />
  );
}
