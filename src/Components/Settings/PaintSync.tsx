import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Loader2, Download, Upload, TriangleAlert } from "lucide-react";
import { Button } from "@/Components/ui/button";
import { Input } from "@/Components/ui/input";
import { cn } from "@/lib/utils";
import {
  experimentalState,
  onSyncEvent,
  publishPaints,
  setGuid as setGuidApi,
  syncPaints,
  type ExperimentalState,
  type SyncEvent,
} from "../../api/mods";
import { useT, type TFunc } from "../../i18n/context";

/** `1723459200000` -> `2 minutes ago`, `0` -> null. */
function ago(t: TFunc, at: number): string | null {
  if (!at) return null;
  const secs = Math.max(0, Math.round((Date.now() - at) / 1000));
  if (secs < 60) return t("sync.agoJustNow");
  const mins = Math.round(secs / 60);
  if (mins < 60) return t("sync.agoMinutes", { count: mins });
  const hours = Math.round(mins / 60);
  if (hours < 24) return t("sync.agoHours", { count: hours });
  return t("sync.agoDays", { count: Math.round(hours / 24) });
}

type RowTone = "good" | "missing" | "info" | "busy";

/**
 * One thing that is either working or isn't, said in a sentence.
 *
 * The panel this belongs to used to report the outcome of nothing at all: publishing and
 * syncing ran in background tasks whose only output was a log line, so a player had no way
 * to tell a working feature from a broken one — and the most common failure, never having
 * published, looked exactly like success. Every row here answers "is this part done, and if
 * not, what do I press".
 */
const StatusRow = ({
  tone,
  title,
  detail,
  action,
}: {
  tone: RowTone;
  title: string;
  detail?: string;
  action?: React.ReactNode;
}) => (
  <div className="flex items-start gap-2.5 py-2">
    {tone === "busy" ? (
      <Loader2 className="mt-[3px] size-[13px] flex-none animate-spin text-muted-foreground" />
    ) : (
      <span
        className={cn(
          "mt-[6px] size-[7px] flex-none rounded-full",
          tone === "good" && "bg-success",
          tone === "missing" && "bg-warning",
          tone === "info" && "bg-muted-foreground/50",
        )}
      />
    )}
    <div className="min-w-0 flex-1">
      <div className="text-[12.5px] text-foreground/85">{title}</div>
      {detail && (
        <div className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground">
          {detail}
        </div>
      )}
    </div>
    {action && <div className="flex-none pt-0.5">{action}</div>}
  </div>
);

/**
 * Enrollment and paint sync.
 *
 * MX Bikes sends no custom content, so other riders render in default liveries unless you
 * already hold their exact paint file. This is the panel that fixes that: publish what
 * you're wearing, pull back what everyone else published.
 *
 * Written as a checklist rather than a pair of buttons, because the thing a player needs to
 * know is not "what can I do here" but "what is still missing". Both halves fail silently by
 * design — publishing is a side errand of an action that already succeeded, and the sync at
 * launch happens while the player is looking at the game — so if this doesn't say it, nothing
 * does.
 */
export const PaintSync = () => {
  const t = useT();
  const [state, setState] = useState<ExperimentalState | null>(null);
  const [guid, setGuid] = useState("");
  const [busy, setBusy] = useState(false);
  const [manualGuid, setManualGuid] = useState(false);
  // What the backend is doing right now, from the `paint-sync` event. `null` when idle.
  const [live, setLive] = useState<SyncEvent["phase"] | null>(null);

  const refresh = useCallback(() => {
    experimentalState()
      .then(setState)
      .catch(() => {});
  }, []);
  useEffect(refresh, [refresh]);

  // Follow the background work. Publishing happens off a preset apply, a launch, or the game
  // rewriting profile.ini; syncing happens when the game starts. None of it is anything the
  // player triggered here, and all of it belongs on screen.
  useEffect(() => {
    const pending = onSyncEvent((e) => {
      setLive(
        e.phase === "publishing" || e.phase === "pulling" ? e.phase : null,
      );
      // Re-read rather than patching from the payload: the backend writes what it achieved
      // to the config, and that record is what survives a restart.
      if (e.phase !== "publishing" && e.phase !== "pulling") refresh();
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, [refresh]);

  const claimGuid = async () => {
    setBusy(true);
    try {
      await setGuidApi(guid.trim());
      toast.success(t("sync.guidSaved"));
      setGuid("");
      refresh();
    } catch (e) {
      toast.error(t("sync.enrollFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const publish = async () => {
    setBusy(true);
    try {
      // Forced: pressing this after a successful publish is otherwise correctly a no-op,
      // which reads as a broken button.
      const r = await publishPaints(true);
      toast.success(
        t("sync.published", { paints: r.published, bikes: r.bikes }),
      );
      if (r.skippedBikes > 0)
        toast.warning(t("sync.skippedBikes", { count: r.skippedBikes }));
      // A livery that never leaves the machine is worth saying out loud; otherwise the rider
      // looks default to everyone else and nothing ever explains why.
      if (r.oversizedPaints > 0)
        toast.warning(t("sync.oversized", { count: r.oversizedPaints }));
      refresh();
    } catch (e) {
      toast.error(t("sync.publishFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const pull = async () => {
    setBusy(true);
    try {
      const r = await syncPaints();
      toast.success(
        t("sync.pulled", {
          installed: r.installed,
          riders: r.riders,
          had: r.alreadyHad,
        }),
      );
      if (r.rejected > 0)
        toast.warning(t("sync.rejected", { count: r.rejected }));
      refresh();
    } catch (e) {
      toast.error(t("sync.pullFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const sync = state?.sync;
  const publishedAgo = ago(t, sync?.publishedAt ?? 0);
  const pulledAgo = ago(t, sync?.pulledAt ?? 0);
  const hasPublished = Boolean(sync?.publishedAt);
  const hasPulled = Boolean(sync?.pulledAt);

  return (
    <div>

      {state?.enrolled ? (
        <>
          <div className="mt-3 divide-y divide-white/[0.05]">
            <StatusRow
              tone="good"
              title={t("sync.ridingAs", { name: state.riderName })}
              detail={
                // A rider name matching no profile on disk publishes nothing, silently. It is
                // the one setup mistake that looks identical to everything working.
                state.profile ? undefined : t("sync.noMatchingProfile")
              }
            />

            <StatusRow
              tone={
                live === "publishing"
                  ? "busy"
                  : hasPublished
                    ? "good"
                    : "missing"
              }
              title={
                live === "publishing"
                  ? t("sync.publishing")
                  : hasPublished
                    ? t("sync.publishedState", {
                        bikes: sync?.publishedBikes ?? 0,
                        paints: sync?.publishedPaints ?? 0,
                      })
                    : t("sync.neverPublished")
              }
              detail={
                hasPublished
                  ? publishedAgo
                    ? t("sync.lastPublished", { ago: publishedAgo })
                    : undefined
                  : t("sync.neverPublishedWhy")
              }
              action={
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  onClick={() => void publish()}
                >
                  <Upload className="size-3.5" /> {t("sync.publishNow")}
                </Button>
              }
            />

            <StatusRow
              tone={
                live === "pulling" ? "busy" : hasPulled ? "good" : "missing"
              }
              title={
                live === "pulling"
                  ? t("sync.pulling")
                  : hasPulled
                    ? t("sync.pulledState", { count: sync?.pulledRiders ?? 0 })
                    : t("sync.neverPulled")
              }
              detail={
                hasPulled
                  ? pulledAgo
                    ? t("sync.lastPulled", { ago: pulledAgo })
                    : undefined
                  : t("sync.neverPulledWhy")
              }
              action={
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  onClick={() => void pull()}
                >
                  <Download className="size-3.5" /> {t("sync.pull")}
                </Button>
              }
            />

            {/* The GUID is the identity that survives a name change. A player can't read it
                off their own machine, so this is no longer something to type: the app takes
                it from the server log the first time one of their servers sees them connect.
                Never an error — a rider name identifies you perfectly well until then. */}
            {state.guid ? (
              <StatusRow
                tone="good"
                title={t("sync.guidClaimed", { guid: state.guid })}
              />
            ) : manualGuid ? (
              <div className="flex flex-wrap items-end gap-2 py-2">
                <label className="flex-1 text-[11.5px] text-muted-foreground">
                  {t("sync.guidHint")}
                  <Input
                    value={guid}
                    onChange={(e) => setGuid(e.target.value)}
                    placeholder={t("sync.guidPlaceholder")}
                    spellCheck={false}
                    className="mt-1.5 h-8 text-[12.5px]"
                  />
                </label>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy || !guid.trim()}
                  onClick={() => void claimGuid()}
                >
                  {t("sync.setGuid")}
                </Button>
              </div>
            ) : (
              <StatusRow
                tone="info"
                title={t("sync.guidPendingTitle")}
                detail={t("sync.guidPending")}
                action={
                  <button
                    onClick={() => setManualGuid(true)}
                    className="cursor-default text-[11.5px] text-muted-foreground underline underline-offset-2 hover:text-foreground"
                  >
                    {t("sync.guidManual")}
                  </button>
                }
              />
            )}
          </div>

          {/* Paints the sync declined to overwrite. Silently doing nothing is exactly the
              failure this replaced, so when it happens it has to be said. */}
          {(sync?.keptYours ?? 0) > 0 && (
            <div className="mt-3 flex items-start gap-2.5 rounded-lg border border-warning/30 bg-warning/[0.08] p-3">
              <TriangleAlert className="mt-[1px] size-4 flex-none text-warning" />
              <div className="flex flex-col gap-0.5">
                <span className="text-[12px] font-semibold text-foreground/85">
                  {t("sync.keptYours", { count: sync?.keptYours ?? 0 })}
                </span>
                <span className="text-[11.5px] leading-relaxed text-muted-foreground">
                  {t("sync.keptYoursWhy")}
                </span>
              </div>
            </div>
          )}

          <p className="mt-3 text-[11.5px] text-muted-foreground">
            {t("sync.autoNote")}
          </p>
        </>
      ) : (
        // No account yet, which on a fresh install is simply "nothing has run".
        // There is nothing to press: the app signs itself up the first time the game
        // starts, so the honest thing to show is what it is waiting for.
        <StatusRow
          tone="info"
          title={t("sync.notStartedTitle")}
          detail={t("sync.notStartedWhy")}
        />
      )}
    </div>
  );
};

export default PaintSync;
