import { useState } from "react";

// `Row` and `Slider` moved to the shared ui controls once the 3D viewer's pose panel wanted
// the same rows; re-exported here so the Designer's own imports read as they always have.
export { Row, Slider } from "@/Components/ui/controls";

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
