import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check } from "lucide-react";
import { blankServer, errorText, parseLegacyPairing, saveServer, testToken, type Server, type ServerView } from "@/lib/api";
import { Button, ErrorLine, Field, Input, Notice, Toggle } from "./ui";

function Section({ title, hint, first = false, children }: { title: string; hint?: string; first?: boolean; children: ReactNode }) {
  return (
    <section className={`flex flex-col gap-4 ${first ? "" : "border-t pt-6"}`}>
      <div className="flex flex-col gap-1">
        <h3 className="font-heading text-base font-extrabold tracking-tight">{title}</h3>
        {hint && <p className="text-sm text-muted-foreground">{hint}</p>}
      </div>
      {children}
    </section>
  );
}

const serverOnly = (v: ServerView | Server): Server => {
  const { hasToken: _drop, ...rest } = v as ServerView;
  void _drop;
  return rest;
};

/** Add or edit a server. The admin token is never shown: the form says whether one is saved,
 *  and offers Replace and Remove. After a save that involves the token, it is tried at once. */
export function ServerForm({
  initial,
  focusToken = false,
  onSaved,
  onCancel,
}: {
  initial: ServerView | null;
  focusToken?: boolean;
  onSaved: (server: ServerView) => void;
  onCancel: () => void;
}) {
  const [server, setServer] = useState<Server>(initial ? serverOnly(initial) : blankServer());
  // What is saved now: the form's own saves update it, so a retry edits the same server and a
  // token removal never writes back older fields.
  const [stored, setStored] = useState<ServerView | null>(initial);
  const [hasToken, setHasToken] = useState(initial?.hasToken ?? false);
  const [replacing, setReplacing] = useState(!initial?.hasToken && focusToken);
  const [token, setToken] = useState("");
  const [pairing, setPairing] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [check, setCheck] = useState<{ saved: ServerView; ok: boolean; message: string } | null>(null);
  const tokenBox = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (focusToken) tokenBox.current?.scrollIntoView({ block: "center" });
  }, [focusToken]);

  const set = <K extends keyof Server>(key: K, value: Server[K]) => setServer({ ...server, [key]: value });
  const port = (text: string) => (text.trim() === "" ? null : Number(text));
  const applyPairing = async () => {
    setError(null);
    try {
      const parsed = await parseLegacyPairing(pairing);
      setServer({ ...server, kind: "legacy", local: false, host: parsed.host, observePort: parsed.port, agentTls: parsed.tls, adminPort: null, logPath: "" });
      setToken(parsed.token);
      setReplacing(true);
      setPairing("");
    } catch (e) { setError(errorText(e)); }
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      if (server.kind === "legacy" && !hasToken && (!replacing || token.trim() === "")) {
        throw new Error("Add the mxb-agent token to connect.");
      }
      const newToken = replacing && token.trim() !== "" ? token.trim() : undefined;
      const saved = await saveServer(server, newToken);
      setStored(saved);
      setServer(serverOnly(saved));
      setHasToken(saved.hasToken);
      setToken("");
      setReplacing(false);
      // Try the token straight away, so "saved" never has to be taken on trust.
      if (saved.hasToken && (saved.kind === "legacy" || saved.adminPort != null)) {
        const result = await testToken(saved.id);
        setCheck({ saved, ...result });
      } else {
        onSaved(saved);
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  const removeToken = async () => {
    if (!stored) return;
    if (!window.confirm("Remove the saved admin token from this PC? Riders stop showing until you add one again.")) return;
    try {
      // The stored server, not unsaved edits in the form.
      setStored(await saveServer(serverOnly(stored), ""));
      setHasToken(false);
      setCheck(null);
    } catch (e) {
      setError(errorText(e));
    }
  };

  if (check) {
    return (
      <div className="flex max-w-xl flex-col gap-5">
        <h2 className="font-heading text-xl font-extrabold tracking-tight">Saved {check.saved.name}</h2>
        <Notice tone={check.ok ? "ok" : "bad"}>
          <span className="font-medium">{check.ok ? (check.saved.kind === "legacy" ? "mxb-agent connected." : "Admin token saved, and it works.") : (check.saved.kind === "legacy" ? "The agent connection didn't work:" : "Admin token saved, but it didn't work:")}</span>
          {!check.ok && <p className="mt-1">{check.message}</p>}
        </Notice>
        <div className="flex gap-2">
          <Button variant="primary" onClick={() => onSaved(check.saved)}>
            Done
          </Button>
          {!check.ok && (
            <Button
              onClick={() => {
                setCheck(null);
                setReplacing(true);
              }}
            >
              Enter a different token
            </Button>
          )}
        </div>
      </div>
    );
  }

  return (
    <form onSubmit={submit} className="flex max-w-xl flex-col gap-6">
      <h2 className="font-heading text-xl font-extrabold tracking-tight">{initial ? `Edit ${initial.name}` : "Add a server"}</h2>

      <Section title="Server" first>
        <Field label="Connection">
          <div className="grid grid-cols-2 rounded-lg bg-secondary p-1">
            {(["native", "legacy"] as const).map((kind) => (
              <button
                key={kind}
                type="button"
                aria-pressed={server.kind === kind}
                onClick={() => {
                  setServer({
                    ...server,
                    kind,
                    local: false,
                    observePort: kind === "native" ? 9809 : 8787,
                    adminPort: kind === "native" ? server.adminPort : null,
                    logPath: kind === "native" ? "/opt/mxbserver/logs/mxbserver.log" : "",
                  });
                  if (kind === "legacy") setReplacing(true);
                }}
                className={`rounded-md px-3 py-2 text-sm font-medium transition ${server.kind === kind ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"}`}
              >
                {kind === "native" ? "MXB Server" : "Legacy connecting"}
              </button>
            ))}
          </div>
        </Field>
        <Field label="Name">
          <Input value={server.name} onChange={(e) => set("name", e.target.value)} placeholder="Lightsail" required />
        </Field>
        <div className="flex items-center justify-between gap-4">
          <div className="flex flex-col">
            <span className="text-sm font-medium">Runs on this PC</span>
            <span className="text-xs text-muted-foreground">{server.kind === "legacy" ? "Connect to mxb-agent on 127.0.0.1." : "No SSH: the app talks to it on 127.0.0.1 and reads its log file directly."}</span>
          </div>
          <Toggle
            label="Runs on this PC"
            checked={server.local}
            onChange={(local) => setServer({ ...server, local, agentTls: local ? false : server.agentTls, host: local ? "" : server.host, logPath: server.kind === "native" ? (local ? "" : "/opt/mxbserver/logs/mxbserver.log") : "" })}
          />
        </div>
      </Section>

      {!server.local && server.kind === "native" && (
        <Section title="SSH" hint="How the app reaches the server. It forwards the server's local-only ports, so nothing new opens on its firewall.">
          <div className="grid grid-cols-[1fr_7rem] gap-3">
            <Field label="Host">
              <Input value={server.host} onChange={(e) => set("host", e.target.value)} placeholder="16.146.6.22" required />
            </Field>
            <Field label="Port">
              <Input type="number" min={1} max={65535} value={server.sshPort} onChange={(e) => set("sshPort", Number(e.target.value))} />
            </Field>
          </div>
          <Field label="User">
            <Input value={server.user} onChange={(e) => set("user", e.target.value)} required />
          </Field>
          <Field label="Private key file" hint="Leave empty to use your ssh agent and ~/.ssh settings.">
            <Input
              value={server.keyPath ?? ""}
              onChange={(e) => set("keyPath", e.target.value || null)}
              placeholder="C:\Users\you\Downloads\LightsailDefaultKey.pem"
            />
          </Field>
        </Section>
      )}

      {!server.local && server.kind === "legacy" && (
        <Section title="mxb-agent" hint="The official dedicated server is controlled by mxb-agent on its host.">
          <Field label="Pairing code" hint="Paste the mxb-agent: line printed when the agent starts.">
            <div className="flex gap-2"><Input value={pairing} onChange={(e) => setPairing(e.target.value)} placeholder="mxb-agent:…" /><Button type="button" disabled={!pairing.trim()} onClick={() => void applyPairing()}>Use code</Button></div>
          </Field>
          <div className="grid grid-cols-[1fr_7rem] gap-3">
            <Field label="Host">
              <Input value={server.host} onChange={(e) => set("host", e.target.value)} placeholder="192.168.1.20" required />
            </Field>
            <Field label="Port">
              <Input type="number" min={1} max={65535} value={server.observePort} onChange={(e) => set("observePort", Number(e.target.value))} />
            </Field>
          </div>
          <div className="flex items-center justify-between gap-4">
            <div><div className="text-sm font-medium">HTTPS</div><div className="text-xs text-muted-foreground">Enable this when mxb-agent is behind a TLS proxy.</div></div>
            <Toggle label="Use HTTPS" checked={server.agentTls} onChange={(agentTls) => set("agentTls", agentTls)} />
          </div>
          {!server.agentTls && <Notice>Use this only on a private network. The agent token is otherwise sent without encryption.</Notice>}
        </Section>
      )}

      {server.kind === "native" && <Section title="Status and logs">
        <Field label="Status port" hint="The server's observe port (server.observe in its config). 9809 unless you changed it.">
          <Input type="number" min={1} max={65535} value={server.observePort} onChange={(e) => set("observePort", Number(e.target.value))} />
        </Field>
        {server.local && <Field
          label="Log file"
          hint={server.local ? "Where the server's output goes, e.g. C:\\dev\\MXB\\local-server\\logs\\mxbserver.log" : undefined}
        >
          <Input value={server.logPath} onChange={(e) => set("logPath", e.target.value)} required />
        </Field>}
      </Section>}

      <div ref={tokenBox}>
        <Section title={server.kind === "legacy" ? "mxb-agent token" : "Admin API"} hint={server.kind === "legacy" ? "Required to control the official dedicated server." : "Lets the app list who's riding and, with a control-scope token, run race and session controls. Needs an [admin] section in the server's config."}>
          {server.kind === "native" && <>
          <Field label="Admin port" hint={server.local ? "The port in [admin] listen, e.g. 9810." : "Leave empty to use the port in the server's [admin] listen (default 9810)."}>
            <Input
              type="number"
              min={1}
              max={65535}
              value={server.adminPort ?? ""}
              placeholder="not set"
              onChange={(e) => set("adminPort", port(e.target.value))}
            />
          </Field>
          </>}
          <div className="flex flex-col gap-2">
            <span className="text-sm font-medium">Admin token</span>
            {hasToken && !replacing ? (
              <div className="flex flex-wrap items-center gap-3">
                <span className="inline-flex items-center gap-1.5 text-sm text-success">
                  <Check className="size-4" /> {server.kind === "legacy" ? "Agent token saved" : "Admin token saved"}
                </span>
                <Button type="button" size="sm" onClick={() => setReplacing(true)}>
                  Replace
                </Button>
                <Button type="button" size="sm" variant="danger" onClick={() => void removeToken()}>
                  Remove
                </Button>
              </div>
            ) : replacing ? (
              <div className="flex flex-col gap-1.5">
                <Input
                  type="password"
                  aria-label="Admin token"
                  autoComplete="off"
                  autoFocus={focusToken}
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  placeholder={server.kind === "legacy" ? "mxb-agent token" : "you.0123abcd…"}
                />
                <span className="text-xs text-muted-foreground">
                  {server.kind === "legacy" ? "The token in mxb-agent's agent.json." : <>From <span className="font-mono">mxbserver admin token new --id app --scope control</span> on the server (read scope only lists riders; race and session controls need control).</>} Kept in your OS keychain; the app never shows it again.
                  {hasToken && (
                    <>
                      {" "}
                      <button type="button" className="text-link underline" onClick={() => setReplacing(false)}>
                        Keep the saved one
                      </button>
                    </>
                  )}
                </span>
              </div>
            ) : (
              <div className="flex items-center gap-3">
                <span className="text-sm text-muted-foreground">No token saved</span>
                <Button type="button" size="sm" onClick={() => setReplacing(true)}>
                  Add a token
                </Button>
              </div>
            )}
          </div>
        </Section>
      </div>

      {error && <ErrorLine text={error} />}
      <div className="sticky bottom-0 flex gap-2 border-t bg-background py-3">
        <Button type="submit" variant="primary" disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </Button>
        <Button type="button" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
