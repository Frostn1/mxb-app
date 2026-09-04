/**
 * What's actually inside a saved feel, and a way to change it.
 *
 * A preset that can only be captured and re-applied is a black box: a rider who wants a
 * *slightly* softer throttle than the one they saved has to go back into the game, change
 * it there, and re-capture. This shows every value the preset carries and lets them nudge
 * it in place.
 *
 * Labels are derived from the game's own key names rather than translated. These are the
 * words MX Bikes uses on its Options screens — in English, in every language — so a rider
 * looking for the setting they just changed finds the name they saw there.
 */
import { useEffect, useMemo, useState } from "react";
import { Loader2, Save } from "lucide-react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { Slider } from "../ui/controls";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
} from "../ui/dialog";
import { useT, type TKey } from "../../i18n/context";
import type { Feel } from "../../api/mods";

/** `profile.ini` sections, in the order a rider thinks about them. */
const SECTION_ORDER = ["input", "aids", "view", "ext_view", "gfx"] as const;

const SECTION_LABEL: Record<string, TKey> = {
  input: "feel.groupInput",
  aids: "feel.groupAids",
  view: "feel.groupView",
  ext_view: "feel.groupExtView",
  gfx: "feel.groupGfx",
};

/**
 * How a stored value maps onto the slider the game draws, for the settings whose mapping
 * was read off the binary's own clamp code.
 *
 * The Options screens are sliders; the files hold fractions. The dialog converts on the way
 * in and the writer converts on the way out, and the conversion is *not* one rule — deadzone
 * clamps at 50, smoothing runs 0-100, and linearity is a 0-200 slider that runs backwards
 * with two different slopes either side of centre. Showing a rider `0.500000` where they set
 * `50`, or worse converting one of these the wrong way, is why each entry here is derived
 * rather than assumed.
 *
 * Anything not in this table keeps its stored value verbatim. An honest raw number beats a
 * confidently wrong converted one.
 */
interface Scale {
  min: number;
  max: number;
  step: number;
  /** Stored value → the number on the game's slider. */
  shown: (v: number) => number;
  /** Back again. */
  stored: (n: number) => number;
}

const PERCENT = (max: number): Scale => ({
  min: 0,
  max,
  step: 1,
  shown: (v) => Math.round(v * 100),
  stored: (n) => n / 100,
});

const SCALES: Record<string, Scale> = {
  // controls.txt — clamps at 0x1400cbde0 (deadzone) and 0x1400cc043 (smoothing).
  deadzone: PERCENT(50),
  "smooth/press": PERCENT(100),
  "smooth/release": PERCENT(100),
  // Linearity, from the clamp at 0x1400cbeb2: stored -0.5..2.0 maps to a 0..200 slider,
  // inverted, hinging at 100. Below centre the slope is -200, above it -50.
  linearity: {
    min: 0,
    max: 200,
    step: 1,
    shown: (v) => Math.round(v < 0 ? 100 - 200 * v : 100 - 50 * v),
    stored: (n) => (n > 100 ? (100 - n) / 200 : (100 - n) / 50),
  },
  // profile.ini — the Input tab's Direct Lean slider, read at 0x1400cd5ce.
  leanhelp_scale: PERCENT(100),
};

/**
 * Proven to be shown as `stored x 100` but with no range derived, so they stay a number box
 * rather than a slider whose ceiling would be a guess — a slider that clamps a value the
 * game accepts is worse than no slider.
 */
const SHOWN_X100 = new Set([
  "corner_anticipation_scale",
  "lean_heading_scale",
  "tilt",
  "tilt_vr",
  "pitch",
  "pitch_vr",
  "distance",
  "height",
  "latoffset",
  "combined_brakes_min",
  "combined_brakes_max",
]);

/**
 * Keys the game's own screens call something else.
 *
 * `leanhelp_scale` is the big one: the Input tab labels that slider **Direct Lean**, and
 * the dialog pushes its value straight into `ID_DIRECTLEAN`. A rider who turned direct lean
 * down would never find it under "Lean help scale".
 */
const LABELS: Record<string, string> = {
  leanhelp_scale: "Direct lean",
  leanhelp: "Lean help",
  sit_direct: "Direct sit",
  corner_anticipation_scale: "Corner anticipation amount",
  lean_heading_scale: "Lean heading amount",
  "smooth/enable": "Smoothing",
  "smooth/press": "Smoothing on press",
  "smooth/release": "Smoothing on release",
  deadzone: "Dead zone",
  show_HUD: "Show HUD",
  "3d_grass": "3D grass",
  drawdistance: "Draw distance",
};

/** `leanhelp_scale` → `Lean help scale`, `smooth/press` → `Smooth press`. */
function prettify(key: string): string {
  const known = LABELS[key];
  if (known) return known;
  const spaced = key
    .replace(/[/_]/g, " ")
    .replace(/\blean\s?help\b/i, "lean help")
    .replace(/\bauto\s?rider\b/i, "auto rider")
    .replace(/\bfb\b/i, "forward-back")
    .replace(/\blr\b/i, "left-right")
    .replace(/\bff\b|forcefeedback/i, "force feedback")
    .replace(/\blatoffset\b/i, "lateral offset")
    .trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/** A value the game stores as a flag, so it reads as a switch rather than a number box. */
function isToggle(key: string, value: string): boolean {
  if (SHOWN_X100.has(key) || SCALES[key]) return false;
  const v = value.trim();
  return v === "0" || v === "1";
}

/** The stored fraction as the number the game's own slider shows, or null if it isn't one. */
function toShown(key: string, stored: string): number | null {
  const scale = SCALES[key];
  const n = Number.parseFloat(stored);
  if (!Number.isFinite(n)) return null;
  if (scale) return scale.shown(n);
  return SHOWN_X100.has(key) ? Math.round(n * 1000) / 10 : null;
}

/** Back the other way, in the six-decimal shape the game writes. */
function fromShown(key: string, shown: string): string {
  const n = Number.parseFloat(shown);
  if (!Number.isFinite(n)) return shown;
  const scale = SCALES[key];
  return (scale ? scale.stored(n) : n / 100).toFixed(6);
}

interface Props {
  feel: Feel | null;
  /** Names already taken, so a rename can't silently overwrite another preset. */
  taken: string[];
  onClose: () => void;
  onSave: (next: Feel, previousName: string) => Promise<void>;
}

export default function FeelEditor({ feel, taken, onClose, onSave }: Props) {
  const t = useT();
  const [draft, setDraft] = useState<Feel | null>(feel);
  const [busy, setBusy] = useState(false);

  useEffect(() => setDraft(feel), [feel]);

  const clash = useMemo(() => {
    const nm = draft?.name.trim().toLowerCase() ?? "";
    if (!nm || nm === feel?.name.toLowerCase()) return false;
    return taken.some((n) => n.toLowerCase() === nm);
  }, [draft, feel, taken]);

  if (!draft || !feel) {
    return <Dialog open={false} onOpenChange={() => undefined} />;
  }

  const setIni = (section: string, key: string, value: string) =>
    setDraft((d) =>
      d
        ? { ...d, ini: { ...d.ini, [section]: { ...d.ini[section], [key]: value } } }
        : d,
    );

  const setTuning = (control: string, key: string, value: string) =>
    setDraft((d) =>
      d
        ? {
            ...d,
            controls: {
              ...d.controls,
              [control]: { ...d.controls[control], [key]: value },
            },
          }
        : d,
    );

  const sections = SECTION_ORDER.filter(
    (s) => Object.keys(draft.ini[s] ?? {}).length > 0,
  );
  const controls = Object.keys(draft.controls ?? {});

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-[1040px]">
        <DialogHeader>
          <DialogTitle>{t("feel.editTitle", { name: feel.name })}</DialogTitle>
        </DialogHeader>

        <label className="flex flex-col gap-1">
          <span className="text-[11px] font-medium text-muted-foreground">
            {t("feel.editName")}
          </span>
          <Input
            className="h-8"
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          />
          {clash && (
            <span className="text-[11px] text-destructive">
              {t("feel.editNameClash", { name: draft.name.trim() })}
            </span>
          )}
        </label>

        <div className="-mx-1 max-h-[66vh] overflow-y-auto px-1">
          {controls.length > 0 && (
            <Group title={t("feel.groupControls")}>
              {controls.map((control) => (
                <div key={control} className="mb-2.5 last:mb-0">
                  <div className="mb-1.5 text-[11.5px] font-semibold text-foreground/85">
                    {control}
                  </div>
                  <Rows
                    entries={Object.entries(draft.controls[control] ?? {})}
                    onChange={(k, v) => setTuning(control, k, v)}
                  />
                </div>
              ))}
            </Group>
          )}

          {sections.map((section) => (
            <Group key={section} title={t(SECTION_LABEL[section])}>
              <Rows
                entries={Object.entries(draft.ini[section] ?? {})}
                onChange={(k, v) => setIni(section, k, v)}
              />
            </Group>
          ))}
        </div>

        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            size="sm"
            disabled={busy || clash || !draft.name.trim()}
            onClick={async () => {
              setBusy(true);
              try {
                await onSave({ ...draft, name: draft.name.trim() }, feel.name);
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
            {t("feel.editSave")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-3.5 last:mb-0">
      <h3 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-faint">
        {title}
      </h3>
      <div className="rounded-xl border border-white/[0.07] bg-card/40 p-3">{children}</div>
    </section>
  );
}

function Rows({
  entries,
  onChange,
}: {
  entries: [string, string][];
  onChange: (key: string, value: string) => void;
}) {
  return (
    <div className="grid grid-cols-1 gap-x-7 gap-y-2 sm:grid-cols-2 xl:grid-cols-3">
      {entries.map(([key, value]) => {
        const shown = toShown(key, value);
        return (
          <div key={key} className="flex items-center justify-between gap-4">
            <span
              className="min-w-0 truncate text-[12px] text-muted-foreground"
              title={`${key} = ${value}`}
            >
              {prettify(key)}
            </span>
            {isToggle(key, value) ? (
              <Switch
                checked={value.trim() === "1"}
                onCheckedChange={(on) => onChange(key, on ? "1" : "0")}
              />
            ) : SCALES[key] && shown !== null ? (
              <span className="flex w-[190px] flex-none items-center gap-2" title={value}>
                <Slider
                  value={shown}
                  min={SCALES[key].min}
                  max={SCALES[key].max}
                  step={SCALES[key].step}
                  onChange={(n) => onChange(key, fromShown(key, String(n)))}
                  format={(n) => String(n)}
                  editable
                />
              </span>
            ) : shown === null ? (
              <Input
                className="h-7 w-[110px] flex-none text-right font-mono text-[11.5px]"
                value={value}
                onChange={(e) => onChange(key, e.target.value)}
              />
            ) : (
              <ScaledInput
                shown={shown}
                stored={value}
                onChange={(v) => onChange(key, fromShown(key, v))}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

/**
 * A field for a value the game shows as a whole number and stores as a fraction.
 *
 * It holds the literal text while focused. Converting on every keystroke and rendering the
 * conversion back would eat a half-typed decimal — `12.` round-trips to `12` before the `5`
 * lands, so `12.5` is unreachable. The draft is dropped on blur, which is where the value
 * snaps back to whatever the stored number really is.
 */
function ScaledInput({
  shown,
  stored,
  onChange,
}: {
  shown: number;
  stored: string;
  onChange: (shown: string) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <Input
      className="h-7 w-[110px] flex-none text-right font-mono text-[11.5px]"
      value={draft ?? String(shown)}
      title={stored}
      onChange={(e) => {
        setDraft(e.target.value);
        onChange(e.target.value);
      }}
      onBlur={() => setDraft(null)}
    />
  );
}
