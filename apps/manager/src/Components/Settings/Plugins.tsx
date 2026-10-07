import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  CheckCircle2,
  Download,
  ExternalLink,
  Loader2,
  Lock,
  RefreshCw,
  Trash2,
  WifiOff,
} from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { cn } from "@frost/shared/lib/utils";
import {
  installPlugin,
  listPlugins,
  removePlugin,
  type PluginView,
} from "@frost/shared/api/plugins";
import { mountPlugin, unmountPlugin } from "@frost/shared/lib/pluginHost";
import { launchStudio } from "@frost/shared/api/mods";
import { useT, type TFunc, type TKey } from "@/i18n";

/**
 * What this row is, in one sentence, plus the one button that changes it.
 *
 * Every plugin is free. One that isn't working has to say which reason it is: no license yet
 * (sign in), not installed, the license needs re-checking, or working. Each sends the person
 * somewhere different.
 */
function describe(t: TFunc<TKey>, p: PluginView): { tone: Tone; title: string; detail: string } {
  // No license yet only needs the account; there are no keys.
  if (p.status === "expired") {
    return { tone: "locked", title: t("plugins.free"), detail: t("plugins.freeSignIn") };
  }
  if (p.status === "stale") {
    return {
      tone: "stale",
      title: t("plugins.needsCheck"),
      detail: t("plugins.needsCheckDetail"),
    };
  }
  const have = t("plugins.free");
  if (!p.published) {
    return { tone: "stale", title: have, detail: t("plugins.noBuildYet") };
  }
  if (!p.installedVersion) {
    return {
      tone: "ready",
      title: have,
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
    detail: t("plugins.activeFreeDetail"),
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
  onOpenStudio,
}: {
  plugin: PluginView;
  busy: boolean;
  onInstall: () => void;
  onRemove: () => void;
  onOpenStudio: () => void;
}) => {
  const t = useT();
  const { tone, title, detail } = describe(t, plugin);
  const Icon = TONE_ICON[tone];
  const canInstall = plugin.published && (plugin.status === "live") && !plugin.ready;
  // A working plugin whose panels are in the other window. Saying where it went is the
  // whole job here: somebody who has just installed MXB Replay and finds no new row in
  // this app has been told the install worked and shown nothing to prove it.
  const inStudio = plugin.ready && plugin.host === "studio";

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
        {inStudio && (
          <p className="mt-1 text-sm text-muted-foreground">{t("plugins.inStudio")}</p>
        )}
      </div>
      <div className="flex shrink-0 flex-wrap gap-2">
        {inStudio && (
          <Button size="sm" variant="outline" onClick={onOpenStudio} disabled={busy}>
            <ExternalLink className="size-3.5" />
            {t("plugins.openStudio")}
          </Button>
        )}
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
 * Everything here works offline except installing: the license is a signed statement
 * the app already holds, so a list that showed nothing without a network would be lying
 * about a plugin that is, right now, running.
 */
const Plugins = () => {
  const t = useT();
  const [plugins, setPlugins] = useState<PluginView[] | null>(null);
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
    // Once, on open. Installing and removing refresh themselves.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const install = async (p: PluginView) => {
    setBusyId(p.id);
    try {
      const name = await installPlugin(p.id);
      // Where it went is only knowable after the install: the manifest that says so arrives
      // inside the bundle. So the list is re-read here rather than after the toast.
      const rows = await listPlugins().catch(() => [] as PluginView[]);
      if (rows.length) setPlugins(rows);
      const row = rows.find((r) => r.id === p.id);
      const studio = (row?.host ?? "studio") === "studio";
      // Mount straight away, when this is the window it belongs in: an install that needs a
      // restart to show up reads as an install that did not work.
      if (!studio) {
        try {
          await mountPlugin(p.id);
        } catch (e) {
          toast.error(String(e));
        }
      }
      toast.success(
        studio ? t("plugins.installedStudio", { name }) : t("plugins.installed", { name }),
      );
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyId(null);
    }
  };

  // Hand over to the window the panels are actually in, naming the plugin so it opens on one
  // of its own rather than on whatever the Studio shows first. A studio that isn't installed
  // says so in the toast.
  const openStudio = async (p: PluginView) => {
    try {
      await launchStudio(p.id);
    } catch (e) {
      toast.error(String(e));
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
            onOpenStudio={() => void openStudio(p)}
          />
        ))}
      </div>
    </div>
  );
};

export default Plugins;
