/**
 * The two controls the designer's side panels are made of.
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
