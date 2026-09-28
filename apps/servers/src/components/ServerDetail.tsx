import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { serverLogs, serverRiders, serverStatus, type ServerView } from "@/lib/api";
import { duration, lapTime, sessionName } from "@/lib/format";
import { usePoll } from "@/lib/usePoll";
import { Button, Card, ErrorLine, HealthBadge, Stat, type Health } from "./ui";

type Tab = "status" | "riders" | "logs";

export function ServerDetail({
  server,
  onEdit,
  onRemove,
}: {
  server: ServerView;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const [tab, setTab] = useState<Tab>("status");
  const status = usePoll(() => serverStatus(server.id), 3000, server.id);
  const health: Health = status.data
    ? status.error
      ? "down"
      : status.data.ready
        ? "ready"
        : "starting"
    : status.error
      ? "down"
      : "unknown";

  return (
    <div className="flex h-full flex-col gap-5">
      <header className="flex flex-wrap items-center gap-4">
        <div className="flex flex-col">
          <h2 className="font-heading text-2xl font-extrabold tracking-tight">{server.name}</h2>
          <span className="font-mono text-xs text-muted-foreground">
            {server.local ? "this PC" : `${server.user}@${server.host}`}
          </span>
        </div>
        <HealthBadge health={health} />
        <div className="ml-auto flex gap-2">
          <Button onClick={onEdit}>Edit</Button>
          <Button variant="danger" onClick={onRemove}>
            Remove
          </Button>
        </div>
      </header>

      <nav className="flex gap-1 border-b" role="tablist">
        {(["status", "riders", "logs"] as Tab[]).map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            onClick={() => setTab(t)}
            className={`-mb-px border-b-2 px-3 py-2 text-sm font-medium capitalize ${
              tab === t ? "border-primary text-foreground" : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            {t}
          </button>
        ))}
      </nav>

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === "status" && <StatusTab poll={status} />}
        {tab === "riders" && <RidersTab server={server} />}
        {tab === "logs" && <LogsTab server={server} />}
      </div>
    </div>
  );
}

function StatusTab({ poll }: { poll: ReturnType<typeof usePoll<Awaited<ReturnType<typeof serverStatus>>>> }) {
  const s = poll.data?.status;
  return (
    <div className="flex flex-col gap-4">
      {poll.error && <ErrorLine text={poll.error} />}
      {s ? (
        <Card className="grid grid-cols-2 gap-5 md:grid-cols-4">
          <Stat label="Session" value={sessionName(s.session)} />
          <Stat
            label="Time left"
            value={s.session_remaining_seconds == null ? "—" : duration(s.session_remaining_seconds)}
          />
          <Stat label="Riders connected" value={s.active_sessions} />
          <Stat label="Uptime" value={duration(s.uptime_seconds)} />
          <Stat label="Version" value={`v${s.version}`} />
          <Stat label="Revision" value={s.revision} />
          <Stat label="Build" value={s.build_id} />
          <Stat label="Datagrams in / out" value={`${s.client_datagrams_total} / ${s.server_datagrams_total}`} />
        </Card>
      ) : (
        !poll.error && <p className="text-sm text-muted-foreground">Connecting over SSH…</p>
      )}
    </div>
  );
}

function RidersTab({ server }: { server: ServerView }) {
  const enabled = server.adminPort != null && server.hasToken;
  const riders = usePoll(() => (enabled ? serverRiders(server.id) : Promise.resolve([])), enabled ? 5000 : 0, `${server.id}-riders`);
  if (!enabled) {
    return (
      <p className="max-w-lg text-sm text-muted-foreground">
        Riders come from the server&apos;s admin API. Set its admin port and a read token under Edit
        (<span className="font-mono">mxbserver admin token new --id you --scope read</span> on the server).
      </p>
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
