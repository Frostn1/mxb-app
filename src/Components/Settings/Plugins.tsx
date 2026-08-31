import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { CheckCircle2, Download, Loader2, Lock, RefreshCw, Trash2, WifiOff } from "lucide-react";
import { Button } from "@/Components/ui/button";
import { Input } from "@/Components/ui/input";
import { cn } from "@/lib/utils";
import {
  installPlugin,
  listPlugins,
  redeemPluginKey,
  removePlugin,
  type PluginView,
} from "@/api/plugins";
import { mountPlugin, unmountPlugin } from "@/lib/pluginHost";
import { useT, type TFunc } from "../../i18n/context";

/** `1756598400` -> `30 September`. Whole days: nobody renews to the minute. */
function until(at: number | null): string | null {
  if (!at) return null;
  return new Date(at * 1000).toLocaleDateString(undefined, {
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

/**
 * What this row is, in one sentence, plus the one button that changes it.
 *
 * A paid thing that isn't working has to say which of the four reasons it is: not bought,
 * bought but not installed, installed but the licence needs re-checking, or working. Every
 * one of those sends the person somewhere different, and a single "unavailable" state would
 * send them all to the same place — support.
 */
function describe(t: TFunc, p: PluginView): { tone: Tone; title: string; detail: string } {
  if (p.status === "expired") {
    return p.expires
      ? {
          tone: "locked",
          title: t("plugins.lapsed"),
          detail: t("plugins.lapsedDetail", { date: until(p.expires) ?? "" }),
        }
      : { tone: "locked", title: t("plugins.notLicensed"), detail: t("plugins.notLicensedDetail") };
  }
  if (p.status === "stale") {
    return {
      tone: "stale",
      title: t("plugins.needsCheck"),
      detail: t("plugins.needsCheckDetail"),
    };
  }
  if (!p.published) {
    return { tone: "stale", title: t("plugins.licensed"), detail: t("plugins.noBuildYet") };
  }
  if (!p.installedVersion) {
    return {
      tone: "ready",
      title: t("plugins.licensed"),
      detail: t("plugins.readyToInstall", { version: p.version ?? "" }),
    };
  }
  if (!p.ready) {
    return {
      tone: "ready",
      title: t("plugins.updateAvailable"),
      detail: t("plugins.updateDetail", {
        installed: p.installedVersion,
        latest: p.version ?? "",
      }),
    };
  }
  return {
    tone: "good",
    title: t("plugins.active"),
    detail: t("plugins.activeDetail", { date: until(p.expires) ?? "" }),
  };
}

type Tone = "good" | "ready" | "stale" | "locked";

const TONE_ICON = {
  good: CheckCircle2,
  ready: Download,
  stale: WifiOff,
  locked: Lock,
} as const;

const TONE_CLASS = {
  good: "text-emerald-500",
  ready: "text-sky-500",
  stale: "text-amber-500",
  locked: "text-muted-foreground",
} as const;

const PluginRow = ({
  plugin,
  busy,
  onInstall,
  onRemove,
}: {
  plugin: PluginView;
  busy: boolean;
  onInstall: () => void;
  onRemove: () => void;
}) => {
  const t = useT();
  const { tone, title, detail } = describe(t, plugin);
  const Icon = TONE_ICON[tone];
  const canInstall = plugin.published && (plugin.status === "live") && !plugin.ready;

  return (
    <div className="flex items-start gap-3 rounded-lg border p-3">
      <Icon className={cn("mt-0.5 size-4 shrink-0", TONE_CLASS[tone])} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="font-medium">{plugin.name}</span>
          {plugin.installedVersion && (
            <span className="text-xs text-muted-foreground">v{plugin.installedVersion}</span>
          )}
        </div>
        {plugin.summary && (
          <p className="mt-0.5 text-sm text-muted-foreground">{plugin.summary}</p>
        )}
        <p className="mt-1.5 text-sm">
          <span className={cn("font-medium", TONE_CLASS[tone])}>{title}</span>
          <span className="text-muted-foreground"> — {detail}</span>
        </p>
      </div>
      <div className="flex shrink-0 gap-2">
        {canInstall && (
          <Button size="sm" onClick={onInstall} disabled={busy}>
            {busy && <Loader2 className="size-3.5 animate-spin" />}
            {plugin.installedVersion ? t("plugins.update") : t("plugins.install")}
          </Button>
        )}
        {plugin.installedVersion && (
          <Button size="sm" variant="ghost" onClick={onRemove} disabled={busy}>
            <Trash2 className="size-3.5" />
          </Button>
        )}
      </div>
    </div>
  );
};

/**
 * The Plugins page.
 *
 * Everything here works offline except redeeming a key: the licence is a signed statement
 * the app already holds, so a list that showed nothing without a network would be lying
 * about a plugin that is, right now, running.
 */
const Plugins = () => {
  const t = useT();
  const [plugins, setPlugins] = useState<PluginView[] | null>(null);
  const [code, setCode] = useState("");
  const [redeeming, setRedeeming] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setPlugins(await listPlugins());
    } catch (e) {
      // A failure here is the control plane being unreachable, which is not a licensing
      // failure — leave whatever is on screen rather than emptying the list.
      if (plugins === null) setPlugins([]);
      console.warn("plugin list failed", e);
    }
  }, [plugins]);

  useEffect(() => {
    void refresh();
    // Once, on open. Redeeming and installing refresh themselves.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const redeem = async () => {
    const trimmed = code.trim();
    if (!trimmed) return;
    setRedeeming(true);
    try {
      const name = await redeemPluginKey(trimmed);
      setCode("");
      toast.success(t("plugins.redeemed", { name }));
      await refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRedeeming(false);
    }
  };

  const install = async (p: PluginView) => {
    setBusyId(p.id);
    try {
      const name = await installPlugin(p.id);
      // Mount straight away: an install that needs a restart to show up reads as an install
      // that did not work.
      try {
        await mountPlugin(p.id);
      } catch (e) {
        toast.error(String(e));
      }
      toast.success(t("plugins.installed", { name }));
      await refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyId(null);
    }
  };

  const remove = async (p: PluginView) => {
    setBusyId(p.id);
    try {
      unmountPlugin(p.id);
      await removePlugin(p.id);
      await refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold">{t("plugins.section")}</h2>
        <p className="text-sm text-muted-foreground">{t("plugins.sectionDesc")}</p>
      </div>

      <div className="space-y-2">
        <label className="text-sm font-medium" htmlFor="plugin-key">
          {t("plugins.keyLabel")}
        </label>
        <div className="flex gap-2">
          <Input
            id="plugin-key"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void redeem();
            }}
            placeholder="FRST-XXXX-XXXX-XXXX"
            autoComplete="off"
            spellCheck={false}
            className="font-mono"
          />
          <Button onClick={() => void redeem()} disabled={redeeming || !code.trim()}>
            {redeeming && <Loader2 className="size-3.5 animate-spin" />}
            {t("plugins.redeem")}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">{t("plugins.keyHelp")}</p>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium">{t("plugins.available")}</span>
          <Button size="sm" variant="ghost" onClick={() => void refresh()}>
            <RefreshCw className="size-3.5" />
            {t("plugins.refresh")}
          </Button>
        </div>

        {plugins === null && (
          <div className="flex items-center gap-2 p-3 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
            {t("plugins.loading")}
          </div>
        )}
        {plugins?.length === 0 && (
          <p className="p-3 text-sm text-muted-foreground">{t("plugins.none")}</p>
        )}
        {plugins?.map((p) => (
          <PluginRow
            key={p.id}
            plugin={p}
            busy={busyId === p.id}
            onInstall={() => void install(p)}
            onRemove={() => void remove(p)}
          />
        ))}
      </div>
    </div>
  );
};

export default Plugins;
