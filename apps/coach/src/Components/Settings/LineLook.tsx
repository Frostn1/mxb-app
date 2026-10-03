import { useEffect, useRef, useState } from "react";
import { RotateCcw } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Slider } from "@frost/shared/Components/ui/slider";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import {
  coachHud,
  coachLineLook,
  coachSetHud,
  coachSetLineLook,
  type Hud,
  type LineColours,
  type LineLook,
  type LineLookState,
  type TextStyle,
} from "@/api/coach";
import { FieldRow, Rule, Section, ToggleRow } from "./parts";

/** How long a colour picker is left still before what it picked is written: dragging across
 *  the picker would otherwise write `hud.ini` dozens of times a second. */
const SAVE_DELAY_MS = 250;

/** The line and what is drawn on it, in the order the rider meets them on the track. Settings
 *  shows them here rather than in the HUD list. */
export const LINE_PARTS = ["ground", "jumps", "pace", "gear"];

const PART_BODY: Partial<Record<string, TKey>> = {
  ground: "line.groundBody",
  jumps: "line.jumpsBody",
  pace: "hud.paceBody",
  gear: "hud.gearBody",
};

const COLOURS: { key: keyof LineColours; label: TKey }[] = [
  { key: "gas", label: "line.colGas" },
  { key: "coast", label: "line.colCoast" },
  { key: "light", label: "line.colLight" },
  { key: "heavy", label: "line.colHeavy" },
  { key: "fast", label: "line.colFast" },
  { key: "slow", label: "line.colSlow" },
];

/** The line's on/off and the marks on it: the same `hud.ini` keys the HUD panel switches. */
function LineParts() {
  const t = useT();
  const [hud, setHud] = useState<Hud | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    coachHud()
      .then(setHud)
      .catch((e) => setError(String(e)));
  }, []);
  const set = (key: string, on: boolean) =>
    coachSetHud(key, on)
      .then(setHud)
      .catch((e) => setError(String(e)));
  if (error) return <p className="text-[12px] text-muted-foreground">{error}</p>;
  if (!hud) return <p className="text-[12px] text-muted-foreground">{t("common.loading")}</p>;
  const parts = LINE_PARTS.map((k) => hud.parts.find((p) => p.key === k)).filter((p) => p !== undefined);
  return (
    <>
      {!hud.enabled && <p className="text-[12px] text-warning">{t("line.hudOff")}</p>}
      {parts.map((p) => {
        const body = PART_BODY[p.key];
        return (
          <ToggleRow
            key={p.key}
            label={p.key === "ground" ? t("line.show") : p.label}
            desc={body ? t(body) : undefined}
            checked={p.on}
            disabled={!hud.enabled}
            onChange={(on) => void set(p.key, on)}
          />
        );
      })}
    </>
  );
}

/** A slider with its value beside it, written when let go. */
function LookSlider({
  label,
  value,
  min,
  max,
  step,
  format,
  onChange,
  onCommit,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
  onCommit: (v: number) => void;
}) {
  return (
    <FieldRow label={label}>
      <div className="flex w-[260px] items-center gap-3">
        <Slider
          className="flex-1"
          min={min}
          max={max}
          step={step}
          value={[value]}
          onValueChange={([v]) => onChange(v)}
          onValueCommit={([v]) => onCommit(v)}
        />
        <span className="w-11 text-right font-mono text-[12px] text-muted-foreground">{format(value)}</span>
      </div>
    </FieldRow>
  );
}

/** The line as it will look, on dirt: its gradient at its opacity and width, and a jump call. */
function Preview({ look }: { look: LineLook }) {
  const c = look.colours;
  return (
    <div className="flex items-center justify-center gap-6 rounded-lg bg-[#6b5640] px-4 py-5">
      <div className="flex flex-col items-center gap-3">
        {look.text && (
          <span
            className={cn(
              "font-mono uppercase leading-none tracking-wider text-white",
              look.textStyle === "bold" && "font-black",
              look.textStyle === "italic" && "italic",
            )}
            style={{ fontSize: `${13 * look.textSize}px`, textShadow: "1px 1px 0 rgba(0,0,0,.6)" }}
          >
            Triple
          </span>
        )}
        <div
          className="w-[300px] rounded-full"
          style={{
            height: `${Math.max(2, 10 * look.width)}px`,
            opacity: look.opacity,
            background: `linear-gradient(90deg, ${c.gas} 0%, ${c.gas} 15%, ${c.coast} 40%, ${c.light} 65%, ${c.heavy} 90%)`,
          }}
        />
      </div>
      <div className="flex flex-col gap-1.5">
        {(["fast", "slow"] as const).map((k) => (
          <span key={k} className="h-2 w-10 rounded-full" style={{ background: c[k] }} />
        ))}
      </div>
    </div>
  );
}

/** The line's width, opacity, colours and the text on it. Live: the recorder re-reads them
 *  about once a second. */
export default function LineLookSettings() {
  const t = useT();
  const [state, setState] = useState<LineLookState | null>(null);
  const [look, setLook] = useState<LineLook | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    coachLineLook()
      .then((s) => {
        setState(s);
        setLook(s.look);
      })
      .catch((e) => setError(String(e)));
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, []);

  const write = (next: LineLook) =>
    coachSetLineLook(next)
      .then((s) => setState(s))
      .catch((e) => setError(String(e)));
  /** Show `next` now; write it now, or once the rider has stopped moving it. */
  const save = (next: LineLook, later = false) => {
    setLook(next);
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    if (later) timer.current = setTimeout(() => void write(next), SAVE_DELAY_MS);
    else void write(next);
  };

  return (
    <>
      <Section title={t("line.title")} desc={t("line.desc")}>
        <LineParts />
      </Section>

      {error && <p className="text-[12px] text-muted-foreground">{error}</p>}
      {state && look && (
        <>
          <Section title={t("line.lookTitle")} desc={t("line.lookDesc")}>
            {state.preLook && <p className="text-[12px] text-warning">{t("line.needs")}</p>}
            <Preview look={look} />
            <LookSlider
              label={t("line.width")}
              value={look.width}
              min={0.25}
              max={3}
              step={0.05}
              format={(v) => `${v.toFixed(2)}×`}
              onChange={(width) => setLook({ ...look, width })}
              onCommit={(width) => save({ ...look, width })}
            />
            <LookSlider
              label={t("line.opacity")}
              value={look.opacity}
              min={0.1}
              max={1}
              step={0.05}
              format={(v) => `${Math.round(v * 100)}%`}
              onChange={(opacity) => setLook({ ...look, opacity })}
              onCommit={(opacity) => save({ ...look, opacity })}
            />
          </Section>

          <Section
            title={t("line.colours")}
            desc={t("line.coloursDesc")}
            titleRight={
              <Button
                size="sm"
                variant="outline"
                onClick={() => save({ ...look, colours: { ...state.defaults.colours } })}
              >
                <RotateCcw className="size-3.5" />
                {t("line.reset")}
              </Button>
            }
          >
            <div className="grid grid-cols-2 gap-x-6 gap-y-3">
              {COLOURS.map(({ key, label }) => (
                <label key={key} className="flex items-center justify-between gap-3">
                  <span className="text-[12.5px] text-foreground/85">{t(label)}</span>
                  <span className="flex items-center gap-2">
                    <span className="font-mono text-[11.5px] uppercase text-muted-foreground">
                      {look.colours[key]}
                    </span>
                    <input
                      type="color"
                      aria-label={t(label)}
                      value={look.colours[key]}
                      onChange={(e) => save({ ...look, colours: { ...look.colours, [key]: e.target.value } }, true)}
                      className="h-7 w-10 cursor-pointer rounded-md border border-border bg-transparent p-0.5"
                    />
                  </span>
                </label>
              ))}
            </div>
          </Section>

          <Section title={t("line.textTitle")} desc={t("line.textDesc")}>
            <ToggleRow
              label={t("line.text")}
              desc={t("line.textBody")}
              checked={look.text}
              onChange={(text) => save({ ...look, text })}
            />
            <Rule />
            <div className={cn("flex flex-col gap-3", !look.text && "pointer-events-none opacity-60")}>
              <LookSlider
                label={t("line.textSize")}
                value={look.textSize}
                min={0.5}
                max={2}
                step={0.05}
                format={(v) => `${v.toFixed(2)}×`}
                onChange={(textSize) => setLook({ ...look, textSize })}
                onCommit={(textSize) => save({ ...look, textSize })}
              />
              <FieldRow label={t("line.textStyle")} desc={t("line.textStyleBody")}>
                <Segmented
                  size="sm"
                  value={look.textStyle}
                  onChange={(v) => save({ ...look, textStyle: v as TextStyle })}
                  options={[
                    { value: "block", label: t("line.styleBlock") },
                    { value: "bold", label: t("line.styleBold") },
                    { value: "italic", label: t("line.styleItalic") },
                  ]}
                />
              </FieldRow>
            </div>
          </Section>
        </>
      )}
    </>
  );
}
