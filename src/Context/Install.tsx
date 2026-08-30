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
  cancelInstall,
  importFile,
  onFrostmodReload,
  onInstallProgress,
  shopInstall,
  type ShopItem,
} from "../api/mods";
import { hubInstall } from "../api/hub";
import type { DownloadSource, InstallStage, ReloadOutcome } from "../types";
import { useDownloads } from "./Downloads";
import { useDropReview } from "./DropReview";
import { useT } from "../i18n/context";

/** Where the bytes come from — a resolvable host, a file the user picked, or something already
 *  bought on one of the two stores.
 *
 *  The two stores are separate kinds rather than one with a flag because they are downloaded by
 *  entirely different means: a shop purchase goes through a WebView (Cloudflare refuses our
 *  HTTP client on that store), while an MXB Hub purchase streams over the same client every
 *  other download uses. Nothing above this line has to know that, but the branch below does. */
export type InstallSource =
  | { kind: "download"; url: string; host: string }
  | { kind: "import"; path: string }
  | { kind: "shop"; item: ShopItem }
  | { kind: "hub"; item: ShopItem };

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

/** The same identity for a job whose destination isn't known yet. Enough to turn away a second
 *  click on the same card; the real key takes over once it resolves. */
function pendingKey({ slug, subpath }: PendingInstall): string {
  return JSON.stringify([slug, subpath, null]);
}

/** What a pending install comes back with: an ordinary download, worked out late. */
export type ResolvedInstall = Omit<StartParams, "source"> & {
  url: string;
  host: string;
};

/**
 * An install the user has asked for whose download link and destination aren't known yet.
 *
 * The Browse grid's quick install has to fetch the mod's page to work both out, and that used
 * to happen before the queue heard about the mod at all — so a bulk selection trickled into the
 * panel one page-fetch at a time, with the rest of it nowhere on screen. Resolving is a stage of
 * the queue instead: the row appears on click, and the page is fetched when its turn comes.
 */
export interface PendingInstall {
  slug: string;
  title: string;
  /** The mod type's install subpath — known on click, and all `pendingKey` needs. */
  subpath: string;
  /** `null` when there is nothing installable. The caller reports that itself: only it knows
   *  whether the host is blocked, the build is server-only, or the page offers no file. */
  resolve: () => Promise<ResolvedInstall | null>;
}

/** How the download history names this source. */
function historySource(source: InstallSource): DownloadSource {
  if (source.kind === "shop") return "shop";
  if (source.kind === "hub") return "hub";
  return source.kind === "download" ? "site" : "file";
}

/** Where a failed install can send the user back to. */
export interface ModTarget {
  slug: string;
  subpath: string;
  categoryId?: number;
}

export interface ActiveInstall extends StartParams {
  /** Identity in the queue panel — the same key `enqueue` dedupes on. */
  key: string;
  stage: InstallStage;
  received?: number;
  total?: number;
  message?: string;
  frostmod: ReloadOutcome | null;
  /** The user asked to stop this one and the backend hasn't unwound yet. Deliberately not an
   *  `InstallStage`: that union mirrors the stages Rust actually emits. */
  cancelling?: boolean;
}

/** One install waiting its turn, as the queue panel needs to show it. */
export interface QueuedInstall {
  key: string;
  slug: string;
  title: string;
  kind: InstallSource["kind"];
  /** Its download link and destination are being looked up right now: it's next up, but there
   *  is nothing to transfer yet. */
  preparing?: boolean;
}

/** A slot in the queue — a job ready to run, or one still to be resolved. */
interface QueueItem {
  key: string;
  slug: string;
  title: string;
  kind: InstallSource["kind"];
  /** `null` until `resolve` has answered. */
  params: StartParams | null;
  resolve?: PendingInstall["resolve"];
  preparing?: boolean;
  /** `resolve` already in flight, started before this item's turn came. See `prefetch`. */
  resolving?: Promise<ResolvedInstall | null>;
}

/**
 * How many installs transfer at once.
 *
 * Not about multiplying throughput: a single MediaFire connection measured at 25–34 MB/s,
 * more than a typical home line carries, and splitting one file across parallel range
 * requests bought 1.00x at eight lanes. What this buys is the line not sitting idle while a
 * mod resolves its link or unpacks — a second or three per mod, which is all there is to win.
 * Two lanes cover that; more would only divide the same pipe further and make the first mod
 * in a batch land later.
 */
const MAX_CONCURRENT = 2;

/** How far ahead of the running jobs to resolve download links. See `prefetch`. */
const PREFETCH_AHEAD = 2;

interface InstallContextValue {
  /** Everything in flight or just finished, oldest first. */
  active: ActiveInstall[];
  /** The in-flight install for one mod, which is what its own page shows. At most one, since
   *  the queue never runs the same slug twice at once. */
  activeFor: (slug: string) => ActiveInstall | null;
  /** Everything waiting behind the running ones, in the order it will run. */
  queued: QueuedInstall[];
  /** Number of installs still waiting (bulk quick-install). */
  queueLength: number;
  /** Drop an install by `key`: a queued one never starts, the active one is stopped mid-transfer.
   *  A no-op once the bytes are down — extraction and placement can't be interrupted safely. */
  cancel: (key: string) => void;
  startInstall: (
    p: Omit<StartParams, "source"> & { url: string; host: string },
  ) => void;
  startImport: (p: Omit<StartParams, "source"> & { path: string }) => void;
  /** Queue a mod whose download link and destination still have to be looked up — it takes its
   *  place in the panel straight away and resolves when the queue reaches it. */
  startPendingInstall: (p: PendingInstall) => void;
  /** A purchase from the shop, queued exactly like any other install. */
  startShopInstall: (p: Omit<StartParams, "source"> & { item: ShopItem }) => void;
  startHubInstall: (p: Omit<StartParams, "source"> & { item: ShopItem }) => void;
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
  const [active, setActive] = useState<ActiveInstall[]>([]);
  // Mirrors `queueRef` for rendering. The ref stays the source of truth — `pump` shifts off it
  // synchronously — but a ref is invisible to React, so the panel reads this copy.
  const [queued, setQueued] = useState<QueuedInstall[]>([]);
  const onInstalledRef = useRef(onInstalled);
  onInstalledRef.current = onInstalled;
  const onOpenModRef = useRef(onOpenMod);
  onOpenModRef.current = onOpenMod;
  // Held in a ref, like the callbacks above, so `run` keeps its empty dep list —
  // switching language mid-transfer must not rebuild the installer.
  const t = useT();
  const tRef = useRef(t);
  tRef.current = t;
  // Same reason: the history is written from inside `run`, which must not be rebuilt.
  const { note } = useDownloads();
  // A download that turns out to be a pack is handed over rather than installed. Held in a
  // ref for the same reason as the rest: `run` keeps its empty dep list.
  const { reviewPlan } = useDropReview();
  const reviewRef = useRef(reviewPlan);
  reviewRef.current = reviewPlan;
  const noteRef = useRef(note);
  noteRef.current = note;
  const clearTimers = useRef<Map<string, number>>(new Map());
  // Waiting installs, in the order they will run.
  const queueRef = useRef<QueueItem[]>([]);
  // How many `pump` loops are draining the queue — one per lane, up to `MAX_CONCURRENT`.
  const lanesRef = useRef(0);
  // What the queue is running right now, so `enqueue` can turn a repeat request away.
  const activeKeysRef = useRef<Set<string>>(new Set());
  // The slugs those jobs are under. The backend keys both cancellation and progress by slug
  // (see `cancel.rs`), so two jobs sharing one must never overlap — `pump` skips past them.
  const activeSlugsRef = useRef<Set<string>>(new Set());
  // Each running job in full, so `cancel` has its slug without depending on `active` state.
  const runningParamsRef = useRef<Map<string, StartParams>>(new Map());
  // Keys the user asked to cancel, so `run` can tell a stop it was asked for from a genuine
  // failure and skip the red toast.
  const cancelledKeysRef = useRef<Set<string>>(new Set());
  // `run`'s retry buttons enqueue rather than re-run, but `enqueue` is defined further
  // down (it needs `pump`, which needs `run`). A ref breaks the cycle without costing
  // `run` its empty dep list.
  const enqueueRef = useRef<((params: StartParams) => void) | null>(null);

  useEffect(() => {
    const timers = clearTimers.current;
    return () => timers.forEach((id) => window.clearTimeout(id));
  }, []);

  /** Update one running install in place, by key. */
  const patch = useCallback(
    (key: string, f: (cur: ActiveInstall) => ActiveInstall) =>
      setActive((cur) => cur.map((it) => (it.key === key ? f(it) : it))),
    [],
  );

  /** Republish the pending queue for rendering. Called wherever `queueRef` moves. */
  const syncQueue = useCallback(() => {
    setQueued(
      queueRef.current.map((it) => ({
        key: it.key,
        slug: it.slug,
        title: it.title,
        kind: it.kind,
        preparing: it.preparing,
      })),
    );
  }, []);

  const run = useCallback(async (params: StartParams) => {
    const { slug, title, subpath, destFolder, source } = params;
    const key = installKey(params);
    const pending = clearTimers.current.get(key);
    if (pending !== undefined) {
      window.clearTimeout(pending);
      clearTimers.current.delete(key);
    }
    // Replace a finished card for the same job (a retry) rather than stacking a second one.
    setActive((cur) => [
      ...cur.filter((it) => it.key !== key),
      { ...params, key, stage: "resolving" as InstallStage, frostmod: null },
    ]);

    // FrostMod's reload event can land just before the install call resolves;
    // stash the outcome so the success toast can mention it.
    let frostOutcome: ReloadOutcome | null = null;
    // The only place the transfer size is known — nothing on disk afterwards tells you how
    // big the download was, and a failed one leaves nothing at all.
    let bytes: number | null = null;

    // Matched on slug because that is all the backend's event carries. Safe with several
    // installs in flight only because `pump` refuses to run one slug twice at once.
    const unlisten = await onInstallProgress((p) => {
      if (p.slug !== slug) return;
      if (p.total) bytes = p.total;
      patch(key, (cur) => ({
        ...cur,
        stage: p.stage,
        received: p.received,
        total: p.total,
        message: p.message,
      }));
    });
    const unlistenFrost = await onFrostmodReload((p) => {
      if (p.slug !== slug) return;
      frostOutcome = p.outcome;
      patch(key, (cur) => ({ ...cur, frostmod: p.outcome }));
    });

    /** One record per finished attempt, whichever way it went. */
    const remember = (status: "installed" | "failed", error: string | null) =>
      noteRef.current({
        title,
        slug,
        subpath,
        destFolder,
        categoryId: params.categoryId ?? null,
        source: historySource(source),
        host: source.kind === "download" ? source.host : null,
        url: source.kind === "download" ? source.url : null,
        bytes,
        status,
        error,
      });

    try {
      if (source.kind === "download") {
        const pack = await addToLibrary(slug, source.url, source.host, subpath, destFolder);
        if (pack) {
          // Not one mod but several — a pack, and nothing has been written. The review sheet
          // owns the staged bytes from here, and records its own history rows when the user
          // commits, so this run is finished: the lane frees and the queue moves on. The card
          // stays up saying what is waiting, because the sheet may be behind another one.
          patch(key, (cur) => ({ ...cur, stage: "review" }));
          reviewRef.current(pack);
          return;
        }
      } else if (source.kind === "shop") {
        await shopInstall(source.item, subpath, destFolder);
      } else if (source.kind === "hub") {
        await hubInstall(source.item, subpath, destFolder);
      } else {
        await importFile(source.path, subpath, destFolder);
      }
      patch(key, (cur) => ({ ...cur, stage: "done" }));
      remember("installed", null);
      onInstalledRef.current?.();
      toast.success(tRef.current("install.installed", { title }), {
        description:
          frostOutcome === "signaled"
            ? tRef.current("install.reloadedDesc")
            : tRef.current("install.addedDesc"),
      });
      // Auto-retire the sidebar/detail card a few seconds after success.
      clearTimers.current.set(
        key,
        window.setTimeout(() => {
          clearTimers.current.delete(key);
          setActive((cur) =>
            cur.filter((it) => !(it.key === key && it.stage === "done")),
          );
        }, 5000),
      );
    } catch (e) {
      // A stop the user asked for isn't a failure: retire the card quietly rather than leaving
      // a red error and a Retry button behind. Note this only runs when the backend actually
      // unwound — an install that finished before the cancel landed took the success path above,
      // which is the honest outcome, since the mod really is installed.
      if (cancelledKeysRef.current.has(key)) {
        setActive((cur) => cur.filter((it) => it.key !== key));
        toast.info(tRef.current("install.cancelled", { title }));
        return;
      }
      const message = String(e);
      patch(key, (cur) => ({ ...cur, stage: "error", message }));
      remember("failed", message);
      // Retry goes through `enqueue`, never straight to `run`: a second impatient click used
      // to start a *parallel* run of the same job.
      const target: ModTarget = { slug, subpath, categoryId: params.categoryId };
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
    } finally {
      cancelledKeysRef.current.delete(key);
      unlisten();
      unlistenFrost();
    }
  }, [patch]);

  /**
   * Start resolving the next few pending jobs before their turn comes.
   *
   * A quick-installed mod arrives knowing only its slug; its download link and destination
   * come from fetching the mod's page, measured at roughly 900 ms. Doing that when the job
   * reaches the front means the connection sits idle for it, once per mod. Started here, it
   * overlaps whatever is transferring, and by the time a lane frees up the answer is waiting.
   *
   * Fire-and-forget by design: `pump` awaits the same promise, and a rejection is handled
   * there. The `catch` only stops an unhandled rejection while nothing is awaiting it yet.
   */
  const prefetch = useCallback(() => {
    let started = 0;
    for (const item of queueRef.current) {
      if (started >= PREFETCH_AHEAD) break;
      if (item.params || !item.resolve) continue;
      if (!item.resolving) {
        item.resolving = item.resolve();
        item.resolving.catch(() => {});
      }
      started += 1;
    }
  }, []);

  /**
   * Drain the queue, up to [`MAX_CONCURRENT`] jobs at once.
   *
   * Each call runs one lane and returns when it can find nothing more to do, so the number of
   * live `pump` calls is the number of transfers in flight. A job whose slug is already
   * running is stepped over rather than waited on — the backend keys cancellation and progress
   * by slug, so two jobs sharing one would report into each other.
   */
  const pump = useCallback(async () => {
    if (lanesRef.current >= MAX_CONCURRENT) return;
    lanesRef.current += 1;
    try {
      for (;;) {
        prefetch();
        // Not necessarily the head: skip anything whose slug is already in flight.
        const head = queueRef.current.find((q) => !activeSlugsRef.current.has(q.slug));
        if (!head) return;

        // Claimed before the first `await` so a second lane can't take the same job.
        activeSlugsRef.current.add(head.slug);
        let params = head.params;
        try {
          if (!params) {
            head.preparing = true;
            syncQueue();
            try {
              // Memoised onto the item, not just awaited: the job stays in the queue while it
              // resolves, so another lane's `prefetch` would otherwise see it as unresolved
              // and fetch the same mod page a second time.
              head.resolving ??= head.resolve!();
              const resolved = await head.resolving;
              if (resolved) {
                const { url, host, ...rest } = resolved;
                params = { ...rest, source: { kind: "download", url, host } };
              }
            } catch {
              // The caller reports its own failures; there's simply nothing to install.
              params = null;
            }
            head.preparing = false;
          }

          // Gone from the queue means cancelled while it resolved — drop it on the way back.
          const at = queueRef.current.indexOf(head);
          if (at < 0) continue;
          queueRef.current.splice(at, 1);
          syncQueue();
          if (!params) continue;

          const key = installKey(params);
          // A pending job only learns its destination here, so this is the first moment its
          // real key exists — and the first chance to see it is the same job as something
          // already queued or running.
          if (
            key !== head.key &&
            (activeKeysRef.current.has(key) ||
              queueRef.current.some((q) => q.key === key))
          ) {
            continue;
          }

          activeKeysRef.current.add(key);
          runningParamsRef.current.set(key, params);
          try {
            await run(params);
          } finally {
            activeKeysRef.current.delete(key);
            runningParamsRef.current.delete(key);
          }
        } finally {
          activeSlugsRef.current.delete(head.slug);
        }
      }
    } finally {
      lanesRef.current -= 1;
    }
  }, [prefetch, run, syncQueue]);

  /** Open as many lanes as there is work and headroom for. */
  const pumpAll = useCallback(() => {
    const want = Math.min(MAX_CONCURRENT, queueRef.current.length);
    for (let i = lanesRef.current; i < want; i += 1) void pump();
  }, [pump]);

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
      if (activeKeysRef.current.has(key)) return;
      if (queueRef.current.some((q) => q.key === key)) return;
      queueRef.current.push({
        key,
        slug: params.slug,
        title: params.title,
        kind: params.source.kind,
        params,
      });
      syncQueue();
      pumpAll();
    },
    [pumpAll, syncQueue],
  );
  enqueueRef.current = enqueue;

  const startPendingInstall = useCallback(
    (p: PendingInstall) => {
      const key = pendingKey(p);
      if (queueRef.current.some((q) => q.key === key)) return;
      // Always a site download: a purchase and a local file both know their source already.
      queueRef.current.push({
        key,
        slug: p.slug,
        title: p.title,
        kind: "download",
        params: null,
        resolve: p.resolve,
      });
      syncQueue();
      pumpAll();
    },
    [pumpAll, syncQueue],
  );

  const cancel = useCallback(
    (key: string) => {
      // Still waiting its turn: drop it here and the backend never hears about it at all. That
      // covers a row still being resolved — `pump` finds it missing and skips it.
      const at = queueRef.current.findIndex((q) => q.key === key);
      if (at >= 0) {
        queueRef.current.splice(at, 1);
        syncQueue();
        return;
      }
      const running = runningParamsRef.current.get(key);
      if (!running) return;
      cancelledKeysRef.current.add(key);
      patch(key, (cur) => ({ ...cur, cancelling: true }));
      // The install command rejects once the transfer loop notices, and `run`'s catch takes it
      // from there. Nothing to await: a failure here just means it stops the ordinary way.
      // Read off the ref rather than `active` so this callback doesn't churn on every progress
      // tick — the panel's buttons would rebuild 60 times a download.
      void cancelInstall(running.slug).catch(() => {});
    },
    [patch, syncQueue],
  );

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
    ({ item, ...rest }) => enqueue({ ...rest, source: { kind: "shop", item } }),
    [enqueue],
  );

  const startHubInstall: InstallContextValue["startHubInstall"] = useCallback(
    ({ item, ...rest }) => enqueue({ ...rest, source: { kind: "hub", item } }),
    [enqueue],
  );

  /** Retire the finished cards, leaving anything still transferring alone. */
  const clear = useCallback(
    () => setActive((cur) => cur.filter((it) => it.stage !== "done" && it.stage !== "error")),
    [],
  );

  const activeFor = useCallback(
    (slug: string) => active.find((it) => it.slug === slug) ?? null,
    [active],
  );

  const value = useMemo(
    () => ({
      active,
      activeFor,
      queued,
      queueLength: queued.length,
      cancel,
      startInstall,
      startImport,
      startHubInstall,
      startPendingInstall,
      startShopInstall,
      clear,
    }),
    [
      active,
      activeFor,
      queued,
      cancel,
      startInstall,
      startImport,
      startHubInstall,
      startPendingInstall,
      startShopInstall,
      clear,
    ],
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
