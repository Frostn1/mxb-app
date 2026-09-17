import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useT, type TKey } from "@/i18n";
import {
  coachSaveSetup,
  coachSelectSetup,
  coachSetupPlan,
  type Finding,
  type SavedSetup,
  type SetupChange,
  type SetupPlan,
} from "@/api/coach";
import { Label } from "../Page";
import BikeFeel, { FEELS, type Feel } from "./BikeFeel";

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

/** Telemetry can't see these at all. */
const ONLY_FELT = ["setup_unstable", "setup_turns_slow"];

/** A change with nowhere left to go: the setting is already at the end of its range. */
const atLimit = (c: SetupChange) =>
  !(c.fromValue && c.toValue && c.fromValue !== c.toValue) && c.from != null && c.from === c.to;

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

/** One change on its own line: the setting and how far it goes, with the reason under it. */
function Change({ c }: { c: SetupChange }) {
  const t = useT();
  return (
    <li className="grid grid-cols-[1fr_auto] items-baseline gap-x-3 gap-y-0.5">
      <span className="text-[12.5px] text-foreground">{t(`setupField.${c.field}` as TKey)}</span>
      <span className="whitespace-nowrap font-mono text-[12px] text-accent-foreground">{amount(c, t)}</span>
      <span className="col-span-2 text-[12px] leading-snug text-muted-foreground">
        {c.why}
        {!c.writes && c.from !== c.to && (
          <span className="ml-1.5 text-[11px] text-faint">{c.conflict ? t("setup.conflict") : t("setup.byHand")}</span>
        )}
      </span>
    </li>
  );
}

/** A list of changes by group, each setting named once. */
function Plan({ groups }: { groups: { key: TKey; changes: SetupChange[] }[] }) {
  const t = useT();
  return (
    <div className="space-y-3">
      {groups.map((g) => (
        <div key={g.key}>
          <div className="mb-1 text-[12px] font-semibold text-foreground">{t(g.key)}</div>
          <ul className="space-y-2 border-l border-border pl-3">
            {g.changes.map((c) => (
              <Change key={c.field} c={c} />
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}

/** Every change behind a set of tips, in group order, with each setting named once. The ones
 *  already as far as they go are counted rather than listed: they are noise in a list the
 *  rider reads top to bottom. */
function useGrouped(plan: SetupPlan | null, skills: string[]) {
  return useMemo(() => {
    if (!plan) return { groups: [], limited: 0 };
    const mine = plan.fixes.filter((f) => skills.includes(f.skill));
    const seen = new Set<string>();
    const all = SETUP_GROUPS.map((g) => {
      const changes = mine
        .filter((f) => g.of(f.skill))
        .flatMap((f) => f.changes)
        .filter((c) => !seen.has(c.field) && seen.add(c.field));
      return {
        key: g.key,
        changes: changes.filter((c) => !atLimit(c)),
        limited: changes.filter(atLimit).length,
      };
    });
    return {
      groups: all.filter((g) => g.changes.length > 0),
      limited: all.reduce((n, g) => n + g.limited, 0),
    };
  }, [plan, skills]);
}

/**
 * Bike setup advice for the whole lap: what the coach would change, said plainly and without
 * the rider picking anything, then how the bike feels as a bike. Shared by the review page and
 * the overlay.
 */
export default function SetupFixes({ path, findings }: { path: string; findings: Finding[] }) {
  const t = useT();
  const [plan, setPlan] = useState<SetupPlan | null>(null);
  const [saving, setSaving] = useState(false);
  /** The setup this page wrote, so it can be pointed at the game once MX Bikes is closed. */
  const [saved, setSaved] = useState<SavedSetup | null>(null);
  const [felt, setFelt] = useState<string[]>([]);
  // Kept apart from the plan so a refetch never flips a feel's check.
  const [used, setUsed] = useState<[number, number] | null>(null);
  useEffect(() => {
    setPlan(null);
    setFelt([]);
    setUsed(null);
    setSaved(null);
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

  // What the coach found on its own, and what the rider's feels added on top.
  const found = useMemo(() => findings.map((f) => f.skill), [findings]);
  const extra = useMemo(() => skills.filter((s) => !found.includes(s)), [skills, found]);
  const foundGroups = useGrouped(plan, found);
  const feltGroups = useGrouped(plan, extra);

  const writes = (plan?.saveAs != null && plan.fixes.some((f) => f.changes.some((c) => c.writes))) ?? false;
  // The one line over the button: how many changes a copy gets, and the first couple by name.
  const summary = useMemo(() => {
    if (!plan) return "";
    const seen = new Set<string>();
    const all = plan.fixes
      .flatMap((f) => f.changes)
      .filter((c) => c.writes && !atLimit(c) && !seen.has(c.field) && seen.add(c.field));
    if (all.length === 0) return "";
    const named = all.slice(0, 2).map((c) => `${t(`setupField.${c.field}` as TKey).toLowerCase()} ${amount(c, t)}`);
    const rest = all.length - named.length;
    const what = [...named, ...(rest > 0 ? [t("setup.andMore", { n: rest })] : [])].join(", ");
    return all.length === 1 ? t("setup.summaryOne", { what }) : t("setup.summaryMany", { n: all.length, what });
  }, [plan, t]);

  const save = async () => {
    setSaving(true);
    try {
      const out = await coachSaveSetup(path, skills);
      setSaved(out);
      // Named, so the rider can check each one in the garage rather than take our word for it.
      const list = out.changed.map((f) => t(`setupField.${f}` as TKey)).join(", ");
      const body = { description: list ? t("setup.savedChanged", { list }) : undefined };
      // Saved either way. Whether the game will load it is its own record's answer, not ours,
      // so the rider hears which of the three it is.
      if (out.selected) toast.success(t("setup.selected", { name: out.name }), body);
      else if (out.gameOpen) toast.success(t("setup.savedGameOpen", { name: out.name }), body);
      else toast.warning(t("setup.savedNotSelected", { name: out.name }), body);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  /** Point the game at a setup already written, for when MX Bikes was open at the time. */
  const selectIt = async () => {
    if (!saved) return;
    setSaving(true);
    try {
      const out = await coachSelectSetup(path, saved.name);
      setSaved(out);
      if (out.selected) toast.success(t("setup.selectDone", { name: out.name }));
      else if (out.gameOpen) toast.error(t("setup.selectGameOpen"));
      else toast.error(t("setup.selectMissed"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      <Label>{t("review.setup")}</Label>
      <div className="border border-border bg-card px-4 py-4">
        {/* The copy it would save, first: the button and what it changes, in one line. */}
        {plan && (writes || plan.why) && (
          <div className="border-b border-border pb-4">
            {writes ? (
              <div className="space-y-2">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
                  {/* Once per lap read: pressing it again writes the same setup under the next
                      free name, which is how a rider ends up with a pile of them. */}
                  <Button size="sm" onClick={save} disabled={saving || saved != null}>
                    {t("setup.save", { name: plan.saveAs ?? "" })}
                  </Button>
                  <span className="min-w-0 flex-1 text-[12px] text-muted-foreground">
                    {saved ? t("setup.savedOnce", { name: saved.name }) : summary || t("setup.saveHint")}
                  </span>
                </div>
                {/* The game holds the file open while it runs, so offer the pick separately. */}
                {saved && !saved.selected && (
                  <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
                    <Button size="sm" variant="outline" onClick={selectIt} disabled={saving}>
                      {t("setup.select")}
                    </Button>
                    <span className="min-w-0 flex-1 text-[12px] text-muted-foreground">{t("setup.selectHint")}</span>
                  </div>
                )}
              </div>
            ) : (
              <p className="text-[12px] text-muted-foreground">{plan.why}</p>
            )}
          </div>
        )}

        <div className="mt-4 grid gap-x-8 gap-y-5 min-[1100px]:grid-cols-2 min-[1100px]:items-start">
          {/* Everything the coach would change, without the rider picking anything. */}
          <div className="min-w-0">
            <div className="mb-1.5 eyebrow">{t("setup.plan")}</div>
            <p className="mb-3 text-[12.5px] text-muted-foreground">{t("setup.planBody")}</p>
            {foundGroups.groups.length === 0 ? (
              <p className="text-[12.5px] text-muted-foreground">{plan ? t("setup.planNone") : t("common.loading")}</p>
            ) : (
              <Plan groups={foundGroups.groups} />
            )}
            {foundGroups.limited > 0 && (
              <p className="mt-3 text-[12px] text-faint">
                {foundGroups.limited === 1 ? t("setup.atLimitOne") : t("setup.atLimitMany", { n: foundGroups.limited })}
              </p>
            )}
          </div>

          {/* How the bike feels, as a bike. */}
          <div className="min-w-0 border-t border-border pt-5 min-[1100px]:border-l min-[1100px]:border-t-0 min-[1100px]:pl-8 min-[1100px]:pt-0">
            <div className="mb-1.5 eyebrow">{t("feel.title")}</div>
            <p className="mb-4 text-[12.5px] text-muted-foreground">{t("feel.body")}</p>
            <BikeFeel felt={felt} onToggle={(skill) => setFelt((v) => (v.includes(skill) ? v.filter((s) => s !== skill) : [...v, skill]))} />
            {feels.length > 0 && (
              <div className="mt-4 space-y-2">
                {feels.map(({ f, seen, against }) => {
                  const says = seen
                    ? t("feel.agrees")
                    : against != null
                      ? t(f.bottom ? "feel.notBottoming" : "feel.usesAll", { pct: against })
                      : t(ONLY_FELT.includes(f.skill) ? "feel.onlyYou" : "feel.notSeen");
                  return (
                    <p key={f.skill} className="text-[12.5px]">
                      <span className="text-foreground">{t(f.key)}.</span> <span className="text-muted-foreground">{says}</span>
                    </p>
                  );
                })}
              </div>
            )}
            {feltGroups.groups.length > 0 && (
              <div className="mt-4 space-y-3">
                <div className="eyebrow">{t("setup.fromFeel")}</div>
                <Plan groups={feltGroups.groups} />
              </div>
            )}
          </div>
        </div>

        {plan?.sag && (
          <p className="mt-5 border-t border-border pt-4 text-[12px] text-muted-foreground">
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
      </div>
    </div>
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
