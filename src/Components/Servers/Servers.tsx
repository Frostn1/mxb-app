import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  Loader2,
  Play,
  Square,
  RotateCw,
  Trash2,
  Plus,
  Plug,
  Cloud,
  Download,
  Upload,
  Globe,
  MessagesSquare,
  Shirt,
  TriangleAlert,
  Server as ServerIcon,
} from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Badge } from "@/Components/ui/badge";
import HelpHint from "@/Components/ui/help-hint";
import { Button } from "@/Components/ui/button";
import { Input } from "@/Components/ui/input";
import { cn } from "@/lib/utils";
import {
  cloudServers,
  destroyCloudServer,
  enrollAccount,
  experimentalState,
  fleetState,
  joinServer,
  listServers,
  onEnrollLink,
  onSyncEvent,
  parsePairing,
  presetsListProfiles,
  publishPaints,
  provisionServer,
  publishServer,
  saveServers,
  DEFAULT_SERVER_REGION,
  SERVER_REGIONS,
  serverAction,
  serverProbe,
  serverSetConfig,
  serverStatus,
  serverTracks,
  setGuid as setGuidApi,
  syncPaints,
  unpublishServer,
  type CloudServer,
  type ExperimentalState,
  type FleetState,
  type SyncEvent,
  type ServerAction,
  type ServerRef,
  type ServerStatus,
} from "../../api/mods";
import { useT, type TFunc } from "../../i18n/context";

/** How often a server's status refreshes while the page is open. */
const POLL_MS = 10000;

/** Invite codes are issued by hand, and this is where they're handed out. */
const DISCORD_URL = "https://discord.gg/3994Rr3ywb";

/** `93784` -> `1d 2h`, `3720` -> `1h 2m`, `45` -> `45s`. */
function uptime(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  return `${Math.floor(h / 24)}d ${h % 24}h`;
}

interface RowProps {
  server: ServerRef;
  onRemove: (id: string) => void;
  /** Publishing writes the registry id back into the saved list, so the page re-reads it. */
  onChanged: () => void;
}

const ServerRow = ({ server, onRemove, onChanged }: RowProps) => {
  const t = useT();
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [track, setTrack] = useState("");
  // What the *host* has installed, not what this PC has — the operator's machine and the
  // server box are different installs, and offering a track the host lacks restarts the
  // server into nothing. `null` while we're still asking.
  const [tracks, setTracks] = useState<string[] | null>(null);
  // The one fact about a server no machine on this end can infer.
  const [region, setRegion] = useState<string>(DEFAULT_SERVER_REGION);

  useEffect(() => {
    let cancelled = false;
    serverTracks(server.id)
      .then((list) => !cancelled && setTracks(list))
      // An agent too old to know `/tracks` shouldn't break the row; the field falls back
      // to free text, which is exactly what it was before.
      .catch(() => !cancelled && setTracks([]));
    return () => {
      cancelled = true;
    };
  }, [server.id]);

  const refresh = useCallback(async () => {
    try {
      const s = await serverStatus(server.id);
      setStatus(s);
      setError(null);
    } catch (e) {
      // Keep the last good status on screen — a blip shouldn't blank the panel — but say
      // plainly that what's shown is stale.
      setError(String(e));
    }
  }, [server.id]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const run = async (action: ServerAction) => {
    setBusy(true);
    try {
      await serverAction(server.id, action);
      toast.success(t("servers.actionDone"));
      await refresh();
    } catch (e) {
      toast.error(t("servers.actionFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const applyTrack = async () => {
    const value = track.trim();
    if (!value) return;
    setBusy(true);
    try {
      await serverSetConfig(server.id, { track: value });
      toast.success(t("servers.trackChanged", { track: value }));
      setTrack("");
      await refresh();
    } catch (e) {
      toast.error(t("servers.actionFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const running = status?.game.running ?? false;
  const listed = Boolean(server.registryId);

  const publish = async () => {
    setBusy(true);
    try {
      const r = await publishServer(server.id, region);
      if (r.published) {
        toast.success(t("servers.published"));
      } else {
        // Recorded but not advertised — the control plane couldn't reach the agent. Saying
        // so plainly beats a success toast for a row nobody will ever see.
        toast.warning(t("servers.publishedUnreachable"));
      }
      onChanged();
    } catch (e) {
      toast.error(t("servers.publishFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const unpublish = async () => {
    setBusy(true);
    try {
      await unpublishServer(server.registryId!);
      toast.success(t("servers.unpublished"));
      onChanged();
    } catch (e) {
      toast.error(t("servers.publishFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  return (
    <div className="rounded-xl border border-white/[0.07] p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span
              className={cn(
                "size-[7px] flex-none rounded-full",
                running ? "bg-success" : "bg-muted-foreground/50",
              )}
            />
            <span className="truncate font-semibold">
              {status?.server.name || server.name}
            </span>
          </div>
          <div className="mt-1 truncate text-[12px] text-muted-foreground">
            {server.url}
          </div>
        </div>
        <button
          onClick={() => onRemove(server.id)}
          title={t("servers.remove")}
          className="cursor-default rounded-md p-1.5 text-muted-foreground hover:bg-white/[0.05]"
        >
          <Trash2 className="size-4" />
        </button>
      </div>

      {error && (
        <div className="mt-3 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12px]">
          {error}
        </div>
      )}

      {status && (
        <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-[12.5px] sm:grid-cols-4">
          <div>
            <dt className="text-muted-foreground">{t("servers.track")}</dt>
            <dd>{status.server.track || "—"}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">{t("servers.slots")}</dt>
            <dd>{status.server.maxClients || "—"}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">{t("servers.uptime")}</dt>
            <dd>
              {running ? uptime(status.game.uptime_secs) : t("servers.stopped")}
            </dd>
          </div>
          <div>
            <dt className="text-muted-foreground">{t("servers.restarts")}</dt>
            <dd>{status.game.restarts}</dd>
          </div>
        </dl>
      )}

      <div className="mt-4 flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={busy || running}
          onClick={() => run("start")}
        >
          <Play className="size-3.5" /> {t("servers.start")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={busy || !running}
          onClick={() => run("stop")}
        >
          <Square className="size-3.5" /> {t("servers.stop")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={busy}
          onClick={() => run("restart")}
        >
          {busy ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <RotateCw className="size-3.5" />
          )}{" "}
          {t("servers.restart")}
        </Button>

        <div className="ml-auto flex items-center gap-2">
          {tracks === null ? (
            <span className="text-[12px] text-muted-foreground">
              {t("servers.trackLoading")}
            </span>
          ) : tracks.length > 0 ? (
            <select
              value={track}
              onChange={(e) => setTrack(e.target.value)}
              className="h-8 w-40 rounded-md border border-white/[0.07] bg-transparent px-2 text-[12.5px]"
            >
              <option value="">{t("servers.trackPlaceholder")}</option>
              {tracks.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          ) : (
            // Nothing to list — an older agent, or a host with no tracks yet. Typing still
            // works, so this degrades to what the field always was.
            <Input
              value={track}
              onChange={(e) => setTrack(e.target.value)}
              placeholder={t("servers.trackPlaceholder")}
              spellCheck={false}
              className="h-8 w-40 text-[12.5px]"
            />
          )}
          {/* Changing the track restarts the game — the .ini is only read at startup. */}
          <Button
            size="sm"
            disabled={busy || !track.trim()}
            onClick={applyTrack}
          >
            {t("servers.setTrack")}
          </Button>
        </div>
      </div>

      {/* Getting the server into everyone else's join list. Nothing is typed here: the
          address is this agent's host plus the port it reports, and the name comes off the
          .ini — only the region is something we can't work out. */}
      <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-white/[0.05] pt-3">
        <Globe className="size-3.5 flex-none text-muted-foreground" />
        {listed ? (
          <>
            <span className="text-[12px] text-muted-foreground">
              {t("servers.listed")}
            </span>
            <Button
              className="ml-auto"
              size="sm"
              variant="outline"
              disabled={busy}
              onClick={() => void unpublish()}
            >
              {t("servers.unpublish")}
            </Button>
          </>
        ) : (
          <>
            <span className="text-[12px] text-muted-foreground">
              {t("servers.notListed")}
            </span>
            <div className="ml-auto flex items-center gap-2">
              <select
                value={region}
                onChange={(e) => setRegion(e.target.value)}
                className="h-8 rounded-md border border-white/[0.07] bg-transparent px-2 text-[12.5px]"
              >
                {SERVER_REGIONS.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
              <Button size="sm" disabled={busy} onClick={() => void publish()}>
                {busy ? <Loader2 className="size-3.5 animate-spin" /> : null}
                {t("servers.publish")}
              </Button>
            </div>
          </>
        )}
      </div>
    </div>
  );
};

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
const PaintSync = () => {
  const t = useT();
  const [state, setState] = useState<ExperimentalState | null>(null);
  const [code, setCode] = useState("");
  const [riderName, setRiderName] = useState("");
  const [guid, setGuid] = useState("");
  const [busy, setBusy] = useState(false);
  // The profiles the game itself wrote. The rider name has to match one of these exactly,
  // so picking from the list is both less typing and the only way to be sure it's right.
  // `null` while scanning; empty means the scan found nothing and we fall back to typing.
  const [profiles, setProfiles] = useState<string[] | null>(null);
  const [manualGuid, setManualGuid] = useState(false);
  // What the backend is doing right now, from the `paint-sync` event. `null` when idle.
  const [live, setLive] = useState<SyncEvent["phase"] | null>(null);

  const refresh = useCallback(() => {
    experimentalState()
      .then(setState)
      .catch(() => {});
  }, []);
  useEffect(refresh, [refresh]);

  useEffect(() => {
    presetsListProfiles()
      .then((scan) => {
        setProfiles(scan.profiles);
        // One profile is the overwhelmingly common case — preselect it so enrolling is the
        // invite code and nothing else.
        if (scan.profiles.length > 0)
          setRiderName((cur) => cur || scan.profiles[0]);
      })
      .catch(() => setProfiles([]));
  }, []);

  // An invite arriving as a clicked `mxb://` link rather than a code to transcribe. This
  // fills the field and stops there — enrolling stays a button the player presses.
  useEffect(() => {
    const pending = onEnrollLink(setCode);
    return () => {
      void pending.then((unlisten) => unlisten());
    };
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

  const enroll = async () => {
    setBusy(true);
    try {
      const name = await enrollAccount(code.trim(), riderName.trim());
      toast.success(t("sync.enrolled", { name }));
      setCode("");
      refresh();
    } catch (e) {
      toast.error(t("sync.enrollFailed"), { description: String(e) });
    }
    setBusy(false);
  };

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
    <div className="mb-5 rounded-xl border border-white/[0.07] p-4">
      <div className="flex items-center gap-2">
        <Shirt className="size-4 text-muted-foreground" />
        <h2 className="font-semibold">{t("sync.title")}</h2>
      </div>
      <p className="mt-1 text-[12.5px] text-muted-foreground">
        {t("sync.desc")}
      </p>

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
        <div className="mt-4 space-y-2">
          {profiles === null ? (
            <div className="h-9 animate-pulse rounded-md bg-white/[0.04]" />
          ) : profiles.length > 0 ? (
            <label className="block text-[11.5px] text-muted-foreground">
              {t("sync.pickProfile")}
              <select
                value={riderName}
                onChange={(e) => setRiderName(e.target.value)}
                className="mt-1.5 h-9 w-full rounded-md border border-white/[0.07] bg-transparent px-2 text-[13px] text-foreground"
              >
                {profiles.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
          ) : (
            // No profiles on disk — a fresh install, or the profiles folder is set wrong.
            // Typing is the only option left, so the exact-match warning still matters.
            <Input
              value={riderName}
              onChange={(e) => setRiderName(e.target.value)}
              placeholder={t("sync.riderNamePlaceholder")}
              spellCheck={false}
            />
          )}
          <Input
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder={t("sync.codePlaceholder")}
            spellCheck={false}
          />
          <p className="text-[11.5px] text-muted-foreground">
            {profiles && profiles.length === 0
              ? t("sync.noProfiles")
              : t("sync.pickProfileHint")}
          </p>
          {/* Where the code comes from. Invites are issued by hand, so without this the
              field is a box with no answer anywhere in the app. */}
          <p className="text-[11.5px] text-muted-foreground">
            {t("sync.whereCode")}
          </p>
          <div className="flex flex-wrap items-center gap-2 pt-1">
            <Button
              size="sm"
              disabled={busy || !code.trim() || !riderName.trim()}
              onClick={() => void enroll()}
            >
              {t("sync.enroll")}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => void openUrl(DISCORD_URL)}
            >
              <MessagesSquare className="size-3.5" /> {t("sync.getCode")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
};

/** `idleSince` + `idleMinutes` -> minutes left, or null while someone is riding. */
function minutesLeft(server: CloudServer): number | null {
  if (!server.idleSince) return null;
  const left = server.idleMinutes - (Date.now() - server.idleSince) / 60000;
  return Math.max(0, Math.round(left));
}

/**
 * A server the control plane runs, from booting to joinable.
 *
 * Pressing Create used to be the end of it: the response carried no token and no address,
 * nothing ever filled either in, and the panel showed a raw instance id and a state string.
 * The server could not be joined, managed or deleted — only waited out. Now the box announces
 * itself when it is up, and this is the row that follows it through.
 */
const CloudServerRow = ({
  server,
  onGone,
  onJoin,
}: {
  server: CloudServer;
  onGone: () => void;
  onJoin: (address: string) => void;
}) => {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [tracks, setTracks] = useState<string[] | null>(null);
  const [track, setTrack] = useState("");

  // Ready means the box has announced itself: it has an address players can connect to and
  // an agent that answered. Anything before that is still coming up.
  const ready = Boolean(server.address && server.agentToken);
  const left = minutesLeft(server);

  const refresh = useCallback(async () => {
    if (!ready) return;
    try {
      setStatus(await serverStatus(server.id));
    } catch {
      // Booting, restarting, or briefly unreachable. The row says "starting" rather than
      // showing an error for something that is expected to resolve itself.
      setStatus(null);
    }
  }, [server.id, ready]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    if (!ready) return;
    let cancelled = false;
    serverTracks(server.id)
      .then((list) => !cancelled && setTracks(list))
      .catch(() => !cancelled && setTracks([]));
    return () => {
      cancelled = true;
    };
  }, [server.id, ready]);

  const run = async (action: ServerAction) => {
    setBusy(true);
    try {
      await serverAction(server.id, action);
      toast.success(t("servers.actionDone"));
      await refresh();
    } catch (e) {
      toast.error(t("servers.actionFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const applyTrack = async () => {
    const value = track.trim();
    if (!value) return;
    setBusy(true);
    try {
      await serverSetConfig(server.id, { track: value });
      toast.success(t("servers.trackChanged", { track: value }));
      setTrack("");
      await refresh();
    } catch (e) {
      toast.error(t("servers.actionFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const destroy = async () => {
    setBusy(true);
    try {
      await destroyCloudServer(server.id);
      toast.success(t("servers.destroyed"));
      onGone();
    } catch (e) {
      toast.error(t("servers.actionFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const running = status?.game.running ?? false;

  return (
    <div className="rounded-xl border border-white/[0.07] p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span
              className={cn(
                "size-[7px] flex-none rounded-full",
                ready && running
                  ? "bg-success"
                  : ready
                    ? "bg-warning"
                    : "bg-muted-foreground/50",
              )}
            />
            <span className="truncate font-semibold">{server.name}</span>
            {server.published && (
              <Badge variant="success">
                <Globe className="size-3" /> {t("servers.inList")}
              </Badge>
            )}
          </div>
          <div className="mt-1 truncate text-[12px] text-muted-foreground">
            {ready ? server.address : t("servers.booting")}
          </div>
        </div>
        <button
          onClick={() => void destroy()}
          disabled={busy}
          title={t("servers.destroy")}
          className="cursor-default rounded-md p-1.5 text-muted-foreground hover:bg-white/[0.05] disabled:opacity-50"
        >
          <Trash2 className="size-4" />
        </button>
      </div>

      {!ready ? (
        // The wait is minutes, not seconds — the bootstrap downloads a 2 GB installer. Saying
        // so is the difference between "still working" and "something has gone wrong".
        <div className="mt-3 space-y-2">
          <div className="flex items-center gap-2 rounded-lg border border-white/[0.07] px-3 py-2 text-[12px] text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" />
            {/* The box reports each step, so a ten-minute wait can say which part it is on
                rather than spinning with nothing to show. */}
            {server.bootstrapStage && server.bootstrapStage !== "failed"
              ? t("servers.bootingStage", { stage: server.bootstrapStage })
              : t("servers.bootingWhy")}
          </div>
          {/* A bootstrap that gave up used to take its own log down with it. This is that
              log, which is the only thing that can say why. */}
          {server.bootstrapStage === "failed" && (
            <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-3">
              <div className="flex items-center gap-2 text-[12px] font-semibold">
                <TriangleAlert className="size-3.5 flex-none text-destructive" />
                {t("servers.bootFailed")}
              </div>
              {server.bootstrapLog && (
                <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-all text-[11px] leading-relaxed text-muted-foreground">
                  {server.bootstrapLog}
                </pre>
              )}
            </div>
          )}
        </div>
      ) : (
        <>
          <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-[12.5px] sm:grid-cols-4">
            <div>
              <dt className="text-muted-foreground">{t("servers.track")}</dt>
              <dd>{status?.server.track || "—"}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">{t("servers.slots")}</dt>
              <dd>{status?.server.maxClients || "—"}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">{t("servers.uptime")}</dt>
              <dd>
                {running
                  ? uptime(status?.game.uptime_secs ?? 0)
                  : t("servers.stopped")}
              </dd>
            </div>
            <div>
              <dt className="text-muted-foreground">
                {t("servers.shutsDown")}
              </dt>
              {/* The bill is the reason this exists, so the countdown is a first-class fact
                  rather than something to discover from a server that vanished. */}
              <dd>
                {left === null
                  ? t("servers.inUse")
                  : t("servers.inMinutes", { count: left })}
              </dd>
            </div>
          </dl>

          <div className="mt-4 flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              disabled={busy}
              onClick={() => onJoin(server.address)}
            >
              <Plug className="size-3.5" /> {t("join.action")}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={busy || running}
              onClick={() => void run("start")}
            >
              <Play className="size-3.5" /> {t("servers.start")}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={busy || !running}
              onClick={() => void run("stop")}
            >
              <Square className="size-3.5" /> {t("servers.stop")}
            </Button>

            <div className="ml-auto flex items-center gap-2">
              {tracks && tracks.length > 0 ? (
                <select
                  value={track}
                  onChange={(e) => setTrack(e.target.value)}
                  className="h-8 w-40 rounded-md border border-white/[0.07] bg-transparent px-2 text-[12.5px]"
                >
                  <option value="">{t("servers.trackPlaceholder")}</option>
                  {tracks.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              ) : (
                <Input
                  value={track}
                  onChange={(e) => setTrack(e.target.value)}
                  placeholder={t("servers.trackPlaceholder")}
                  spellCheck={false}
                  className="h-8 w-40 text-[12.5px]"
                />
              )}
              <Button
                size="sm"
                disabled={busy || !track.trim()}
                onClick={() => void applyTrack()}
              >
                {t("servers.setTrack")}
              </Button>
            </div>
          </div>
        </>
      )}
    </div>
  );
};

/**
 * Create a server without owning a machine.
 *
 * The control plane launches it — the app never holds a cloud credential, because a desktop
 * binary can be unpacked and a key inside one would let anyone spend our money.
 *
 * The running count is read from EC2 rather than from our own records: that is the number
 * being billed, and the two disagree exactly when something has already gone wrong. A player
 * deciding whether to start another server should be looking at the real one.
 */
const CreateServer = ({ onJoin }: { onJoin: (address: string) => void }) => {
  const t = useT();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [fleet, setFleet] = useState<FleetState | null>(null);
  const [mine, setMine] = useState<CloudServer[] | null>(null);
  const [unavailable, setUnavailable] = useState<string | null>(null);

  const refresh = useCallback(() => {
    fleetState()
      .then((f) => {
        setFleet(f);
        setUnavailable(null);
      })
      // Not enrolled, or this deployment can't provision. Either way the panel explains
      // itself rather than showing a broken control.
      .catch((e) => setUnavailable(String(e)));
    cloudServers()
      .then(setMine)
      .catch(() => setMine([]));
  }, []);
  useEffect(refresh, [refresh]);

  // A booting server changes state on its own, and the only way to notice is to ask again.
  useEffect(() => {
    const waiting = mine?.some((s) => !s.address || !s.agentToken) ?? false;
    if (!waiting) return;
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [mine, refresh]);

  const create = async () => {
    setBusy(true);
    try {
      await provisionServer(name.trim());
      toast.success(t("servers.creating"));
      setName("");
      refresh();
    } catch (e) {
      toast.error(t("servers.createFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  const atCap = Boolean(fleet && fleet.running >= fleet.cap);

  return (
    <div className="mb-5 rounded-xl border border-white/[0.07] p-4">
      <div className="flex items-center gap-2">
        <Cloud className="size-4 text-muted-foreground" />
        <h2 className="font-semibold">{t("servers.createTitle")}</h2>
        {fleet && (
          <span className="ml-auto text-[12px] text-muted-foreground">
            {t("servers.runningOfCap", {
              count: fleet.running,
              cap: fleet.cap,
            })}
          </span>
        )}
      </div>
      <p className="mt-1 text-[12.5px] text-muted-foreground">
        {t("servers.createDesc")}
      </p>

      {unavailable ? (
        <p className="mt-3 text-[11.5px] text-muted-foreground">
          {unavailable}
        </p>
      ) : (
        <>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("servers.namePlaceholder")}
              className="h-9 flex-1"
            />
            <Button
              size="sm"
              disabled={busy || atCap || name.trim().length < 2}
              onClick={() => void create()}
            >
              {busy ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Plus className="size-3.5" />
              )}
              {t("servers.create")}
            </Button>
          </div>
          {/* Refusing at the button, with the reason, beats a 409 from the control plane
              phrased as an error. */}
          {atCap && (
            <p className="mt-2 text-[11.5px] text-warning">
              {t("servers.atCap", { cap: fleet?.cap ?? 0 })}
            </p>
          )}
        </>
      )}

      {mine && mine.length > 0 && (
        <div className="mt-4 space-y-3">
          {mine.map((s) => (
            <CloudServerRow
              key={s.id}
              server={s}
              onGone={refresh}
              onJoin={onJoin}
            />
          ))}
        </div>
      )}
    </div>
  );
};

const Servers = () => {
  const t = useT();
  const [servers, setServers] = useState<ServerRef[]>([]);
  const [adding, setAdding] = useState(false);
  const [probing, setProbing] = useState(false);
  const [draft, setDraft] = useState({ name: "", url: "", token: "" });
  const [pairingBlob, setPairingBlob] = useState("");
  const [manualServer, setManualServer] = useState(false);

  /**
   * Fill the address and token from a pasted pairing code.
   *
   * Decoded as it's typed rather than behind a button: a pairing code is only ever pasted
   * whole, so the moment it parses there's nothing left to confirm. A partial value — the
   * first keystroke of a paste on a slow machine — simply doesn't parse, and says nothing
   * until the operator stops.
   */
  const onPairingPaste = async (value: string) => {
    setPairingBlob(value);
    if (!value.trim()) return;
    try {
      const { url, token } = await parsePairing(value);
      setDraft((d) => ({ ...d, url, token }));
    } catch {
      // Silent: this fires on every keystroke, and a half-pasted code is not an error the
      // operator needs to be told about. `add` reports it if they try to submit anyway.
    }
  };

  // Re-read from the backend rather than patching local state: publishing writes the
  // registry id into the saved list on the Rust side, so that file is the source of truth.
  const reload = useCallback(() => {
    void listServers()
      .then(setServers)
      .catch(() => {});
  }, []);
  useEffect(reload, [reload]);

  /**
   * Ride on a server from the page that manages it.
   *
   * The same `-directconnect` launch the Join dialog performs — a server you can start, stop
   * and set the track on but have to go and find in a list is not really joinable from here.
   */
  const join = async (address: string) => {
    try {
      const outcome = await joinServer(address);
      if (outcome === "already_running") toast.info(t("join.alreadyRunning"));
      else toast.success(t("join.launching", { address }));
    } catch (e) {
      toast.error(t("join.failed"), { description: String(e) });
    }
  };

  const persist = async (next: ServerRef[]) => {
    setServers(next);
    try {
      await saveServers(next);
    } catch (e) {
      toast.error(t("servers.saveFailed"), { description: String(e) });
    }
  };

  /**
   * Add a server, naming it from the host rather than from the operator.
   *
   * The agent already knows what the server is called — it's in the `.ini` it manages — so
   * the name field is a fallback, not a requirement. Probing first also means a wrong
   * address or token fails here, with a message, instead of being saved as a row that
   * never loads.
   */
  const add = async () => {
    const { name, url, token } = draft;
    if (!url.trim() || !token.trim()) return;
    setProbing(true);

    let resolved = name.trim();
    try {
      const status = await serverProbe(url.trim(), token.trim());
      const fromHost = status.server.name?.trim();
      if (!resolved && fromHost) {
        resolved = fromHost;
        toast.success(t("servers.probed", { name: fromHost }));
      }
    } catch (e) {
      // Refuse rather than saving something we couldn't reach: a dead row is the failure
      // mode this probe exists to prevent. A name typed by hand doesn't rescue it.
      toast.error(t("servers.probeFailed"), { description: String(e) });
      setProbing(false);
      return;
    }

    // crypto.randomUUID keeps ids unique without a counter that a reordered or partially
    // removed list could collide with.
    const entry: ServerRef = {
      id: crypto.randomUUID(),
      // The host had no name set and the operator gave none — fall back to the address, so
      // the row is still identifiable in a list of several.
      name: resolved || url.trim(),
      url: url.trim(),
      token: token.trim(),
    };
    await persist([...servers, entry]);
    setDraft({ name: "", url: "", token: "" });
    setPairingBlob("");
    setManualServer(false);
    setProbing(false);
    setAdding(false);
  };

  return (
    // The Dashboard gives every view a fixed, `overflow-hidden` box and expects the view to
    // scroll itself. This page never did: it was short enough to fit, so everything below
    // the fold was simply clipped with no scrollbar to suggest there was more.
    //
    // The column-plus-`min-h-0` shape is copied from Settings and Presets rather than
    // invented. A bare `h-full overflow-y-auto` does not work here — the percentage has no
    // definite height to resolve against, so the box grows to fit its content, never
    // overflows, and is clipped by the parent exactly as before. `min-h-0` is the part that
    // makes it real: without it a flex child refuses to shrink below its content.
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-4xl p-6">
          <header className="mb-5">
            <div className="flex items-center gap-1.5">
              <h1 className="text-xl font-semibold">{t("servers.title")}</h1>
              {/* Every other screen has one of these; this is the screen that needed it most. */}
              <HelpHint
                title={t("servers.title")}
                description={t("servers.help")}
              />
            </div>
            <p className="mt-1 text-[13px] text-muted-foreground">
              {t("servers.subtitle")}
            </p>
          </header>

          <PaintSync />

          <CreateServer onJoin={join} />

          {servers.length === 0 && !adding && (
            <div className="rounded-xl border border-dashed border-white/[0.1] p-8 text-center">
              <ServerIcon className="mx-auto size-6 text-muted-foreground" />
              <p className="mt-2 text-[13px] text-muted-foreground">
                {t("servers.empty")}
              </p>
            </div>
          )}

          <div className="space-y-3">
            {servers.map((s) => (
              <ServerRow
                key={s.id}
                server={s}
                onRemove={(id) =>
                  void persist(servers.filter((x) => x.id !== id))
                }
                onChanged={reload}
              />
            ))}
          </div>

          {adding ? (
            <div className="mt-4 space-y-2 rounded-xl border border-white/[0.07] p-4">
              {/* One field, not four. The pairing code carries the address and the token, and
              the name comes off the host — so the manual fields are collapsed behind a
              disclosure rather than sitting here implying they all need filling in. */}
              <Input
                autoFocus
                value={pairingBlob}
                onChange={(e) => void onPairingPaste(e.target.value)}
                placeholder={t("servers.pairingPlaceholder")}
                spellCheck={false}
              />
              <p className="text-[11.5px] text-muted-foreground">
                {t("servers.pairingWhere")}
              </p>

              {manualServer ? (
                <>
                  <Input
                    value={draft.url}
                    onChange={(e) =>
                      setDraft({ ...draft, url: e.target.value })
                    }
                    placeholder="http://203.0.113.10:8787"
                    spellCheck={false}
                  />
                  <Input
                    type="password"
                    value={draft.token}
                    onChange={(e) =>
                      setDraft({ ...draft, token: e.target.value })
                    }
                    placeholder={t("servers.tokenPlaceholder")}
                    spellCheck={false}
                  />
                  <Input
                    value={draft.name}
                    onChange={(e) =>
                      setDraft({ ...draft, name: e.target.value })
                    }
                    placeholder={t("servers.nameOptional")}
                  />
                </>
              ) : (
                <button
                  onClick={() => setManualServer(true)}
                  className="cursor-default text-left text-[11.5px] text-muted-foreground underline underline-offset-2 hover:text-foreground"
                >
                  {t("servers.manualEntry")}
                </button>
              )}
              <div className="flex justify-end gap-2 pt-1">
                <Button
                  size="sm"
                  variant="outline"
                  disabled={probing}
                  onClick={() => setAdding(false)}
                >
                  {t("common.cancel")}
                </Button>
                <Button
                  size="sm"
                  disabled={probing || !draft.url.trim() || !draft.token.trim()}
                  onClick={() => void add()}
                >
                  {probing && <Loader2 className="size-3.5 animate-spin" />}
                  {probing ? t("servers.probing") : t("servers.add")}
                </Button>
              </div>
            </div>
          ) : (
            <Button
              className="mt-4"
              size="sm"
              variant="outline"
              onClick={() => setAdding(true)}
            >
              <Plus className="size-3.5" /> {t("servers.add")}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
};

export default Servers;
