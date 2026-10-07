/**
 * Storage: where the mods folder's space goes, and what can safely go.
 *
 * Four views over one scan. The scan is the Library's own (one library scan per mods folder,
 * done in Rust on a blocking thread), so nothing here can disagree with the Library about
 * what is installed. The last answer is kept across visits and shown at once while a fresh
 * one runs behind it; leftovers and duplicates follow the overview, one at a time, so the
 * screen is usable as soon as the sizes are in.
 *
 * Nothing is removed without a confirmation that lists every file and the total. Mods go
 * through the Library's uninstall path; archives only if the last scan reported them. Both
 * end in the Recycle Bin.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { Check, FileArchive, Lock, RotateCw, Trash2 } from "lucide-react";
import { useT, type TKey } from "@/i18n";
import { displayName, formatBytes, formatDay } from "@frost/shared/lib/mods";
import { Button } from "@frost/shared/Components/ui/button";
import HelpHint from "@frost/shared/Components/ui/help-hint";
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
import { cn } from "@frost/shared/lib/utils";
import { ContextBarLeft, ContextBarRight, ContextTab } from "../Shell/ContextBar";
import { useViewActive } from "../Shell/RetainedView";
import { dropScans } from "../Library/scanCache";
import { useDownloads } from "../../Context/Downloads";
import { redownloadSources } from "../../lib/redownload";
import {
  storageDuplicates,
  storageLeftovers,
  storageRemoveMods,
  storageScan,
  storageTrashArchives,
  type DuplicateGroup,
  type Leftover,
  type RemoveReport,
  type StorageOverview,
  type StorageScan,
} from "../../api/storage";
import { duplicateRemovals, duplicateWaste, totalBytes } from "./selection";

type Tab = "overview" | "leftovers" | "duplicates" | "big";

type BucketKey = Exclude<keyof StorageOverview, "total">;

const BUCKETS: { key: BucketKey; label: TKey; color: string }[] = [
  { key: "tracks", label: "storage.bucketTracks", color: "bg-primary" },
  { key: "bikes", label: "storage.bucketBikes", color: "bg-sky-400" },
  { key: "paints", label: "storage.bucketPaints", color: "bg-amber-400" },
  { key: "gear", label: "storage.bucketGear", color: "bg-violet-400" },
  { key: "other", label: "storage.bucketOther", color: "bg-white/30" },
];

function bucketOf(category: string): BucketKey {
  if (category === "track") return "tracks";
  if (category === "bike" || category === "bikeModelSwap" || category === "sound") return "bikes";
  if (category.endsWith("Paint") || category === "goggles") return "paints";
  if (["helmet", "boots", "protection", "animation", "gloves"].includes(category)) return "gear";
  return "other";
}

/** The last answers, kept across visits so a return shows them at once. */
const remembered: {
  scan?: StorageScan;
  leftovers?: Leftover[];
  duplicates?: DuplicateGroup[];
} = {};

/** One line in a removal confirmation. */
interface PendingItem {
  name: string;
  path: string;
  size: number;
}

interface Pending {
  items: PendingItem[];
  run: () => Promise<RemoveReport>;
}

const BIG_PAGE = 150;

export default function Storage({ onChanged }: { onChanged: () => void }) {
  const t = useT();
  const active = useViewActive();
  const { records } = useDownloads();
  const [tab, setTab] = useState<Tab>("overview");
  const [scan, setScan] = useState(remembered.scan);
  const [leftovers, setLeftovers] = useState(remembered.leftovers);
  const [duplicates, setDuplicates] = useState(remembered.duplicates);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);

  const [pickedArchives, setPickedArchives] = useState<Set<string>>(new Set());
  const [keep, setKeep] = useState<Map<string, string>>(new Map());
  const [skipGroups, setSkipGroups] = useState<Set<string>>(new Set());
  const [pickedMods, setPickedMods] = useState<Set<string>>(new Set());
  const [bigShown, setBigShown] = useState(BIG_PAGE);

  const running = useRef(false);
  const refresh = useCallback(async () => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      const s = await storageScan();
      remembered.scan = s;
      setScan(s);
      const l = await storageLeftovers().catch(() => [] as Leftover[]);
      remembered.leftovers = l;
      setLeftovers(l);
      // Content matches are pre-selected; a name-only match waits for the player.
      setPickedArchives(new Set(l.filter((x) => x.matchedBy !== "name").map((x) => x.path)));
      const d = await storageDuplicates().catch(() => [] as DuplicateGroup[]);
      remembered.duplicates = d;
      setDuplicates(d);
      setPickedMods((prev) => new Set([...prev].filter((p) => s.mods.some((m) => m.path === p))));
    } catch (e) {
      setError(String(e));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }, []);

  // First look, and every return: show what we had, rescan behind it.
  useEffect(() => {
    if (active) void refresh();
  }, [active, refresh]);

  const mods = useMemo(() => scan?.mods ?? [], [scan]);
  const sources = useMemo(() => redownloadSources(mods, records), [mods, records]);
  const leftoverBytes = totalBytes(leftovers ?? []);
  const dupWaste = duplicateWaste(duplicates ?? []);

  const confirmRemoval = async () => {
    if (!pending) return;
    const job = pending;
    setPending(null);
    setBusy(true);
    try {
      const report = await job.run();
      if (report.removed.length)
        toast.success(t("storage.removed", { count: report.removed.length }), {
          description: t("storage.removedDesc", { size: formatBytes(report.bytes) }),
        });
      if (report.failed.length)
        toast.error(t("storage.removeFailed", { count: report.failed.length }), {
          description: report.failed.map(([p, why]) => `${p}: ${why}`).join("\n"),
        });
    } catch (e) {
      toast.error(t("storage.removeFailed", { count: job.items.length }), { description: String(e) });
    } finally {
      setBusy(false);
    }
    // The Library's remembered scans no longer describe the tree.
    dropScans();
    onChanged();
    setPickedMods(new Set());
    await refresh();
  };

  const askArchives = () => {
    const items = (leftovers ?? []).filter((l) => pickedArchives.has(l.path));
    if (!items.length) return;
    setPending({ items, run: () => storageTrashArchives(items.map((i) => i.path)) });
  };

  const askDuplicates = () => {
    const items = (duplicates ?? [])
      .filter((g) => !skipGroups.has(g.id))
      .flatMap((g) => duplicateRemovals(g, keep.get(g.id)));
    if (!items.length) return;
    setPending({
      items,
      run: () => storageRemoveMods(items.map((m) => ({ path: m.path, subpath: m.subpath }))),
    });
  };

  const askMods = () => {
    const items = mods.filter((m) => pickedMods.has(m.path) && !m.secured);
    if (!items.length) return;
    setPending({
      items,
      run: () => storageRemoveMods(items.map((m) => ({ path: m.path, subpath: m.subpath }))),
    });
  };

  const counts: Record<Tab, number | null> = {
    overview: null,
    leftovers: leftovers?.length ?? null,
    duplicates: duplicates?.length ?? null,
    big: null,
  };

  return (
    <div className="flex h-full flex-col">
      <ContextBarLeft>
        {(["overview", "leftovers", "duplicates", "big"] as const).map((v) => (
          <ContextTab key={v} active={tab === v} onSelect={() => setTab(v)}>
            <span className="flex items-center gap-1.5">
              {t(TAB_LABEL[v])}
              {counts[v] !== null && (
                <span className="tabular-figures text-faint">{counts[v]}</span>
              )}
            </span>
          </ContextTab>
        ))}
      </ContextBarLeft>
      <ContextBarRight>
        <Button variant="outline" size="sm" onClick={() => void refresh()} disabled={busy}>
          <RotateCw className={cn("size-3.5", busy && "animate-spin")} /> {t("storage.rescan")}
        </Button>
        <HelpHint title={t("nav.storage")} description={t("storage.help")} />
      </ContextBarRight>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error && (
          <p className="mb-4 select-text rounded-xl border border-destructive/30 p-3 text-[12px] text-destructive">
            {t("storage.scanFailed")}: {error}
          </p>
        )}
        {!scan ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {busy ? t("storage.scanning") : t("common.loading")}
          </p>
        ) : tab === "overview" ? (
          <Overview
            scan={scan}
            leftoverBytes={leftovers ? leftoverBytes : null}
            duplicateBytes={duplicates ? dupWaste : null}
            onOpen={setTab}
          />
        ) : tab === "leftovers" ? (
          <Section
            desc={t("storage.leftoversDesc")}
            empty={leftovers?.length === 0 ? t("storage.leftoversEmpty") : null}
            checking={!leftovers}
            footer={
              <ActionBar
                count={pickedArchives.size}
                size={totalBytes((leftovers ?? []).filter((l) => pickedArchives.has(l.path)))}
                label={t("storage.moveToBin")}
                disabled={busy}
                onAction={askArchives}
              />
            }
          >
            {(leftovers ?? []).map((l) => (
              <Row
                key={l.path}
                checked={pickedArchives.has(l.path)}
                onToggle={() => setPickedArchives((s) => toggled(s, l.path))}
                icon={<FileArchive className="mt-0.5 size-4 flex-none text-muted-foreground" />}
                title={l.name}
                size={l.size}
                lines={[
                  <span key="p" className="truncate select-text">{l.path}</span>,
                  <span key="m" className={cn(l.matchedBy === "name" && "text-amber-400")}>
                    {t(l.location === "cache" ? "storage.locCache" : "storage.locDownloads")} ·{" "}
                    {t(MATCH_LABEL[l.matchedBy])}
                    {l.matches.length > 0 &&
                      ` · ${t("storage.installedAs", { names: l.matches.map(displayName).join(", ") })}`}
                  </span>,
                ]}
              />
            ))}
          </Section>
        ) : tab === "duplicates" ? (
          <Section
            desc={t("storage.duplicatesDesc")}
            empty={duplicates?.length === 0 ? t("storage.duplicatesEmpty") : null}
            checking={!duplicates}
            footer={
              <ActionBar
                count={(duplicates ?? []).filter((g) => !skipGroups.has(g.id)).reduce((n, g) => n + g.items.length - 1, 0)}
                size={(duplicates ?? [])
                  .filter((g) => !skipGroups.has(g.id))
                  .reduce((n, g) => n + totalBytes(duplicateRemovals(g, keep.get(g.id))), 0)}
                label={t("storage.removeExtra")}
                disabled={busy}
                onAction={askDuplicates}
              />
            }
          >
            {(duplicates ?? []).map((g) => (
              <DuplicateCard
                key={g.id}
                group={g}
                included={!skipGroups.has(g.id)}
                keepPath={keep.get(g.id) ?? g.items[0]?.path}
                onToggle={() => setSkipGroups((s) => toggled(s, g.id))}
                onKeep={(path) => setKeep((m) => new Map(m).set(g.id, path))}
              />
            ))}
          </Section>
        ) : (
          <Section
            desc={t("storage.bigDesc")}
            note={
              scan.coachSessions > 0
                ? t("storage.lastUsedCoach", { count: scan.coachSessions })
                : t("storage.lastUsedNone")
            }
            empty={mods.length === 0 ? t("storage.bigEmpty") : null}
            checking={false}
            footer={
              <ActionBar
                count={pickedMods.size}
                size={totalBytes(mods.filter((m) => pickedMods.has(m.path)))}
                label={t("storage.uninstall")}
                disabled={busy}
                onAction={askMods}
              />
            }
          >
            {mods.slice(0, bigShown).map((m) => (
              <Row
                key={m.path}
                checked={pickedMods.has(m.path)}
                disabled={m.secured}
                onToggle={() => setPickedMods((s) => toggled(s, m.path))}
                icon={
                  m.secured ? (
                    <Lock className="mt-0.5 size-4 flex-none text-muted-foreground" />
                  ) : null
                }
                title={displayName(m.name)}
                size={m.size}
                lines={[
                  <span key="c">
                    {t(BUCKETS.find((b) => b.key === bucketOf(m.category))!.label)}
                    {m.secured && ` · ${t("storage.protected")}`}
                    {scan.coachSessions > 0 &&
                      (m.category === "track" || m.category === "bike") &&
                      ` · ${
                        m.lastUsed
                          ? t("storage.lastRidden", { date: formatDay(m.lastUsed) })
                          : t("storage.neverRidden")
                      }`}
                    {sources.has(m.path) && ` · ${t("storage.canRedownload")}`}
                  </span>,
                  <span key="p" className="truncate select-text text-faint">{m.path}</span>,
                ]}
              />
            ))}
            {mods.length > bigShown && (
              <Button variant="ghost" size="sm" onClick={() => setBigShown((n) => n + BIG_PAGE)}>
                {t("storage.showMore")}
              </Button>
            )}
          </Section>
        )}
      </div>

      <AlertDialog open={!!pending} onOpenChange={(o) => !o && setPending(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("storage.confirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("storage.confirmBody", {
                count: pending?.items.length ?? 0,
                size: formatBytes(totalBytes(pending?.items ?? [])),
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="max-h-[40vh] overflow-y-auto rounded-lg border border-white/[0.07]">
            {(pending?.items ?? []).map((i) => (
              <div key={i.path} className="flex items-baseline gap-3 border-b border-white/[0.05] px-3 py-1.5 last:border-b-0">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[12px] font-semibold">{i.name}</div>
                  <div className="truncate select-text text-[11px] text-faint">{i.path}</div>
                  {sources.has(i.path) && (
                    <div className="text-[11px] text-primary">{t("storage.canRedownload")}</div>
                  )}
                </div>
                <span className="flex-none text-[11.5px] tabular-figures text-muted-foreground">
                  {formatBytes(i.size)}
                </span>
              </div>
            ))}
          </div>
          <div className="flex justify-between text-[12.5px] font-semibold">
            <span>{t("storage.confirmTotal")}</span>
            <span className="tabular-figures">{formatBytes(totalBytes(pending?.items ?? []))}</span>
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmRemoval()}>
              {t("storage.moveToBin")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

const TAB_LABEL: Record<Tab, TKey> = {
  overview: "storage.tabOverview",
  leftovers: "storage.tabLeftovers",
  duplicates: "storage.tabDuplicates",
  big: "storage.tabBig",
};

const MATCH_LABEL: Record<Leftover["matchedBy"], TKey> = {
  hash: "storage.matchHash",
  files: "storage.matchFiles",
  name: "storage.matchName",
};

function toggled<T>(set: Set<T>, value: T): Set<T> {
  const next = new Set(set);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return next;
}

function Overview({
  scan,
  leftoverBytes,
  duplicateBytes,
  onOpen,
}: {
  scan: StorageScan;
  leftoverBytes: number | null;
  duplicateBytes: number | null;
  onOpen: (tab: Tab) => void;
}) {
  const t = useT();
  const o = scan.overview;
  const pct = (n: number) => (o.total > 0 ? (n / o.total) * 100 : 0);
  return (
    <div className="flex max-w-[760px] flex-col gap-6 pt-2">
      <section className="flex flex-col gap-3 rounded-xl border border-white/[0.07] bg-card p-4">
        <div className="flex items-baseline justify-between">
          <span className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">
            {t("storage.total")}
          </span>
          <span className="text-[20px] font-semibold tabular-figures">{formatBytes(o.total) || "0 B"}</span>
        </div>
        <div className="flex h-2.5 overflow-hidden rounded-full bg-white/[0.06]">
          {BUCKETS.map((b) => (
            <div key={b.key} className={b.color} style={{ width: `${pct(o[b.key])}%` }} />
          ))}
        </div>
        <div className="flex flex-col gap-1.5">
          {BUCKETS.map((b) => (
            <div key={b.key} className="flex items-center gap-2 text-[12.5px]">
              <span className={cn("size-2.5 flex-none rounded-full", b.color)} />
              <span className="flex-1">{t(b.label)}</span>
              <span className="text-faint tabular-figures">{Math.round(pct(o[b.key]))}%</span>
              <span className="w-20 text-right tabular-figures">{formatBytes(o[b.key]) || "0 B"}</span>
            </div>
          ))}
        </div>
      </section>

      <Reclaim
        title={t("storage.leftoverTotal")}
        desc={t("storage.leftoverTotalDesc")}
        bytes={leftoverBytes}
        onOpen={() => onOpen("leftovers")}
      />
      <Reclaim
        title={t("storage.duplicateTotal")}
        desc={t("storage.duplicateTotalDesc")}
        bytes={duplicateBytes}
        onOpen={() => onOpen("duplicates")}
      />
    </div>
  );
}

function Reclaim({
  title,
  desc,
  bytes,
  onOpen,
}: {
  title: string;
  desc: string;
  bytes: number | null;
  onOpen: () => void;
}) {
  const t = useT();
  return (
    <section className="flex items-center gap-4 rounded-xl border border-white/[0.07] bg-card p-4">
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-semibold">{title}</div>
        <p className="mt-0.5 text-[11.5px] text-muted-foreground">{desc}</p>
      </div>
      <span className="flex-none text-[15px] font-semibold tabular-figures">
        {bytes === null ? t("storage.checking") : formatBytes(bytes) || "0 B"}
      </span>
      <Button variant="outline" size="sm" onClick={onOpen} disabled={!bytes}>
        {t("storage.review")}
      </Button>
    </section>
  );
}

function Section({
  desc,
  note,
  empty,
  checking,
  footer,
  children,
}: {
  desc: string;
  note?: string;
  empty: string | null;
  checking: boolean;
  footer: React.ReactNode;
  children: React.ReactNode;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-3 pt-2">
      <p className="text-[12px] text-muted-foreground">{desc}</p>
      {note && <p className="text-[11.5px] text-faint">{note}</p>}
      {checking ? (
        <p className="py-16 text-center text-[13px] text-muted-foreground">{t("storage.checking")}</p>
      ) : empty ? (
        <div className="rounded-xl border border-dashed border-white/[0.08] px-4 py-8 text-center text-[12.5px] text-muted-foreground">
          {empty}
        </div>
      ) : (
        <>
          <div className="flex flex-col gap-2">{children}</div>
          {footer}
        </>
      )}
    </div>
  );
}

function ActionBar({
  count,
  size,
  label,
  disabled,
  onAction,
}: {
  count: number;
  size: number;
  label: string;
  disabled: boolean;
  onAction: () => void;
}) {
  const t = useT();
  return (
    <div className="sticky bottom-0 flex items-center justify-end gap-3 rounded-xl border border-white/[0.07] bg-card/95 p-3 backdrop-blur">
      <span className="text-[12px] text-muted-foreground">
        {t("storage.selected", { count, size: formatBytes(size) || "0 B" })}
      </span>
      <Button variant="destructive" size="sm" onClick={onAction} disabled={disabled || count === 0}>
        <Trash2 className="size-3.5" /> {label}
      </Button>
    </div>
  );
}

function Tick({
  checked,
  disabled,
  onToggle,
  label,
}: {
  checked: boolean;
  disabled?: boolean;
  onToggle: () => void;
  label: string;
}) {
  return (
    <button
      onClick={onToggle}
      disabled={disabled}
      aria-label={label}
      aria-pressed={checked}
      className={cn(
        "mt-0.5 flex size-4 flex-none cursor-default items-center justify-center rounded border transition-colors disabled:opacity-30",
        checked ? "u-selected border-transparent" : "border-white/20",
      )}
    >
      {checked && <Check className="size-3" />}
    </button>
  );
}

function Row({
  checked,
  disabled,
  onToggle,
  icon,
  title,
  size,
  lines,
}: {
  checked: boolean;
  disabled?: boolean;
  onToggle: () => void;
  icon: React.ReactNode;
  title: string;
  size: number;
  lines: React.ReactNode[];
}) {
  const t = useT();
  return (
    <div className="flex items-start gap-3 rounded-xl border border-white/[0.07] bg-card p-3">
      <Tick
        checked={checked && !disabled}
        disabled={disabled}
        onToggle={onToggle}
        label={checked ? t("common.deselect") : t("common.select")}
      />
      {icon}
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate text-[13px] font-semibold">{title}</span>
        {lines.map((l, i) => (
          <div key={i} className="flex min-w-0 text-[11.5px] text-muted-foreground">
            {l}
          </div>
        ))}
      </div>
      <span className="flex-none text-[12.5px] font-semibold tabular-figures">{formatBytes(size)}</span>
    </div>
  );
}

function DuplicateCard({
  group,
  included,
  keepPath,
  onToggle,
  onKeep,
}: {
  group: DuplicateGroup;
  included: boolean;
  keepPath: string | undefined;
  onToggle: () => void;
  onKeep: (path: string) => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-white/[0.07] bg-card p-3">
      <div className="flex items-center gap-3">
        <Tick
          checked={included}
          onToggle={onToggle}
          label={included ? t("common.deselect") : t("common.select")}
        />
        <span className="flex-1 truncate text-[13px] font-semibold">
          {displayName(group.items[0]?.name ?? "")}
        </span>
        <span className="text-[11.5px] text-muted-foreground">
          {t("storage.copies", { count: group.items.length, size: formatBytes(group.size) })}
        </span>
      </div>
      <div className="flex flex-col gap-1 pl-7">
        {group.items.map((m) => {
          const kept = m.path === keepPath;
          return (
            <div key={m.path} className="flex items-center gap-2 text-[11.5px]">
              <button
                onClick={() => onKeep(m.path)}
                aria-pressed={kept}
                className={cn(
                  "flex-none cursor-default rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide",
                  kept
                    ? "border-primary/25 text-primary"
                    : included
                      ? "border-destructive/30 text-destructive"
                      : "border-white/[0.08] text-muted-foreground",
                )}
              >
                {kept ? t("storage.keep") : t("storage.remove")}
              </button>
              <span className="truncate select-text text-muted-foreground">{m.path}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
