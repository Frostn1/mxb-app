import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";

export type BuildStep = 1 | 2 | 3;

/**
 * The three-step gist of the tab, above everything else: base bike, then parts, then build.
 * Nothing here gates anything — the whole screen already works in any order — it just says
 * where a rider probably is, so "what do I do first" isn't a question the layout leaves open.
 */
export default function StepHeader({ step }: { step: BuildStep }) {
  const t = useT();
  const steps: { n: BuildStep; label: string }[] = [
    { n: 1, label: t("bike.step1") },
    { n: 2, label: t("bike.step2") },
    { n: 3, label: t("bike.step3") },
  ];
  return (
    <div className="flex shrink-0 items-center gap-1.5 border-b border-border bg-window px-4 py-1 text-[11px]">
      {steps.map((s, i) => (
        <span key={s.n} className="flex items-center gap-1.5">
          <span className={cn("font-semibold", s.n === step ? "text-primary" : "text-faint")}>
            {s.n} {s.label}
          </span>
          {i < steps.length - 1 && <span className="text-faint">→</span>}
        </span>
      ))}
    </div>
  );
}
