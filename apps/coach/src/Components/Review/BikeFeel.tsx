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

/** Knobbly tyre: a suggestion of blocks round the rim, not every knob. */
function Knobs({ cx, cy, r, colour, width }: { cx: number; cy: number; r: number; colour: string; width: number }) {
  const n = 16;
  return (
    <g stroke={colour} strokeWidth={width}>
      {Array.from({ length: n }, (_, i) => {
        const a = (i / n) * Math.PI * 2;
        const [c, s] = [Math.cos(a), Math.sin(a)];
        return <line key={i} x1={cx + c * r} y1={cy + s * r} x2={cx + c * (r + 4)} y2={cy + s * (r + 4)} />;
      })}
    </g>
  );
}

/**
 * A modern four-stroke motocrosser from its right-hand side, front wheel to the right: tall on
 * its suspension with the engine high off the ground, a 21 in front and 19 in rear on knobblies,
 * long upside-down forks, the mudguard riding well clear of the front wheel, and one near-flat
 * line from the bars to the tail. Each part lights up when a feel beside it is on.
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
      <line x1={30} y1={176} x2={330} y2={176} stroke="var(--border)" strokeWidth={1.5} />

      {/* Rear wheel, 19 in, with the sprocket */}
      <circle cx={96} cy={138} r={38} stroke={c("shock")} strokeWidth={w("shock", 2.5)} />
      <Knobs cx={96} cy={138} r={38} colour={c("shock")} width={w("shock", 1.5)} />
      <circle cx={96} cy={138} r={25} stroke={c("shock")} strokeWidth={1} opacity={0.5} />
      <circle cx={96} cy={138} r={13} stroke={c("shock")} strokeWidth={w("shock", 1.5)} opacity={0.75} />
      <circle cx={96} cy={138} r={4} fill={c("shock")} stroke="none" />

      {/* Swingarm, shock under the seat, and the chain */}
      <line x1={148} y1={112} x2={96} y2={138} stroke={c("shock")} strokeWidth={w("shock", 7)} />
      <line x1={158} y1={94} x2={150} y2={76} stroke={c("shock")} strokeWidth={w("shock", 6)} />
      <line x1={150} y1={106} x2={95} y2={126} stroke={c("shock")} strokeWidth={w("shock", 1.5)} opacity={0.8} />
      <circle cx={150} cy={108} r={5} stroke={c("shock")} strokeWidth={1.5} opacity={0.8} />

      {/* Front wheel, 21 in */}
      <circle cx={272} cy={134} r={42} stroke={c("fork")} strokeWidth={w("fork", 2.5)} />
      <Knobs cx={272} cy={134} r={42} colour={c("fork")} width={w("fork", 1.5)} />
      <circle cx={272} cy={134} r={28} stroke={c("fork")} strokeWidth={1} opacity={0.5} />
      <circle cx={272} cy={134} r={15} stroke={c("fork")} strokeWidth={w("fork", 1.5)} opacity={0.75} />
      <circle cx={272} cy={134} r={4} fill={c("fork")} stroke="none" />

      {/* Upside-down forks, guard, and the mudguard riding high over the wheel */}
      <line x1={228} y1={44} x2={254} y2={98} stroke={c("fork")} strokeWidth={w("fork", 7)} />
      <line x1={254} y1={98} x2={272} y2={134} stroke={c("fork")} strokeWidth={w("fork", 4.5)} />
      <line x1={258} y1={102} x2={270} y2={128} stroke={c("fork")} strokeWidth={w("fork", 2.5)} opacity={0.7} />
      <path d="M 226 74 Q 262 58 302 80" stroke={c("fork")} strokeWidth={w("fork", 3)} />

      {/* Bars, front plate, and the long flat tank-to-tail line */}
      <line x1={228} y1={44} x2={228} y2={37} stroke={c("chassis")} strokeWidth={w("chassis", 3)} />
      <line x1={208} y1={35} x2={248} y2={34} stroke={c("chassis")} strokeWidth={w("chassis", 4)} />
      <path d="M 244 40 Q 262 50 254 70" stroke={c("chassis")} strokeWidth={w("chassis", 2.5)} opacity={0.85} />
      <path d="M 218 68 C 192 68 152 72 126 74 Q 102 78 88 66" stroke={c("chassis")} strokeWidth={w("chassis", 3)} />
      <line x1={124} y1={82} x2={192} y2={76} stroke={c("chassis")} strokeWidth={w("chassis", 2)} opacity={0.8} />
      <path d="M 196 72 L 212 70 L 218 94 L 200 98 Z" stroke={c("chassis")} strokeWidth={w("chassis", 2.5)} />

      {/* Frame: head, spar, downtube and cradle */}
      <line x1={224} y1={54} x2={196} y2={72} stroke={c("chassis")} strokeWidth={w("chassis", 3)} />
      <path d="M 224 58 C 214 78 204 92 196 104" stroke={c("chassis")} strokeWidth={w("chassis", 3)} />
      <path d="M 196 108 L 158 118 L 148 112" stroke={c("chassis")} strokeWidth={w("chassis", 2.5)} />
      <line x1={190} y1={76} x2={124} y2={80} stroke={c("chassis")} strokeWidth={w("chassis", 2)} opacity={0.8} />
      <line x1={146} y1={118} x2={136} y2={121} stroke={c("chassis")} strokeWidth={w("chassis", 3)} />

      {/* Engine low in the frame, skid plate, header up into the silencer */}
      <rect x={156} y={94} width={42} height={30} rx={6} stroke={c("engine")} strokeWidth={w("engine", 3)} />
      <path d="M 178 94 L 196 90 L 200 74 L 182 78 Z" stroke={c("engine")} strokeWidth={w("engine", 2.5)} />
      <path d="M 152 124 Q 176 132 200 120" stroke={c("engine")} strokeWidth={w("engine", 2.5)} opacity={0.85} />
      <path
        d="M 200 80 C 216 76 222 90 214 100 C 198 114 160 110 138 96 C 126 89 112 85 102 83"
        stroke={c("engine")}
        strokeWidth={w("engine", 2.5)}
        opacity={0.85}
      />
      <line x1={122} y1={88} x2={94} y2={80} stroke={c("engine")} strokeWidth={w("engine", 6)} />
    </svg>
  );
}

/** Where each group sits around the drawing once its column is wide enough: the bike faces
 *  right, so the front end is on the right and the rear on the left. */
const PLACE: Record<Part, string> = {
  fork: "@[560px]:col-start-3 @[560px]:row-start-2",
  shock: "@[560px]:col-start-1 @[560px]:row-start-2",
  chassis: "@[560px]:col-start-2 @[560px]:row-start-1",
  engine: "@[560px]:col-start-2 @[560px]:row-start-3",
};

/** The chips lean towards the bike, so each group reads as pointing at its part. */
const ALIGN: Record<Part, string> = {
  fork: "@[560px]:justify-start",
  shock: "@[560px]:justify-end",
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
            <div className={cn("mb-1.5 eyebrow", g.part === "shock" && "@[560px]:text-right")}>{t(g.key)}</div>
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
