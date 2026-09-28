import { useCallback, useEffect, useMemo, useState } from "react";
import {
  configApply,
  configLoad,
  configPreview,
  configValidate,
  errorText,
  type ApplyResult,
  type ConfigField,
  type ConfigState,
  type FieldValue,
  type ServerView,
} from "@/lib/api";
import { Button, Card, ErrorLine } from "./ui";

const sectionTitle: Record<string, string> = {
  ghost: "Ghost bots  [ghost]",
  events: "Events  [events]",
  native: "Native switches  [native]",
};

const modeText: Record<string, string> = {
  systemd: "systemd service: applying restarts it with systemctl",
  bare: "hand-started process: applying restarts it the way the deploy scripts do",
  local: "this PC: applying restarts it from its saved start command",
};

type Step =
  | { kind: "edit" }
  | { kind: "review"; text: string; diff: string; check: { ok: boolean; output: string } | null }
  | { kind: "done"; result: ApplyResult };

const same = (a: FieldValue, b: FieldValue) => JSON.stringify(a) === JSON.stringify(b);

/** Typed editing of the server's config: edit, review the diff, check with the server's own
 *  binary, then apply (backup, replace, restart, roll back if it isn't ready). */
export function ConfigTab({ server }: { server: ServerView }) {
  const [state, setState] = useState<ConfigState | null>(null);
  const [values, setValues] = useState<Record<string, FieldValue>>({});
  const [step, setStep] = useState<Step>({ kind: "edit" });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setBusy("Reading the config…");
    setError(null);
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

  const review = async () => {
    if (!state) return;
    setBusy("Checking the change with the server's own binary…");
    setError(null);
    try {
      const preview = await configPreview(state.text, changes);
      setStep({ kind: "review", text: preview.text, diff: preview.diff, check: null });
      const check = await configValidate(server.id, preview.text);
      setStep({ kind: "review", text: preview.text, diff: preview.diff, check });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const apply = async (text: string) => {
    if (!state) return;
    if (!window.confirm(`Apply to ${server.name}? The server restarts, so connected riders are dropped.`)) return;
    setBusy("Backing up, applying and restarting… (up to a minute)");
    setError(null);
    try {
      setStep({ kind: "done", result: await configApply(server.id, state.sha, text) });
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
        {error && <Button onClick={() => void load()}>Try again</Button>}
      </div>
    );
  }

  const sections = [...new Set(state.fields.map((f) => f.section))];

  return (
    <div className="flex max-w-3xl flex-col gap-5">
      <p className="text-xs text-muted-foreground">
        <span className="font-mono">{state.path}</span> · {modeText[state.mode] ?? state.mode}
      </p>
      {error && <ErrorLine text={error} />}
      {busy && <p className="text-sm text-muted-foreground">{busy}</p>}

      {step.kind === "edit" &&
        sections.map((section) => (
          <Card key={section} className="flex flex-col gap-4">
            <h3 className="font-heading font-extrabold tracking-tight">{sectionTitle[section] ?? `[${section}]`}</h3>
            {state.fields
              .filter((f) => f.section === section)
              .map((f) => {
                const name = `${f.section}.${f.key}`;
                return (
                  <FieldEditor
                    key={name}
                    field={f}
                    value={values[name] ?? null}
                    dirty={name in changes}
                    onChange={(v) => setValues({ ...values, [name]: v })}
                  />
                );
              })}
          </Card>
        ))}

      {step.kind === "edit" && (
        <div className="sticky bottom-0 flex items-center gap-3 border-t bg-background py-3">
          <Button variant="primary" disabled={!changed || !!busy} onClick={() => void review()}>
            Review {changed || ""} change{changed === 1 ? "" : "s"}
          </Button>
          <Button disabled={!changed || !!busy} onClick={() => setValues(state.values)}>
            Discard
          </Button>
          <Button disabled={!!busy} onClick={() => void load()}>
            Reload from server
          </Button>
        </div>
      )}

      {step.kind === "review" && (
        <div className="flex flex-col gap-4">
          <Diff text={step.diff} />
          {step.check && (
            <Card className="flex flex-col gap-2">
              <span className={`text-sm font-medium ${step.check.ok ? "text-success" : "text-destructive"}`}>
                {step.check.ok ? "The server accepts this config." : "The server refuses this config."}
              </span>
              {step.check.output && (
                <pre className="max-h-48 overflow-auto rounded-md bg-muted p-2 font-mono text-xs whitespace-pre-wrap">
                  {step.check.output}
                </pre>
              )}
            </Card>
          )}
          <div className="flex gap-3">
            <Button
              variant="primary"
              disabled={!step.check?.ok || !!busy}
              onClick={() => void apply(step.text)}
            >
              Apply and restart
            </Button>
            <Button disabled={!!busy} onClick={() => setStep({ kind: "edit" })}>
              Back to editing
            </Button>
          </div>
        </div>
      )}

      {step.kind === "done" && (
        <Card className="flex flex-col gap-3">
          <span
            className={`text-sm font-medium ${step.result.result === "applied" ? "text-success" : "text-destructive"}`}
          >
            {step.result.result === "applied"
              ? "Applied. The server restarted and is ready."
              : step.result.result === "rolled-back"
                ? "The server wasn't ready with the new config, so the backup was put back and it restarted on that."
                : "The change failed and the server may be down. Check the Logs tab."}
          </span>
          {step.result.backup && (
            <span className="text-xs text-muted-foreground">
              Backup: <span className="font-mono">{step.result.backup}</span>
            </span>
          )}
          {step.result.output && (
            <pre className="max-h-48 overflow-auto rounded-md bg-muted p-2 font-mono text-xs whitespace-pre-wrap">
              {step.result.output}
            </pre>
          )}
          <Button onClick={() => void load()}>Load the config again</Button>
        </Card>
      )}
    </div>
  );
}

function Diff({ text }: { text: string }) {
  if (!text) return <p className="text-sm text-muted-foreground">No change to the file.</p>;
  return (
    <pre className="overflow-auto rounded-lg border bg-card p-3 font-mono text-xs leading-relaxed">
      {text.split("\n").map((line, i) => {
        const color = line.startsWith("+") && !line.startsWith("+++")
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
    </pre>
  );
}

const inputClass = "h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring";

/** Keeps its own text while typing, so a trailing comma isn't eaten. */
function BikesInput({ value, onChange }: { value: FieldValue; onChange: (v: FieldValue) => void }) {
  const format = (v: FieldValue) => (v === "random" ? "random" : Array.isArray(v) ? v.join(", ") : "");
  const parse = (t: string): FieldValue => {
    const s = t.trim();
    if (!s) return null;
    if (s === "random") return "random";
    return s.split(",").map((x) => x.trim()).filter(Boolean);
  };
  const [text, setText] = useState(format(value));
  // Discard and Reload change the value from outside; show it unless the text already means it.
  useEffect(() => {
    setText((t) => (same(parse(t), value) ? t : format(value)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(value)]);
  return (
    <input
      className={`${inputClass} w-full font-mono`}
      placeholder='unset, "random", or bike ids separated by commas'
      value={text}
      onChange={(e) => {
        setText(e.target.value);
        onChange(parse(e.target.value));
      }}
    />
  );
}

function FieldEditor({
  field,
  value,
  dirty,
  onChange,
}: {
  field: ConfigField;
  value: FieldValue;
  dirty: boolean;
  onChange: (v: FieldValue) => void;
}) {
  const k = field.kind;
  let control: React.ReactNode;
  if (k.type === "bool") {
    control = (
      <select
        className={inputClass}
        value={value === null ? "" : String(value)}
        onChange={(e) => onChange(e.target.value === "" ? null : e.target.value === "true")}
      >
        <option value="">default (unset)</option>
        <option value="true">on</option>
        <option value="false">off</option>
      </select>
    );
  } else if (k.type === "int" || k.type === "float") {
    control = (
      <input
        type="number"
        className={`${inputClass} w-36`}
        min={k.min}
        max={k.max}
        step={k.type === "int" ? 1 : "any"}
        placeholder="unset"
        value={typeof value === "number" ? value : ""}
        onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
      />
    );
  } else if (k.type === "choice") {
    control = (
      <select className={inputClass} value={typeof value === "string" ? value : ""} onChange={(e) => onChange(e.target.value || null)}>
        <option value="">default (unset)</option>
        {k.options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
    );
  } else if (k.type === "racing") {
    const current = value === true ? "yield" : value === false ? "off" : typeof value === "string" ? value : "";
    control = (
      <select
        className={inputClass}
        value={current}
        onChange={(e) => onChange(e.target.value === "" ? null : e.target.value === "off" ? false : e.target.value)}
      >
        <option value="">default (unset)</option>
        <option value="off">off</option>
        <option value="neutral">neutral</option>
        <option value="yield">yield</option>
        <option value="block">block</option>
      </select>
    );
  } else if (k.type === "bikes") {
    control = <BikesInput value={value} onChange={onChange} />;
  } else {
    control = (
      <input
        className={`${inputClass} w-full font-mono`}
        placeholder="unset"
        value={typeof value === "string" ? value : ""}
        onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
      />
    );
  }
  return (
    <div className="grid grid-cols-[14rem_1fr] items-start gap-4">
      <div className="flex flex-col">
        <span className="text-sm font-medium">
          {field.label}
          {dirty && <span className="ml-1.5 text-xs text-primary">changed</span>}
        </span>
        <span className="font-mono text-xs text-muted-foreground">{field.key}</span>
      </div>
      <div className="flex flex-col gap-1">
        {control}
        <span className="text-xs text-muted-foreground">{field.help}</span>
      </div>
    </div>
  );
}
