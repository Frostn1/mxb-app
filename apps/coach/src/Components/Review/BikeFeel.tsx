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

/**
 * A motocross bike from the side, facing left: two wheels the same size a wheelbase apart, the
 * fork on a believable rake, one flat line from the tank over the seat, the engine in the frame
 * and the swingarm out to the rear axle. Each part lights up when a feel beside it is on.
 */
function Bike({ lit }: { lit: Part[] }) {
  const on = (p: Part) => lit.includes(p);
  const c = (p: Part) => (on(p) ? "var(--primary)" : "currentColor");
  const w = (p: Part, base: number) => (on(p) ? base + 1 : base);
  return (
    <svg
      viewBox="0 0 360 200"
      aria-hidden="true"
      className="h-auto w-full text-muted-foreground"
      fill="none"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <line x1={26} y1={184} x2={334} y2={184} stroke="var(--border)" strokeWidth={1.5} />

      {/* Rear: wheel, swingarm and shock */}
      <circle cx={256} cy={140} r={42} stroke={c("shock")} strokeWidth={w("shock", 3)} />
      <circle cx={256} cy={140} r={26} stroke={c("shock")} strokeWidth={1} opacity={0.5} />
      <circle cx={256} cy={140} r={6} fill={c("shock")} stroke="none" />
      <line x1={196} y1={126} x2={256} y2={140} stroke={c("shock")} strokeWidth={w("shock", 8)} />
      <line x1={192} y1={82} x2={212} y2={124} stroke={c("shock")} strokeWidth={w("shock", 7)} />

      {/* Front: wheel, fork and fender */}
      <circle cx={80} cy={140} r={42} stroke={c("fork")} strokeWidth={w("fork", 3)} />
      <circle cx={80} cy={140} r={26} stroke={c("fork")} strokeWidth={1} opacity={0.5} />
      <circle cx={80} cy={140} r={6} fill={c("fork")} stroke="none" />
      <line x1={80} y1={140} x2={123} y2={55} stroke={c("fork")} strokeWidth={w("fork", 6)} />
      <line x1={88} y1={142} x2={131} y2={58} stroke={c("fork")} strokeWidth={w("fork", 3)} opacity={0.75} />
      <path d="M 42 112 Q 62 82 104 84" stroke={c("fork")} strokeWidth={w("fork", 3)} />

      {/* Chassis: bars, frame, tank, seat and rear fender */}
      <line x1={127} y1={50} x2={104} y2={41} stroke={c("chassis")} strokeWidth={w("chassis", 4)} />
      <path d="M 124 64 L 158 120 L 196 126 L 190 80 Z" stroke={c("chassis")} strokeWidth={w("chassis", 3)} />
      <path d="M 128 60 Q 154 58 180 66 L 180 72 Q 152 68 128 70 Z" stroke={c("chassis")} strokeWidth={w("chassis", 2.5)} />
      <path d="M 180 66 L 250 72 L 254 80 L 182 74 Z" stroke={c("chassis")} strokeWidth={w("chassis", 2.5)} />
      <path d="M 248 72 Q 286 74 302 92" stroke={c("chassis")} strokeWidth={w("chassis", 3)} />

      {/* Engine and pipe */}
      <rect x={152} y={100} width={50} height={44} rx={9} stroke={c("engine")} strokeWidth={w("engine", 3)} />
      <path
        d="M 156 104 C 138 102 132 122 146 132 C 176 150 226 148 250 136"
        stroke={c("engine")}
        strokeWidth={w("engine", 3)}
        opacity={0.85}
      />
    </svg>
  );
}

/** Where each group sits around the drawing once its column is wide enough for it. */
const PLACE: Record<Part, string> = {
  fork: "@[560px]:col-start-1 @[560px]:row-start-2",
  shock: "@[560px]:col-start-3 @[560px]:row-start-2",
  chassis: "@[560px]:col-start-2 @[560px]:row-start-1",
  engine: "@[560px]:col-start-2 @[560px]:row-start-3",
};

/** The chips lean towards the bike, so each group reads as pointing at its part. */
const ALIGN: Record<Part, string> = {
  fork: "@[560px]:justify-end",
  shock: "@[560px]:justify-start",
  chassis: "@[560px]:justify-center",
  engine: "@[560px]:justify-center",
};

/**
 * How the bike feels, as a bike: each thing a rider can say sits at the part it's about. In a
 * column too narrow for the drawing it drops out and the groups stack, each still labelled.
 */
export default function BikeFeel({ felt, onToggle }: { felt: string[]; onToggle: (skill: string) => void }) {
  const t = useT();
  const lit = FEEL_GROUPS.filter((g) => g.feels.some((f) => felt.includes(f.skill))).map((g) => g.part);
  return (
    <div className="@container">
      <div className="grid gap-x-4 gap-y-3 @[560px]:grid-cols-[minmax(0,0.85fr)_minmax(190px,1.4fr)_minmax(0,0.85fr)] @[560px]:items-center">
        {FEEL_GROUPS.map((g) => (
          <div key={g.part} role="group" aria-label={t(g.key)} className={cn("min-w-0", PLACE[g.part])}>
            <div className={cn("mb-1.5 eyebrow", g.part === "fork" && "@[560px]:text-right")}>{t(g.key)}</div>
            <div className={cn("flex flex-wrap gap-1.5", ALIGN[g.part])}>
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
        <div className="hidden @[560px]:col-start-2 @[560px]:row-start-2 @[560px]:block">
          <Bike lit={lit} />
        </div>
      </div>
    </div>
  );
}
