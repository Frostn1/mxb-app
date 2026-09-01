import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Flag,
  Layers,
  Loader2,
  RefreshCw,
  RotateCcw,
  Search,
  Share2,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { Button } from "@/Components/ui/button";
import HelpHint from "@/Components/ui/help-hint";
import { Segmented } from "@/Components/ui/segmented";
import { Switch } from "@/Components/ui/switch";
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from "@/Components/ui/select";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from "@/Components/ui/alert-dialog";
import {
  modsStateApply,
  modsStateDelete,
  modsStatePlan,
  modsStateRestoreAll,
  modsStateScan,
  modsStateSet,
  onModsChanged,
  presetsList,
  presetsListBikes,
  presetsListProfiles,
  presetsSave,
} from "../../api/mods";
import type {
  ModEntry,
  ModsStateOutcome,
  Preset,
  PresetContent,
  StatePlan,
} from "../../types";
import { useT } from "../../i18n/context";
import { displayName, formatBytes } from "../../lib/mods";
import { CATEGORY_LABEL, categoryIcon } from "../Library/categories";
import { useShare } from "../../Context/Share";
import { ContentDialog } from "./ContentDialog";

type Tab = "race" | "mods";

/** The content type a mod belongs to, from its `rel` path (`mods/tracks/…`). */
export function contentType(rel: string): "tracks" | "bikes" | "rider" {
  const seg = rel.split("/")[1]?.toLowerCase();
  return seg === "bikes" ? "bikes" : seg === "rider" ? "rider" : "tracks";
}

/** A preset's content, with the `null`/absent case folded into an empty one. */
export function contentOf(preset: Preset): PresetContent {
  return { tracks: [], keep: [], ...(preset.content ?? {}) };
}

/**
 * Manage: what MX Bikes is allowed to see.
 *
 * The game mounts every archive under `mods/` at startup, so a big library is paid for on
 * every load. Two halves, and they meet in the middle:
 *
 * - **Race presets** — a saved preset plus the content that race needs (its track, the OEM
 *   pack). One click puts the look on and takes everything else out of the way.
 * - **Mods** — the same enable/disable by hand, for the times a preset isn't the answer,
 *   plus deleting.
 *
 * Nothing is destroyed by disabling: mods move to `<MX Bikes>/mxbapp_disabled` and come
 * back to the exact path they left from. Deleting goes through the recycle bin, like the
 * Library's uninstall.
 */
export default function Manage() {
  const t = useT();
  const [tab, setTab] = useState<Tab>("race");
  const [mods, setMods] = useState<ModEntry[]>([]);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Where a race preset's cosmetics land. Same two pickers Presets uses.
  const [profiles, setProfiles] = useState<string[]>([]);
  const [profile, setProfile] = useState("");
  const [bikes, setBikes] = useState<string[]>([]);
  const [bike, setBike] = useState("");

  const [editing, setEditing] = useState<Preset | null>(null);
  const [racing, setRacing] = useState<{ preset: Preset; plan: StatePlan } | null>(null);
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [deleting, setDeleting] = useState<ModEntry | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [scanned, saved] = await Promise.all([modsStateScan(), presetsList()]);
      setMods(scanned);
      setPresets(saved);
      setError(null);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // A mod installed from Browse (in either window) belongs in this list right away.
  useEffect(() => {
    const un = onModsChanged(() => void load());
    return () => {
      void un.then((f) => f());
    };
  }, [load]);

  useEffect(() => {
    presetsListProfiles()
      .then((scan) => {
        setProfiles(scan.profiles);
        setProfile((p) => (scan.profiles.includes(p) ? p : scan.profiles[0] ?? ""));
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!profile) {
      setBikes([]);
      return;
    }
    let cancelled = false;
    presetsListBikes(profile)
      .then((bs) => {
        if (cancelled) return;
        setBikes(bs);
        setBike((b) => (bs.includes(b) ? b : bs[0] ?? ""));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [profile]);

  const disabledCount = useMemo(() => mods.filter((m) => !m.enabled).length, [mods]);

  /** Run a Manage operation, report what it did, and re-read the folder. */
  const run = useCallback(
    async (op: () => Promise<ModsStateOutcome>, done: (o: ModsStateOutcome) => string) => {
      setBusy(true);
      try {
        const out = await op();
        toast.success(done(out));
        if (out.failed.length > 0) {
          toast.warning(
            t("manage.someFailed", {
              count: out.failed.length,
              first: out.failed[0][1],
            }),
          );
        }
        await load();
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setBusy(false);
      }
    },
    [load, t],
  );

  const toggleMod = (m: ModEntry, enabled: boolean) =>
    run(
      () => modsStateSet([m.rel], enabled),
      () =>
        enabled
          ? t("manage.enabledOne", { name: displayName(m.name) })
          : t("manage.disabledOne", { name: displayName(m.name) }),
    );

  const openRace = async (preset: Preset) => {
    setBusy(true);
    try {
      setRacing({ preset, plan: await modsStatePlan(preset.name) });
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  const confirmRace = async () => {
    const preset = racing?.preset;
    setRacing(null);
    if (!preset) return;
    await run(
      () => modsStateApply(preset.name, profile, bike),
      (o) => t("manage.raceApplied", { name: preset.name, count: o.disabled }),
    );
  };

  const saveContent = async (preset: Preset, content: PresetContent) => {
    setEditing(null);
    setBusy(true);
    try {
      await presetsSave({ ...preset, content });
      toast.success(t("manage.contentSaved", { name: preset.name }));
      await load();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-1.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">{t("nav.manage")}</h1>
          <HelpHint title={t("nav.manage")} description={t("manage.help")} />
        </div>
        <Segmented
          value={tab}
          onChange={(v) => setTab(v as Tab)}
          options={[
            { value: "race", label: t("manage.tabRace") },
            {
              value: "mods",
              label: (
                <span className="flex items-center gap-1.5">
                  {t("manage.tabMods")}
                  <span className="text-muted-foreground">{mods.length}</span>
                </span>
              ),
            },
          ]}
        />
        <div className="ml-auto flex items-center gap-2">
          {disabledCount > 0 && (
            <>
              <span className="text-[12px] text-muted-foreground">
                {t("manage.disabledCount", { count: disabledCount })}
              </span>
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => setRestoreOpen(true)}
              >
                <RotateCcw className="size-3.5" /> {t("manage.restoreAll")}
              </Button>
            </>
          )}
          <Button variant="outline" size="sm" onClick={load} disabled={loading || busy}>
            <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />{" "}
            {t("locker.rescan")}
          </Button>
        </div>
      </header>

      {error ? (
        <p className="select-text px-7 py-16 text-center text-[13px] text-destructive">
          {error}
        </p>
      ) : loading ? (
        <p className="px-7 py-16 text-center text-[13px] text-muted-foreground">
          {t("library.scanning")}
        </p>
      ) : tab === "race" ? (
        <RacePanel
          presets={presets}
          mods={mods}
          busy={busy}
          profiles={profiles}
          profile={profile}
          onProfile={setProfile}
          bikes={bikes}
          bike={bike}
          onBike={setBike}
          onEdit={setEditing}
          onRace={openRace}
        />
      ) : (
        <ModsPanel
          mods={mods}
          busy={busy}
          onToggle={toggleMod}
          onDelete={setDeleting}
          onBulk={(rels, enabled) =>
            run(
              () => modsStateSet(rels, enabled),
              (o) =>
                enabled
                  ? t("manage.enabledMany", { count: o.enabled })
                  : t("manage.disabledMany", { count: o.disabled }),
            )
          }
        />
      )}

      <ContentDialog
        preset={editing}
        mods={mods}
        onClose={() => setEditing(null)}
        onSave={saveContent}
      />

      <AlertDialog open={Boolean(racing)} onOpenChange={(o) => !o && setRacing(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("manage.raceTitle", { name: racing?.preset.name ?? "" })}
            </AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="flex flex-col gap-2">
                <span>
                  {t("manage.raceBody", {
                    keep: racing?.plan.keep.length ?? 0,
                    disable: racing?.plan.disable.length ?? 0,
                  })}
                </span>
                {(racing?.plan.enable.length ?? 0) > 0 && (
                  <span>
                    {t("manage.raceReEnable", { count: racing?.plan.enable.length ?? 0 })}
                  </span>
                )}
                {profile && bike ? (
                  <span>{t("manage.raceLook", { profile, bike })}</span>
                ) : (
                  <span>{t("manage.raceNoLook")}</span>
                )}
                {racing && !racing.plan.keep.some((m) => contentType(m.rel) === "bikes") && (
                  <span className="text-destructive">{t("manage.raceNoBike")}</span>
                )}
                {racing?.plan.gameRunning && (
                  <span className="text-destructive">{t("manage.raceGameRunning")}</span>
                )}
                {(racing?.plan.unresolved.length ?? 0) > 0 && (
                  <span className="text-muted-foreground">
                    {t("manage.raceUnresolved", {
                      slots: (racing?.plan.unresolved ?? [])
                        .map((u) => u.value)
                        .join(", "),
                    })}
                  </span>
                )}
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={confirmRace}>
              {t("manage.raceGo")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={restoreOpen} onOpenChange={setRestoreOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("manage.restoreTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("manage.restoreBody", { count: disabledCount })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setRestoreOpen(false);
                void run(modsStateRestoreAll, (o) =>
                  t("manage.restored", { count: o.enabled }),
                );
              }}
            >
              {t("manage.restoreAll")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={Boolean(deleting)} onOpenChange={(o) => !o && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("manage.deleteTitle", {
                name: deleting ? displayName(deleting.name) : "",
              })}
            </AlertDialogTitle>
            <AlertDialogDescription>{t("manage.deleteBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                const target = deleting;
                setDeleting(null);
                if (!target) return;
                void run(
                  () => modsStateDelete([target.rel]),
                  () => t("manage.deleted", { name: displayName(target.name) }),
                );
              }}
            >
              {t("library.uninstall")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

/** The saved presets, each with the content its race needs. */
function RacePanel({
  presets,
  mods,
  busy,
  profiles,
  profile,
  onProfile,
  bikes,
  bike,
  onBike,
  onEdit,
  onRace,
}: {
  presets: Preset[];
  mods: ModEntry[];
  busy: boolean;
  profiles: string[];
  profile: string;
  onProfile: (v: string) => void;
  bikes: string[];
  bike: string;
  onBike: (v: string) => void;
  onEdit: (p: Preset) => void;
  onRace: (p: Preset) => void;
}) {
  const t = useT();
  const byRel = useMemo(() => new Map(mods.map((m) => [m.rel.toLowerCase(), m])), [mods]);
  const label = (rel: string) =>
    displayName(byRel.get(rel.toLowerCase())?.name ?? rel.split("/").pop() ?? rel);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
      <div className="mb-4 flex items-center gap-2.5 rounded-xl border border-white/[0.07] bg-card px-3.5 py-3">
        <span className="text-[12px] font-semibold text-muted-foreground">
          {t("manage.applyLookTo")}
        </span>
        <Select value={profile} onValueChange={onProfile}>
          <SelectTrigger className="h-8 w-[150px] text-[12.5px]">
            <SelectValue placeholder={t("presets.profile")} />
          </SelectTrigger>
          <SelectContent>
            {profiles.map((p) => (
              <SelectItem key={p} value={p}>
                {p}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={bike} onValueChange={onBike}>
          <SelectTrigger className="h-8 w-[150px] text-[12.5px]">
            <SelectValue placeholder={t("category.bike")} />
          </SelectTrigger>
          <SelectContent>
            {bikes.map((b) => (
              <SelectItem key={b} value={b}>
                {b}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <HelpHint title={t("manage.applyLookTo")} description={t("manage.applyLookHelp")} />
      </div>

      {presets.length === 0 ? (
        <p className="py-16 text-center text-[13px] text-muted-foreground">
          {t("manage.noPresets")}
        </p>
      ) : (
        <div className="flex flex-col gap-2.5">
          {presets.map((preset) => {
            const content = contentOf(preset);
            const ready = content.tracks.length > 0 || content.keep.length > 0;
            return (
              <div
                key={preset.name}
                className="flex items-center gap-3 rounded-xl border border-white/[0.07] bg-card p-3.5"
              >
                <Flag className="size-4 flex-none text-faint" />
                <div className="flex min-w-0 flex-col">
                  <span className="truncate text-[13.5px] font-semibold">
                    {preset.name}
                  </span>
                  <span className="truncate text-[11.5px] text-muted-foreground">
                    {ready
                      ? [
                          content.tracks.length > 0
                            ? content.tracks.map(label).join(", ")
                            : t("manage.noTrack"),
                          t("manage.pinnedCount", { count: content.keep.length }),
                        ].join(" · ")
                      : t("manage.noContentYet")}
                  </span>
                </div>
                <div className="ml-auto flex flex-none items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy}
                    onClick={() => onEdit(preset)}
                  >
                    <Layers className="size-3.5" /> {t("manage.editContent")}
                  </Button>
                  <Button
                    size="sm"
                    disabled={busy || !ready}
                    title={ready ? undefined : t("manage.noContentYet")}
                    onClick={() => onRace(preset)}
                  >
                    {busy ? (
                      <Loader2 className="size-3.5 animate-spin" />
                    ) : (
                      <Flag className="size-3.5" />
                    )}{" "}
                    {t("manage.raceMode")}
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

const TYPE_FILTERS = ["all", "tracks", "bikes", "rider"] as const;
type TypeFilter = (typeof TYPE_FILTERS)[number];

/** Every mod, on or off, with a switch each. */
function ModsPanel({
  mods,
  busy,
  onToggle,
  onDelete,
  onBulk,
}: {
  mods: ModEntry[];
  busy: boolean;
  onToggle: (m: ModEntry, enabled: boolean) => void;
  onDelete: (m: ModEntry) => void;
  onBulk: (rels: string[], enabled: boolean) => void;
}) {
  const t = useT();
  // A mod is named here by its rel, which is what a share code carries anyway — and it
  // reaches a mod switched *off* too, parked outside the content folder.
  const { shareFiles } = useShare();
  const [search, setSearch] = useState("");
  const [type, setType] = useState<TypeFilter>("all");

  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return mods.filter(
      (m) =>
        (type === "all" || contentType(m.rel) === type) &&
        (q === "" || m.name.toLowerCase().includes(q) || m.folder.toLowerCase().includes(q)),
    );
  }, [mods, search, type]);

  const shownEnabled = shown.filter((m) => m.enabled);
  const shownDisabled = shown.filter((m) => !m.enabled);

  return (
    <>
      <div className="flex flex-none items-center gap-3 px-7 pb-3">
        <Segmented
          size="sm"
          value={type}
          onChange={(v) => setType(v as TypeFilter)}
          options={TYPE_FILTERS.map((f) => ({
            value: f,
            label: t(
              f === "all"
                ? "browseCat.all"
                : f === "tracks"
                  ? "modType.tracks"
                  : f === "bikes"
                    ? "modType.bikes"
                    : "modType.rider",
            ),
          }))}
        />
        <div className="flex w-[240px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-2">
          <Search className="size-3.5 text-faint" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("library.searchPlaceholder")}
            className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
          />
        </div>
        <div className="ml-auto flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={busy || shownEnabled.length === 0}
            onClick={() => onBulk(shownEnabled.map((m) => m.rel), false)}
          >
            {t("manage.disableShown", { count: shownEnabled.length })}
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={busy || shownDisabled.length === 0}
            onClick={() => onBulk(shownDisabled.map((m) => m.rel), true)}
          >
            {t("manage.enableShown", { count: shownDisabled.length })}
          </Button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {shown.length === 0 ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {mods.length === 0 ? t("manage.noMods") : t("library.noMatches")}
          </p>
        ) : (
          <div className="flex flex-col gap-1.5">
            {shown.map((m) => {
              const Icon = categoryIcon(m.category);
              const categoryKey = CATEGORY_LABEL[m.category];
              return (
                <div
                  key={m.rel}
                  className={cn(
                    "flex items-center gap-3 rounded-lg border border-white/[0.07] bg-card px-3.5 py-2.5 transition-opacity",
                    !m.enabled && "opacity-55",
                  )}
                >
                  <Switch
                    checked={m.enabled}
                    disabled={busy}
                    onCheckedChange={(v) => onToggle(m, v)}
                  />
                  <Icon className="size-4 flex-none text-faint" />
                  <span className="min-w-0 truncate text-[13px] font-medium">
                    {displayName(m.name)}
                  </span>
                  <span className="flex-none text-[11.5px] text-faint">
                    {[categoryKey ? t(categoryKey) : m.category, m.folder, formatBytes(m.size)]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                  <button
                    disabled={busy}
                    onClick={() => shareFiles([m.rel])}
                    title={t("share.action")}
                    aria-label={t("share.action")}
                    className="ml-auto flex-none cursor-default rounded-md p-1 text-faint transition-colors hover:bg-foreground/[0.06] hover:text-primary disabled:opacity-40"
                  >
                    <Share2 className="size-3.5" />
                  </button>
                  <button
                    disabled={busy}
                    onClick={() => onDelete(m)}
                    title={t("library.uninstallAction")}
                    aria-label={t("library.uninstallAction")}
                    className="flex-none cursor-default rounded-md p-1 text-faint transition-colors hover:bg-destructive/15 hover:text-destructive disabled:opacity-40"
                  >
                    <Trash2 className="size-3.5" />
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </>
  );
}
