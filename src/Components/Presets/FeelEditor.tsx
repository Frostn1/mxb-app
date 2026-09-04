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
 * Settings the game stores as a fraction and *shows* as a whole number.
 *
 * The Options dialog reads the stored float and multiplies it by 100 for its slider; the
 * writer divides by 100 on the way back out. So `0.500000` in the file is the `50` the
 * rider set, and showing them the raw float would be showing them a number they have never
 * seen. Each slider has its own ceiling — deadzone clamps at 50, others run past 100 — so
 * these render as a plain number rather than a bounded slider that could clamp a value the
 * game accepts.
 *
 * Read off the binary: the writer divides exactly these by 100.0 (`0x140353948` /
 * `0x140353a1c`), and the input dialog multiplies the tuning values by the same constant.
 */
const SHOWN_X100 = new Set([
  // profile.ini
  "leanhelp_scale",
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
  // controls.txt tuning
  "gain",
  "deadzone",
  "linearity",
  "smooth/press",
  "smooth/release",
  "forcefeedback/maxforce",
  "forcefeedback/deadzone",
  "forcefeedback/linearity",
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
  if (SHOWN_X100.has(key)) return false;
  const v = value.trim();
  return v === "0" || v === "1";
}

/** The stored fraction as the number the game's own slider shows, or null if it isn't one. */
function toShown(key: string, stored: string): number | null {
  if (!SHOWN_X100.has(key)) return null;
  const n = Number.parseFloat(stored);
  return Number.isFinite(n) ? Math.round(n * 1000) / 10 : null;
}

/** Back the other way, in the six-decimal shape the game writes. */
function fromShown(shown: string): string {
  const n = Number.parseFloat(shown);
  return Number.isFinite(n) ? (n / 100).toFixed(6) : shown;
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
      <DialogContent className="max-w-[720px]">
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

        <div className="-mx-1 max-h-[52vh] overflow-y-auto px-1">
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
    <div className="grid grid-cols-1 gap-x-5 gap-y-1.5 sm:grid-cols-2">
      {entries.map(([key, value]) => {
        const shown = toShown(key, value);
        return (
          <div key={key} className="flex items-center justify-between gap-3">
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
                onChange={(v) => onChange(key, fromShown(v))}
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
