/** One shared, non-overlapping async poll for any number of subscribers. */
export class AsyncPollingStore<T> {
  private readonly listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private pending: Promise<void> | undefined;
  private generation = 0;

  constructor(
    private value: T,
    private readonly fallback: T,
    private readonly everyMs: number,
    private readonly probe: () => Promise<T>,
  ) {}

  getSnapshot = (): T => this.value;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    if (this.listeners.size === 1) {
      this.generation += 1;
      void this.cycle(this.generation);
    }
    return () => {
      this.listeners.delete(listener);
      if (this.listeners.size === 0) {
        this.generation += 1;
        if (this.timer !== undefined) {
          clearTimeout(this.timer);
          this.timer = undefined;
        }
      }
    };
  };

  refresh = (): Promise<void> => {
    if (this.pending) return this.pending;
    this.pending = this.probe()
      .catch(() => this.fallback)
      .then((next) => {
        if (Object.is(next, this.value)) return;
        this.value = next;
        for (const listener of this.listeners) listener();
      })
      .finally(() => {
        this.pending = undefined;
      });
    return this.pending;
  };

  private async cycle(generation: number): Promise<void> {
    await this.refresh();
    if (this.listeners.size === 0 || generation !== this.generation) return;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.cycle(generation);
    }, this.everyMs);
  }
}
