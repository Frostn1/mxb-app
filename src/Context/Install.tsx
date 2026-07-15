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
  source: InstallSource;
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
  /** Install a purchased MX Bikes Shop track (to the tracks root). */
  startShopInstall: (item: ShopItem) => void;
  /** Clear a finished (done/error) install card. */
  clear: () => void;
}

const InstallContext = createContext<InstallContextValue | null>(null);

export function InstallProvider({
  onInstalled,
  children,
}: {
  onInstalled?: () => void;
  children: ReactNode;
}) {
  const [active, setActive] = useState<ActiveInstall | null>(null);
  const [queueLength, setQueueLength] = useState(0);
  const onInstalledRef = useRef(onInstalled);
  onInstalledRef.current = onInstalled;
  const clearTimer = useRef<number | null>(null);
  // Installs run one at a time (the engine handles a single transfer); extra
  // requests wait in this queue and are drained sequentially.
  const queueRef = useRef<StartParams[]>([]);
  const runningRef = useRef(false);

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
      toast.success(`${title} installed`, {
        description:
          frostOutcome === "signaled"
            ? "Game reloaded via FrostMod — it's live now."
            : "Added to your library.",
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
      toast.error(`Install failed — ${title}`, {
        description: message,
        duration: Infinity,
        action: { label: "Retry", onClick: () => void run(params) },
      });
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
        await run(next);
      }
    } finally {
      runningRef.current = false;
    }
  }, [run]);

  const enqueue = useCallback(
    (params: StartParams) => {
      queueRef.current.push(params);
      setQueueLength(queueRef.current.length);
      void pump();
    },
    [pump],
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
    (item) =>
      enqueue({
        slug: item.slug,
        title: item.title,
        subpath: "mods/tracks",
        destFolder: "",
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

export function useInstall() {
  const ctx = useContext(InstallContext);
  if (!ctx) throw new Error("useInstall must be used within InstallProvider");
  return ctx;
}
