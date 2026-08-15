import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  clearDownloadHistory,
  downloadHistory,
  forgetDownload,
  recordDownload,
} from "../api/mods";
import type { DownloadRecord, NewDownload } from "../types";

/** When the Downloads page was last looked at, so the sidebar can badge failures that
 *  happened since. Local to the machine the downloads happened on, hence localStorage. */
const SEEN_KEY = "frost-downloads-seen";

function readSeen(): number {
  const raw = Number(localStorage.getItem(SEEN_KEY));
  return Number.isFinite(raw) ? raw : 0;
}

interface DownloadsContextValue {
  /** Newest first. */
  records: DownloadRecord[];
  loading: boolean;
  refresh: () => void;
  /** Note a finished download — installed or failed. Fire-and-forget by design: a history
   *  write must never turn a good install into a failed one. */
  note: (entry: NewDownload) => void;
  forget: (id: string) => void;
  clear: () => void;
  /** Failed downloads since the page was last opened. Failures used to vanish with their
   *  toast, so this is what makes one noticeable at all. */
  unseenFailures: number;
  markSeen: () => void;
}

const DownloadsContext = createContext<DownloadsContextValue | null>(null);

/**
 * The download history, held in one place.
 *
 * Two things write to it — [`Install`](./Install.tsx) for anything fetched from the site or
 * the shop, and [`DropReview`](./DropReview.tsx) for files imported or dragged in — and two
 * read it: the Downloads page and the sidebar's failure badge. A shared provider keeps that
 * to one copy of the list instead of each corner fetching its own.
 */
export function DownloadsProvider({ children }: { children: ReactNode }) {
  const [records, setRecords] = useState<DownloadRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [seen, setSeen] = useState(readSeen);

  const refresh = useCallback(() => {
    setLoading(true);
    downloadHistory()
      .then(setRecords)
      .catch(() => setRecords([]))
      .finally(() => setLoading(false));
  }, []);

  useEffect(refresh, [refresh]);

  const note = useCallback((entry: NewDownload) => {
    void recordDownload(entry)
      .then((saved) => {
        // Prepend what the backend actually stored rather than re-reading the file: it owns
        // the id and the timestamp, and this keeps a bulk install to one write each.
        if (saved) setRecords((cur) => [saved, ...cur]);
      })
      .catch(() => {});
  }, []);

  const forget = useCallback((id: string) => {
    setRecords((cur) => cur.filter((r) => r.id !== id));
    void forgetDownload(id).catch(() => {});
  }, []);

  const clear = useCallback(() => {
    setRecords([]);
    void clearDownloadHistory().catch(() => {});
  }, []);

  const markSeen = useCallback(() => {
    const now = Date.now();
    localStorage.setItem(SEEN_KEY, String(now));
    setSeen(now);
  }, []);

  const unseenFailures = useMemo(
    () => records.filter((r) => r.status === "failed" && r.at > seen).length,
    [records, seen],
  );

  const value = useMemo(
    () => ({ records, loading, refresh, note, forget, clear, unseenFailures, markSeen }),
    [records, loading, refresh, note, forget, clear, unseenFailures, markSeen],
  );

  return (
    <DownloadsContext.Provider value={value}>{children}</DownloadsContext.Provider>
  );
}

export function useDownloads() {
  const ctx = useContext(DownloadsContext);
  if (!ctx) throw new Error("useDownloads must be used within DownloadsProvider");
  return ctx;
}
