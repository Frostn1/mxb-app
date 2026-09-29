import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { configLoad, errorText, serverLogs, serverRiders, serverSession, serverStatus, type ServerView } from "@/lib/api";
import { duration, lapTime, sessionName } from "@/lib/format";
import { usePoll } from "@/lib/usePoll";
import { ConfigTab } from "./ConfigTab";
import { TracksTab } from "./TracksTab";
import { VersionTab } from "./VersionTab";
import { Button, Card, ErrorLine, Notice, OverflowMenu, Stat, StatusBadge, type MenuItem } from "./ui";

type Tab = "status" | "riders" | "tracks" | "version" | "logs" | "config";

export function ServerDetail({
  server,
  menu,
  onSetUpToken,
}: {
  server: ServerView;
  menu: MenuItem[];
  onSetUpToken: () => void;
}) {
  const [tab, setTab] = useState<Tab>("status");
  const status = usePoll(() => serverStatus(server.id), 3000, server.id);

  return (
    <div className="flex h-full flex-col gap-6">
      <header className="flex flex-wrap items-end gap-x-5 gap-y-2">
        <div className="flex min-w-0 flex-col">
          <h2 className="truncate font-heading text-2xl font-extrabold tracking-tight">{server.name}</h2>
          <div className="flex items-center gap-3 text-xs text-muted-foreground">
            <span className="font-mono">{server.local ? "this PC" : `${server.user}@${server.host}`}</span>
            <StatusBadge report={status.data} error={status.error} />
          </div>
        </div>
        <div className="ml-auto">
          <OverflowMenu items={menu} label={`Actions for ${server.name}`} />
        </div>
      </header>

      <nav className="flex gap-1 border-b" role="tablist">
        {(["status", "riders", "tracks", "version", "logs", "config"] as Tab[]).map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            onClick={() => setTab(t)}
            className={`-mb-px border-b-2 px-3 py-2 text-sm font-medium capitalize ${
              tab === t ? "border-primary text-foreground" : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            {t === "config" ? "Settings" : t}
          </button>
        ))}
      </nav>

      <div className={`min-h-0 flex-1 ${tab === "config" ? "overflow-hidden" : "overflow-auto"}`}>
        {tab === "status" && <StatusTab server={server} poll={status} onSetUpToken={onSetUpToken} />}
        {tab === "riders" && <RidersTab server={server} onSetUpToken={onSetUpToken} />}
        {tab === "tracks" && <TracksTab server={server} />}
        {tab === "version" && <VersionTab server={server} />}
        {tab === "logs" && <LogsTab server={server} />}
        {tab === "config" && <ConfigTab server={server} />}
      </div>
    </div>
  );
}

/** What the admin token is for, and how to get one, for a server without it. */
function TokenHelper({ server, onSetUpToken }: { server: ServerView; onSetUpToken: () => void }) {
  return (
    <Notice>
      <div className="flex flex-col gap-2">
        <span className="font-medium">Set up an admin token to see who&apos;s riding</span>
        <span className="text-muted-foreground">
          Status and logs work without one. The token lets this app ask the server for its rider list (names, bikes,
          laps, best laps, ping). It&apos;s a password for the server&apos;s admin API, which only listens on the
          server itself.
        </span>
        <ol className="ml-4 list-decimal text-muted-foreground">
          <li>
            On the server, make a read-only token:{" "}
            <span className="font-mono text-foreground">mxbserver admin token new --id you --scope read</span>
          </li>
          <li>
            Add the printed entry to its tokens file, and give the config an <span className="font-mono">[admin]</span>{" "}
            section with <span className="font-mono">listen = &quot;127.0.0.1:9810&quot;</span>, then restart it.
          </li>
          <li>Paste the token here. It&apos;s kept in {server.local ? "Windows Credential Manager" : "your OS keychain"}.</li>
        </ol>
        <div>
          <Button variant="primary" size="sm" onClick={onSetUpToken}>
            Set up admin token
          </Button>
        </div>
      </div>
    </Notice>
  );
}

function StatusTab({
  server,
  poll,
  onSetUpToken,
}: {
  server: ServerView;
  poll: ReturnType<typeof usePoll<Awaited<ReturnType<typeof serverStatus>>>>;
  onSetUpToken: () => void;
}) {
  const report = poll.data;
  const s = report?.status;
  const needsToken = server.local && (server.adminPort == null || !server.hasToken);
  return (
    <div className="flex flex-col gap-5">
      {report && report.state !== "online" && (
        <Notice tone={report.state === "starting" ? "info" : "bad"}>{report.detail}</Notice>
      )}
      {!report && poll.error && <ErrorLine text={poll.error} />}
      {s && report?.state !== "offline" && (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_15rem]">
          <Card className="flex flex-col gap-4">
            <h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Current session</h3>
            <div className="grid grid-cols-3 gap-5">
              <Stat label="Stage" value={sessionName(s.session)} />
              <Stat label="Time left" value={s.session_remaining_seconds == null ? "—" : duration(s.session_remaining_seconds)} />
              <Stat label="Riders" value={s.active_sessions} />
            </div>
          </Card>
          <Card className="flex flex-col gap-4">
            <h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Server</h3>
            <div className="grid grid-cols-3 gap-5">
              <Stat label="Running for" value={duration(s.uptime_seconds)} />
              <Stat label="Version" value={`v${s.version}`} />
              <Stat label="Commit" value={s.revision} />
            </div>
          </Card>
          <Card className="flex flex-col gap-4">
            <h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Diagnostics</h3>
            <Stat label="Packets in / out" value={`${s.client_datagrams_total} / ${s.server_datagrams_total}`} />
            <Stat label="Ready" value={s.ready ? "Yes" : "No"} />
          </Card>
        </div>
      )}
      {!server.local && <SessionControls server={server} current={s?.session ?? ""} remaining={s?.session_remaining_seconds ?? null} refresh={poll.refresh} />}
      {!report && !poll.error && (
        <p className="text-sm text-muted-foreground">{server.local ? "Connecting…" : "Connecting over SSH…"}</p>
      )}
      {needsToken && <TokenHelper server={server} onSetUpToken={onSetUpToken} />}
    </div>
  );
}

function RidersTab({ server, onSetUpToken }: { server: ServerView; onSetUpToken: () => void }) {
  const enabled = !server.local || (server.adminPort != null && server.hasToken);
  const riders = usePoll(() => (enabled ? serverRiders(server.id) : Promise.resolve([])), enabled ? 5000 : 0, `${server.id}-riders`);
  if (!enabled) {
    return (
      <div className="max-w-3xl">
        <TokenHelper server={server} onSetUpToken={onSetUpToken} />
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-3">
      {riders.error && <ErrorLine text={riders.error} />}
      {riders.data && riders.data.length === 0 && <p className="text-sm text-muted-foreground">Nobody is connected.</p>}
      {riders.data && riders.data.length > 0 && (
        <table className="w-full text-sm">
          <thead className="text-left text-xs text-muted-foreground">
            <tr>
              <th className="py-2 font-medium">Rider</th>
              <th className="font-medium">Bike</th>
              <th className="font-medium">State</th>
              <th className="text-right font-medium">Laps</th>
              <th className="text-right font-medium">Best lap</th>
              <th className="text-right font-medium">Ping</th>
              <th className="text-right font-medium">Connected</th>
            </tr>
          </thead>
          <tbody className="font-mono tabular-nums">
            {riders.data.map((r) => (
              <tr key={r.connection_id} className="border-t">
                <td className="py-2 font-sans">{r.name}</td>
                <td>{r.bike ?? "—"}</td>
                <td>{r.state}</td>
                <td className="text-right">{r.laps}</td>
                <td className="text-right">{lapTime(r.best_lap_seconds)}</td>
                <td className="text-right">{r.ping_ms == null ? "—" : `${r.ping_ms} ms`}</td>
                <td className="text-right">{duration(r.connected_seconds)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function SessionControls({ server, current, remaining, refresh }: { server: ServerView; current: string; remaining: number | null; refresh: () => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const act = async (action: "jump" | "advance" | "restart", to?: "practice" | "qualifying" | "warmup" | "race") => {
    const destination = to ? sessionName(to) : "";
    const prompt = action === "jump"
      ? `Switch the live server to ${destination}?`
      : action === "advance"
        ? "Advance the live server to its next stage?"
        : `Restart ${sessionName(current)} from the beginning?`;
    if (!window.confirm(prompt)) return;
    const label = to ? `Starting ${to}…` : action === "advance" ? "Advancing session…" : "Restarting session…";
    setBusy(label); setError(null);
    try {
      await serverSession(server.id, action, to);
      refresh();
    } catch (e) {
      const message = errorText(e);
      if (message.includes("already running") || message.includes("countdown always runs")) refresh();
      else setError(message);
    } finally { setBusy(null); }
  };
  const running = /^running\((practice|qualifying|warmup|race)\)$/.exec(current)?.[1] ?? "";
  const countdown = /^countdown(?:\((practice|qualifying|warmup|race)\)|\s*\{\s*next:\s*(practice|qualifying|warmup|race)\s*\})$/i.exec(current);
  const pending = (countdown?.[1] ?? countdown?.[2] ?? "").toLowerCase();
  const locked = !!pending;
  useEffect(() => { if (locked) setError(null); }, [locked]);
  const config = usePoll(() => configLoad(server.id), 30_000, `${server.id}-race-config`);
  const number = (key: string, fallback: number) => {
    const value = config.data?.values[`sessions.${key}`];
    return typeof value === "number" ? value : fallback;
  };
  const details = {
    practice: number("practice_minutes", 20) === 0 ? "Until advanced" : `${number("practice_minutes", 20)} min`,
    qualifying: `${number("qualifying_minutes", 15)} min`,
    warmup: `${number("warmup_minutes", 5)} min`,
    race: `${number("race_minutes", 20)} min + ${number("race_extra_laps", 2)} laps`,
  } as const;
  return (
    <Card className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3"><h3 className="font-heading text-base font-extrabold">Race control</h3>{busy ? <span className="truncate text-xs text-muted-foreground">{busy}</span> : locked ? <span className="truncate text-xs font-medium text-primary">{sessionName(pending)} starts in {remaining == null ? "a moment" : duration(remaining)}</span> : null}</div>
        <div className="flex gap-2">
          <Button size="sm" variant="ghost" disabled={!!busy || locked} title={locked ? "The countdown must finish" : undefined} onClick={() => void act("restart")}>Restart current</Button>
          <Button size="sm" variant="primary" disabled={!!busy || locked} title={locked ? "The countdown must finish" : undefined} onClick={() => void act("advance")}>Next stage</Button>
        </div>
      </div>
      <div className="grid overflow-hidden rounded-lg border sm:grid-cols-4">
        {(["practice", "qualifying", "warmup", "race"] as const).map((stage, index) => (
          <button
            key={stage}
            type="button"
            disabled={!!busy || locked || running === stage}
            onClick={() => void act("jump", stage)}
            aria-current={running === stage || pending === stage ? "step" : undefined}
            className={`flex items-center gap-3 border-b px-4 py-3 text-left transition disabled:cursor-default sm:border-b-0 sm:border-r sm:last:border-r-0 ${running === stage || pending === stage ? "bg-primary/10 text-primary" : "hover:bg-accent disabled:opacity-50"}`}
          >
            <span className="grid size-6 shrink-0 place-items-center rounded-full bg-secondary font-mono text-xs text-muted-foreground">{index + 1}</span>
            <span className="flex min-w-0 flex-1 items-center justify-between gap-2 text-sm font-medium capitalize">
              <span className="flex flex-col"><span>{stage === "warmup" ? "Warm-up" : stage}</span><span className="text-[11px] font-normal normal-case text-muted-foreground">{details[stage]}</span></span>
              {running === stage && <span className="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary-foreground">Current</span>}
              {pending === stage && <span className="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary-foreground">Next</span>}
            </span>
          </button>
        ))}
      </div>
      {error && <ErrorLine text={error} />}
    </Card>
  );
}

function LogsTab({ server }: { server: ServerView }) {
  const [follow, setFollow] = useState(true);
  const [filter, setFilter] = useState("");
  const logs = usePoll(() => serverLogs(server.id, 500), follow ? 4000 : 0, `${server.id}-logs`);
  const bottom = useRef<HTMLDivElement>(null);
  const lines = (logs.data ?? []).filter((l) => !filter || l.toLowerCase().includes(filter.toLowerCase()));

  useEffect(() => {
    if (follow) bottom.current?.scrollIntoView({ block: "end" });
  }, [logs.data, follow]);

  return (
    <div className="flex h-full flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter"
          className="h-9 w-64 rounded-md border border-input bg-background px-3 text-sm"
        />
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} />
          Follow
        </label>
        <Button onClick={logs.refresh} disabled={logs.loading} aria-label="Refresh logs">
          <RefreshCw className="size-4" /> Refresh
        </Button>
        <span className="text-xs text-muted-foreground">last 500 lines of {server.logPath}</span>
      </div>
      {logs.error && <ErrorLine text={logs.error} />}
      <pre className="min-h-0 flex-1 overflow-auto rounded-lg bg-muted p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap">
        {lines.join("\n")}
        <div ref={bottom} />
      </pre>
    </div>
  );
}
