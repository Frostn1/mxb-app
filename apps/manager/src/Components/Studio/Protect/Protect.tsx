import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  FileLock2,
  FolderOpen,
  Loader2,
  Lock,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import {
  contentLockPlan,
  contentLockRun,
  experimentalState,
  localGuid,
  onContentLockProgress,
  revealInExplorer,
  setGuid as saveGuid,
} from "@frost/shared/api/mods";
import type { LockItem, LockProgress } from "@frost/shared/types";
import { formatBytes } from "@frost/shared/lib/mods";
import { useT } from "@frost/shared/i18n/context";

/** A GUID as the game prints it. The Rust side is the authority; this is the same rule,
 *  applied as you type so a bad paste is visible before the run rather than after it. */
const GUID_RE = /^[0-9A-F]{18}$/;

/** Why a file will be left alone, in the order the backend can report it. */
const SKIP_LABEL = {
  junk: "protect.skipJunk",
  empty: "protect.skipEmpty",
  protected: "protect.skipProtected",
} as const;

function normalize(raw: string): string {
  return raw.trim().replace(/^0x/i, "").toUpperCase();
}

/**
 * Protect — lock a creator's files to the GUIDs of the people allowed to load them.
 *
 * A locked file is bound to one GUID, so shipping to five buyers means five copies. That is
 * the whole shape of this screen: pick the files once, paste the GUIDs, and get a folder per
 * buyer. The originals are only ever read — a creator's plaintext is the one thing they
 * can't get back.
 */
export default function Protect() {
  const t = useT();
  const [items, setItems] = useState<LockItem[]>([]);
  const [roots, setRoots] = useState<string[]>([]);
  const [guidText, setGuidText] = useState("");
  const [outDir, setOutDir] = useState("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<LockProgress | null>(null);
  const [mine, setMine] = useState<string | null>(null);
  const [claimed, setClaimed] = useState("");
  const [reading, setReading] = useState(false);

  useEffect(() => {
    let alive = true;
    const un = onContentLockProgress((p) => {
      if (alive) setProgress(p);
    });
    return () => {
      alive = false;
      void un.then((f) => f());
    };
  }, []);

  // The GUID this account has already claimed. The Servers tab claims it from a server log
  // and this screen can read it out of the game — either way there is one value, so both
  // places agree about who you are.
  useEffect(() => {
    experimentalState()
      .then((s) => setClaimed(s.guid?.trim() || ""))
      .catch(() => {});
  }, []);
  const own = mine || claimed;

  const guids = useMemo(() => {
    const seen = new Set<string>();
    return guidText
      .split(/[\s,;]+/)
      .map(normalize)
      .filter((g) => g.length > 0)
      .filter((g) => (seen.has(g) ? false : (seen.add(g), true)))
      .map((g) => ({ guid: g, ok: GUID_RE.test(g) }));
  }, [guidText]);

  const valid = guids.filter((g) => g.ok).map((g) => g.guid);
  const lockable = items.filter((i) => !i.skip);
  const skipped = items.length - lockable.length;
  const totalBytes = lockable.reduce((n, i) => n + i.bytes, 0);

  const rescan = useCallback(async (paths: string[]) => {
    setBusy(true);
    try {
      setItems(await contentLockPlan(paths));
      setRoots(paths);
    } catch (e) {
      toast.error(t("protect.planFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  }, [t]);

  const pick = async (directory: boolean) => {
    const picked = await openDialog({ multiple: true, directory });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (paths.length) await rescan([...new Set([...roots, ...paths])]);
  };

  const pickOut = async () => {
    const picked = await openDialog({ multiple: false, directory: true });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (path) setOutDir(path);
  };

  const readOwn = async () => {
    setReading(true);
    try {
      const g = await localGuid();
      if (!g) {
        toast.info(t("protect.ownNotFound"), {
          description: t("protect.ownNotFoundWhy"),
        });
        return;
      }
      setMine(g);
      // Claim it while we have it: the Servers tab wants the same value, and it otherwise
      // waits for one of your servers to see you connect.
      if (!claimed) await saveGuid(g).catch(() => {});
      toast.success(t("protect.ownFound", { guid: g }));
    } catch (e) {
      toast.error(t("protect.ownFailed"), { description: String(e) });
    } finally {
      setReading(false);
    }
  };

  const run = async () => {
    setBusy(true);
    setProgress(null);
    try {
      const r = await contentLockRun(roots, valid, outDir);
      toast.success(
        t("protect.done", { files: r.written, guids: r.guids }),
        {
          description: formatBytes(r.bytes),
          action: {
            label: t("protect.showFolder"),
            onClick: () => void revealInExplorer(r.outDir).catch(() => {}),
          },
        },
      );
    } catch (e) {
      toast.error(t("protect.failed"), { description: String(e) });
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const ready = lockable.length > 0 && valid.length > 0 && outDir.length > 0;

  const head = (label: string) => (
    <div className="flex flex-none items-center gap-2.5 px-6 pb-2.5 pt-4">
      <span className="u-skew h-3 w-1 bg-primary" />
      <h2 className="font-cond text-[13px] font-bold uppercase tracking-[0.2em] text-foreground">
        {label}
      </h2>
    </div>
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-0 flex-1">
        {/* ── What is being locked ─────────────────────────────────────────── */}
        <section className="flex min-w-0 flex-1 flex-col border-r border-border">
          {head(t("protect.filesTitle"))}
          <div className="flex flex-none flex-wrap items-center gap-2 px-6 pb-3">
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void pick(false)}>
              <FileLock2 className="size-3.5" /> {t("protect.addFiles")}
            </Button>
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void pick(true)}>
              <FolderOpen className="size-3.5" /> {t("protect.addFolder")}
            </Button>
            {items.length > 0 && (
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                onClick={() => {
                  setItems([]);
                  setRoots([]);
                }}
              >
                <Trash2 className="size-3.5" /> {t("protect.clear")}
              </Button>
            )}
            {items.length > 0 && (
              <span className="ml-auto tabular-figures text-[11.5px] text-muted-foreground">
                {t("protect.summary", { files: lockable.length, size: formatBytes(totalBytes) })}
                {skipped > 0 && ` · ${t("protect.summarySkipped", { count: skipped })}`}
              </span>
            )}
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto px-6">
            {items.length > 0 ? (
              <ul className="border border-border bg-card text-[12px]">
                {items.map((it) => (
                  <li
                    key={it.abs}
                    className={cn(
                      "flex items-center justify-between gap-3 border-b border-border/60 px-3 py-1.5 last:border-b-0",
                      it.skip && "text-muted-foreground",
                    )}
                  >
                    <span className="truncate font-mono">{it.rel}</span>
                    <span className="flex-none tabular-figures text-[11px] text-muted-foreground">
                      {it.skip ? t(SKIP_LABEL[it.skip]) : formatBytes(it.bytes)}
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="border border-dashed border-border px-4 py-8 text-center text-[12.5px] leading-relaxed text-muted-foreground">
                {t("protect.filesDesc")}
              </p>
            )}
          </div>

          <div className="mx-6 my-4 flex flex-none items-center gap-2.5 border-l-2 border-primary bg-primary-tint px-3 py-2.5">
            <span className="text-[11.5px] leading-snug text-muted-foreground">
              {t("protect.outDesc")}
            </span>
          </div>
        </section>

        {/* ── Who it is being locked to ─────────────────────────────────────── */}
        <aside className="flex w-[452px] flex-none flex-col overflow-y-auto">
          {head(t("protect.guidsTitle"))}
          <p className="flex-none px-6 pb-3 text-[11.5px] leading-relaxed text-muted-foreground">
            {t("protect.guidsDesc")}
          </p>

          <div className="flex-none px-6">
            <textarea
              value={guidText}
              onChange={(e) => setGuidText(e.target.value)}
              spellCheck={false}
              rows={4}
              placeholder={t("protect.guidsPlaceholder")}
              className="w-full border border-input bg-card px-3 py-2 font-mono text-[12.5px] outline-none focus:border-ring"
            />
            {guids.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1.5">
                {guids.map((g) => (
                  <span
                    key={g.guid}
                    title={g.ok ? undefined : t("protect.guidBad")}
                    className={cn(
                      "border px-2 py-0.5 font-mono text-[11px]",
                      g.ok
                        ? "border-border text-muted-foreground"
                        : "border-destructive/40 bg-destructive/10 text-destructive",
                    )}
                  >
                    {g.guid}
                  </span>
                ))}
              </div>
            )}
          </div>

          {/* Your own GUID lives with the others: this is the box it gets pasted into,
              and locking a test copy to yourself is how you check a mod before sending it. */}
          <div className="mt-4 flex-none border-t border-border px-6 pt-4">
            <div className="font-cond text-[10.5px] font-semibold uppercase tracking-[0.2em] text-faint">
              {t("protect.ownTitle")}
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <code
                className={cn(
                  "border border-border px-2.5 py-1.5 font-mono text-[12.5px]",
                  own ? "text-foreground" : "text-muted-foreground",
                )}
              >
                {own || t("protect.ownUnknown")}
              </code>
              <Button size="sm" variant="outline" disabled={reading} onClick={() => void readOwn()}>
                {reading ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <RefreshCw className="size-3.5" />
                )}
                {t("protect.readOwn")}
              </Button>
              {own && (
                <button
                  onClick={() => setGuidText((s) => (s ? `${s.trimEnd()}\n${own}` : own))}
                  className="cursor-default text-[11.5px] text-muted-foreground underline underline-offset-2 hover:text-foreground"
                >
                  {t("protect.useMine")}
                </button>
              )}
            </div>
          </div>

          <div className="mt-4 flex-none border-t border-border px-6 pb-5 pt-4">
            <div className="font-cond text-[10.5px] font-semibold uppercase tracking-[0.2em] text-faint">
              {t("protect.outTitle")}
            </div>
            <div className="mt-2 flex items-center gap-2">
              <Input
                value={outDir}
                onChange={(e) => setOutDir(e.target.value)}
                spellCheck={false}
                placeholder={t("protect.outPlaceholder")}
                className="h-8 flex-1 text-[12.5px]"
              />
              <Button size="sm" variant="outline" onClick={() => void pickOut()}>
                <FolderOpen className="size-3.5" /> {t("protect.browse")}
              </Button>
            </div>
          </div>
        </aside>
      </div>

      {/* ── Run ───────────────────────────────────────────────────────────── */}
      <div className="flex h-[52px] flex-none items-center gap-3 border-t border-border bg-window px-6">
        {progress && (
          <span className="font-mono text-[11.5px] text-muted-foreground">
            {progress.done}/{progress.total} · {progress.guid} · {progress.file}
          </span>
        )}
        <div className="flex-1" />
        <Button disabled={busy || !ready} onClick={() => void run()}>
          {busy ? <Loader2 className="size-4 animate-spin" /> : <Lock className="size-4" />}
          {t("protect.lock", { files: lockable.length, guids: valid.length })}
        </Button>
      </div>
    </div>
  );
}
