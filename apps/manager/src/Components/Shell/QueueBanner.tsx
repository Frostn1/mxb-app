import { useEffect } from "react";
import { Hourglass, Plug, X } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { onServerQueue, queueLeave } from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import { useServerQueue } from "@/lib/useServerQueue";

/**
 * The server line you're in, from any tab. Mounted once in the top rail, so it also owns the
 * toasts: the turn can come while you're on another screen or in another app.
 */
const QueueBanner = () => {
  const t = useT();
  const queue = useServerQueue();

  useEffect(() => {
    const unlisten = onServerQueue((q) => {
      const name = q.name || q.address;
      if (q.phase === "turn") {
        toast.info(t("queue.turn", { name }), { duration: 60_000 });
      } else if (q.phase === "launched") {
        toast.success(t("queue.launching", { name }));
      } else if (q.phase === "joined") {
        toast.success(t("queue.joined", { name }));
      } else if (q.phase === "ended" && q.error === "missed") {
        toast.warning(t("queue.missed", { name }));
      } else if (q.phase === "ended" && q.error) {
        toast.error(q.error);
      }
    });
    return () => {
      unlisten.then((f) => f()).catch(() => {});
    };
  }, [t]);

  if (!queue) return null;
  const name = queue.name || queue.address;
  const turn = queue.phase === "turn" || queue.phase === "launched";
  const label =
    queue.phase === "turn"
      ? t("queue.turnBanner", { name })
      : queue.phase === "launched"
        ? t("queue.launchingBanner", { name })
        : t("queue.banner", { position: queue.position, name });
  const title = [
    queue.error === "offline" ? t("queue.offline") : "",
    queue.players !== null && queue.maxPlayers !== null
      ? t("serverBrowser.ridersCount", { players: queue.players, maxPlayers: queue.maxPlayers })
      : "",
    t("queue.onlyApp"),
  ]
    .filter(Boolean)
    .join("\n");

  return (
    <div
      title={title}
      className={cn(
        "mr-1 flex h-[26px] max-w-[280px] items-center gap-1.5 rounded-md pl-2 pr-1 text-[12.5px]",
        turn ? "bg-primary text-primary-foreground" : "bg-popover text-foreground",
      )}
    >
      {turn ? (
        <Plug className="size-3.5 shrink-0" />
      ) : (
        <Hourglass className={cn("size-3.5 shrink-0", queue.error && "text-destructive")} />
      )}
      <span className="truncate tabular-figures">{label}</span>
      <button
        onClick={() => queueLeave().catch(() => {})}
        title={t("queue.leave")}
        aria-label={t("queue.leave")}
        className="grid size-5 shrink-0 cursor-default place-items-center rounded opacity-70 hover:opacity-100"
      >
        <X className="size-3" />
      </button>
    </div>
  );
};

export default QueueBanner;
