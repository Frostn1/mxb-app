import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Download, ExternalLink, FolderOpen, Loader2, RefreshCw, Search, Star, X } from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Switch } from "@frost/shared/Components/ui/switch";
import CachedImg from "@frost/shared/Components/ui/cached-img";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@frost/shared/Components/ui/alert-dialog";
import {
  gamePluginDetail,
  gamePluginInstall,
  gamePluginOpenConfig,
  gamePluginReadConfig,
  gamePluginReveal,
  gamePluginSetEnabled,
  gamePluginSetRaceOff,
  gamePluginUninstall,
  gamePluginWriteConfig,
  gamePluginsDisableAll,
  gamePluginsLatest,
  gamePluginsList,
  gamePluginsRestore,
  hasUpdate,
  type GamePlugin,
  type GamePluginCatalogEntry,
  type GamePluginConflict,
  type GamePluginDetail,
  type GamePluginLatest,
  type GamePluginsView,
} from "@frost/shared/api/gamePlugins";
import { formatDate } from "@frost/shared/lib/mods";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import { useFrostmod } from "@/Context/FrostmodContext";
import { useViewActive } from "../Shell/RetainedView";
import RichDescription from "../ModDetail/RichDescription";
import type { SectionId } from "../Settings/Settings";

const CATEGORIES = ["", "hud", "graphics", "physics", "fixes", "timing"] as const;
type Category = (typeof CATEGORIES)[number];
const CATEGORY_LABEL: Record<Category, TKey> = {
  "": "gamePlugins.catAll",
  hud: "gamePlugins.catHud",
  graphics: "gamePlugins.catGraphics",
  physics: "gamePlugins.catPhysics",
  fixes: "gamePlugins.catFixes",
  timing: "gamePlugins.catTiming",
};
/** A category's name on a card or page; "other" has no filter chip of its own. */
function categoryLabel(category: string): TKey {
  return category === "other" ? "gamePlugins.catOther" : (CATEGORY_LABEL[category as Category] ?? "gamePlugins.catOther");
}
const SORTS = ["popular", "new", "name"] as const;
type Sort = (typeof SORTS)[number];
const SORT_LABEL: Record<Sort, TKey> = {
  popular: "gamePlugins.sortPopular",
  new: "gamePlugins.sortNew",
  name: "gamePlugins.sortName",
};

/** Notices the player closed, by a key that changes when the notice's subject does. */
/** The guide for plugin authors, in the public repo like the extensions guide. */
const AUTHOR_DOCS = "https://github.com/Frostn1/mxb-app/blob/main/docs/plugins.md";

const DISMISSED_KEY = "mxb.gamePlugins.dismissedNotices";
function readDismissed(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(DISMISSED_KEY) ?? "[]") as string[]);
  } catch {
    return new Set();
  }
}
function writeDismissed(keys: Set<string>) {
  try {
    localStorage.setItem(DISMISSED_KEY, JSON.stringify([...keys]));
  } catch {
    // Private storage off: the notice simply comes back next time.
  }
}
const noticeKey = (c: GamePluginConflict) => `${c.level}:${[...c.ids].sort().join("+")}`;

function message(error: unknown) {
  return String(error).replace(/^Error:\s*/, "");
}

/**
 * The Plugins tab: what's installed at the top in a few compact rows, Browse underneath, and
 * one page per plugin. Laid out like the extensions Marketplace so the two read as one app.
 */
export default function GamePlugins({ onOpenSettings }: { onOpenSettings?: (section: SectionId) => void }) {
  const t = useT();
  const active = useViewActive();
  const { integrationChoice, enableIntegration, useAppOnly: chooseAppOnly, status, installing } = useFrostmod();
  const [data, setData] = useState<GamePluginsView | null>(null);
  const [latest, setLatest] = useState<Record<string, GamePluginLatest>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [category, setCategory] = useState<Category>("");
  const [sort, setSort] = useState<Sort>("popular");
  const [dismissed, setDismissed] = useState(readDismissed);
  const [integrationBusy, setIntegrationBusy] = useState(false);
  const pageRef = useRef<HTMLDivElement>(null);

  // A plugin's page opens at its top, not wherever the list was scrolled to. The scrolling
  // box may be this view or one the shell wraps it in, so every one up the tree is reset.
  useEffect(() => {
    if (!open) return;
    for (let el: HTMLElement | null = pageRef.current; el; el = el.parentElement) el.scrollTop = 0;
  }, [open]);

  const load = useCallback(async () => {
    try {
      const view = await gamePluginsList();
      setData(view);
      // Said once, when it happens: the app moved a plugin aside on its own.
      for (const name of view.autoTurnedOff) toast.info(t("gamePlugins.coveredTurnedOff", { name }));
    } catch (error) {
      toast.error(message(error));
    }
  }, [t]);

  // Plugins change behind the app's back (an author's installer, a hand copy), so the folder
  // is read again each time the tab is shown. Latest versions are cached server side.
  useEffect(() => {
    if (!active) return;
    void load();
    void gamePluginsLatest()
      .then((rows) => setLatest(Object.fromEntries(rows.map((row) => [row.id, row]))))
      .catch(() => {});
  }, [active, load]);

  const run = useCallback(
    async (key: string, action: () => Promise<unknown>, done?: string) => {
      setBusy(key);
      try {
        await action();
        if (done) toast.success(done);
      } catch (error) {
        toast.error(message(error));
      } finally {
        await load();
        setBusy(null);
      }
    },
    [load],
  );

  const installed = useMemo(() => data?.installed.filter((p) => p.kind === "thirdParty") ?? [], [data]);
  const ours = useMemo(() => data?.installed.filter((p) => p.kind === "mxbApp" && p.id !== "frostmod") ?? [], [data]);
  const notices = (data?.conflicts ?? []).filter((c) => !dismissed.has(noticeKey(c)));
  const locked = busy !== null || data?.gameRunning === true;

  const browse = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const rows = (data?.catalog ?? []).filter(
      (c) =>
        (!category || c.category === category) &&
        (!needle || `${c.name} ${c.author} ${c.summary}`.toLowerCase().includes(needle)),
    );
    const views = (c: GamePluginCatalogEntry) => latest[c.id]?.views ?? 0;
    const updated = (c: GamePluginCatalogEntry) => Date.parse(latest[c.id]?.updated ?? "") || 0;
    return [...rows].sort((a, b) =>
      sort === "popular" ? views(b) - views(a) : sort === "new" ? updated(b) - updated(a) : a.name.localeCompare(b.name),
    );
  }, [data, q, category, sort, latest]);

  const dismiss = (c: GamePluginConflict) => {
    const next = new Set(dismissed);
    next.add(noticeKey(c));
    setDismissed(next);
    writeDismissed(next);
  };

  const setIntegration = async (enabled: boolean) => {
    setIntegrationBusy(true);
    try {
      if (enabled) await enableIntegration();
      else await chooseAppOnly();
    } finally {
      setIntegrationBusy(false);
    }
  };

  const actions = {
    toggle: (p: GamePlugin, on: boolean) => void run(p.id, () => gamePluginSetEnabled(p.id, on)),
    raceOff: (p: GamePlugin, off: boolean) => void run(p.id, () => gamePluginSetRaceOff(p.id, off)),
    install: (id: string, name: string, update: boolean) =>
      void run(
        id,
        () => gamePluginInstall(id),
        update ? t("gamePlugins.updated", { name }) : t("gamePlugins.installedToast", { name }),
      ),
    uninstall: (p: GamePlugin) => void run(p.id, () => gamePluginUninstall(p.id)),
  };

  if (open && data) {
    return (
      <div ref={pageRef} className="h-full overflow-y-auto px-6 pb-10">
        <PluginPage
          id={open}
          view={data}
          plugin={data.installed.find((p) => p.id === open)}
          entry={data.catalog.find((c) => c.id === open)}
          latest={latest[open]}
          locked={locked}
          working={busy === open}
          onBack={() => setOpen(null)}
          actions={actions}
        />
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto px-6 pb-10">
      <div className="mx-auto max-w-[1200px] space-y-4 pt-2">
        {data?.gameRunning && <p className="text-[12.5px] text-warning">{t("gamePlugins.closeGame")}</p>}

        {notices.map((c) => (
          <div
            key={noticeKey(c)}
            className={cn(
              "flex items-center gap-3 rounded-md border px-3 py-2 text-[12.5px]",
              c.level === "error"
                ? "border-destructive/40 text-destructive"
                : c.level === "warning"
                  ? "border-warning/40 text-warning"
                  : "border-border text-muted-foreground",
            )}
          >
            <span className="min-w-0 flex-1">{c.message}</span>
            <button
              type="button"
              className="shrink-0 cursor-default text-muted-foreground hover:text-foreground"
              aria-label={t("gamePlugins.dismiss")}
              onClick={() => dismiss(c)}
            >
              <X className="size-3.5" />
            </button>
          </div>
        ))}

        <section>
          <div className="mb-1 flex items-center gap-3">
            <h2 className="text-[11.5px] font-semibold uppercase tracking-wide text-muted-foreground">
              {t("gamePlugins.installed")} {installed.length > 0 && installed.length}
            </h2>
            {data && <RacingControl data={data} locked={locked} working={busy === "racing"} run={run} />}
            <span className="ml-auto text-[11.5px] text-muted-foreground" title={t("gamePlugins.gameBuild")}>
              {data?.gameBuild?.replace(/^beta/i, "Beta ")}
            </span>
            <Button variant="ghost" size="sm" disabled={busy !== null} onClick={() => void load()}>
              <RefreshCw className="size-3.5" />
              {t("common.refresh")}
            </Button>
          </div>
          <div className="divide-y divide-border border-y border-border">
            <div className="flex items-center gap-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-semibold">{t("gamePlugins.frostmodTitle")}</div>
                <ul className="mt-1 space-y-0.5 text-[11.5px] text-muted-foreground">
                  <li>{t("gamePlugins.frostmodPaintSync")}</li>
                  <li>{t("gamePlugins.frostmodRefresh")}</li>
                  <li>{t("gamePlugins.frostmodCrashFixes")}</li>
                  <li>{t("gamePlugins.frostmodAussieMode")}</li>
                </ul>
              </div>
              <Button variant="ghost" size="sm" className="h-7" onClick={() => onOpenSettings?.("frostmod")}>
                {t("gamePlugins.settings")}
              </Button>
              {(integrationBusy || installing || (integrationChoice === "enabled" && status === null)) && (
                <Loader2 className="size-3.5 animate-spin text-muted-foreground" />
              )}
              <Switch
                checked={integrationChoice === "enabled"}
                disabled={integrationBusy || installing || (integrationChoice === "enabled" && status === null)}
                onCheckedChange={(enabled) => void setIntegration(enabled)}
                aria-label={t("gamePlugins.frostmodTitle")}
              />
            </div>
            {installed.map((p) => (
              <InstalledRow
                key={p.id}
                plugin={p}
                latest={latest[p.id]}
                locked={locked}
                working={busy === p.id}
                onOpen={() => setOpen(p.id)}
                actions={actions}
              />
            ))}
            {ours.map((p) => (
              <div key={p.id} className="flex items-center gap-3 py-2 text-[12.5px]">
                <span className="w-5" />
                <span className="font-semibold">{p.name}</span>
                <span className="text-muted-foreground">{p.enabled ? t("gamePlugins.on") : t("gamePlugins.off")}</span>
                <span className="ml-auto text-[11.5px] text-muted-foreground">{p.note}</span>
              </div>
            ))}
          </div>
          {data && installed.length === 0 && (
            <p className="pt-3 text-[12.5px] text-muted-foreground">{t("gamePlugins.noneInstalled")}</p>
          )}
        </section>

        <section className="space-y-3 pt-4">
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="text-[11.5px] font-semibold uppercase tracking-wide text-muted-foreground">
              {t("gamePlugins.browse")}
            </h2>
            <Button variant="ghost" size="sm" className="h-7" onClick={() => void openUrl(AUTHOR_DOCS)}>
              <ExternalLink className="size-3.5" />
              {t("gamePlugins.forAuthors")}
            </Button>
            <div className="relative ml-auto w-full max-w-[360px]">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="h-8 pl-8"
                placeholder={t("gamePlugins.searchPlaceholder")}
                value={q}
                onChange={(e) => setQ(e.target.value)}
              />
            </div>
            <select
              className="h-8 rounded-md border border-input bg-transparent px-2 text-[12.5px]"
              value={sort}
              onChange={(e) => setSort(e.target.value as Sort)}
              aria-label={t("gamePlugins.sortBy")}
            >
              {SORTS.map((x) => (
                <option key={x} value={x}>
                  {t(SORT_LABEL[x])}
                </option>
              ))}
            </select>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {CATEGORIES.map((c) => (
              <button
                key={c || "all"}
                type="button"
                onClick={() => setCategory(c)}
                className={cn(
                  "cursor-default rounded-full border px-3 py-1 text-[12px]",
                  category === c ? "border-primary text-foreground" : "border-border text-muted-foreground hover:text-foreground",
                )}
              >
                {t(CATEGORY_LABEL[c])}
              </button>
            ))}
          </div>
          {data && browse.length === 0 && (
            <p className="py-8 text-center text-[12.5px] text-muted-foreground">{t("gamePlugins.noResults")}</p>
          )}
          <div className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-3">
            {browse.map((c) => (
              <Card
                key={c.id}
                c={c}
                plugin={data?.installed.find((p) => p.id === c.id)}
                latest={latest[c.id]}
                locked={locked}
                working={busy === c.id}
                onOpen={() => setOpen(c.id)}
                onInstall={(update) => actions.install(c.id, c.name, update)}
              />
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}

type Actions = {
  toggle: (p: GamePlugin, on: boolean) => void;
  raceOff: (p: GamePlugin, off: boolean) => void;
  install: (id: string, name: string, update: boolean) => void;
  uninstall: (p: GamePlugin) => void;
};

/**
 * Racing mode as one compact control beside "Installed": the button, and on hover what it
 * turns off. When it's on, the same place says what is off and turns it all back on.
 */
function RacingControl({
  data,
  locked,
  working,
  run,
}: {
  data: GamePluginsView;
  locked: boolean;
  working: boolean;
  run: (key: string, action: () => Promise<unknown>, done?: string) => Promise<void>;
}) {
  const t = useT();
  const on = data.racingOff.length > 0;
  const names = (on ? data.racingOff : data.racingPlanned).join(", ");
  const detail = on
    ? t("gamePlugins.racingIsOn", { names })
    : data.racingPlanned.length > 0
      ? t("gamePlugins.racingPlanned", { names })
      : t("gamePlugins.racingNothing");
  const auto = on && data.racingByJoin ? t("gamePlugins.racingByJoin") : data.raceMode ? t("gamePlugins.racingAuto") : t("gamePlugins.racingAutoOff");
  return (
    <div className="flex min-w-0 items-center gap-2 text-[12px]" title={`${detail}\n${auto}`}>
      <Button
        size="sm"
        variant={on ? "default" : "outline"}
        className="h-7"
        disabled={locked || (!on && data.racingPlanned.length === 0)}
        onClick={() =>
          void (on
            ? run("racing", gamePluginsRestore, t("gamePlugins.racingTurnedOff"))
            : run("racing", gamePluginsDisableAll, t("gamePlugins.racingTurnedOn")))
        }
      >
        {working && <Loader2 className="size-3.5 animate-spin" />}
        {on ? t("gamePlugins.racingTurnBackOn") : t("gamePlugins.racingTurnOn")}
      </Button>
      <span className={cn("truncate", on ? "text-primary" : "text-muted-foreground")}>{detail}</span>
    </div>
  );
}

/** The row's racing tag: what racing mode does with this plugin, changed with a click. */
function RacingTag({ plugin, disabled, onChange }: { plugin: GamePlugin; disabled: boolean; onChange: (off: boolean) => void }) {
  const t = useT();
  // A plugin another one covers says so on its own line; "always on" would contradict it.
  if (plugin.coveredBy) return null;
  if (plugin.raceLocked) {
    return <span className="text-[11.5px] text-muted-foreground">{t("gamePlugins.racingTagLocked")}</span>;
  }
  return (
    <button
      type="button"
      disabled={disabled}
      title={t("gamePlugins.racingTagHelp")}
      onClick={() => onChange(!plugin.raceOff)}
      className={cn(
        "cursor-default rounded border px-2 py-0.5 text-[11.5px] transition-colors disabled:opacity-50",
        plugin.raceOff
          ? "border-warning/50 text-warning hover:bg-warning/10"
          : "border-border text-muted-foreground hover:text-foreground",
      )}
    >
      {plugin.raceOff ? t("gamePlugins.racingTagOff") : t("gamePlugins.racingTagStay")}
    </button>
  );
}

function Thumb({ src, name, className }: { src?: string; name: string; className?: string }) {
  return src ? (
    <CachedImg src={src} alt="" className={cn("object-cover", className)} />
  ) : (
    <div className={cn("flex items-center justify-center bg-foreground/[0.06] font-semibold text-foreground/40", className)}>
      {name.slice(0, 1).toUpperCase()}
    </div>
  );
}

/** The version line: what's installed (or when), then what's newest. */
function versionLine(t: ReturnType<typeof useT>, plugin: GamePlugin, latest?: GamePluginLatest) {
  const parts: string[] = [];
  if (plugin.version) parts.push(`v${plugin.version}`);
  else if (plugin.installedUpdated) parts.push(t("gamePlugins.installedFrom", { date: formatDate(plugin.installedUpdated) }));
  else if (plugin.source !== "none") parts.push(t("gamePlugins.versionUnknown"));
  if (latest?.version && latest.version !== plugin.version) parts.push(t("gamePlugins.latest", { version: latest.version }));
  return parts.join(" · ");
}

function installable(source: string) {
  return source === "github" || source === "mxbmods";
}

function InstalledRow({
  plugin,
  latest,
  locked,
  working,
  onOpen,
  actions,
}: {
  plugin: GamePlugin;
  latest?: GamePluginLatest;
  locked: boolean;
  working: boolean;
  onOpen: () => void;
  actions: Actions;
}) {
  const t = useT();
  const update = installable(plugin.source) && hasUpdate(plugin, latest);
  // A plugin outside the catalog is never installed by the app: its update is on its page.
  const pageUpdate = !installable(plugin.source) && hasUpdate(plugin, latest) ? (latest?.page ?? plugin.homepage) : undefined;
  return (
    <div className="flex items-center gap-3 py-2">
      <button type="button" onClick={onOpen} className="flex min-w-0 flex-1 cursor-default items-center gap-3 text-left">
        <Thumb src={latest?.thumbnail} name={plugin.name} className="h-8 w-14 shrink-0 rounded text-[14px]" />
        <span className={cn("min-w-0", !plugin.enabled && "opacity-60")}>
          <span className="block truncate text-[13px] font-semibold">
            {plugin.name}
            {!plugin.enabled && <span className="ml-2 text-[11.5px] font-normal text-muted-foreground">{t("gamePlugins.off")}</span>}
          </span>
          <span className="block truncate text-[11.5px] text-muted-foreground">
            {[
              plugin.author || t("gamePlugins.unknownAuthor"),
              versionLine(t, plugin, latest),
              plugin.coveredBy && t("gamePlugins.coveredBy", { name: plugin.coveredBy }),
            ]
              .filter(Boolean)
              .join(" · ")}
          </span>
        </span>
      </button>
      {plugin.coveredBy && plugin.enabled && (
        <Button variant="outline" size="sm" className="h-7" disabled={locked} onClick={() => actions.toggle(plugin, false)}>
          {t("gamePlugins.coveredTurnOff", { name: plugin.coveredBy })}
        </Button>
      )}
      {pageUpdate && (
        <Button size="sm" variant="outline" className="h-7" onClick={() => void openUrl(pageUpdate)}>
          <ExternalLink className="size-3.5" />
          {t("gamePlugins.updateOnPage")}
        </Button>
      )}
      {update && (
        <Button size="sm" className="h-7" disabled={locked} onClick={() => actions.install(plugin.id, plugin.name, true)}>
          {working && <Loader2 className="size-3.5 animate-spin" />}
          {t("gamePlugins.update")}
        </Button>
      )}
      <RacingTag plugin={plugin} disabled={locked} onChange={(off) => actions.raceOff(plugin, off)} />
      {working && !update ? (
        <Loader2 className="size-4 animate-spin text-muted-foreground" />
      ) : (
        <Switch checked={plugin.enabled} disabled={locked} onCheckedChange={(on) => actions.toggle(plugin, on)} aria-label={plugin.name} />
      )}
    </div>
  );
}

function Card({
  c,
  plugin,
  latest,
  locked,
  working,
  onOpen,
  onInstall,
}: {
  c: GamePluginCatalogEntry;
  plugin?: GamePlugin;
  latest?: GamePluginLatest;
  locked: boolean;
  working: boolean;
  onOpen: () => void;
  onInstall: (update: boolean) => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col overflow-hidden rounded-lg border border-border bg-card">
      <button type="button" onClick={onOpen} className="aspect-[16/9] w-full cursor-default overflow-hidden border-b border-border">
        <Thumb src={latest?.thumbnail} name={c.name} className="size-full text-[28px]" />
      </button>
      <div className="flex flex-1 flex-col gap-1.5 p-3">
        <button type="button" onClick={onOpen} className="cursor-default text-left">
          <div className="truncate text-[13.5px] font-semibold text-foreground">{c.name}</div>
          <div className="text-[11.5px] text-muted-foreground">
            {c.author} · {t(categoryLabel(c.category))}
          </div>
        </button>
        <p className="line-clamp-2 text-[12px] text-foreground/75">{c.summary}</p>
        <div className="mt-auto flex items-center justify-between gap-2 pt-1.5">
          <span className="flex min-w-0 items-center gap-1.5 whitespace-nowrap text-[11.5px] text-muted-foreground">
            {latest?.rating != null && latest.rating > 0 && (
              <>
                <Star className="size-3 fill-primary text-primary" />
                {latest.rating.toFixed(1)}
              </>
            )}
            {latest?.version && <span>v{latest.version}</span>}
          </span>
          <CardAction c={c} plugin={plugin} latest={latest} locked={locked} working={working} onInstall={onInstall} />
        </div>
      </div>
    </div>
  );
}

function CardAction({
  c,
  plugin,
  latest,
  locked,
  working,
  onInstall,
}: {
  c: Pick<GamePluginCatalogEntry, "source" | "page">;
  plugin?: GamePlugin;
  latest?: GamePluginLatest;
  locked: boolean;
  working: boolean;
  onInstall: (update: boolean) => void;
}) {
  const t = useT();
  if (plugin) {
    if (installable(plugin.source) && hasUpdate(plugin, latest)) {
      return (
        <Button size="sm" disabled={locked} onClick={() => onInstall(true)}>
          {working ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
          {t("gamePlugins.update")}
        </Button>
      );
    }
    return (
      <Button size="sm" variant="secondary" disabled>
        {t("gamePlugins.installedBadge")}
      </Button>
    );
  }
  if (c.source === "external") {
    const page = c.page ?? latest?.page;
    return (
      <Button size="sm" variant="outline" disabled={!page} onClick={() => page && void openUrl(page)}>
        <ExternalLink className="size-3.5" />
        {t("gamePlugins.getIt")}
      </Button>
    );
  }
  return (
    <Button size="sm" disabled={locked} onClick={() => onInstall(false)}>
      {working ? <Loader2 className="size-3.5 animate-spin" /> : <Download className="size-3.5" />}
      {working ? t("gamePlugins.installing") : t("gamePlugins.install")}
    </Button>
  );
}

/** One plugin's page, installed or not: pictures, description, facts, settings, actions. */
function PluginPage({
  id,
  view,
  plugin,
  entry,
  latest,
  locked,
  working,
  onBack,
  actions,
}: {
  id: string;
  view: GamePluginsView;
  plugin?: GamePlugin;
  entry?: GamePluginCatalogEntry;
  latest?: GamePluginLatest;
  locked: boolean;
  working: boolean;
  onBack: () => void;
  actions: Actions;
}) {
  const t = useT();
  const [d, setD] = useState<GamePluginDetail | null>(null);
  const [shot, setShot] = useState(0);
  const [confirmUninstall, setConfirmUninstall] = useState(false);
  const name = plugin?.name ?? entry?.name ?? id;
  const author = plugin?.author || entry?.author || t("gamePlugins.unknownAuthor");
  const category = (plugin?.category ?? entry?.category ?? "other") as Category;
  const summary = plugin?.summary || entry?.summary || "";
  const source = plugin?.source ?? entry?.source ?? "none";

  useEffect(() => {
    let live = true;
    setD(null);
    setShot(0);
    if (!id.startsWith("file:")) {
      gamePluginDetail(id).then((x) => live && setD(x), () => {});
    }
    return () => {
      live = false;
    };
  }, [id]);

  const images = d?.images.length ? d.images : latest?.thumbnail ? [latest.thumbnail] : [];
  const page = d?.page ?? latest?.page ?? entry?.page ?? plugin?.homepage;
  const notices = view.conflicts.filter((c) => c.ids.includes(id));

  return (
    <div className="mx-auto max-w-[980px] space-y-5 pt-2">
      <Button size="sm" variant="ghost" onClick={onBack}>
        <ArrowLeft className="size-3.5" />
        {t("gamePlugins.back")}
      </Button>

      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <h1 className="text-[22px] font-semibold text-foreground">{name}</h1>
          <div className="mt-0.5 text-[12.5px] text-muted-foreground">
            {[author, t(categoryLabel(category)), plugin && versionLine(t, plugin, latest)]
              .filter(Boolean)
              .join(" · ")}
          </div>
          {d?.rating != null && d.rating > 0 && (
            <div className="mt-2 flex items-center gap-1.5 text-[12px] text-muted-foreground">
              <Star className="size-3.5 fill-primary text-primary" />
              {d.rating.toFixed(1)}
              {d.ratings != null && <span>({d.ratings})</span>}
              {d.views != null && <span>· {t("gamePlugins.views", { n: d.views.toLocaleString() })}</span>}
            </div>
          )}
        </div>
        <div className="flex items-center gap-2">
          {plugin ? (
            <>
              {installable(plugin.source) && (
                <Button size="sm" variant={hasUpdate(plugin, latest) ? "default" : "outline"} disabled={locked} onClick={() => actions.install(id, name, true)}>
                  {working ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
                  {hasUpdate(plugin, latest) ? t("gamePlugins.update") : t("gamePlugins.reinstall")}
                </Button>
              )}
              <Button size="sm" variant="outline" disabled={locked} onClick={() => setConfirmUninstall(true)}>
                {t("gamePlugins.uninstall")}
              </Button>
              <label className="flex items-center gap-2 text-[12px] text-muted-foreground">
                {plugin.enabled ? t("gamePlugins.on") : t("gamePlugins.off")}
                <Switch checked={plugin.enabled} disabled={locked} onCheckedChange={(on) => actions.toggle(plugin, on)} aria-label={name} />
              </label>
            </>
          ) : entry ? (
            <CardAction c={entry} latest={latest} locked={locked} working={working} onInstall={() => actions.install(id, name, false)} />
          ) : null}
        </div>
      </div>

      {notices.map((c) => (
        <p key={c.message} className={cn("text-[12.5px]", c.level === "error" ? "text-destructive" : c.level === "warning" ? "text-warning" : "text-muted-foreground")}>
          {c.message}
        </p>
      ))}
      {plugin?.coveredBy && plugin.enabled && (
        <Button variant="outline" size="sm" disabled={locked} onClick={() => actions.toggle(plugin, false)}>
          {t("gamePlugins.coveredTurnOff", { name: plugin.coveredBy })}
        </Button>
      )}

      {images.length > 0 && (
        <div className="space-y-2">
          <div className="aspect-[16/9] w-full overflow-hidden rounded-lg border border-border bg-card">
            <CachedImg src={images[shot]} alt="" className="size-full object-contain" />
          </div>
          {images.length > 1 && (
            <div className="flex flex-wrap gap-2">
              {images.map((s, i) => (
                <button
                  key={s}
                  type="button"
                  onClick={() => setShot(i)}
                  className={cn("h-14 w-24 cursor-default overflow-hidden rounded border", i === shot ? "border-primary" : "border-border opacity-70")}
                >
                  <CachedImg src={s} alt="" className="size-full object-cover" />
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="grid gap-6 md:grid-cols-[1fr_300px]">
        <div className="min-w-0 space-y-5">
          {plugin && plugin.configs.some((c) => !c.path.endsWith("/") && !c.path.endsWith("\\")) && (
            <SettingsEditor plugin={plugin} locked={locked} />
          )}
          {d?.descriptionHtml ? (
            <div className="text-[13px] leading-relaxed text-foreground/85">
              <RichDescription html={d.descriptionHtml} />
            </div>
          ) : (
            <p className="text-[13px] text-foreground/85">{summary}</p>
          )}
        </div>
        <dl className="space-y-2 border-t border-border pt-3 text-[12px] md:border-l md:border-t-0 md:pl-5 md:pt-0">
          {plugin?.version && <Row k={t("gamePlugins.installedVersion")} v={`v${plugin.version}`} />}
          {latest?.version && <Row k={t("gamePlugins.latestVersion")} v={`v${latest.version}`} />}
          {(d?.updated ?? latest?.updated) && <Row k={t("gamePlugins.updatedLabel")} v={formatDate((d?.updated ?? latest?.updated)!)} />}
          {plugin?.madeFor && <Row k={t("gamePlugins.madeFor")} v={plugin.madeFor.replace(/^beta/i, "Beta ")} />}
          <Row k={t("gamePlugins.sourceLabel")} v={t(source === "github" ? "gamePlugins.sourceGithub" : source === "mxbmods" ? "gamePlugins.sourceMxbmods" : source === "external" ? "gamePlugins.sourceExternal" : "gamePlugins.sourceNone")} />
          {plugin && (
            <div className="space-y-1.5 pt-2">
              <div className="text-muted-foreground">{t("gamePlugins.racingTitle")}</div>
              <RacingTag plugin={plugin} disabled={locked} onChange={(off) => actions.raceOff(plugin, off)} />
            </div>
          )}
          {plugin && plugin.keys.length > 0 && (
            <div className="space-y-1 pt-2">
              <div className="text-muted-foreground">{t("gamePlugins.keys")}</div>
              {plugin.keys.map((key) => (
                <div key={`${key.key}-${key.label}`} className="flex items-center gap-2">
                  <kbd className="rounded border border-border px-1.5 py-0.5 font-mono text-[11px] text-foreground">{key.key}</kbd>
                  <span className="text-foreground/80">{key.label}</span>
                </div>
              ))}
            </div>
          )}
          {plugin && plugin.presets.length > 0 && <Row k={t("gamePlugins.presets")} v={plugin.presets.join(", ")} />}
          {plugin && (
            <div className="space-y-1 pt-2">
              <div className="text-muted-foreground">{t("gamePlugins.files")}</div>
              <p className="break-all font-mono text-[11px] text-foreground/70">{plugin.files.join("  ")}</p>
              <Button variant="ghost" size="sm" className="-ml-2" onClick={() => void gamePluginReveal(id).catch((e) => toast.error(message(e)))}>
                <FolderOpen className="size-3.5" />
                {t("gamePlugins.showFolder")}
              </Button>
            </div>
          )}
          {page && (
            <Button variant="ghost" size="sm" className="-ml-2" onClick={() => void openUrl(page)}>
              <ExternalLink className="size-3.5" />
              {t("gamePlugins.openPage")}
            </Button>
          )}
        </dl>
      </div>

      <AlertDialog open={confirmUninstall} onOpenChange={setConfirmUninstall}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("gamePlugins.uninstallTitle", { name })}</AlertDialogTitle>
            <AlertDialogDescription>{t("gamePlugins.uninstallBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                setConfirmUninstall(false);
                if (plugin) {
                  actions.uninstall(plugin);
                  onBack();
                }
              }}
            >
              {t("gamePlugins.uninstall")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between gap-3">
      <dt className="text-muted-foreground">{k}</dt>
      <dd className="text-right text-foreground/85">{v}</dd>
    </div>
  );
}

/**
 * The plugin's settings files, editable in place. Saving keeps the previous text as `.bak`
 * beside it; "Open in editor" hands the same file to the system's own editor.
 */
function SettingsEditor({ plugin, locked }: { plugin: GamePlugin; locked: boolean }) {
  const t = useT();
  const files = plugin.configs.filter((c) => !c.path.endsWith("/") && !c.path.endsWith("\\"));
  const [path, setPath] = useState(files[0]?.path ?? "");
  const [text, setText] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let live = true;
    setText(null);
    gamePluginReadConfig(plugin.id, path).then(
      (x) => {
        if (!live) return;
        setText(x);
        setSaved(x);
      },
      (e) => live && toast.error(message(e)),
    );
    return () => {
      live = false;
    };
  }, [plugin.id, path]);

  const save = async () => {
    if (text == null) return;
    setSaving(true);
    try {
      await gamePluginWriteConfig(plugin.id, path, text);
      setSaved(text);
      toast.success(t("gamePlugins.settingsSaved"));
    } catch (e) {
      toast.error(message(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <div className="text-[11.5px] font-semibold uppercase tracking-wide text-muted-foreground">{t("gamePlugins.settings")}</div>
        {files.length > 1 && (
          <select className="h-7 rounded-md border border-input bg-transparent px-2 text-[12px]" value={path} onChange={(e) => setPath(e.target.value)}>
            {files.map((f) => (
              <option key={f.path} value={f.path}>
                {f.label}
              </option>
            ))}
          </select>
        )}
        <div className="ml-auto flex gap-2">
          <Button variant="ghost" size="sm" onClick={() => void gamePluginOpenConfig(plugin.id, path).catch((e) => toast.error(message(e)))}>
            <ExternalLink className="size-3.5" />
            {t("gamePlugins.openInEditor")}
          </Button>
          <Button size="sm" disabled={locked || saving || text == null || text === saved} onClick={() => void save()}>
            {saving && <Loader2 className="size-3.5 animate-spin" />}
            {t("gamePlugins.save")}
          </Button>
        </div>
      </div>
      <textarea
        spellCheck={false}
        className="h-64 w-full resize-y rounded-md border border-input bg-transparent p-2 font-mono text-[12px] text-foreground"
        value={text ?? ""}
        disabled={text == null}
        onChange={(e) => setText(e.target.value)}
      />
    </div>
  );
}
