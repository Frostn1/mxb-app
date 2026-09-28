import { useState } from "react";
import { blankServer, errorText, saveServer, type Server, type ServerView } from "@/lib/api";
import { Button, ErrorLine, Field, Input } from "./ui";

/** Add or edit a server. The admin token is write-only here: it goes to the OS keychain and is
 *  never shown again. */
export function ServerForm({
  initial,
  onSaved,
  onCancel,
}: {
  initial: ServerView | null;
  onSaved: (server: ServerView) => void;
  onCancel: () => void;
}) {
  const [server, setServer] = useState<Server>(initial ?? blankServer());
  const [token, setToken] = useState("");
  const [clearToken, setClearToken] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const set = <K extends keyof Server>(key: K, value: Server[K]) => setServer({ ...server, [key]: value });
  const port = (text: string) => (text.trim() === "" ? null : Number(text));

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      const tokenArg = clearToken ? "" : token.trim() === "" ? undefined : token.trim();
      onSaved(await saveServer(server, tokenArg));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <form onSubmit={submit} className="flex max-w-xl flex-col gap-5">
      <h2 className="font-heading text-xl font-extrabold tracking-tight">
        {initial ? `Edit ${initial.name}` : "Add a server"}
      </h2>
      <p className="text-sm text-muted-foreground">
        The app reaches the server over SSH and forwards its loopback-only ports, so nothing new
        has to be opened on the server&apos;s firewall.
      </p>
      <Field label="Name">
        <Input value={server.name} onChange={(e) => set("name", e.target.value)} placeholder="Lightsail" required />
      </Field>
      <div className="grid grid-cols-[1fr_7rem] gap-3">
        <Field label="SSH host">
          <Input value={server.host} onChange={(e) => set("host", e.target.value)} placeholder="16.146.6.22" required />
        </Field>
        <Field label="SSH port">
          <Input type="number" min={1} max={65535} value={server.sshPort} onChange={(e) => set("sshPort", Number(e.target.value))} />
        </Field>
      </div>
      <Field label="SSH user">
        <Input value={server.user} onChange={(e) => set("user", e.target.value)} required />
      </Field>
      <Field label="Private key file" hint="Leave empty to use your ssh agent and ~/.ssh defaults.">
        <Input
          value={server.keyPath ?? ""}
          onChange={(e) => set("keyPath", e.target.value || null)}
          placeholder="C:\Users\you\Downloads\LightsailDefaultKey.pem"
        />
      </Field>
      <div className="grid grid-cols-2 gap-3">
        <Field label="Observe port" hint="/status and /readyz (server.observe)">
          <Input type="number" min={1} max={65535} value={server.observePort} onChange={(e) => set("observePort", Number(e.target.value))} />
        </Field>
        <Field label="Admin port" hint="Optional; needed for riders ([admin] listen)">
          <Input type="number" min={1} max={65535} value={server.adminPort ?? ""} onChange={(e) => set("adminPort", port(e.target.value))} />
        </Field>
      </div>
      <Field
        label="Admin token"
        hint={
          initial?.hasToken
            ? "A token is saved in your OS keychain. Type a new one to replace it."
            : "From `mxbserver admin token new --id you --scope read`. Stored in your OS keychain."
        }
      >
        <Input type="password" autoComplete="off" value={token} onChange={(e) => setToken(e.target.value)} disabled={clearToken} />
      </Field>
      {initial?.hasToken && (
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={clearToken} onChange={(e) => setClearToken(e.target.checked)} />
          Remove the saved token
        </label>
      )}
      <Field label="Log file">
        <Input value={server.logPath} onChange={(e) => set("logPath", e.target.value)} />
      </Field>
      {error && <ErrorLine text={error} />}
      <div className="flex gap-2">
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
