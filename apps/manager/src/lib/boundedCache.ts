/** A tiny insertion-order LRU with both entry and retained-byte ceilings. */
export class BoundedCache<V> {
  private readonly rows = new Map<string, { value: V; bytes: number; lineage: string }>();
  private retained = 0;

  constructor(
    private readonly maxEntries: number,
    private readonly maxBytes: number,
  ) {}

  get(key: string): V | undefined {
    const row = this.rows.get(key);
    if (!row) return undefined;
    this.rows.delete(key);
    this.rows.set(key, row);
    return row.value;
  }

  set(key: string, lineage: string, value: V, bytes: number): void {
    for (const [held, row] of this.rows) {
      if (held !== key && row.lineage === lineage) this.delete(held);
    }
    this.delete(key);
    this.rows.set(key, { value, bytes, lineage });
    this.retained += bytes;
    while (this.rows.size > this.maxEntries || this.retained > this.maxBytes) {
      const oldest = this.rows.keys().next().value;
      if (oldest === undefined) break;
      this.delete(oldest);
    }
  }

  has(key: string): boolean {
    return this.rows.has(key);
  }

  delete(key: string): void {
    const row = this.rows.get(key);
    if (!row) return;
    this.retained -= row.bytes;
    this.rows.delete(key);
  }

  entries(): Array<[string, V]> {
    return Array.from(this.rows, ([key, row]) => [key, row.value]);
  }

  get size(): number {
    return this.rows.size;
  }

  get bytes(): number {
    return this.retained;
  }
}
