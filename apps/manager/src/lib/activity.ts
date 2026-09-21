export type ActivityStatus = "info" | "success" | "error";

export interface ActivityRecord {
  id: string;
  at: number;
  title: string;
  detail?: string;
  status: ActivityStatus;
}

export interface NewActivityRecord {
  title: string;
  detail?: string;
  status?: ActivityStatus;
}

const STORAGE_KEY = "mxb.activity.v1";
const MAX_RECORDS = 100;
const listeners = new Set<() => void>();

function isActivityRecord(value: unknown): value is ActivityRecord {
  if (!value || typeof value !== "object") return false;
  const record = value as Partial<ActivityRecord>;
  return (
    typeof record.id === "string" &&
    typeof record.at === "number" &&
    Number.isFinite(record.at) &&
    typeof record.title === "string" &&
    (record.detail === undefined || typeof record.detail === "string") &&
    (record.status === "info" || record.status === "success" || record.status === "error")
  );
}

function notifyListeners() {
  listeners.forEach((listener) => listener());
}

function makeId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/** Read the app's local activity history. Invalid or older data is ignored safely. */
export function readActivityRecords(): ActivityRecord[] {
  if (typeof window === "undefined") return [];
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value
      .filter(isActivityRecord)
      .sort((a, b) => b.at - a.at)
      .slice(0, MAX_RECORDS);
  } catch {
    return [];
  }
}

/**
 * Record an automatic/system action shown on the Activity screen.
 *
 * This is intentionally separate from DownloadRecord: integration choices, verification,
 * and other app actions are not downloads and should never be made to look like one.
 */
export function recordActivity(input: NewActivityRecord): ActivityRecord {
  const record: ActivityRecord = {
    id: makeId(),
    at: Date.now(),
    title: input.title.trim(),
    detail: input.detail?.trim() || undefined,
    status: input.status ?? "info",
  };
  if (typeof window === "undefined") return record;

  try {
    const next = [record, ...readActivityRecords()].slice(0, MAX_RECORDS);
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    notifyListeners();
  } catch {
    // Activity is an audit aid, not part of the action itself. Storage failures must not make
    // a successful install or user choice fail.
  }
  return record;
}

export function clearActivityRecords() {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(STORAGE_KEY);
    notifyListeners();
  } catch {
    // localStorage can be unavailable in restricted webviews.
  }
}

/** Subscribe to activity changes made in this window or another app window/tab. */
export function subscribeToActivity(listener: () => void): () => void {
  listeners.add(listener);
  if (typeof window === "undefined") return () => listeners.delete(listener);

  const onStorage = (event: StorageEvent) => {
    if (event.key === STORAGE_KEY) listener();
  };
  window.addEventListener("storage", onStorage);
  return () => {
    listeners.delete(listener);
    window.removeEventListener("storage", onStorage);
  };
}
