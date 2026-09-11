import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FileSearch, FolderOpen, Loader2, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import {
  contentLockAvailable,
  contentLockPlan,
  modelSwapLineup,
  scanModelSwaps,
} from "@frost/shared/api/mods";
import type { BikeModels, LockItem } from "@frost/shared/types";
import { useT } from "@/i18n";

/** A bike folder as people say it: the OEM prefix and underscores dropped. */
const bikeName = (b: string) => b.replace(/^MX[0-9E]OEM_/i, "").replace(/_/g, " ");

/**
 * Diagnose — questions about files already on disk: which bikes a model swap lines up with,
 * and which GUID a protected file is locked to.
 */
export default function Diagnose() {
  return (
    <div className="flex h-full min-h-0">
      <SwapLineup />
      <LockedGuids />
    </div>
  );
}

function Head({ label, children }: { label: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-none items-center gap-2 px-6 pb-2.5 pt-4">
      <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{label}</h2>
      {children}
    </div>
  );
}

function SwapLineup() {
  const t = useT();
  const [bikes, setBikes] = useState<BikeModels[] | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [lineup, setLineup] = useState<string[] | null>(null);

  const scan = useCallback(() => {
    setBikes(null);
    scanModelSwaps()
      .then(setBikes)
      .catch(() => setBikes([]));
  }, []);
  useEffect(() => scan(), [scan]);

  useEffect(() => {
    setLineup(null);
    if (!picked) return;
    let alive = true;
    modelSwapLineup(picked)
      .then((b) => alive && setLineup(b))
      .catch(() => alive && setLineup([]));
    return () => {
      alive = false;
    };
  }, [picked]);

  return (
    <section className="flex min-w-0 flex-1 flex-col border-r border-border">
      <Head label={t("diagnose.swapTitle")}>
        <Button size="sm" variant="ghost" className="ml-auto" onClick={scan}>
          <RefreshCw className="size-3.5" />
        </Button>
      </Head>
      <p className="flex-none px-6 pb-3 text-[11.5px] leading-relaxed text-muted-foreground">
        {t("diagnose.swapDesc")}
      </p>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-5">
        {bikes === null ? (
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        ) : bikes.length === 0 ? (
          <p className="border border-dashed border-border px-4 py-8 text-center text-[12.5px] text-muted-foreground">
            {t("diagnose.swapEmpty")}
          </p>
        ) : (
          <ul className="border border-border bg-card text-[12px]">
            {bikes.map((b) => {
              const on = b.bike === picked;
              return (
                <li key={b.bike} className="border-b border-border/60 last:border-b-0">
                  <button
                    onClick={() => setPicked(on ? null : b.bike)}
                    className={cn(
                      "flex w-full cursor-default items-center justify-between gap-3 px-3 py-1.5 text-left",
                      on ? "bg-primary-tint" : "hover:text-foreground",
                    )}
                  >
                    <span className="truncate" title={b.bike}>
                      {bikeName(b.bike)}
                    </span>
                    <span className="flex-none text-[11px] text-muted-foreground">{b.active}</span>
                  </button>
                  {on && (
                    <div className="px-3 pb-2.5 pt-1.5 text-[11.5px]">
                      {lineup === null ? (
                        <Loader2 className="size-3.5 animate-spin text-muted-foreground" />
                      ) : lineup.length ? (
                        <>
                          <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
                            {t("diagnose.linesUp")}
                          </div>
                          <ul className="mt-1.5 flex flex-wrap gap-1.5">
                            {lineup.map((l) => (
                              <li
                                key={l}
                                title={l}
                                className="border border-border px-2 py-0.5 text-muted-foreground"
                              >
                                {bikeName(l)}
                              </li>
                            ))}
                          </ul>
                        </>
                      ) : (
                        <span className="text-muted-foreground">{t("diagnose.linesUpNone")}</span>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}

function LockedGuids() {
  const t = useT();
  // Reading a file's trailer needs the optional local module, same as locking does.
  const [available, setAvailable] = useState(false);
  const [items, setItems] = useState<LockItem[]>([]);
  const [roots, setRoots] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    contentLockAvailable()
      .then(setAvailable)
      .catch(() => {});
  }, []);

  const pick = async (directory: boolean) => {
    const picked = await openDialog({ multiple: true, directory });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (!paths.length) return;
    const all = [...new Set([...roots, ...paths])];
    setBusy(true);
    try {
      setItems(await contentLockPlan(all));
      setRoots(all);
    } catch (e) {
      toast.error(t("protect.planFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <Head label={t("diagnose.guidTitle")} />
      <p className="flex-none px-6 pb-3 text-[11.5px] leading-relaxed text-muted-foreground">
        {t(available ? "diagnose.guidDesc" : "diagnose.guidUnavailable")}
      </p>
      {available && (
        <>
          <div className="flex flex-none flex-wrap items-center gap-2 px-6 pb-3">
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void pick(false)}>
              <FileSearch className="size-3.5" /> {t("protect.addFiles")}
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
            {busy && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-5">
            {items.length > 0 && (
              <ul className="border border-border bg-card text-[12px]">
                {items.map((it) => (
                  <li
                    key={it.abs}
                    className={cn(
                      "flex items-center justify-between gap-3 border-b border-border/60 px-3 py-1.5 last:border-b-0",
                      !it.guid && "text-muted-foreground",
                    )}
                  >
                    <span className="truncate font-mono" title={it.abs}>
                      {it.rel}
                    </span>
                    <span className="flex-none select-text font-mono text-[11px]">
                      {it.guid
                        ? /^0+$/.test(it.guid)
                          ? t("protect.lockedToNobody")
                          : it.guid
                        : t("diagnose.notProtected")}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </>
      )}
    </section>
  );
}
