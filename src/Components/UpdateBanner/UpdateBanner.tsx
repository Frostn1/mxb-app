import { ArrowUpCircle, Loader2, X } from "lucide-react";
import { Button } from "@/Components/ui/button";
import { useUpdate } from "@/Context/Update";

/**
 * Slim, dismissible bar shown at the top of the app when a newer signed build
 * is available. Renders nothing when there's no update pending.
 */
export default function UpdateBanner() {
  const { available, installing, progress, install, dismiss } = useUpdate();
  if (!available) return null;

  return (
    <div className="flex items-center gap-3 border-b border-primary/25 bg-primary/10 px-4 py-2 text-sm text-foreground">
      <ArrowUpCircle className="size-4 shrink-0 text-primary" />
      <span className="min-w-0 truncate">
        <span className="font-semibold">MXB App v{available.version}</span> is
        available.
        <span className="ml-1 text-muted-foreground">
          {installing
            ? progress != null
              ? `Downloading… ${progress}%`
              : "Downloading…"
            : "Update to get the latest features and fixes."}
        </span>
      </span>

      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        <Button size="sm" onClick={() => void install()} disabled={installing}>
          {installing ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <ArrowUpCircle className="size-3.5" />
          )}
          {installing ? "Updating…" : "Update & restart"}
        </Button>
        <Button
          size="icon"
          variant="ghost"
          className="size-8"
          onClick={dismiss}
          disabled={installing}
          aria-label="Dismiss update notification"
        >
          <X className="size-4" />
        </Button>
      </div>
    </div>
  );
}
