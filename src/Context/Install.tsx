import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AlertTriangle, X } from "lucide-react";
import { toast } from "sonner";
import {
  addToLibrary,
  importFile,
  onFrostmodReload,
  onInstallProgress,
  shopInstall,
  type ShopItem,
} from "../api/mods";
import type { InstallStage, ReloadOutcome } from "../types";
import { useT } from "../i18n/context";

/** Where the bytes come from — a resolvable host, a file the user picked, or a
 * purchased item from the authenticated MX Bikes Shop. */
export type InstallSource =
  | { kind: "download"; url: string; host: string }
  | { kind: "import"; path: string }
  | { kind: "shop"; item: ShopItem };

interface StartParams {
  slug: string;
  title: string;
  subpath: string;
  destFolder: string;
  /** Browse category the mod was opened under — lets a failed install jump back
   *  to its detail page with the right livery routing. */
  categoryId?: number;
  source: InstallSource;
}

/** What makes two install requests the same job: the same mod going to the same place.
 *  A repeat click (most often Retry on a failure that is still on screen) collapses onto
 *  the one already running; the same paint sent to a second bike does not. */
function installKey({ slug, subpath, destFolder }: StartParams): string {
  return JSON.stringify([slug, subpath, destFolder]);
}

/** Where a failed install can send the user back to. Shop installs have no
 *  browse page, so they get no target. */
export interface ModTarget {
  slug: string;
  subpath: string;
  categoryId?: number;
}

export interface ActiveInstall extends StartParams {
  stage: InstallStage;
  received?: number;
  total?: number;
  message?: string;
  frostmod: ReloadOutcome | null;
}

interface InstallContextValue {
  /** The single in-flight (or just-finished) install, or `null`. */
  active: ActiveInstall | null;
  /** Number of installs waiting behind the active one (bulk quick-install). */
  queueLength: number;
  startInstall: (
    p: Omit<StartParams, "source"> & { url: string; host: string },
  ) => void;
  startImport: (p: Omit<StartParams, "source"> & { path: string }) => void;
  /** Install a purchased MX Bikes Shop track. Defaults to the tracks root when the caller
   *  hasn't resolved a folder (see `resolveTrackDest`). */
  startShopInstall: (item: ShopItem, destFolder?: string) => void;
  /** Clear a finished (done/error) install card. */
  clear: () => void;
}

const InstallContext = createContext<InstallContextValue | null>(null);

export function InstallProvider({
  onInstalled,
  onOpenMod,
  children,
}: {
  onInstalled?: () => void;
  /** Navigate to a mod's detail page — used by the failure toast so a user who
   *  browsed away can get back to the error and retry. */
  onOpenMod?: (target: ModTarget) => void;
  children: ReactNode;
}) {
  const [active, setActive] = useState<ActiveInstall | null>(null);
  const [queueLength, setQueueLength] = useState(0);
  const onInstalledRef = useRef(onInstalled);
  onInstalledRef.current = onInstalled;
  const onOpenModRef = useRef(onOpenMod);
  onOpenModRef.current = onOpenMod;
  // Held in a ref, like the callbacks above, so `run` keeps its empty dep list —
  // switching language mid-transfer must not rebuild the installer.
  const t = useT();
  const tRef = useRef(t);
  tRef.current = t;
  const clearTimer = useRef<number | null>(null);
  // Installs run one at a time (the engine handles a single transfer); extra
  // requests wait in this queue and are drained sequentially.
  const queueRef = useRef<StartParams[]>([]);
  const runningRef = useRef(false);
  // What the queue is draining right now, so `enqueue` can turn a repeat request away.
  const activeKeyRef = useRef<string | null>(null);
  // `run`'s retry buttons enqueue rather than re-run, but `enqueue` is defined further
  // down (it needs `pump`, which needs `run`). A ref breaks the cycle without costing
  // `run` its empty dep list.
  const enqueueRef = useRef<((params: StartParams) => void) | null>(null);

  useEffect(
    () => () => {
      if (clearTimer.current) window.clearTimeout(clearTimer.current);
    },
    [],
  );

  const run = useCallback(async (params: StartParams) => {
    const { slug, title, subpath, destFolder, source } = params;
    if (clearTimer.current) window.clearTimeout(clearTimer.current);
    setActive({ ...params, stage: "resolving", frostmod: null });

    // FrostMod's reload event can land just before the install call resolves;
    // stash the outcome so the success toast can mention it.
    let frostOutcome: ReloadOutcome | null = null;

    const unlisten = await onInstallProgress((p) => {
      if (p.slug !== slug) return;
      setActive((cur) =>
        cur && cur.slug === slug
          ? {
              ...cur,
              stage: p.stage,
              received: p.received,
              total: p.total,
              message: p.message,
            }
          : cur,
      );
    });
    const unlistenFrost = await onFrostmodReload((p) => {
      if (p.slug !== slug) return;
      frostOutcome = p.outcome;
      setActive((cur) =>
        cur && cur.slug === slug ? { ...cur, frostmod: p.outcome } : cur,
      );
    });

    try {
      if (source.kind === "download") {
        await addToLibrary(slug, source.url, source.host, subpath, destFolder);
      } else if (source.kind === "shop") {
        await shopInstall(source.item, destFolder);
      } else {
        await importFile(source.path, subpath, destFolder);
      }
      setActive((cur) =>
        cur && cur.slug === slug ? { ...cur, stage: "done" } : cur,
      );
      onInstalledRef.current?.();
      toast.success(tRef.current("install.installed", { title }), {
        description:
          frostOutcome === "signaled"
            ? tRef.current("install.reloadedDesc")
            : tRef.current("install.addedDesc"),
      });
      // Auto-retire the sidebar/detail card a few seconds after success.
      clearTimer.current = window.setTimeout(() => {
        setActive((cur) =>
          cur && cur.slug === slug && cur.stage === "done" ? null : cur,
        );
      }, 5000);
    } catch (e) {
      const message = String(e);
      setActive((cur) =>
        cur && cur.slug === slug ? { ...cur, stage: "error", message } : cur,
      );
      // Shop items have no browse page, so their failure stays a plain toast.
      const target: ModTarget | null =
        source.kind === "shop"
          ? null
          : { slug, subpath, categoryId: params.categoryId };
      if (!target) {
        toast.error(tRef.current("install.failed", { title }), {
          description: message,
          duration: Infinity,
          action: {
            label: tRef.current("common.retry"),
            onClick: () => enqueueRef.current?.(params),
          },
        });
      } else {
        toast.custom(
          (id) => (
            <InstallFailedToast
              title={title}
              message={message}
              onOpen={() => {
                toast.dismiss(id);
                onOpenModRef.current?.(target);
              }}
              onRetry={() => {
                toast.dismiss(id);
                enqueueRef.current?.(params);
              }}
              onDismiss={() => toast.dismiss(id)}
            />
          ),
          { duration: Infinity },
        );
      }
    } finally {
      unlisten();
      unlistenFrost();
    }
  }, []);

  // Drain the queue sequentially — one install fully finishes before the next
  // starts, so `active` always reflects the single in-flight transfer.
  const pump = useCallback(async () => {
    if (runningRef.current) return;
    runningRef.current = true;
    try {
      while (queueRef.current.length) {
        const next = queueRef.current.shift()!;
        setQueueLength(queueRef.current.length);
        activeKeyRef.current = installKey(next);
        try {
          await run(next);
        } finally {
          activeKeyRef.current = null;
        }
      }
    } finally {
      runningRef.current = false;
    }
  }, [run]);

  const enqueue = useCallback(
    (params: StartParams) => {
      // Nothing gets installed twice at once. The retry on a failed install used to call
      // `run` straight out of the toast, so a second, impatient click started a *parallel*
      // run of the same download — two runs racing over one job, which is how an install
      // ended up copying a file the other one had already cleaned up.
      //
      // Keyed on the destination as well as the mod: installing one livery onto two
      // different bikes is two real installs and both belong in the queue.
      const key = installKey(params);
      if (activeKeyRef.current === key) return;
      if (queueRef.current.some((q) => installKey(q) === key)) return;
      queueRef.current.push(params);
      setQueueLength(queueRef.current.length);
      void pump();
    },
    [pump],
  );
  enqueueRef.current = enqueue;

  const startInstall: InstallContextValue["startInstall"] = useCallback(
    ({ url, host, ...rest }) =>
      enqueue({ ...rest, source: { kind: "download", url, host } }),
    [enqueue],
  );

  const startImport: InstallContextValue["startImport"] = useCallback(
    ({ path, ...rest }) =>
      enqueue({ ...rest, source: { kind: "import", path } }),
    [enqueue],
  );

  const startShopInstall: InstallContextValue["startShopInstall"] = useCallback(
    (item, destFolder = "") =>
      enqueue({
        slug: item.slug,
        title: item.title,
        subpath: "mods/tracks",
        destFolder,
        source: { kind: "shop", item },
      }),
    [enqueue],
  );

  const clear = useCallback(() => setActive(null), []);

  const value = useMemo(
    () => ({
      active,
      queueLength,
      startInstall,
      startImport,
      startShopInstall,
      clear,
    }),
    [active, queueLength, startInstall, startImport, startShopInstall, clear],
  );

  return (
    <InstallContext.Provider value={value}>{children}</InstallContext.Provider>
  );
}

/** Persistent failure banner. The whole card is clickable — it reopens the mod's
 *  page, where the error card and its retry/destination controls live. The
 *  surrounding toast still carries the shared card chrome (background, border,
 *  radius) from `Components/ui/sonner`; a custom toast only loses the padding and
 *  the built-in close button, so this supplies those. */
function InstallFailedToast({
  title,
  message,
  onOpen,
  onRetry,
  onDismiss,
}: {
  title: string;
  message: string;
  onOpen: () => void;
  onRetry: () => void;
  onDismiss: () => void;
}) {
  const t = useT();
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") onOpen();
      }}
      title={t("install.openModPage")}
      className="group flex w-full cursor-default flex-col gap-1 rounded-xl px-4 py-3 transition-colors hover:bg-foreground/[0.04]"
    >
      <div className="flex items-start gap-2">
        <AlertTriangle className="mt-px size-3.5 flex-none text-destructive" />
        <span className="flex-1 font-bold text-destructive">
          {t("install.failed", { title })}
        </span>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDismiss();
          }}
          aria-label={t("common.dismiss")}
          className="-mr-1 -mt-0.5 flex-none cursor-default rounded-full p-0.5 text-muted-foreground opacity-0 transition-opacity hover:text-foreground group-hover:opacity-100"
        >
          <X className="size-3.5" />
        </button>
      </div>
      <span className="line-clamp-3 pl-[22px] text-[11.5px] text-muted-foreground">
        {message}
      </span>
      <div className="mt-1.5 flex items-center justify-between gap-2 pl-[22px]">
        <span className="text-[11px] text-muted-foreground">
          {t("install.clickToOpen")}
        </span>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRetry();
          }}
          className="flex-none cursor-default rounded-md bg-primary px-2 py-1 text-[11.5px] font-semibold text-primary-foreground transition-[filter] hover:brightness-110"
        >
          {t("common.retry")}
        </button>
      </div>
    </div>
  );
}

export function useInstall() {
  const ctx = useContext(InstallContext);
  if (!ctx) throw new Error("useInstall must be used within InstallProvider");
  return ctx;
}
