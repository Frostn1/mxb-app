import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";

export type BuildStep = 1 | 2 | 3;

/**
 * The three-step gist of the tab, inline at the start of the one shared header row (not a
 * strip of its own — a step breadcrumb, a base-bike picker and a Blender status each in their
 * own bar was three rows saying less between them than one row can). Nothing here gates
 * anything — the whole screen already works in any order — it just says where a rider
 * probably is, so "what do I do first" isn't a question the layout leaves open.
 */
export default function StepHeader({ step }: { step: BuildStep }) {
  const t = useT();
  const steps: { n: BuildStep; label: string }[] = [
    { n: 1, label: t("bike.step1") },
    { n: 2, label: t("bike.step2") },
    { n: 3, label: t("bike.step3") },
  ];
  return (
    <div className="flex shrink-0 items-center gap-1.5 text-[11px]">
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
