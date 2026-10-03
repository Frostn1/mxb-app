import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight, RotateCcw } from "lucide-react";
import {
  configApply,
  configApplyLive,
  configCheck,
  configClassify,
  configLoad,
  configPreview,
  configValidate,
  errorText,
  peekConfig,
  serverRestartService,
  serverRiders,
  serverSession,
  type ApplyResult,
  type ConfigField,
  type ConfigCheck,
  type ConfigState,
  type FieldValue,
  type LiveApplyResult,
  type ServerView,
} from "@/lib/api";
import { zonesFrom } from "@/lib/cuts";
import type { ReloadChange } from "@/lib/reload";
import { BADGES, changesTracks, classesFor, reloadClass, waitsForTrackLoad, type ReloadClass } from "@/lib/reload";
import { CutZones } from "./CutZones";
import { Button, ErrorLine, Notice, Toggle } from "./ui";

/** The page's topics, in order. */
const GROUPS = [
  // The server itself first, then the stock server's own groups, then our extras.
  { id: "server", title: "Server" },
  { id: "event", title: "Event" },
  { id: "sessions", title: "Session lengths" },
  { id: "rules", title: "Rules" },
  { id: "penalties", title: "Penalties" },
  { id: "weather", title: "Weather & Track" },
  { id: "riders", title: "Riders" },
  { id: "operations", title: "Logs, live & admin" },
  { id: "ghosts", title: "Bots" },
  { id: "cuts", title: "Cuts" },
  { id: "deformation", title: "Deformation" },
  { id: "listing", title: "Listing" },
  { id: "logging", title: "Event logging" },
  { id: "advanced", title: "Advanced" },
] as const;

type Step =
  | { kind: "edit" }
  | {
      kind: "review";
      text: string;
      diff: string;
      check: { ok: boolean; output: string } | null;
      /** The running server's own dry run vetted it and no track changed: applying needs no
       *  slow check over SSH (the server's reload validates, and the old file goes back on a refusal). */
      fast?: boolean;
      /** How each changed key applies: the server's answer, else the app's own table. */
      classes: Record<string, ReloadClass>;
    }
  | { kind: "done"; result: ApplyResult }
  | { kind: "live"; result: LiveApplyResult; text: string };

const same = (a: FieldValue, b: FieldValue) => JSON.stringify(a) === JSON.stringify(b);

/** The server's settings as a form: change, review, apply. Applying checks the file with the
 *  server's own binary, backs it up and replaces it, then the running server reloads it: live
 *  settings apply at once, race rules at the next session, and a restart (which disconnects
 *  riders) is only offered when a restart-only setting changed. */
export function ConfigTab({ server }: { server: ServerView }) {
  const cached = peekConfig(server.id);
  const [state, setState] = useState<ConfigState | null>(cached);
  const [values, setValues] = useState<Record<string, FieldValue>>(cached?.values ?? {});
  const [step, setStep] = useState<Step>({ kind: "edit" });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activeGroup, setActiveGroup] = useState<(typeof GROUPS)[number]["id"]>("event");
  const [query, setQuery] = useState("");
  const [reloaded, setReloaded] = useState(false);

  const load = useCallback(async () => {
    if (!peekConfig(server.id)) setBusy("Reading settings…");
    setError(null);
    setReloaded(false);
    try {
      const s = await configLoad(server.id);
      setState(s);
      setValues(s.values);
      setStep({ kind: "edit" });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  }, [server.id]);

  useEffect(() => {
    void load();
  }, [load]);

  const changes = useMemo(() => {
    const out: Record<string, FieldValue> = {};
    if (!state) return out;
    for (const [k, v] of Object.entries(values)) if (!same(v, state.values[k] ?? null)) out[k] = v;
    return out;
  }, [values, state]);
  const changed = Object.keys(changes).length;
  const matches = useCallback((f: ConfigField) => {
    const q = query.trim().toLowerCase();
    return !q || `${f.label} ${f.help}`.toLowerCase().includes(q);
  }, [query]);

  useEffect(() => {
    if (!state || !query || state.fields.some((f) => f.group === activeGroup && matches(f))) return;
    const first = GROUPS.find((g) => state.fields.some((f) => f.group === g.id && matches(f)));
    if (first) setActiveGroup(first.id);
  }, [activeGroup, matches, query, state]);

  const review = async () => {
    if (!state) return;
    setBusy("Checking the new settings with the running server…");
    setError(null);
    try {
      const preview = await configPreview(state.text, changes);
      const keys = Object.keys(changes);
      let fast = false;
      const show = (check: { ok: boolean; output: string } | null, told: { changes?: ReloadChange[] } | null) =>
        setStep({ kind: "review", text: preview.text, diff: preview.diff, check, fast, classes: classesFor(keys, told?.changes ?? null) });
      show(null, null);
      // The quick check: the running server's own dry run (it parses the file, range-checks every
      // value and classifies each change) over the admin tunnel that is already open, so there is
      // no new SSH connection and it answers in about a round trip.
      const quick = await configCheck(server.id, preview.text).catch((): ConfigCheck => ({ checked: false }));
      if (quick.checked) {
        const told = quick.classify ?? null;
        show({ ok: !!quick.ok, output: quick.output ?? "" }, told);
        // Only a change to which tracks play is worth the slow check, which loads them.
        if (!quick.ok || !changesTracks(keys)) {
          fast = !!quick.ok;
          show({ ok: !!quick.ok, output: quick.output ?? "" }, told);
          return;
        }
        setBusy("The settings are accepted. Loading the changed tracks on the server to check them too (a few seconds per track)…");
        show({ ok: true, output: "" }, told);
        const slow = await configValidate(server.id, preview.text);
        show(slow, told);
        return;
      }
      // A server that cannot answer a dry run (no token, an older version): run the server
      // binary once over SSH, as before.
      setBusy("The server can't check a draft itself, so it is being run once over SSH (up to a minute)…");
      const check = await configValidate(server.id, preview.text);
      const told = await configClassify(server.id, preview.text).catch(() => null);
      show(check, told);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  /** The old way: replace the file and restart. Only for a server too old to reload. */
  const applyWithRestart = async (text: string) => {
    if (!state) return;
    if (!(await confirmRestart())) return;
    setBusy("Saving a backup, applying and restarting… (up to a minute)");
    setError(null);
    try {
      setStep({ kind: "done", result: await configApply(server.id, state.sha, text) });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const apply = async (text: string) => {
    if (!state) return;
    setBusy("Saving a backup and applying to the running server…");
    setError(null);
    try {
      const fast = step.kind === "review" && !!step.fast;
      setBusy(fast ? "Saving a backup and reloading the running server…" : "Saving a backup, checking and reloading the running server…");
      setStep({ kind: "live", result: await configApplyLive(server.id, state.sha, text, fast), text });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  /** Asks, with how many riders a restart would disconnect right now. */
  const confirmRestart = async () => {
    const riders = await serverRiders(server.id).then((r) => r.length).catch(() => null);
    const who = riders === null ? "anyone riding" : riders === 0 ? "nobody (no riders on now)" : `${riders} rider${riders === 1 ? "" : "s"}`;
    return window.confirm(`Restart ${server.name}? This disconnects ${who}.`);
  };

  /** "Apply now" for what waits for the next track load: restart the event on the same track. */
  const reloadEventNow = async () => {
    const riders = await serverRiders(server.id).then((r) => r.length).catch(() => null);
    const who = riders === null ? "Any riders on" : riders === 0 ? "No riders are on, so nobody is affected." : `${riders} rider${riders === 1 ? "" : "s"} on the server`;
    const consequence = riders === 0 ? who : `${who} will go back to the pits.`;
    if (!window.confirm(`Reload the current event on ${server.name}? It restarts on the same track from practice. ${consequence}`)) return;
    setBusy("Reloading the event…");
    setError(null);
    try {
      await serverSession(server.id, "reload");
      setReloaded(true);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const restartNow = async () => {
    if (!(await confirmRestart())) return;
    setBusy("Restarting… (up to a minute)");
    setError(null);
    try {
      await serverRestartService(server.id);
      await load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  if (!state) {
    return (
      <div className="flex flex-col gap-3">
        {busy && <p className="text-sm text-muted-foreground">{busy}</p>}
        {error && <ErrorLine text={error} />}
        {error && (
          <div>
            <Button onClick={() => void load()}>Try again</Button>
          </div>
        )}
      </div>
    );
  }

  const setValue = (f: ConfigField, v: FieldValue) => setValues({ ...values, [`${f.section}.${f.key}`]: v });

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      {error && <ErrorLine text={error} />}

      {step.kind === "edit" && (
        <>
          <div className="grid min-h-0 flex-1 gap-6 md:grid-cols-[13rem_minmax(0,1fr)]">
            <div className="flex min-h-0 flex-col gap-3">
              <input className="h-9 rounded-md border border-input bg-card px-3 text-sm outline-none focus:border-ring" placeholder="Search settings" value={query} onChange={(e) => setQuery(e.target.value)} />
              <nav className="flex gap-1 overflow-x-auto md:flex-col md:overflow-visible" aria-label="Settings categories">
              {GROUPS.map((g) => {
                const changedHere = state.fields.filter((f) => f.group === g.id && `${f.section}.${f.key}` in changes).length + (g.id === "cuts" && "cuts.zones" in changes ? 1 : 0);
                const found = state.fields.filter((f) => f.group === g.id && matches(f)).length;
                return (
                  <button
                    key={g.id}
                    type="button"
                    onClick={() => setActiveGroup(g.id)}
                    aria-current={activeGroup === g.id ? "page" : undefined}
                    className={`flex shrink-0 items-center justify-between gap-3 rounded-md px-3 py-2 text-left text-sm font-medium ${
                      activeGroup === g.id ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/60 hover:text-foreground"
                    }`}
                  >
                    <span>{g.title}</span>
                    <span className="flex items-center gap-1.5">
                      {query && <span className="text-xs text-muted-foreground">{found}</span>}
                      {changedHere > 0 && <span className="rounded-full bg-primary px-1.5 text-xs text-primary-foreground">{changedHere}</span>}
                    </span>
                  </button>
                );
              })}
              </nav>
            </div>
            {GROUPS.filter((g) => g.id === activeGroup).map((g) => {
              const fields = state.fields.filter((f) => f.group === g.id && matches(f));
              return (
                <Group key={g.id} title={g.title}>
                {fields.length === 0 && <p className="py-4 text-sm text-muted-foreground">No matching settings.</p>}
                <FieldList fields={fields.filter((f) => !f.advanced)} values={values} changes={changes} onChange={setValue} />
                {g.id === "deformation" && <RutsReset server={server} />}
                {g.id === "cuts" && (
                  <CutZones server={server} zones={zonesFrom(values["cuts.zones"])} onChange={(zones) => setValues({ ...values, "cuts.zones": zones })} />
                )}
                {fields.some((f) => f.advanced) && (
                  <Disclosure title="More settings">
                    <FieldList fields={fields.filter((f) => f.advanced)} values={values} changes={changes} onChange={setValue} />
                  </Disclosure>
                )}
              </Group>
              );
            })}
          </div>
          <div className="flex shrink-0 items-center justify-end gap-3 border-t bg-background py-3">
            {busy && <span className="mr-auto text-sm text-muted-foreground">{busy}</span>}
            <Button disabled={!changed || !!busy} onClick={() => setValues(state.values)}>
              Reset changes
            </Button>
            <Button variant="primary" disabled={!changed || !!busy} onClick={() => void review()}>
              {changed ? `Save ${changed} change${changed === 1 ? "" : "s"}` : "Saved"}
            </Button>
          </div>
        </>
      )}

      {step.kind === "review" && (
        // The tab doesn't scroll, so the diff is the part that does: it takes whatever height is
        // left and scrolls both ways, while the heading, the check and the buttons stay put.
        <div className="flex min-h-0 flex-1 flex-col gap-5">
          <h3 className="shrink-0 font-heading text-lg font-extrabold tracking-tight">Review the change</h3>
          <Diff text={step.diff} />
          <ChangeClasses classes={step.classes} />
          {!step.check && busy && <p className="shrink-0 text-sm text-muted-foreground">{busy}</p>}
          {step.check && (
            <div className="shrink-0">
              <Notice tone={step.check.ok ? "ok" : "bad"}>
                <span className="font-medium">
                  {step.check.ok ? "The server accepts these settings." : "The server refuses these settings."}
                </span>
                {!step.check.ok && step.check.output && (
                  <pre className="mt-2 max-h-48 overflow-auto font-mono text-xs whitespace-pre-wrap">{step.check.output}</pre>
                )}
              </Notice>
            </div>
          )}
          <div className="flex shrink-0 gap-3 border-t bg-background py-3">
            <Button variant="primary" disabled={!step.check?.ok || !!busy} onClick={() => void apply(step.text)}>
              Apply
            </Button>
            <Button disabled={!!busy} onClick={() => setStep({ kind: "edit" })}>
              Back
            </Button>
            {busy && step.check && <span className="text-sm text-muted-foreground">{busy}</span>}
          </div>
        </div>
      )}

      {step.kind === "live" && (
        <LiveResult
          result={step.result}
          busy={!!busy}
          onRestart={() => void restartNow()}
          onApplyNow={() => void reloadEventNow()}
          reloaded={reloaded}
          onApplyWithRestart={() => void applyWithRestart(step.text)}
          onBack={() => void load()}
        />
      )}
      {step.kind === "live" && busy && <p className="text-sm text-muted-foreground">{busy}</p>}

      {step.kind === "done" && (
        <div className="flex min-h-0 flex-col gap-4 overflow-y-auto">
          <Notice tone={step.result.result === "applied" ? "ok" : "bad"}>
            <span className="font-medium">
              {step.result.result === "applied"
                ? "Applied. The server restarted and is ready."
                : step.result.result === "rolled-back"
                  ? "The server didn't come back with the new settings, so the previous ones were put back and it restarted on those."
                  : "Applying failed, and the server may be down. Check the Logs tab."}
            </span>
            {step.result.backup && (
              <p className="mt-1 text-xs text-muted-foreground">
                The previous settings are saved as <span className="font-mono">{step.result.backup}</span>
              </p>
            )}
          </Notice>
          {step.result.result !== "applied" && step.result.output && (
            <pre className="max-h-48 overflow-auto rounded-lg bg-muted p-3 font-mono text-xs whitespace-pre-wrap">{step.result.output}</pre>
          )}
          <div>
            <Button onClick={() => void load()}>Back to settings</Button>
          </div>
        </div>
      )}
    </div>
  );
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="min-h-0 overflow-y-auto pr-6">
      <h3 className="font-heading text-lg font-extrabold tracking-tight">{title}</h3>
      <div className="mt-3 flex flex-col">{children}</div>
    </section>
  );
}

function Disclosure({ title, children }: { title: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="mt-2 border-t pt-3">
      <button
        type="button"
        className="inline-flex items-center gap-1.5 text-sm font-medium text-muted-foreground hover:text-foreground"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
      >
        {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        {title}
      </button>
      {open && <div className="mt-2 flex flex-col">{children}</div>}
    </div>
  );
}

function FieldList({
  fields,
  values,
  changes,
  onChange,
}: {
  fields: ConfigField[];
  values: Record<string, FieldValue>;
  changes: Record<string, FieldValue>;
  onChange: (f: ConfigField, v: FieldValue) => void;
}) {
  return (
    <div className="flex flex-col divide-y">
      {fields.map((f) => {
        const name = `${f.section}.${f.key}`;
        return (
          <SettingRow
            key={name}
            field={f}
            value={values[name] ?? null}
            changed={name in changes}
            onChange={(v) => onChange(f, v)}
          />
        );
      })}
    </div>
  );
}

/** One setting: its name and one line of help on the left, the control on the right, and
 *  what happens when it's left at the default underneath. */
function SettingRow({
  field,
  value,
  changed,
  onChange,
}: {
  field: ConfigField;
  value: FieldValue;
  changed: boolean;
  onChange: (v: FieldValue) => void;
}) {
  const isDefault = value === null;
  return (
    <div className="grid grid-cols-1 gap-3 py-4 sm:grid-cols-[1fr_minmax(13rem,17rem)] sm:gap-6">
      <div className="flex flex-col gap-0.5">
        <span className="text-sm font-medium">
          {field.label}
          {changed && <span className="ml-2 rounded bg-primary/10 px-1.5 py-0.5 text-xs text-primary">changed</span>}
          <ReloadBadge kind={reloadClass(`${field.section}.${field.key}`)} />
        </span>
        {field.help && <span className="text-sm text-muted-foreground">{field.help}</span>}
        {field.status === "saved" && (
          <span className="text-xs text-muted-foreground">
            Saved and reported by the server. Riders are not told yet, so it has no effect in game for now.
          </span>
        )}
        {field.stock && <span className="font-mono text-xs text-muted-foreground">stock: {field.stock}</span>}
        {field.status !== "elsewhere" && field.status !== "unsupported" && <span className="text-xs text-muted-foreground">
          {isDefault ? `Using the default: ${field.defaultText || "server default"}` : `Default: ${field.defaultText || "server default"}`}
          {!isDefault && field.kind.type !== "bool" && (
            <button type="button" className="ml-2 inline-flex items-center gap-1 text-link hover:underline" onClick={() => onChange(null)}>
              <RotateCcw className="size-3" /> Use default
            </button>
          )}
        </span>}
      </div>
      <div className="flex items-start sm:justify-end">
        {field.status === "elsewhere" || field.status === "unsupported" ? (
          <span className="rounded bg-secondary px-2 py-1 text-xs text-muted-foreground">
            {field.status === "elsewhere" ? "Set elsewhere" : "Not supported yet"}
          </span>
        ) : (
          <Control field={field} value={value} onChange={onChange} />
        )}
      </div>
    </div>
  );
}

const inputClass = "h-9 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/30";

function Segmented({ options, value, onChange }: { options: { value: string; label: string }[]; value: string; onChange: (v: string) => void }) {
  return (
    <div className="inline-flex flex-wrap rounded-md bg-secondary p-0.5" role="radiogroup">
      {options.map((o) => (
        <button
          type="button"
          key={o.value}
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={`rounded px-2.5 py-1 text-xs font-medium transition ${
            value === o.value ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Control({ field, value, onChange }: { field: ConfigField; value: FieldValue; onChange: (v: FieldValue) => void }) {
  const k = field.kind;
  switch (k.type) {
    case "bool":
      // Every switch here defaults to off, so off leaves the setting out of the file.
      return <Toggle label={field.label} checked={value === true} onChange={(on) => onChange(on ? true : null)} />;
    case "tri":
      return (
        <Segmented
          options={[
            { value: "default", label: "Default" },
            { value: "on", label: "On" },
            { value: "off", label: "Off" },
          ]}
          value={value === true ? "on" : value === false ? "off" : "default"}
          onChange={(v) => onChange(v === "on" ? true : v === "off" ? false : null)}
        />
      );
    case "int":
    case "float":
      return <NumberControl field={field} min={k.min} max={k.max} step={k.type === "int" ? 1 : k.max - k.min <= 20 ? 0.5 : 1} value={value} onChange={onChange} />;
    case "choice":
      return (
        <Segmented
          options={k.options.map((o) => ({ value: o, label: o.charAt(0).toUpperCase() + o.slice(1) }))}
          value={typeof value === "string" ? value : ""}
          onChange={(v) => onChange(v)}
        />
      );
    case "racing": {
      const current = value === true ? "yield" : typeof value === "string" ? value : "off";
      return (
        <Segmented
          options={[
            { value: "off", label: "Off" },
            { value: "neutral", label: "Neutral" },
            { value: "yield", label: "Yield" },
            { value: "block", label: "Block" },
          ]}
          value={current}
          onChange={(v) => onChange(v === "off" ? null : v)}
        />
      );
    }
    case "bikes":
      return <BikesControl value={value} onChange={onChange} />;
    default:
      return (
        <input
          className={`${inputClass} font-mono`}
          placeholder={field.defaultText || "not set"}
          value={typeof value === "string" ? value : ""}
          onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
        />
      );
  }
}

/** A slider for short ranges (with the number beside it), a number box otherwise. */
function NumberControl({
  field,
  min,
  max,
  step,
  value,
  onChange,
}: {
  field: ConfigField;
  min: number;
  max: number;
  step: number;
  value: FieldValue;
  onChange: (v: FieldValue) => void;
}) {
  const n = typeof value === "number" ? value : null;
  // "20 min" -> 20: where an unset value sits, and the box's placeholder.
  const fallback = Number.parseFloat(field.defaultText);
  const shown = Number.isFinite(fallback) ? fallback : min;
  const slider = max - min <= 120;
  const box = (
    <div className="flex items-center gap-1.5">
      <input
        type="number"
        className="h-9 w-[4.5rem] shrink-0 rounded-md border border-input bg-background px-2 text-right font-mono text-sm tabular-nums outline-none focus:border-ring focus:ring-2 focus:ring-ring/30"
        min={min}
        max={max}
        step={step}
        placeholder={Number.isFinite(fallback) ? String(fallback) : "—"}
        value={n ?? ""}
        onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
        aria-label={field.label}
      />
      <span className="w-7 text-sm text-muted-foreground">{field.unit}</span>
    </div>
  );
  if (!slider) return box;
  return (
    <div className="flex w-full items-center gap-3">
      <input
        type="range"
        className="w-full accent-[var(--primary)]"
        min={min}
        max={max}
        step={step}
        value={n ?? shown}
        style={{ opacity: n == null ? 0.45 : 1 }}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label={field.label}
      />
      {box}
    </div>
  );
}

/** Same bike for everyone (the default), random from the server's bikes, or a list. */
function BikesControl({ value, onChange }: { value: FieldValue; onChange: (v: FieldValue) => void }) {
  const mode = value === "random" ? "random" : Array.isArray(value) ? "list" : "same";
  const [text, setText] = useState(Array.isArray(value) ? value.join(", ") : "");
  useEffect(() => {
    if (Array.isArray(value)) {
      setText((t) => (same(t.split(",").map((s) => s.trim()).filter(Boolean), value) ? t : value.join(", ")));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(value)]);
  const list = (t: string) => {
    setText(t);
    const ids = t.split(",").map((s) => s.trim()).filter(Boolean);
    onChange(ids.length ? ids : []);
  };
  return (
    <div className="flex w-full flex-col items-end gap-2">
      <Segmented
        options={[
          { value: "same", label: "Same" },
          { value: "random", label: "Random" },
          { value: "list", label: "List" },
        ]}
        value={mode}
        onChange={(m) => onChange(m === "same" ? null : m === "random" ? "random" : text ? text.split(",").map((s) => s.trim()).filter(Boolean) : [])}
      />
      {mode === "list" && (
        <input
          className={`${inputClass} font-mono`}
          placeholder="bike ids, separated by commas"
          value={text}
          onChange={(e) => list(e.target.value)}
        />
      )}
    </div>
  );
}

const BADGE_STYLE = {
  ok: { color: "var(--success)", background: "color-mix(in srgb, var(--success) 12%, transparent)" },
  info: { color: "var(--primary)", background: "color-mix(in srgb, var(--primary) 12%, transparent)" },
  warn: { color: "var(--destructive)", background: "color-mix(in srgb, var(--destructive) 12%, transparent)" },
} as const;

/** Puts the ground back to the track's start layout now, for every rider on, a few blocks at a
 *  time (the same way live ruts are sent): nobody is disconnected. */
function RutsReset({ server }: { server: ServerView }) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reset = async () => {
    if (!window.confirm(`Reset the track ruts on ${server.name}? The ground goes back to its start layout for everyone riding, a few blocks at a time. Nobody is disconnected.`)) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await serverSession(server.id, "reset_ruts");
      setMessage("The ruts are being reset. Riders see the ground return to its start layout over the next moments.");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-wrap items-center gap-3 border-b py-4">
      <Button disabled={busy} onClick={() => void reset()} title="Clears the ruts riders and bots have dug, without a restart">
        Reset track ruts now
      </Button>
      <span className="text-sm text-muted-foreground">Clears the ruts dug so far on the running track. Nobody is disconnected.</span>
      {message && <span className="w-full text-sm text-success">{message}</span>}
      {error && <ErrorLine text={error} />}
    </div>
  );
}

/** "Applies now", "Next session", "Next track load" or "Needs restart", with a tooltip. */
export function ReloadBadge({ kind }: { kind: ReloadClass }) {
  const badge = BADGES[kind];
  return (
    <span className="ml-2 rounded px-1.5 py-0.5 text-xs font-normal" style={BADGE_STYLE[badge.tone]} title={badge.title}>
      {badge.label}
    </span>
  );
}

/** Each changed setting and how the running server takes it. */
function ChangeClasses({ classes }: { classes: Record<string, ReloadClass> }) {
  const keys = Object.keys(classes).sort();
  if (!keys.length) return null;
  const restart = keys.filter((k) => classes[k] === "restart").length;
  return (
    <div className="shrink-0 rounded-xl border bg-card p-4 text-sm">
      <p className="mb-2 text-muted-foreground">
        {restart === 0
          ? "No restart needed: nobody riding is disconnected."
          : `Everything else applies without a restart. ${restart} setting${restart === 1 ? " needs" : "s need"} a restart to take effect; you can restart after applying.`}
      </p>
      <ul className="flex flex-col gap-1">
        {keys.map((k) => (
          <li key={k} className="flex items-center justify-between gap-3 font-mono text-xs">
            <span>{k}</span>
            <ReloadBadge kind={classes[k]} />
          </li>
        ))}
      </ul>
    </div>
  );
}

/** After a live apply: what applied, what waits for the next session, and the restart offer
 *  when (and only when) a restart-only setting changed. */
function LiveResult({
  result,
  busy,
  onRestart,
  onApplyNow,
  reloaded,
  onApplyWithRestart,
  onBack,
}: {
  result: LiveApplyResult;
  busy: boolean;
  onRestart: () => void;
  /** Reload the current event so what waits for the next track load takes effect. */
  onApplyNow: () => void;
  reloaded: boolean;
  onApplyWithRestart: () => void;
  onBack: () => void;
}) {
  const reload = result.reload ?? {};
  const applied = reload.applied ?? [];
  const deferred = reload.deferred ?? [];
  const deferredClasses = classesFor(deferred, reload.changes ?? null);
  const nextSession = deferred.filter((k) => deferredClasses[k] !== "next_event");
  const nextLoad = deferred.filter((k) => deferredClasses[k] === "next_event");
  const restart = reload.restart_required ?? [];
  const ok = result.result === "applied";
  // A server from before hot reload answers without `deferred` and only reloads a few
  // admission keys: the file is saved, but a restart is what applies it.
  const old = ok && result.reload !== null && reload.deferred === undefined;
  const list = (title: string, keys: string[], kind: ReloadClass) =>
    keys.length > 0 && (
      <div>
        <p className="mb-1 text-sm font-medium">{title}</p>
        <ul className="flex flex-col gap-1">
          {keys.map((k) => (
            <li key={k} className="flex items-center justify-between gap-3 font-mono text-xs">
              <span>{k}</span>
              <ReloadBadge kind={kind} />
            </li>
          ))}
        </ul>
      </div>
    );
  return (
    <div className="flex min-h-0 flex-col gap-4 overflow-y-auto">
      <Notice tone={ok ? "ok" : "bad"}>
        <span className="font-medium">
          {old
            ? "Saved, but this server version can't apply settings live. Restart it to apply them (or update the server)."
            : ok
            ? restart.length
              ? "Saved and applied what the running server can take live. Nobody was disconnected."
              : "Applied to the running server. Nobody was disconnected."
            : "Not applied: the server is still on its previous settings."}
        </span>
        {result.backup && (
          <p className="mt-1 text-xs text-muted-foreground">
            The previous settings are saved as <span className="font-mono">{result.backup}</span>
          </p>
        )}
      </Notice>
      {!ok && result.output && <pre className="max-h-48 overflow-auto rounded-lg bg-muted p-3 font-mono text-xs whitespace-pre-wrap">{result.output}</pre>}
      {ok && !old && (
        <div className="flex flex-col gap-3 rounded-xl border bg-card p-4">
          {list("Applied now", applied, "hot")}
          {list("Applies at the next session", nextSession, "next_session")}
          {list("Applies at the next track load", nextLoad, "next_event")}
          {waitsForTrackLoad(Object.values(deferredClasses)) && (
            <div className="flex flex-wrap items-center gap-3">
              <Button disabled={busy || reloaded} onClick={onApplyNow} title="Restarts the event on the same track. Riders go back to the pits.">
                {reloaded ? "Event reloaded" : "Apply now"}
              </Button>
              <span className="text-xs text-muted-foreground">
                {reloaded ? "The event restarted on the same track with these settings." : "Reloads the current event on the same track. Riders go back to the pits."}
              </span>
            </div>
          )}
          {list("Needs a restart to take effect", restart, "restart")}
          {!applied.length && !deferred.length && !restart.length && <p className="text-sm text-muted-foreground">Nothing the server uses changed.</p>}
        </div>
      )}
      <div className="flex gap-3">
        {ok && (old || restart.length > 0) && (
          <Button variant="primary" disabled={busy} onClick={onRestart}>
            Restart now
          </Button>
        )}
        {!ok && (
          <Button disabled={busy} onClick={onApplyWithRestart}>
            Apply with a restart instead
          </Button>
        )}
        <Button disabled={busy} onClick={onBack}>
          Back to settings
        </Button>
      </div>
    </div>
  );
}

function Diff({ text }: { text: string }) {
  if (!text) return <p className="text-sm text-muted-foreground">Nothing changes in the file.</p>;
  return (
    // min-h-0 lets it shrink below its content inside the review column; long lines scroll
    // sideways (w-max keeps every row as wide as the longest, so the colours line up).
    <pre data-testid="config-diff" className="min-h-0 overflow-auto rounded-xl border bg-card p-4 font-mono text-xs leading-relaxed">
      <div className="w-max min-w-full">
      {text.split("\n").map((line, i) => {
        const color =
          line.startsWith("+") && !line.startsWith("+++")
            ? "var(--success)"
            : line.startsWith("-") && !line.startsWith("---")
              ? "var(--destructive)"
              : line.startsWith("@@")
                ? "var(--muted-foreground)"
                : undefined;
        return (
          <div key={i} style={{ color }}>
            {line || " "}
          </div>
        );
      })}
      </div>
    </pre>
  );
}
