import { isLegacyStatus, serverStatus, type ServerView } from "@/lib/api";
import { duration, sessionName } from "@/lib/format";
import { usePoll } from "@/lib/usePoll";
import { Card, OverflowMenu, Stat, StatusBadge, type MenuItem } from "./ui";

/** One server on the fleet screen: status, session, uptime. Polls every 5 s. */
export function FleetCard({ server, onOpen, menu }: { server: ServerView; onOpen: () => void; menu: MenuItem[] }) {
  const poll = usePoll(() => serverStatus(server.id), 5000, `${server.id}-fleet`);
  const s = poll.data?.status;
  const legacy = s && isLegacyStatus(s) ? s : null;
  const native = s && !isLegacyStatus(s) ? s : null;

  return (
    <Card className="relative flex flex-col gap-4 transition hover:border-primary">
      <div className="flex items-start justify-between gap-2">
        <button
          type="button"
          onClick={onOpen}
          className="flex min-w-0 flex-col text-left after:absolute after:inset-0 after:rounded-xl focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:outline-ring"
        >
          <span className="truncate font-heading text-lg font-extrabold tracking-tight">{server.name}</span>
          <span className="truncate font-mono text-xs text-muted-foreground">{server.local ? "this PC" : server.host}{server.kind === "legacy" ? " · Legacy connecting" : ""}</span>
        </button>
        {/* Above the card-wide click target. */}
        <div className="relative z-10">
          <OverflowMenu items={menu} label={`Actions for ${server.name}`} />
        </div>
      </div>
      <StatusBadge report={poll.data} error={poll.error} />
      {native && poll.data?.state !== "offline" && (
        <div className="grid grid-cols-2 gap-3">
          <Stat label="Session" value={sessionName(native.session)} />
          <Stat label="Up for" value={duration(native.uptime_seconds)} />
        </div>
      )}
      {legacy && <div className="grid grid-cols-2 gap-3"><Stat label="Track" value={legacy.server.track ?? "—"} /><Stat label="Up for" value={duration(legacy.game.uptime_secs)} /></div>}
    </Card>
  );
}
