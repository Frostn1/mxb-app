type Entry<V> = {
  promise: Promise<V>;
  createdAt: number;
  pending: boolean;
};

/** Reuse an in-flight or recently started async read, while never caching a rejection. */
export class RecentPromiseCache<K, V> {
  private readonly entries = new Map<K, Entry<V>>();

  constructor(private readonly ttlMs: number) {}

  get(key: K, load: () => Promise<V>, now = Date.now()): Promise<V> {
    const found = this.entries.get(key);
    if (found && (found.pending || now - found.createdAt < this.ttlMs)) return found.promise;

    const promise = load();
    const entry = { promise, createdAt: now, pending: true };
    this.entries.set(key, entry);
    void promise.then(
      () => {
        entry.pending = false;
      },
      () => {
        if (this.entries.get(key) === entry) this.entries.delete(key);
      },
    );
    return promise;
  }

  clear(): void {
    this.entries.clear();
  }
}
