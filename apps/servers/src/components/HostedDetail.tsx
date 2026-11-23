import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { errorText, type ServerView } from "@/lib/api";
import { hostedReport, hostedRestart, hostedSaveSettings, hostedServer, type HostedServer, type HostedSettings } from "@/lib/hosted";
import { usePoll } from "@/lib/usePoll";
import { Button, Card, ErrorLine, Field, Notice, OverflowMenu, Stat, StatusBadge, type MenuItem } from "./ui";

const select = "h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/30";

/** A server hosted by mxbsecure: status, settings and restart, through the control plane. */
export function HostedDetail({ server, menu }: { server: ServerView; menu: MenuItem[] }) {
  const poll = usePoll(() => hostedServer(server.id), 5000, `${server.id}-hosted`);
  const hosted = poll.data;

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-end gap-x-5 gap-y-2">
        <div className="flex min-w-0 flex-col">
          <h2 className="truncate font-heading text-2xl font-extrabold tracking-tight">{server.name}</h2>
          <div className="flex items-center gap-3 text-xs text-muted-foreground">
            <span className="font-mono">{hosted?.address ?? server.host ?? ""}</span>
            <span>Hosted</span>
            <StatusBadge report={hosted ? hostedReport(hosted) : null} error={poll.error} />
          </div>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={poll.refresh} disabled={poll.loading}>
            <RefreshCw className={`size-3.5 ${poll.loading ? "animate-spin" : ""}`} /> Refresh
          </Button>
          <OverflowMenu items={menu} label={`Actions for ${server.name}`} />
        </div>
      </header>
      {!hosted && poll.error && <ErrorLine text={poll.error} />}
      {hosted && <HostedBody id={server.id} hosted={hosted} onChanged={poll.refresh} />}
    </div>
  );
}

function HostedBody({ id, hosted, onChanged }: { id: string; hosted: HostedServer; onChanged: () => void }) {
  const [draft, setDraft] = useState<HostedSettings>({});
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  useEffect(() => setDraft({}), [id]);

  const track = draft.track ?? hosted.settings.track ?? "";
  const bikeSet = draft.bikeSet ?? hosted.settings.bikeSet ?? "";
  const maxRiders = draft.maxRiders ?? hosted.settings.maxRiders;
  const dirty = Object.keys(draft).length > 0;
  const ready = hosted.state === "ready";

  const run = async (task: () => Promise<unknown>, done: string) => {
    setBusy(true);
    setMessage(null);
    try {
      await task();
      setMessage({ ok: true, text: done });
      onChanged();
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <Card className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        <Stat label="Address" value={hosted.address ?? "—"} />
        <Stat label="Region" value={hosted.regionLabel} />
        <Stat label="Type" value={hosted.type === "legacy" ? "Legacy" : "mxbserver"} />
        <Stat label="Riders" value={hosted.riders} />
      </Card>
      {!ready && hosted.state !== "failed" && (
        <Notice>
          <span className="font-medium">{hosted.progress.steps[hosted.progress.step - 1] ?? "Starting"}</span>
        </Notice>
      )}
      {hosted.state === "failed" && <Notice tone="bad"><span className="font-medium">{hosted.error ?? "This server failed."}</span></Notice>}

      <section className="flex flex-col gap-4">
        <h3 className="font-heading text-base font-extrabold tracking-tight">Settings</h3>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Track">
            <select className={select} value={track} onChange={(e) => setDraft({ ...draft, track: e.target.value })} disabled={busy || hosted.options.tracks.length === 0}>
              {!track && <option value="">—</option>}
              {hosted.options.tracks.map((t) => (
                <option key={t.id} value={t.id}>{t.name}</option>
              ))}
            </select>
          </Field>
          {hosted.options.bikeSets.length > 0 && (
            <Field label="Bikes">
              <select className={select} value={bikeSet} onChange={(e) => setDraft({ ...draft, bikeSet: e.target.value })} disabled={busy}>
                {hosted.options.bikeSets.map((b) => (
                  <option key={b.id} value={b.id}>{b.name}</option>
                ))}
              </select>
            </Field>
          )}
          <Field label="Max riders">
            <select className={select} value={maxRiders} onChange={(e) => setDraft({ ...draft, maxRiders: Number(e.target.value) })} disabled={busy}>
              {Array.from({ length: hosted.options.maxRiders }, (_, i) => i + 1).map((n) => (
                <option key={n} value={n}>{n}</option>
              ))}
            </select>
          </Field>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="primary" disabled={busy || !dirty} onClick={() => void run(async () => { await hostedSaveSettings(id, draft); setDraft({}); }, "Saved.")}>
            Save
          </Button>
          <Button
            variant="danger"
            disabled={busy || !ready}
            onClick={() => {
              if (window.confirm(`Restart ${hosted.name}? Riders are disconnected.`)) void run(() => hostedRestart(id), "Restarting.");
            }}
          >
            Restart
          </Button>
        </div>
        {message && (message.ok ? <p className="text-sm text-muted-foreground">{message.text}</p> : <ErrorLine text={message.text} />)}
      </section>
    </div>
  );
}
