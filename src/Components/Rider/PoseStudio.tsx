import { useCallback, useEffect, useMemo, useState } from "react";
import { RotateCcw, User } from "lucide-react";
import { useT, type TKey } from "../../i18n/context";
import { Button } from "../ui/button";
import { Row, Slider } from "../ui/controls";
import type { Loadout } from "../../types";
import { ViewerPanel } from "../Viewer/ViewerPanel";
import { EMPTY_LOADOUT, loadScans, pickedModel, type Scans } from "../../lib/presets";
import { useConfig } from "../../Context/Config";
import {
  applyQuickMove,
  BONE_GROUPS,
  boneLabel,
  clampTurn,
  isRestPose,
  NO_POSE,
  QUICK_MOVES,
  TURN_LIMIT,
  turnOf,
  withTurn,
  type BoneGroupId,
  type QuickMoveId,
  type RiderPose,
} from "../../lib/riderPose";

/**
 * Where a rider's pose is remembered, keyed by the profile it was built for.
 *
 * Machine-local rather than part of the preset: a pose is a preview, and nothing the game
 * reads. Putting it in a preset would put it in share codes too, and those have to keep
 * meaning the same thing to an older build.
 */
const POSE_KEY = "mxb.pose.v1";

const GROUP_LABEL: Record<BoneGroupId, TKey> = {
  torso: "pose.group.torso",
  arms: "pose.group.arms",
  hands: "pose.group.hands",
  legs: "pose.group.legs",
};

const MOVE_LABEL: Record<QuickMoveId, TKey> = {
  legsWide: "pose.move.legsWide",
  legsNarrow: "pose.move.legsNarrow",
  leftLegForward: "pose.move.leftLegForward",
  elbowsUp: "pose.move.elbowsUp",
  leanIn: "pose.move.leanIn",
};

/** The three turns of a bone, in the order the sliders show them. */
const AXES: { at: 0 | 1 | 2; label: TKey }[] = [
  { at: 0, label: "pose.axis.bend" },
  { at: 1, label: "pose.axis.twist" },
  { at: 2, label: "pose.axis.splay" },
];

function readSaved(profile: string): RiderPose {
  try {
    const all = JSON.parse(localStorage.getItem(POSE_KEY) ?? "{}") as Record<string, RiderPose>;
    return all[profile] ?? NO_POSE;
  } catch {
    return NO_POSE;
  }
}

function writeSaved(profile: string, pose: RiderPose): void {
  try {
    const all = JSON.parse(localStorage.getItem(POSE_KEY) ?? "{}") as Record<string, RiderPose>;
    if (isRestPose(pose)) delete all[profile];
    else all[profile] = pose;
    localStorage.setItem(POSE_KEY, JSON.stringify(all));
  } catch {
    // A browser with storage turned off still poses; it just forgets between visits.
  }
}

interface PoseStudioProps {
  /** The preset to show. The Pose view never edits it — see the note on the summary below. */
  initialLoadout?: Loadout | null;
  initialBike?: string | null;
  onLoaded?: () => void;
}

/**
 * The Pose studio.
 *
 * A preset as it stands — bike, model swap, rider, gear, paints — with one thing you can
 * change: where the rider's limbs are. Everything else is deliberately read-only. The Rider
 * tab next door is where a look is composed; this is where it is stood in a position, and a
 * second set of pickers here would only be a second place to change the same slots.
 *
 * The pose reaches the preview and nothing else. MX Bikes takes the rider's posture from a
 * riding style — an animation set in `mods/rider/animations` — and nothing this writes could
 * change that.
 */
export default function PoseStudio({
  initialLoadout,
  initialBike,
  onLoaded,
}: PoseStudioProps) {
  const t = useT();
  const { bikePreview } = useConfig();
  const [scans, setScans] = useState<Scans | null>(null);
  const [loadout, setLoadout] = useState<Loadout>(EMPTY_LOADOUT);
  const [bike, setBike] = useState("");
  const [pose, setPose] = useState<RiderPose>(NO_POSE);
  // Closed to start: the dots on the rider are the way in, and a wall of sliders reads as the
  // opposite of that.
  const [open, setOpen] = useState<BoneGroupId | null>(null);

  useEffect(() => {
    let alive = true;
    loadScans()
      .then((s) => alive && setScans(s))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  // A preset handed over by Presets or the Rider tab.
  useEffect(() => {
    if (!initialLoadout) return;
    setLoadout(initialLoadout);
    if (initialBike) setBike(initialBike);
    onLoaded?.();
  }, [initialLoadout, initialBike, onLoaded]);

  // Each rider profile keeps its own pose: the rigs differ in what they bind, and a turn that
  // suits one model's shoulders is not the same turn on another's.
  const profile = loadout.rider || "default";
  useEffect(() => setPose(readSaved(profile)), [profile]);
  useEffect(() => writeSaved(profile, pose), [profile, pose]);

  const bikeVariant = useMemo(
    () => loadout.modelSwap || pickedModel(bike, loadout, scans),
    [bike, loadout, scans],
  );
  const showBike = bikePreview && !!bike;

  const turn = useCallback(
    (bone: string, at: 0 | 1 | 2, deg: number) => {
      setPose((p) => {
        const next = turnOf(p, bone);
        next[at] = clampTurn(deg);
        return withTurn(p, bone, next);
      });
    },
    [],
  );

  const summary: { label: TKey; value: string }[] = [
    { label: "pose.bike", value: bike },
    { label: "slot.modelSwap", value: loadout.modelSwap },
    { label: "slot.rider", value: loadout.rider },
    { label: "slot.helmet", value: loadout.helmet },
    { label: "slot.boots", value: loadout.boots },
    { label: "slot.protection", value: loadout.protection },
  ];

  return (
    <div className="flex min-h-0 flex-1 gap-4 px-7 pb-6">
      <div className="flex min-w-[300px] flex-1 flex-col gap-4 overflow-y-auto">
        {/* What is being posed. Read-only on purpose — see the component note. */}
        <section className="rounded-lg border border-border bg-card/40 p-3">
          <header className="mb-2 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
            <User className="h-3.5 w-3.5" />
            {t("pose.showing")}
          </header>
          <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-[11px]">
            {summary.map((s) => (
              <div key={s.label} className="flex min-w-0 justify-between gap-2">
                <dt className="text-muted-foreground">{t(s.label)}</dt>
                <dd className="truncate font-medium" title={s.value}>
                  {s.value || t("pose.none")}
                </dd>
              </div>
            ))}
          </dl>
        </section>

        <section>
          <header className="mb-2 flex items-center justify-between">
            <h2 className="text-[11px] font-medium text-muted-foreground">{t("pose.quick")}</h2>
            <Button
              size="sm"
              variant="ghost"
              className="h-6 gap-1 px-2 text-[11px]"
              disabled={isRestPose(pose)}
              onClick={() => setPose(NO_POSE)}
            >
              <RotateCcw className="h-3 w-3" />
              {t("pose.reset")}
            </Button>
          </header>
          <div className="flex flex-wrap gap-1.5">
            {QUICK_MOVES.map((m) => (
              <Button
                key={m.id}
                size="sm"
                variant="outline"
                className="h-7 px-2.5 text-[11px]"
                onClick={() => setPose((p) => applyQuickMove(p, m))}
              >
                {t(MOVE_LABEL[m.id])}
              </Button>
            ))}
          </div>
          <p className="mt-1.5 text-[10px] leading-snug text-muted-foreground">
            {t("pose.quickHint")}
          </p>
        </section>

        <p className="-mb-1 text-[10px] leading-snug text-muted-foreground">
          {t("pose.dragHint")}
        </p>

        {BONE_GROUPS.map((g) => (
          <section key={g.id} className="rounded-lg border border-border">
            <button
              type="button"
              className="flex w-full items-center justify-between px-3 py-2 text-left text-[12px] font-medium"
              onClick={() => setOpen((o) => (o === g.id ? null : g.id))}
            >
              {t(GROUP_LABEL[g.id])}
              <span className="text-[10px] text-muted-foreground">
                {g.bones.filter((b) => pose[b]).length || ""}
              </span>
            </button>
            {open === g.id && (
              <div className="flex flex-col gap-3 border-t border-border px-3 py-2.5">
                {g.bones.map((bone) => (
                  <div key={bone} className="flex flex-col gap-1">
                    <div className="text-[11px] font-medium">{boneLabel(bone)}</div>
                    {AXES.map((a) => (
                      <Row key={a.at} label={t(a.label)}>
                        <Slider
                          value={turnOf(pose, bone)[a.at]}
                          min={-TURN_LIMIT}
                          max={TURN_LIMIT}
                          step={1}
                          onChange={(v) => turn(bone, a.at, v)}
                          format={(v) => `${v}°`}
                        />
                      </Row>
                    ))}
                  </div>
                ))}
              </div>
            )}
          </section>
        ))}
      </div>

      <div className="min-h-0 w-[46%] min-w-[320px] flex-none">
        <ViewerPanel
          loadout={loadout}
          riderOnly={!showBike}
          bikeId={showBike ? bike : undefined}
          bikeVariant={bikeVariant}
          riderPose={pose}
          onRiderPose={setPose}
          className="h-full"
        />
      </div>
    </div>
  );
}
