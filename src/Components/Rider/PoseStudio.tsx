import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Camera, Image as ImageIcon, RotateCcw, User } from "lucide-react";
import { save as pickSavePath } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useT, type TKey } from "../../i18n/context";
import { Button } from "../ui/button";
import { Switch } from "../ui/switch";
import { Row, Slider } from "../ui/controls";
import { ViewerPanel } from "../Viewer/ViewerPanel";
import type { CaptureFn } from "../Viewer/ModelViewer";
import { pickedModel } from "../../lib/presets";
import { photoSave } from "../../api/mods";
import { useConfig } from "../../Context/Config";
import { useRiderKit } from "./RiderKitContext";
import { DEFAULT_SCENE, SCENES, type SceneId } from "../../lib/viewerScene";
import {
  applyQuickMove,
  BONE_GROUPS,
  canMove,
  boneLabel,
  clampTurn,
  isRestPose,
  NO_POSE,
  QUICK_MOVES,
  TURN_LIMIT,
  turnOf,
  withTurn,
  type BoneGroupId,
  type PosableRig,
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
  sitOnBike: "pose.move.sitOnBike",
};

const SCENE_LABEL: Record<SceneId, TKey> = {
  studio: "pose.scene.studio",
  white: "pose.scene.white",
  sky: "pose.scene.sky",
  sunset: "pose.scene.sunset",
  dusk: "pose.scene.dusk",
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

/** A file name a photo can be offered under. */
function photoName(profile: string, bike: string): string {
  const stem = [profile || "rider", bike].filter(Boolean).join("-");
  return `${stem.replace(/[^a-z0-9._-]+/gi, "-").replace(/^-+|-+$/g, "") || "rider"}.png`;
}

/**
 * The Pose studio.
 *
 * The kit as the Rider tab has it — bike, model swap, rider, gear, paints — with one thing you
 * can change: where the rider's limbs are. Everything else is deliberately read-only. The
 * Rider tab next door is where a look is composed; this is where it is stood in a position,
 * and a second set of pickers here would only be a second place to change the same slots.
 *
 * The pose reaches the preview and nothing else. MX Bikes takes the rider's posture from a
 * riding style — an animation set in `mods/rider/animations` — and nothing this writes could
 * change that.
 */
export default function PoseStudio() {
  const t = useT();
  const { bikePreview } = useConfig();
  const { scans, loadout, bike, hidden } = useRiderKit();
  const [pose, setPose] = useState<RiderPose>(NO_POSE);
  // Closed to start: the dots on the rider are the way in, and a wall of sliders reads as the
  // opposite of that.
  const [open, setOpen] = useState<BoneGroupId | null>(null);
  // The rig the ready-made moves are stated against, handed up by the viewer once the model
  // is on screen. Null until then, which is why the moves start out disabled.
  const [rig, setRig] = useState<PosableRig | null>(null);
  const [scene, setScene] = useState<SceneId>(DEFAULT_SCENE);
  const [photo, setPhoto] = useState(false);
  const [saving, setSaving] = useState(false);
  const capture = useRef<CaptureFn | null>(null);
  // Which joint a drag has just taken hold of, and a count so grabbing the same one twice
  // scrolls to it twice.
  const [grabbed, setGrabbed] = useState<{ bone: string; n: number } | null>(null);
  const groupAt = useRef<Partial<Record<BoneGroupId, HTMLElement | null>>>({});
  const boneAt = useRef<Record<string, HTMLElement | null>>({});

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

  const turn = useCallback((bone: string, at: 0 | 1 | 2, deg: number) => {
    setPose((p) => {
      const next = turnOf(p, bone);
      next[at] = clampTurn(deg);
      return withTurn(p, bone, next);
    });
  }, []);

  // A dot on a wrist is a fine way to move an arm, and says nothing about where the numbers
  // behind it are. Open that joint's group and put it under the reader's eyes.
  const onGrab = useCallback((bone: string) => {
    const group = BONE_GROUPS.find((g) => g.bones.includes(bone));
    if (group) setOpen(group.id);
    setGrabbed((g) => ({ bone, n: (g?.n ?? 0) + 1 }));
  }, []);

  useEffect(() => {
    if (!grabbed) return;
    const group = BONE_GROUPS.find((g) => g.bones.includes(grabbed.bone));
    // The row exists only once its group is open, so this runs again when `open` changes.
    const el = boneAt.current[grabbed.bone] ?? (group ? groupAt.current[group.id] : null);
    el?.scrollIntoView({ behavior: "smooth", block: "nearest" });
    const timer = setTimeout(() => setGrabbed(null), 1800);
    return () => clearTimeout(timer);
  }, [grabbed, open]);

  const onCapture = useCallback((c: CaptureFn | null) => {
    capture.current = c;
  }, []);

  const onPhoto = useCallback(async () => {
    const shoot = capture.current;
    if (!shoot) return;
    const dest = await pickSavePath({
      defaultPath: photoName(loadout.rider, bike),
      filters: [{ name: "PNG", extensions: ["png"] }],
    });
    if (!dest) return;
    setSaving(true);
    try {
      // Twice the panel, so a shot taken in a 400 px column is still worth posting.
      const shot = shoot(2);
      if (!shot) throw new Error(t("pose.photoFailed"));
      const path = await photoSave(dest, shot.buffer as ArrayBuffer);
      toast.success(t("pose.photoSaved"), { description: path });
    } catch (e) {
      toast.error(t("pose.photoFailed"), {
        description: String(e).replace(/^Error:\s*/, ""),
      });
    } finally {
      setSaving(false);
    }
  }, [bike, loadout.rider, t]);

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
        {/* What is being posed — whatever the Rider tab has. Read-only on purpose; see the
            note on the component. */}
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

        {/* Photo: what to stand the rider against, and how to get the frame out. */}
        <section className="rounded-lg border border-border bg-card/40 p-3">
          <header className="mb-2 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
            <ImageIcon className="h-3.5 w-3.5" />
            {t("pose.photo")}
          </header>
          <div className="mb-2 flex flex-wrap gap-1.5">
            {SCENES.map((s) => (
              <button
                key={s.id}
                type="button"
                onClick={() => setScene(s.id)}
                className={cn(
                  "rounded border px-2 py-1 text-[11px] leading-none transition-colors",
                  scene === s.id
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-border text-muted-foreground hover:text-foreground",
                )}
              >
                {t(SCENE_LABEL[s.id])}
              </button>
            ))}
          </div>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <label className="flex items-center gap-2 text-[11px] text-muted-foreground">
              <Switch checked={photo} onCheckedChange={setPhoto} />
              {t("pose.cleanFrame")}
            </label>
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1 px-2.5 text-[11px]"
              disabled={saving}
              onClick={() => void onPhoto()}
            >
              <Camera className="h-3 w-3" />
              {t("pose.savePhoto")}
            </Button>
          </div>
          <p className="mt-1.5 text-[10px] leading-snug text-muted-foreground">
            {t("pose.photoHint")}
          </p>
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
                // A move is a place to send a joint, so it needs the rig to say where that
                // is — and a rig that binds no spine has nothing to lean.
                disabled={!rig || !canMove(m, rig.bones)}
                onClick={() =>
                  rig && setPose((p) => applyQuickMove(p, m, rig.bones, rig.frame))
                }
              >
                {t(MOVE_LABEL[m.id])}
              </Button>
            ))}
          </div>
          <p className="mt-1.5 text-[10px] leading-snug text-muted-foreground">
            {t(rig ? "pose.quickHint" : "pose.quickWaiting")}
          </p>
        </section>

        <p className="-mb-1 text-[10px] leading-snug text-muted-foreground">
          {t("pose.dragHint")}
        </p>

        {BONE_GROUPS.map((g) => (
          <section
            key={g.id}
            ref={(el) => {
              groupAt.current[g.id] = el;
            }}
            className="rounded-lg border border-border"
          >
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
                  <div
                    key={bone}
                    ref={(el) => {
                      boneAt.current[bone] = el;
                    }}
                    className={cn(
                      "flex flex-col gap-1 rounded transition-colors",
                      grabbed?.bone === bone && "bg-primary/10 ring-1 ring-primary/40",
                    )}
                  >
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
          hiddenParts={hidden}
          riderPose={pose}
          onRiderPose={setPose}
          onPoseGrab={onGrab}
          onRiderRig={setRig}
          onCaptureReady={onCapture}
          offerOnBike
          scene={scene}
          photo={photo}
          className="h-full"
        />
      </div>
    </div>
  );
}
