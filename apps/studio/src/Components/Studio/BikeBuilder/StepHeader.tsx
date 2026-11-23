import { Check } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";

export type BuildStep = 1 | 2 | 3;

/**
 * The three-step gist of the tab: Base bike → Parts → Build. It says where a rider probably
 * is, and a stage they've already been through is a button back to it — going back never
 * costs anything by itself (the parts stay where they were placed); only a choice made there
 * that would replace work asks first, and that ask lives with the choice, not here.
 *
 * `reached` is how far the build has actually got; `active` is the stage being looked at,
 * which is `reached` unless the rider stepped back.
 */
export default function StepHeader({
  reached,
  active,
  onGo,
}: {
  reached: BuildStep;
  active: BuildStep;
  onGo: (step: BuildStep) => void;
}) {
  const t = useT();
  const steps: { n: BuildStep; label: string }[] = [
    { n: 1, label: t("bike.step1") },
    { n: 2, label: t("bike.step2") },
    { n: 3, label: t("bike.step3") },
  ];
  return (
    <nav className="flex shrink-0 items-center gap-1 text-[11.5px]" aria-label={t("bike.build")}>
      {steps.map((s, i) => {
        const done = s.n < reached;
        const here = s.n === active;
        const open = s.n <= reached && !here;
        return (
          <span key={s.n} className="flex items-center gap-1">
            <button
              type="button"
              disabled={!open}
              onClick={() => onGo(s.n)}
              aria-current={here ? "step" : undefined}
              title={
                open ? t("bike.goBack", { step: s.label }) : s.n > reached ? t(`bike.hint.locked${s.n as 2 | 3}`) : undefined
              }
              className={cn(
                "flex items-center gap-1 rounded px-1.5 py-0.5 font-semibold transition-colors",
                // The shared selected look (studio.css), not a tint of our own.
                here && "u-selected",
                !here && open && "text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
                !here && !open && "text-faint",
                "disabled:cursor-default",
              )}
            >
              {done ? (
                <Check className="size-3 text-success" />
              ) : (
                <span className="tabular-nums">{s.n}</span>
              )}
              {s.label}
            </button>
            {i < steps.length - 1 && <span className="text-faint">→</span>}
          </span>
        );
      })}
    </nav>
  );
}
