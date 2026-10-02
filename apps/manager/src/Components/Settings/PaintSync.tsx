import { useCallback, useEffect, useState } from "react";
import { Loader2, TriangleAlert } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";
import {
  experimentalState,
  onSyncEvent,
  paintSyncNotSharing,
  type ExperimentalState,
  type SyncEvent,
} from "@frost/shared/api/mods";
import { useT, type TFunc, type TKey } from "@/i18n";
import ViewOnlyPaints from "./ViewOnlyPaints";

/** `1723459200000` -> `2 minutes ago`, `0` -> null. */
function ago(t: TFunc<TKey>, at: number): string | null {
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
 * Written as a checklist with nothing to press: publishing and syncing run on their own
 * (turning the feature on, joining a server, riders arriving). Written that way because the thing a player needs to
 * know is not "what can I do here" but "what is still missing". Both halves fail silently by
 * design — publishing is a side errand of an action that already succeeded, and the sync at
 * launch happens while the player is looking at the game — so if this doesn't say it, nothing
 * does.
 */
export const PaintSync = () => {
  const t = useT();
  const [state, setState] = useState<ExperimentalState | null>(null);
  // What the backend is doing right now, from the `paint-sync` event. `null` when idle.
  const [live, setLive] = useState<SyncEvent["phase"] | null>(null);

  const refresh = useCallback(() => {
    experimentalState()
      .then(setState)
      .catch(() => {});
  }, []);
  useEffect(refresh, [refresh]);

  // Riders on the grid who aren't sharing paints. Asked while the panel is open; the answer
  // only exists while the game is on a server and the app is in its paint-sync room.
  const [notSharing, setNotSharing] = useState<string[]>([]);
  useEffect(() => {
    const ask = () =>
      paintSyncNotSharing()
        .then(setNotSharing)
        .catch(() => {});
    ask();
    const id = window.setInterval(ask, 15_000);
    return () => window.clearInterval(id);
  }, []);

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
            />

            {/* The GUID is the identity that survives a name change. A player can't read it
                off their own machine, so this is no longer something to type: the app takes
                it from the server log the first time one of their servers sees them connect.
                Never an error — a rider name identifies you perfectly well until then. */}
            {state.guid && (
              <StatusRow
                tone="good"
                title={t("sync.guidClaimed", { guid: state.guid })}
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

          {notSharing.length > 0 && (
            <StatusRow
              tone="info"
              title={t("sync.notSharing", { names: notSharing.join(", ") })}
              detail={t("sync.notSharingWhy")}
            />
          )}

          <p className="mt-3 text-[11.5px] text-muted-foreground">
            {t("sync.autoNote")}
          </p>

          <ViewOnlyPaints />
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
