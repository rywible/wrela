/** Byte-bounded immutable CPU products. Workers clone published artifacts before
 * transferring their buffers, so a consumer cannot detach cached products. */
export class ProductCache<T> {
  private entries = new Map<string, { value: T; bytes: number }>();
  private bytes = 0;
  hits = 0;
  misses = 0;
  constructor(
    readonly maxBytes: number,
    readonly maxEntries = 32,
  ) {}
  get(key: string): T | undefined {
    const entry = this.entries.get(key);
    if (!entry) {
      this.misses++;
      return undefined;
    }
    this.hits++;
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.value;
  }
  set(key: string, value: T, bytes: number) {
    const previous = this.entries.get(key);
    if (previous) {
      this.bytes -= previous.bytes;
      this.entries.delete(key);
    }
    if (bytes > this.maxBytes) return;
    this.entries.set(key, { value, bytes });
    this.bytes += bytes;
    while (this.bytes > this.maxBytes || this.entries.size > this.maxEntries) {
      const oldest = this.entries.entries().next().value;
      if (!oldest) break;
      this.entries.delete(oldest[0]);
      this.bytes -= oldest[1].bytes;
    }
  }
  get metrics() {
    return { entries: this.entries.size, bytes: this.bytes, hits: this.hits, misses: this.misses };
  }
  clear() {
    this.entries.clear();
    this.bytes = 0;
    this.hits = 0;
    this.misses = 0;
  }
}
