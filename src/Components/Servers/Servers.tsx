import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  Loader2,
  Play,
  Square,
  RotateCw,
  Trash2,
  Plus,
  Cloud,
  Download,
  Globe,
  MessagesSquare,
  Shirt,
  Server as ServerIcon,
} from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "@/Components/ui/button";
import { Input } from "@/Components/ui/input";
import { cn } from "@/lib/utils";
import {
  enrollAccount,
  experimentalState,
  fleetState,
  listServers,
  onEnrollLink,
  parsePairing,
  presetsListProfiles,
  provisionServer,
  publishServer,
  saveServers,
  SERVER_REGIONS,
  serverAction,
  serverProbe,
  serverSetConfig,
  serverStatus,
  serverTracks,
  setGuid as setGuidApi,
  syncPaints,
  unpublishServer,
  type ExperimentalState,
  type FleetState,
  type ServerAction,
  type ServerRef,
  type ServerStatus,
} from "../../api/mods";
import { useT } from "../../i18n/context";

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
  const [region, setRegion] = useState<string>(SERVER_REGIONS[0]);

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
          <div className="mt-1 truncate text-[12px] text-muted-foreground">{server.url}</div>
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
            <dd>{running ? uptime(status.game.uptime_secs) : t("servers.stopped")}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">{t("servers.restarts")}</dt>
            <dd>{status.game.restarts}</dd>
          </div>
        </dl>
      )}

      <div className="mt-4 flex flex-wrap items-center gap-2">
        <Button size="sm" variant="outline" disabled={busy || running} onClick={() => run("start")}>
          <Play className="size-3.5" /> {t("servers.start")}
        </Button>
        <Button size="sm" variant="outline" disabled={busy || !running} onClick={() => run("stop")}>
          <Square className="size-3.5" /> {t("servers.stop")}
        </Button>
        <Button size="sm" variant="outline" disabled={busy} onClick={() => run("restart")}>
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <RotateCw className="size-3.5" />}{" "}
          {t("servers.restart")}
        </Button>

        <div className="ml-auto flex items-center gap-2">
          {tracks === null ? (
            <span className="text-[12px] text-muted-foreground">{t("servers.trackLoading")}</span>
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
          <Button size="sm" disabled={busy || !track.trim()} onClick={applyTrack}>
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
            <span className="text-[12px] text-muted-foreground">{t("servers.listed")}</span>
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
            <span className="text-[12px] text-muted-foreground">{t("servers.notListed")}</span>
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

/**
 * Enrollment and paint sync.
 *
 * MX Bikes sends no custom content, so other riders render in default liveries unless you
 * already hold their exact paint file. This is the panel that fixes that: publish what
 * you're wearing, pull back what everyone else published.
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
        if (scan.profiles.length > 0) setRiderName((cur) => cur || scan.profiles[0]);
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

  const pull = async () => {
    setBusy(true);
    try {
      const r = await syncPaints();
      toast.success(
        t("sync.pulled", { installed: r.installed, riders: r.riders, had: r.alreadyHad }),
      );
      if (r.rejected > 0) toast.warning(t("sync.rejected", { count: r.rejected }));
    } catch (e) {
      toast.error(t("sync.pullFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  return (
    <div className="mb-5 rounded-xl border border-white/[0.07] p-4">
      <div className="flex items-center gap-2">
        <Shirt className="size-4 text-muted-foreground" />
        <h2 className="font-semibold">{t("sync.title")}</h2>
      </div>
      <p className="mt-1 text-[12.5px] text-muted-foreground">{t("sync.desc")}</p>

      {state?.enrolled ? (
        <>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <span className="text-[12.5px] text-muted-foreground">
              {t("sync.ridingAs", { name: state.riderName })}
            </span>
            <Button className="ml-auto" size="sm" disabled={busy} onClick={() => void pull()}>
              {busy ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Download className="size-3.5" />
              )}{" "}
              {t("sync.pull")}
            </Button>
          </div>

          <p className="mt-2 text-[11.5px] text-muted-foreground">{t("sync.autoNote")}</p>

          {/* The GUID is the identity that survives a name change. A player can't read it
              off their own machine, so this is no longer something to type: the app takes
              it from the server log the first time one of their servers sees them connect.
              The manual field stays for anyone who doesn't run a server. */}
          {state.guid ? (
            <p className="mt-3 text-[11.5px] text-muted-foreground">
              {t("sync.guidClaimed", { guid: state.guid })}
            </p>
          ) : manualGuid ? (
            <div className="mt-3 flex flex-wrap items-end gap-2">
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
            <p className="mt-3 text-[11.5px] text-muted-foreground">
              {t("sync.guidPending")}{" "}
              <button
                onClick={() => setManualGuid(true)}
                className="cursor-default underline underline-offset-2 hover:text-foreground"
              >
                {t("sync.guidManual")}
              </button>
            </p>
          )}
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
            {profiles && profiles.length === 0 ? t("sync.noProfiles") : t("sync.pickProfileHint")}
          </p>
          {/* Where the code comes from. Invites are issued by hand, so without this the
              field is a box with no answer anywhere in the app. */}
          <p className="text-[11.5px] text-muted-foreground">{t("sync.whereCode")}</p>
          <div className="flex flex-wrap items-center gap-2 pt-1">
            <Button
              size="sm"
              disabled={busy || !code.trim() || !riderName.trim()}
              onClick={() => void enroll()}
            >
              {t("sync.enroll")}
            </Button>
            <Button size="sm" variant="outline" onClick={() => void openUrl(DISCORD_URL)}>
              <MessagesSquare className="size-3.5" /> {t("sync.getCode")}
            </Button>
          </div>
        </div>
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
 * The running count is shown next to the button, and it is read from EC2 rather than from
 * our own records: that is the number being billed, and the two disagree exactly when
 * something has already gone wrong. A player deciding whether to start another server
 * should be looking at the real one.
 */
const CreateServer = ({ onCreated }: { onCreated: () => void }) => {
  const t = useT();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [fleet, setFleet] = useState<FleetState | null>(null);
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
  }, []);
  useEffect(refresh, [refresh]);

  const create = async () => {
    setBusy(true);
    try {
      await provisionServer(name.trim());
      toast.success(t("servers.creating"));
      setName("");
      refresh();
      onCreated();
    } catch (e) {
      toast.error(t("servers.createFailed"), { description: String(e) });
    }
    setBusy(false);
  };

  return (
    <div className="mb-5 rounded-xl border border-white/[0.07] p-4">
      <div className="flex items-center gap-2">
        <Cloud className="size-4 text-muted-foreground" />
        <h2 className="font-semibold">{t("servers.createTitle")}</h2>
        {fleet && (
          <span className="ml-auto text-[12px] text-muted-foreground">
            {t("servers.runningCount", { count: fleet.instances.length })}
          </span>
        )}
      </div>
      <p className="mt-1 text-[12.5px] text-muted-foreground">{t("servers.createDesc")}</p>

      {unavailable ? (
        <p className="mt-3 text-[11.5px] text-muted-foreground">{unavailable}</p>
      ) : (
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("servers.namePlaceholder")}
            className="h-9 flex-1"
          />
          <Button size="sm" disabled={busy || name.trim().length < 2} onClick={() => void create()}>
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Plus className="size-3.5" />}
            {t("servers.create")}
          </Button>
        </div>
      )}

      {fleet && fleet.instances.length > 0 && (
        <dl className="mt-3 space-y-1">
          {fleet.instances.map((i) => (
            <div key={i.instanceId} className="flex items-center gap-2 text-[12px]">
              <span
                className={cn(
                  "size-[7px] flex-none rounded-full",
                  i.state === "running" ? "bg-success" : "bg-muted-foreground/50",
                )}
              />
              <span className="text-muted-foreground">{i.instanceId}</span>
              <span>{i.publicIp ?? "—"}</span>
              <span className="ml-auto text-muted-foreground">{i.state}</span>
            </div>
          ))}
        </dl>
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
    void listServers().then(setServers).catch(() => {});
  }, []);
  useEffect(reload, [reload]);

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
    <div className="mx-auto w-full max-w-4xl p-6">
      <header className="mb-5">
        <h1 className="text-xl font-semibold">{t("servers.title")}</h1>
        <p className="mt-1 text-[13px] text-muted-foreground">{t("servers.subtitle")}</p>
      </header>

      <PaintSync />

      <CreateServer onCreated={reload} />

      {servers.length === 0 && !adding && (
        <div className="rounded-xl border border-dashed border-white/[0.1] p-8 text-center">
          <ServerIcon className="mx-auto size-6 text-muted-foreground" />
          <p className="mt-2 text-[13px] text-muted-foreground">{t("servers.empty")}</p>
        </div>
      )}

      <div className="space-y-3">
        {servers.map((s) => (
          <ServerRow
            key={s.id}
            server={s}
            onRemove={(id) => void persist(servers.filter((x) => x.id !== id))}
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
          <p className="text-[11.5px] text-muted-foreground">{t("servers.pairingWhere")}</p>

          {manualServer ? (
            <>
              <Input
                value={draft.url}
                onChange={(e) => setDraft({ ...draft, url: e.target.value })}
                placeholder="http://203.0.113.10:8787"
                spellCheck={false}
              />
              <Input
                type="password"
                value={draft.token}
                onChange={(e) => setDraft({ ...draft, token: e.target.value })}
                placeholder={t("servers.tokenPlaceholder")}
                spellCheck={false}
              />
              <Input
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
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
            <Button size="sm" variant="outline" disabled={probing} onClick={() => setAdding(false)}>
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
        <Button className="mt-4" size="sm" variant="outline" onClick={() => setAdding(true)}>
          <Plus className="size-3.5" /> {t("servers.add")}
        </Button>
      )}
    </div>
  );
};

export default Servers;
