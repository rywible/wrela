/** Optional optimizations compile only when requested. Callers retain a correct
 * general pipeline until a specialization is ready. Device replacement cancels
 * publication of old results, even though WebGPU compilation cannot be aborted. */
export class PipelineVariants<K, V> {
  private entries = new Map<
    K,
    { create: () => Promise<V>; started: boolean; value?: V; pending?: Promise<void> }
  >();
  constructor(private readonly report: (error: unknown) => void) {}
  define(key: K, create: () => Promise<V>) {
    this.entries.set(key, { create, started: false });
  }
  get(key: K): V | undefined {
    const entry = this.entries.get(key);
    if (!entry) return undefined;
    if (!entry.started) {
      entry.started = true;
      entry.pending = Promise.resolve().then(async () => {
        if (this.entries.get(key) !== entry) return;
        try {
          const value = await entry.create();
          if (this.entries.get(key) === entry) entry.value = value;
        } catch (error) {
          if (this.entries.get(key) === entry) this.report(error);
        }
      });
    }
    return entry.value;
  }
  /** Wait only for requested variants; do not compile unused combinations. */
  async settle() {
    await Promise.all([...this.entries.values()].map((entry) => entry.pending));
  }
  clear() {
    this.entries.clear();
  }
}
