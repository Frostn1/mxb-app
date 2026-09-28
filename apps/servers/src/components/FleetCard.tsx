import { serverStatus, type ServerView } from "@/lib/api";
import { duration, sessionName } from "@/lib/format";
import { usePoll } from "@/lib/usePoll";
import { Card, ErrorLine, HealthBadge, Stat, type Health } from "./ui";

/** One server on the fleet screen: health, session, riders, build. Polls every 5 s. */
export function FleetCard({ server, onOpen }: { server: ServerView; onOpen: () => void }) {
  const poll = usePoll(() => serverStatus(server.id), 5000, `${server.id}-fleet`);
  const s = poll.data?.status;
  const health: Health = poll.error ? "down" : !poll.data ? "unknown" : poll.data.ready ? "ready" : "starting";

  return (
    <button onClick={onOpen} className="text-left focus-visible:outline-2 focus-visible:outline-ring rounded-xl">
      <Card className="flex flex-col gap-4 transition hover:border-primary">
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 flex-col">
            <span className="truncate font-heading text-lg font-extrabold tracking-tight">{server.name}</span>
            <span className="truncate font-mono text-xs text-muted-foreground">{server.local ? "this PC" : server.host}</span>
          </div>
          <HealthBadge health={health} />
        </div>
        {s && (
          <div className="grid grid-cols-3 gap-3">
            <Stat label="Session" value={sessionName(s.session)} />
            <Stat label="Riders" value={s.active_sessions} />
            <Stat label="Up" value={duration(s.uptime_seconds)} />
          </div>
        )}
        {s && <span className="font-mono text-xs text-muted-foreground">{s.revision} · build {s.build_id}</span>}
        {poll.error && <ErrorLine text={poll.error} />}
      </Card>
    </button>
  );
}
