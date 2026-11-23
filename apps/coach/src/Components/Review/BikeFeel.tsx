import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";

/** One thing a rider can say about the bike, with the setup tip it means. `end` marks the ones
 *  the travel used can argue with: 0 is the fork, 1 the shock. */
export type Feel = { key: TKey; skill: string; end?: 0 | 1; bottom?: boolean };

/** The part of the bike a feel is about. The drawing lights it up when one is picked. */
export type Part = "fork" | "shock" | "chassis" | "engine";

/** The feels, grouped at the part they concern, in the order they sit around the drawing. */
export const FEEL_GROUPS: { part: Part; key: TKey; feels: Feel[] }[] = [
  {
    part: "fork",
    key: "feel.group.fork",
    feels: [
      { key: "feel.frontBottoms", skill: "setup_bottoming_fork", end: 0, bottom: true },
      { key: "feel.frontHarsh", skill: "setup_stiff_fork", end: 0 },
      { key: "feel.dives", skill: "setup_brake_dive" },
      { key: "feel.packs", skill: "setup_packing_fork" },
      { key: "feel.pushes", skill: "setup_front_push" },
    ],
  },
  {
    part: "shock",
    key: "feel.group.shock",
    feels: [
      { key: "feel.rearBottoms", skill: "setup_bottoming_shock", end: 1, bottom: true },
      { key: "feel.rearHarsh", skill: "setup_stiff_shock", end: 1 },
      { key: "feel.squats", skill: "setup_exit_squat" },
      { key: "feel.kicks", skill: "setup_shock_kick" },
    ],
  },
  {
    part: "chassis",
    key: "feel.group.chassis",
    feels: [
      { key: "feel.unstable", skill: "setup_unstable" },
      { key: "feel.turnsSlow", skill: "setup_turns_slow" },
    ],
  },
  {
    part: "engine",
    key: "feel.group.engine",
    feels: [
      { key: "feel.revsOut", skill: "setup_gearing_tall" },
      { key: "feel.bogs", skill: "setup_gearing_short" },
    ],
  },
];

/** Every feel, flat, in the order the groups sit. */
export const FEELS: Feel[] = FEEL_GROUPS.flatMap((g) => g.feels);

/** Where each group sits once its column is wide enough: laid out like the bike, front end on
 *  the right and rear on the left, the side the rendered bike shows them on. */
const PLACE: Record<Part, string> = {
  fork: "@[560px]:col-start-3 @[560px]:row-start-2",
  shock: "@[560px]:col-start-1 @[560px]:row-start-2",
  chassis: "@[560px]:col-start-2 @[560px]:row-start-1",
  engine: "@[560px]:col-start-2 @[560px]:row-start-3",
};

/** The chips lean outwards, so each group reads as pointing at the end it is about. */
const ALIGN: Record<Part, string> = {
  fork: "@[560px]:justify-start",
  shock: "@[560px]:justify-end",
  chassis: "@[560px]:justify-center",
  engine: "@[560px]:justify-center",
};

/**
 * How the bike feels, as a bike: each thing a rider can say sits at the part it's about.
 *
 * There was a drawing of a bike in the middle of these chips. It was a drawing of *a* bike and
 * not the rider's, and the real one is now rendered above them and moves the part a pick is
 * about — which is what the drawing was standing in for. In a narrow column the groups stack,
 * each still labelled.
 */
export default function BikeFeel({ felt, onToggle, compact = false }: { felt: string[]; onToggle: (skill: string) => void; compact?: boolean }) {
  const t = useT();
  return (
    <div className="@container">
      <div className={cn("grid gap-x-4 gap-y-3", compact ? "grid-cols-2 items-start" : "@[560px]:grid-cols-[minmax(0,0.85fr)_minmax(190px,1.4fr)_minmax(0,0.85fr)] @[560px]:items-center")}>
        {FEEL_GROUPS.map((g) => (
          <div key={g.part} role="group" aria-label={t(g.key)} className={cn("min-w-0", !compact && PLACE[g.part])}>
            <div className={cn("mb-1.5 eyebrow", !compact && g.part === "shock" && "@[560px]:text-right")}>{t(g.key)}</div>
            <div className={cn("flex flex-wrap gap-1.5", !compact && ALIGN[g.part])}>
              {g.feels.map((f) => {
                const on = felt.includes(f.skill);
                return (
                  <button
                    key={f.skill}
                    type="button"
                    aria-pressed={on}
                    onClick={() => onToggle(f.skill)}
                    className={cn(
                      "rounded-full border px-2.5 py-1 text-left text-[12px] transition-colors",
                      on
                        ? "border-accent-foreground bg-accent text-accent-foreground"
                        : "border-border text-muted-foreground hover:border-foreground/30 hover:text-foreground",
                    )}
                  >
                    {t(f.key)}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
