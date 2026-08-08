import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  Loader2,
  Play,
  Square,
  RotateCw,
  Trash2,
  Plus,
  Download,
  Shirt,
  Server as ServerIcon,
} from "lucide-react";
import { Button } from "@/Components/ui/button";
import { Input } from "@/Components/ui/input";
import { cn } from "@/lib/utils";
import {
  enrollAccount,
  experimentalState,
  listServers,
  saveServers,
  serverAction,
  serverSetConfig,
  serverStatus,
  syncPaints,
  type ExperimentalState,
  type ServerAction,
  type ServerRef,
  type ServerStatus,
} from "../../api/mods";
import { useT } from "../../i18n/context";

/** How often a server's status refreshes while the page is open. */
const POLL_MS = 10000;

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
}

const ServerRow = ({ server, onRemove }: RowProps) => {
  const t = useT();
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [track, setTrack] = useState("");

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
          <Input
            value={track}
            onChange={(e) => setTrack(e.target.value)}
            placeholder={t("servers.trackPlaceholder")}
            spellCheck={false}
            className="h-8 w-40 text-[12.5px]"
          />
          {/* Changing the track restarts the game — the .ini is only read at startup. */}
          <Button size="sm" disabled={busy || !track.trim()} onClick={applyTrack}>
            {t("servers.setTrack")}
          </Button>
        </div>
      </div>
    </div>
  );
};

/**
 * Enrolment and paint sync.
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
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    experimentalState()
      .then(setState)
      .catch(() => {});
  }, []);
  useEffect(refresh, [refresh]);

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

  const pull = async () => {
    setBusy(true);
    try {
      const r = await syncPaints("eu-frankfurt-1");
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
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <span className="text-[12.5px] text-muted-foreground">
            {t("sync.ridingAs", { name: state.riderName })}
          </span>
          <Button className="ml-auto" size="sm" disabled={busy} onClick={() => void pull()}>
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Download className="size-3.5" />}{" "}
            {t("sync.pull")}
          </Button>
        </div>
      ) : (
        <div className="mt-4 space-y-2">
          <Input
            value={riderName}
            onChange={(e) => setRiderName(e.target.value)}
            placeholder={t("sync.riderNamePlaceholder")}
            spellCheck={false}
          />
          <Input
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder={t("sync.codePlaceholder")}
            spellCheck={false}
          />
          <p className="text-[11.5px] text-muted-foreground">{t("sync.riderNameHint")}</p>
          <Button
            size="sm"
            disabled={busy || !code.trim() || !riderName.trim()}
            onClick={() => void enroll()}
          >
            {t("sync.enroll")}
          </Button>
        </div>
      )}
    </div>
  );
};

const Servers = () => {
  const t = useT();
  const [servers, setServers] = useState<ServerRef[]>([]);
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState({ name: "", url: "", token: "" });

  useEffect(() => {
    void listServers().then(setServers).catch(() => {});
  }, []);

  const persist = async (next: ServerRef[]) => {
    setServers(next);
    try {
      await saveServers(next);
    } catch (e) {
      toast.error(t("servers.saveFailed"), { description: String(e) });
    }
  };

  const add = async () => {
    const { name, url, token } = draft;
    if (!name.trim() || !url.trim() || !token.trim()) return;
    // crypto.randomUUID keeps ids unique without a counter that a reordered or partially
    // removed list could collide with.
    const entry: ServerRef = {
      id: crypto.randomUUID(),
      name: name.trim(),
      url: url.trim(),
      token: token.trim(),
    };
    await persist([...servers, entry]);
    setDraft({ name: "", url: "", token: "" });
    setAdding(false);
  };

  return (
    <div className="mx-auto w-full max-w-4xl p-6">
      <header className="mb-5">
        <h1 className="text-xl font-semibold">{t("servers.title")}</h1>
        <p className="mt-1 text-[13px] text-muted-foreground">{t("servers.subtitle")}</p>
      </header>

      <PaintSync />

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
          />
        ))}
      </div>

      {adding ? (
        <div className="mt-4 space-y-2 rounded-xl border border-white/[0.07] p-4">
          <Input
            autoFocus
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            placeholder={t("servers.namePlaceholder")}
          />
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
          <div className="flex justify-end gap-2 pt-1">
            <Button size="sm" variant="outline" onClick={() => setAdding(false)}>
              {t("common.cancel")}
            </Button>
            <Button size="sm" onClick={() => void add()}>
              {t("servers.add")}
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
