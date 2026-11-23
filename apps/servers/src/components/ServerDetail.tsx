import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { configLoad, errorText, isLegacyStatus, legacyProcess, presence, serverLogs, serverRestartService, serverRiders, serverSession, serverStatus, type ServerView } from "@/lib/api";
import { duration, lapTime, sessionName } from "@/lib/format";
import { usePoll } from "@/lib/usePoll";
import { runAction } from "@/lib/actions";
import { useEventFeed } from "@/lib/useEventFeed";
import { ConfigTab } from "./ConfigTab";
import { CutMapCard } from "./CutMapCard";
import { EventsTab } from "./EventsTab";
import { LegacySettings } from "./LegacySettings";
import { TracksTab } from "./TracksTab";
import { VersionTab } from "./VersionTab";
import { Button, Card, ErrorLine, Notice, OverflowMenu, Stat, StatusBadge, type MenuItem } from "./ui";

type Tab = "status" | "riders" | "events" | "tracks" | "version" | "logs" | "config";

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
  // The event feed starts with the first visit to the tab and keeps collecting after it.
  const [eventsSeen, setEventsSeen] = useState<string | null>(null);
  const status = usePoll(() => serverStatus(server.id), 3000, server.id);
  const tabs: Tab[] = server.kind === "legacy" ? ["status", "riders", "tracks", "logs", "config"] : ["status", "riders", "events", "tracks", "version", "logs", "config"];

  return (
    <div className="flex h-full flex-col gap-6">
      <header className="flex flex-wrap items-end gap-x-5 gap-y-2">
        <div className="flex min-w-0 flex-col">
          <h2 className="truncate font-heading text-2xl font-extrabold tracking-tight">{server.name}</h2>
          <div className="flex items-center gap-3 text-xs text-muted-foreground">
            <span className="font-mono">{server.local ? "this PC" : server.kind === "legacy" ? server.host : `${server.user}@${server.host}`}</span>
            {server.kind === "legacy" && <span>Legacy connecting</span>}
            <StatusBadge report={status.data} error={status.error} />
          </div>
        </div>
        <div className="ml-auto">
          <OverflowMenu items={menu} label={`Actions for ${server.name}`} />
        </div>
      </header>

      <nav className="flex gap-1 border-b" role="tablist">
        {tabs.map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            onClick={() => {
              setTab(t);
              if (t === "events") setEventsSeen(server.id);
            }}
            className={`-mb-px border-b-2 px-3 py-2 text-sm font-medium capitalize ${
              tab === t ? "border-primary text-foreground" : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            {t === "config" ? "Settings" : t}
          </button>
        ))}
      </nav>

      <div className={`min-h-0 flex-1 ${tab === "config" || tab === "events" ? "overflow-hidden" : "overflow-auto"}`}>
        {tab === "status" && <StatusTab server={server} poll={status} onSetUpToken={onSetUpToken} />}
        {tab === "riders" && <RidersTab server={server} onSetUpToken={onSetUpToken} />}
        {(tab === "events" || eventsSeen === server.id) && server.kind === "native" && <EventsPanel key={server.id} server={server} hidden={tab !== "events"} />}
        {tab === "tracks" && <TracksTab server={server} />}
        {tab === "version" && <VersionTab server={server} />}
        {tab === "logs" && <LogsTab server={server} />}
        {tab === "config" && (server.kind === "legacy" ? <LegacySettings server={server} /> : <ConfigTab server={server} />)}
      </div>
    </div>
  );
}

/** Mounted from the first visit to Events on, so the feed keeps collecting on other tabs. */
function EventsPanel({ server, hidden }: { server: ServerView; hidden: boolean }) {
  const feed = useEventFeed(server);
  return <div className={hidden ? "hidden" : "h-full"}><EventsTab server={server} feed={feed} /></div>;
}

function AgentHelper({ onSetUpToken }: { onSetUpToken: () => void }) {
  return <Notice><div className="flex items-center justify-between gap-4"><span>Add the token from mxb-agent&apos;s agent.json to connect.</span><Button size="sm" variant="primary" onClick={onSetUpToken}>Add token</Button></div></Notice>;
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
  if (server.kind === "legacy" && s && isLegacyStatus(s)) {
    return <LegacyStatusPanel server={server} status={s} detail={report?.detail ?? ""} refresh={poll.refresh} />;
  }
  const native = s && !isLegacyStatus(s) ? s : null;
  const needsToken = server.kind === "legacy" ? !server.hasToken : server.local && (server.adminPort == null || !server.hasToken);
  return (
    <div className="flex flex-col gap-5">
      {report && report.state !== "online" && (
        <Notice tone={report.state === "starting" ? "info" : "bad"}>{report.detail}</Notice>
      )}
      {!report && poll.error && <ErrorLine text={poll.error} />}
      {native && report?.state !== "offline" && (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_15rem]">
          <Card className="flex flex-col gap-4">
            <h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Current session</h3>
            <div className="grid grid-cols-3 gap-5">
              <Stat label="Stage" value={sessionName(native.session)} />
              <Stat label="Time left" value={native.session_remaining_seconds == null ? "—" : duration(native.session_remaining_seconds)} />
              <Stat label="Riders" value={presence(native).riders} />
              {presence(native).bots > 0 && <Stat label="Bots" value={presence(native).bots} />}
            </div>
          </Card>
          <Card className="flex flex-col gap-4">
            <h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Server</h3>
            <div className="grid grid-cols-3 gap-5">
              <Stat label="Running for" value={duration(native.uptime_seconds)} />
              <Stat label="Version" value={`v${native.version}`} />
              <Stat label="Commit" value={native.revision} />
            </div>
          </Card>
          <Card className="flex flex-col gap-4">
            <h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Diagnostics</h3>
            <Stat label="Packets in / out" value={`${native.client_datagrams_total} / ${native.server_datagrams_total}`} />
            <Stat label="Ready" value={native.ready ? "Yes" : "No"} />
          </Card>
        </div>
      )}
      {!server.local && native && <SessionControls server={server} current={native.session} remaining={native.session_remaining_seconds} refresh={poll.refresh} />}
      {native && report?.state !== "offline" && server.kind === "native" && <CutMapCard server={server} />}
      {!report && !poll.error && (
        <p className="text-sm text-muted-foreground">{server.local ? "Connecting…" : "Connecting over SSH…"}</p>
      )}
      {needsToken && (server.kind === "legacy" ? <AgentHelper onSetUpToken={onSetUpToken} /> : <TokenHelper server={server} onSetUpToken={onSetUpToken} />)}
    </div>
  );
}

function RidersTab({ server, onSetUpToken }: { server: ServerView; onSetUpToken: () => void }) {
  const enabled = server.kind === "legacy" ? server.hasToken : (!server.local || (server.adminPort != null && server.hasToken));
  const riders = usePoll(() => (enabled ? serverRiders(server.id) : Promise.resolve([])), enabled ? 5000 : 0, `${server.id}-riders`);
  if (!enabled) {
    return (
      <div className="max-w-3xl">
        {server.kind === "legacy" ? <AgentHelper onSetUpToken={onSetUpToken} /> : <TokenHelper server={server} onSetUpToken={onSetUpToken} />}
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

function LegacyStatusPanel({ server, status, detail, refresh }: { server: ServerView; status: Extract<Awaited<ReturnType<typeof serverStatus>>["status"], { kind: "stock" }>; detail: string; refresh: () => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const act = async (action: "start" | "stop" | "restart") => {
    if ((action === "stop" || action === "restart") && !window.confirm(`${action === "stop" ? "Stop" : "Restart"} ${server.name}? Connected riders will be disconnected.`)) return;
    setBusy(action); setError(null);
    try { await legacyProcess(server.id, action); refresh(); }
    catch (e) { setError(errorText(e)); }
    finally { setBusy(null); }
  };
  return <div className="flex flex-col gap-5">
    {!status.game.running && <Notice tone="bad">{detail}</Notice>}
    <div className="grid gap-4 lg:grid-cols-3">
      <Card className="space-y-4"><h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Official server</h3><div className="grid grid-cols-2 gap-5"><Stat label="State" value={status.game.running ? "Running" : "Stopped"} /><Stat label="Running for" value={duration(status.game.uptime_secs)} /></div></Card>
      <Card className="space-y-4"><h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Event</h3><div className="grid grid-cols-2 gap-5"><Stat label="Track" value={status.server.track ?? "—"} /><Stat label="Maximum riders" value={status.server.maxClients ?? "—"} /></div></Card>
      <Card className="space-y-4"><h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Process</h3><div className="grid grid-cols-2 gap-5"><Stat label="PID" value={status.game.pid ?? "—"} /><Stat label="Crash restarts" value={status.game.restarts} /></div></Card>
    </div>
    <Card className="flex items-center justify-between gap-4"><div><h3 className="font-heading text-base font-extrabold">Server controls</h3>{busy && <p className="text-xs text-muted-foreground">{busy === "restart" ? "Restarting…" : busy === "stop" ? "Stopping…" : "Starting…"}</p>}</div><div className="flex gap-2">{status.game.running ? <><Button disabled={!!busy} onClick={() => void act("restart")}>Restart</Button><Button variant="danger" disabled={!!busy} onClick={() => void act("stop")}>Stop</Button></> : <Button variant="primary" disabled={!!busy} onClick={() => void act("start")}>Start</Button>}</div></Card>
    {error && <ErrorLine text={error} />}
  </div>;
}

export function SessionControls({ server, current, remaining, refresh }: { server: ServerView; current: string; remaining: number | null; refresh: () => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pendingStage, setPendingStage] = useState<string | null>(null);
  const act = async (action: "jump" | "advance" | "restart", to?: "practice" | "qualifying" | "warmup" | "race") => {
    const destination = to ? sessionName(to) : "";
    const inProgress = current.startsWith("running(race") || /^countdown/i.test(current);
    const prompt = action === "jump"
      ? inProgress
        ? `Switch the live server to ${destination}? This ends the ${/^countdown/i.test(current) ? "countdown" : "race"} in progress and riders will be moved.`
        : `Switch the live server to ${destination}?`
      : action === "advance"
        ? "Advance the live server to its next stage?"
        : `Restart ${sessionName(current)} from the beginning?`;
    if (!window.confirm(prompt)) return;
    const label = to ? `Starting ${to}…` : action === "advance" ? "Advancing session…" : "Restarting session…";
    // The clicked stage is marked at once; the mark is dropped if the server refuses.
    setBusy(label); setError(null); setPendingStage(to ?? null);
    const result = await runAction({ name: `session_${action}`, run: () => serverSession(server.id, action, to) });
    if (result.ok) refresh();
    else if (result.error.includes("already running") || result.error.includes("countdown always runs") || result.error.includes("race runs until it is over")) refresh();
    else setError(result.error);
    setPendingStage(null); setBusy(null);
  };
  const restartServer = async () => {
    if (!window.confirm(`Restart the ${server.name} service? Connected riders will be disconnected.`)) return;
    setBusy("Restarting the server…"); setError(null);
    try { await serverRestartService(server.id); refresh(); }
    catch (e) { setError(errorText(e)); }
    finally { setBusy(null); }
  };
  const running = /^running\((practice|qualifying|warmup|race)\)$/.exec(current)?.[1] ?? "";
  const countdown = /^countdown(?:\((practice|qualifying|warmup|race)\)|\s*\{\s*next:\s*(practice|qualifying|warmup|race)\s*\})$/i.exec(current);
  const pending = (countdown?.[1] ?? countdown?.[2] ?? "").toLowerCase();
  const locked = !!pending;
  const raceLocked = running === "race";
  const controlsLocked = locked || raceLocked;
  useEffect(() => { if (controlsLocked) setError(null); }, [controlsLocked]);
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
        <div className="flex min-w-0 items-center gap-3"><h3 className="font-heading text-base font-extrabold">Race control</h3>{busy ? <span className="truncate text-xs text-muted-foreground">{busy}</span> : locked ? <span className="truncate text-xs font-medium text-primary">{sessionName(pending)} starts in {remaining == null ? "a moment" : duration(remaining)}</span> : raceLocked ? <span className="truncate text-xs text-muted-foreground">Pick a stage to jump, or wait for the race to finish</span> : null}</div>
        <div className="flex gap-2">
          {!server.local && <Button size="sm" variant="ghost" disabled={!!busy} title="systemctl restart mxbserver" onClick={() => void restartServer()}>Restart server</Button>}
          <Button size="sm" variant="ghost" disabled={!!busy || controlsLocked} title={raceLocked ? "The race must finish" : locked ? "The countdown must finish" : undefined} onClick={() => void act("restart")}>Restart current</Button>
          <Button size="sm" variant="primary" disabled={!!busy || controlsLocked} title={raceLocked ? "The race must finish" : locked ? "The countdown must finish" : undefined} onClick={() => void act("advance")}>Next stage</Button>
        </div>
      </div>
      <div className="grid overflow-hidden rounded-lg border sm:grid-cols-4">
        {(["practice", "qualifying", "warmup", "race"] as const).map((stage, index) => (
          <button
            key={stage}
            type="button"
            disabled={!!busy || running === stage}
            onClick={() => void act("jump", stage)}
            aria-current={running === stage || pending === stage || pendingStage === stage ? "step" : undefined}
            className={`flex items-center gap-3 border-b px-4 py-3 text-left transition disabled:cursor-default sm:border-b-0 sm:border-r sm:last:border-r-0 ${running === stage || pending === stage ? "bg-primary/10 text-primary" : "hover:bg-accent disabled:opacity-50"}`}
          >
            <span className="grid size-6 shrink-0 place-items-center rounded-full bg-secondary font-mono text-xs text-muted-foreground">{index + 1}</span>
            <span className="flex min-w-0 flex-1 items-center justify-between gap-2 text-sm font-medium capitalize">
              <span className="flex flex-col"><span>{stage === "warmup" ? "Warm-up" : stage}</span><span className="text-[11px] font-normal normal-case text-muted-foreground">{details[stage]}</span></span>
              {pendingStage === stage && <span className="rounded-full bg-primary/20 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary">Starting…</span>}{running === stage && !pendingStage && <span className="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary-foreground">Current</span>}
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
        <span className="text-xs text-muted-foreground">last 500 lines of {server.local ? server.logPath : "journalctl -u mxbserver"}</span>
      </div>
      {logs.error && <ErrorLine text={logs.error} />}
      <pre className="min-h-0 flex-1 overflow-auto rounded-lg bg-muted p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap">
        {lines.join("\n")}
        <div ref={bottom} />
      </pre>
    </div>
  );
}
