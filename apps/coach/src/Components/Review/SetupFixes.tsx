import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useT, type TKey } from "@/i18n";
import {
  coachSaveSetup,
  coachSetupPlan,
  type Finding,
  type SetupChange,
  type SetupFix,
  type SetupPlan,
} from "@/api/coach";
import { Label } from "../Page";

const SUSPENSION = ["setup_brake_dive", "setup_exit_squat", "setup_shock_kick", "setup_rear_low", "setup_front_low"];
const SETUP_GROUPS: { key: TKey; of: (skill: string) => boolean }[] = [
  {
    key: "review.group.suspension",
    of: (s) =>
      s.startsWith("setup_bottoming") ||
      s.startsWith("setup_stiff") ||
      s.startsWith("setup_packing") ||
      s.startsWith("setup_sag") ||
      SUSPENSION.includes(s),
  },
  { key: "review.group.gearing", of: (s) => s.startsWith("setup_gearing") },
  { key: "review.group.shifting", of: (s) => s.startsWith("setup_shift") },
  { key: "review.group.chassis", of: (s) => s === "setup_swingarm" || s === "setup_front_push" },
  { key: "review.group.tyres", of: (s) => s === "setup_pressure" },
  { key: "review.group.ground", of: (s) => s === "setup_sand" || s === "setup_hardpack" || s === "setup_mud" },
];

type Feel = { key: TKey; skill: string; end?: 0 | 1; bottom?: boolean };

/** What a rider can say about the bike, each with the setup tip it means. `end` marks the ones
 *  the travel used can argue with. */
const FEELS: Feel[] = [
  { key: "feel.frontBottoms", skill: "setup_bottoming_fork", end: 0, bottom: true },
  { key: "feel.rearBottoms", skill: "setup_bottoming_shock", end: 1, bottom: true },
  { key: "feel.frontHarsh", skill: "setup_stiff_fork", end: 0 },
  { key: "feel.rearHarsh", skill: "setup_stiff_shock", end: 1 },
  { key: "feel.dives", skill: "setup_brake_dive" },
  { key: "feel.squats", skill: "setup_exit_squat" },
  { key: "feel.kicks", skill: "setup_shock_kick" },
  { key: "feel.packs", skill: "setup_packing_fork" },
  { key: "feel.pushes", skill: "setup_front_push" },
  { key: "feel.unstable", skill: "setup_unstable" },
  { key: "feel.turnsSlow", skill: "setup_turns_slow" },
  { key: "feel.revsOut", skill: "setup_gearing_tall" },
  { key: "feel.bogs", skill: "setup_gearing_short" },
];
/** Telemetry can't see these at all. */
const ONLY_FELT = ["setup_unstable", "setup_turns_slow"];

/** What the laps say about a feel: they show it too, or the travel used says otherwise (the
 *  share, in percent), or neither. */
function feelCheck(f: Feel, findings: Finding[], used: [number, number] | null): { seen: boolean; against?: number } {
  if (findings.some((x) => x.skill === f.skill)) return { seen: true };
  if (f.end != null && used) {
    const u = used[f.end];
    if (f.bottom ? u < 0.9 : u > 0.95) return { seen: false, against: Math.round(u * 100) };
  }
  return { seen: false };
}

/** Bike setup advice for the whole lap, by what it's about, with the changes behind each tip
 *  against the setup the rider had on, and a copy of that setup with them made. Shared by the
 *  review page and the overlay. */
export default function SetupFixes({ path, findings }: { path: string; findings: Finding[] }) {
  const t = useT();
  const [plan, setPlan] = useState<SetupPlan | null>(null);
  const [saving, setSaving] = useState(false);
  const [felt, setFelt] = useState<string[]>([]);
  // Kept apart from the plan so a refetch never flips a feel's check.
  const [used, setUsed] = useState<[number, number] | null>(null);
  useEffect(() => {
    setPlan(null);
    setFelt([]);
    setUsed(null);
  }, [path]);
  const feels = useMemo(
    () => FEELS.filter((f) => felt.includes(f.skill)).map((f) => ({ f, ...feelCheck(f, findings, used) })),
    [felt, findings, used],
  );
  // A feel the laps argue with adds no fix. Keyed on the names so the same list never refetches.
  const key = [...new Set([...findings.map((f) => f.skill), ...feels.filter((x) => x.against == null).map((x) => x.f.skill)])].join(",");
  const skills = useMemo(() => key.split(",").filter(Boolean), [key]);
  useEffect(() => {
    let live = true;
    coachSetupPlan(path, skills)
      .then((p) => {
        if (!live) return;
        setPlan(p);
        setUsed(p.travelUsed);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [path, skills]);
  const writes = (plan?.saveAs != null && plan.fixes.some((f) => f.changes.some((c) => c.writes))) ?? false;
  const save = async () => {
    setSaving(true);
    try {
      const saved = await coachSaveSetup(path, skills);
      // Named, so the rider can check each one in the garage rather than take our word for it.
      const list = saved.changed.map((f) => t(`setupField.${f}` as TKey)).join(", ");
      toast.success(t("setup.saved", { name: saved.name }), {
        description: list ? t("setup.savedChanged", { list }) : undefined,
      });
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div>
      <Label>{t("review.setup")}</Label>
      <div className="space-y-4 border border-border bg-card px-4 py-3">
        <div>
          <div className="mb-1.5 eyebrow">{t("feel.title")}</div>
          <p className="mb-2 text-[12.5px] text-muted-foreground">{t("feel.body")}</p>
          <div className="flex flex-wrap gap-1.5">
            {FEELS.map((f) => {
              const on = felt.includes(f.skill);
              return (
                <button
                  key={f.skill}
                  type="button"
                  aria-pressed={on}
                  onClick={() => setFelt((v) => (on ? v.filter((s) => s !== f.skill) : [...v, f.skill]))}
                  className={`border px-2 py-1 text-[12px] ${
                    on ? "border-accent-foreground text-accent-foreground" : "border-border text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t(f.key)}
                </button>
              );
            })}
          </div>
          {feels.length > 0 && (
            <div className="mt-3 space-y-3">
              {feels.map(({ f, seen, against }) => {
                const fix = !seen && against == null ? plan?.fixes.find((x) => x.skill === f.skill) : undefined;
                const says = seen
                  ? t("feel.agrees")
                  : against != null
                    ? t(f.bottom ? "feel.notBottoming" : "feel.usesAll", { pct: against })
                    : t(ONLY_FELT.includes(f.skill) ? "feel.onlyYou" : "feel.notSeen");
                return (
                  <div key={f.skill}>
                    <p className="text-[12.5px]">
                      <span className="text-foreground">{t(f.key)}.</span> <span className="text-muted-foreground">{says}</span>
                    </p>
                    {fix && <Changes fix={fix} />}
                  </div>
                );
              })}
            </div>
          )}
        </div>
        {SETUP_GROUPS.map((g) => {
          const mine = findings.filter((f) => g.of(f.skill));
          if (mine.length === 0) return null;
          return (
            <div key={g.key}>
              <div className="mb-1.5 eyebrow">{t(g.key)}</div>
              {/* Each tip with the changes behind it right under it. */}
              <div className="space-y-3">
                {mine.map((f) => {
                  const fix = plan?.fixes.find((x) => x.skill === f.skill);
                  return (
                    <div key={f.skill + f.title}>
                      <Notes findings={[f]} />
                      {fix && <Changes fix={fix} />}
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
        {plan?.sag && (
          <p className="text-[12px] text-muted-foreground">
            {plan.sag.still
              ? t("setup.sagStill", {
                  front: Math.round(plan.sag.metres[0] * 1000),
                  fp: Math.round(plan.sag.share[0] * 100),
                  rear: Math.round(plan.sag.metres[1] * 1000),
                  rp: Math.round(plan.sag.share[1] * 100),
                })
              : t("setup.sagRiding")}
          </p>
        )}
        {plan && (writes || plan.why) && (
          <div className="border-t border-border pt-3">
            {writes ? (
              <div className="flex flex-wrap items-center gap-3">
                <Button size="sm" onClick={save} disabled={saving}>
                  {t("setup.save", { name: plan.saveAs ?? "" })}
                </Button>
                <span className="text-[12px] text-muted-foreground">{t("setup.saveHint")}</span>
              </div>
            ) : (
              <p className="text-[12px] text-muted-foreground">{plan.why}</p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/** How far one change goes: the values where the bike's file says, else the direction. */
function amount(c: SetupChange, t: ReturnType<typeof useT>): string {
  if (c.fromValue && c.toValue && c.fromValue !== c.toValue) return `${c.fromValue} → ${c.toValue}`;
  if (c.from != null && c.from === c.to) return t("setup.atLimit");
  const n = Math.abs(c.steps);
  if (c.field === "forkOil") return t(c.steps > 0 ? "setup.moreOil" : "setup.lessOil");
  if (c.field === "frontSprocket" || c.field === "rearSprocket") return `${c.steps > 0 ? "+" : "−"}${n}T`;
  if (c.field === "swingarmLength" || c.field === "rodLength") return t(c.steps > 0 ? "setup.longer" : "setup.shorter");
  if (c.field === "forkHeight") return t(c.steps > 0 ? "setup.frontHigher" : "setup.frontLower");
  if (c.field === "forkOffset") return t(c.steps > 0 ? "setup.moreOffset" : "setup.lessOffset");
  const up = c.steps > 0;
  // Rebound reads as slower or faster, preload as more or less, the rest firmer or softer.
  const [one, many]: [TKey, TKey] =
    c.field === "forkRebound" || c.field === "shockRebound"
      ? up ? ["setup.slowerOne", "setup.slowerMany"] : ["setup.fasterOne", "setup.fasterMany"]
      : c.field === "forkPreload" || c.field === "shockPreload"
        ? up ? ["setup.moreOne", "setup.moreMany"] : ["setup.lessOne", "setup.lessMany"]
        : up ? ["setup.firmerOne", "setup.firmerMany"] : ["setup.softerOne", "setup.softerMany"];
  return n === 1 ? t(one) : t(many, { n });
}

/** The changes behind one tip, in the order to try them. */
function Changes({ fix }: { fix: SetupFix }) {
  const t = useT();
  return (
    <ol className="mt-2 space-y-1.5">
      {fix.changes.map((c, i) => (
        <li key={c.field} className="grid grid-cols-[14px_1fr_auto] items-baseline gap-x-2 text-[12.5px]">
          <span className="font-mono text-faint">{i + 1}</span>
          <span>
            <span className="text-foreground">{t(`setupField.${c.field}` as TKey)}</span>{" "}
            <span className="text-muted-foreground">{c.why}</span>
            {!c.writes && c.from !== c.to && (
              <span className="ml-1.5 text-[11px] text-faint">
                {c.conflict ? t("setup.conflict") : t("setup.byHand")}
              </span>
            )}
          </span>
          <span className="whitespace-nowrap font-mono text-[12px] text-accent-foreground">{amount(c, t)}</span>
        </li>
      ))}
    </ol>
  );
}

export function Num({ n }: { n: number }) {
  return (
    <span className="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full bg-foreground text-[10px] font-bold text-background">
      {n}
    </span>
  );
}

/** Tips as a list; numbered from `start` when they match chart markers. */
export function Notes({ findings, start }: { findings: Finding[]; start?: number }) {
  return (
    <ul className="space-y-3">
      {findings.map((f, k) => (
        <li key={k} className="flex gap-2.5">
          {start != null && <Num n={start + k} />}
          <div>
            <div className="text-[13px] font-semibold">{f.title}</div>
            <div className="mt-0.5 text-[12.5px] leading-snug text-muted-foreground">{f.detail}</div>
          </div>
        </li>
      ))}
    </ul>
  );
}
