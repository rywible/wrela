export type JobState = "queued" | "generating" | "prepared" | "failed";
type Job<T> = {
  key: string;
  priority: number;
  run: (signal: AbortSignal) => Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
  state: JobState;
  controller: AbortController;
  timeoutMs: number;
  waitFor?: Job<T>;
  cancel?: (reason: Error, retry?: boolean) => void;
};
/** Bounded logical work plus a bounded quarantine for generators that ignore abort.
 * A timed-out generator never publishes and cannot admit unlimited replacement work. */
export class BoundedScheduler<T> {
  private queue: Job<T>[] = [];
  private running = new Set<Job<T>>();
  private quarantined = 0;
  private jobs = new Map<string, { promise: Promise<T>; job: Job<T> }>();
  private closed = false;
  constructor(
    readonly concurrency = 2,
    readonly maxQueued = 192,
    readonly timeoutMs = 10000,
  ) {
    if (
      !Number.isInteger(concurrency) ||
      concurrency < 1 ||
      concurrency > 8 ||
      !Number.isInteger(maxQueued) ||
      maxQueued < 1 ||
      maxQueued > 1024 ||
      !Number.isFinite(timeoutMs) ||
      timeoutMs <= 0
    )
      throw new RangeError("Invalid scheduler limits");
  }
  get metrics() {
    return { queued: this.queue.length, running: this.running.size, quarantined: this.quarantined };
  }
  request(
    key: string,
    priority: number,
    run: (signal: AbortSignal) => Promise<T>,
    options: { urgent?: boolean } = {},
  ): Promise<T> {
    if (this.closed) return Promise.reject(new Error("Scheduler disposed"));
    const known = this.jobs.get(key);
    if (known) {
      known.job.priority = Math.max(known.job.priority, priority);
      this.sort();
      if (options.urgent && !known.job.waitFor) this.admitUrgent(priority);
      return known.promise;
    }
    if (this.queue.length >= this.maxQueued) return Promise.reject(new Error("Generation queue full"));
    let job!: Job<T>;
    const promise = new Promise<T>((resolve, reject) => {
      job = {
        key,
        priority,
        run,
        resolve,
        reject,
        state: "queued",
        controller: new AbortController(),
        timeoutMs: this.timeoutMs,
      };
      this.queue.push(job);
    });
    this.jobs.set(key, { promise, job });
    this.sort();
    if (options.urgent) this.admitUrgent(priority);
    this.pump();
    return promise;
  }
  private admitUrgent(priority: number) {
    if (
      this.running.size < this.concurrency ||
      this.quarantined >= this.concurrency ||
      this.queue.length >= this.maxQueued
    )
      return;
    const optional = [...this.running]
      .filter((job) => job.priority < priority)
      .sort((a, b) => a.priority - b.priority)[0];
    optional?.cancel?.(new Error("Yielded generation capacity to collision-critical work"), true);
  }
  private sort() {
    this.queue.sort((a, b) => b.priority - a.priority || a.key.localeCompare(b.key));
  }
  private remove(job: Job<T>) {
    if (this.jobs.get(job.key)?.job === job) this.jobs.delete(job.key);
  }
  private pump() {
    if (this.quarantined >= this.concurrency * 2 && !this.running.size) {
      for (const job of this.queue.splice(0)) {
        this.remove(job);
        job.reject(new Error("Generation capacity exhausted by unresponsive jobs"));
      }
      return;
    }
    while (
      !this.closed &&
      this.running.size < this.concurrency &&
      this.running.size + this.quarantined < this.concurrency * 2 &&
      this.queue.length
    ) {
      const next = this.queue.findIndex((queued) => !queued.waitFor);
      if (next < 0) break;
      const [job] = this.queue.splice(next, 1);
      this.running.add(job);
      job.state = "generating";
      let settled = false;
      const timer = setTimeout(
        () => job.cancel?.(new Error(`Generation timed out: ${job.key}`)),
        job.timeoutMs,
      );
      job.cancel = (reason, retry = false) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.running.delete(job);
        const entry = this.jobs.get(job.key);
        this.remove(job);
        if (retry && entry) {
          const replacement: Job<T> = {
            ...job,
            controller: new AbortController(),
            state: "queued",
            waitFor: job,
            cancel: undefined,
          };
          this.jobs.set(job.key, { promise: entry.promise, job: replacement });
          this.queue.push(replacement);
          this.sort();
        }
        this.quarantined++;
        job.state = "failed";
        job.controller.abort(reason);
        if (!retry) job.reject(reason);
        this.pump();
      };
      Promise.resolve()
        .then(() => job.run(job.controller.signal))
        .then(
          (result) => {
            if (settled) return;
            settled = true;
            job.state = "prepared";
            job.resolve(result);
          },
          (error) => {
            if (settled) return;
            settled = true;
            job.state = "failed";
            job.reject(error);
          },
        )
        .finally(() => {
          clearTimeout(timer);
          if (this.running.delete(job)) this.remove(job);
          else this.quarantined--;
          for (const queued of this.queue) if (queued.waitFor === job) queued.waitFor = undefined;
          this.pump();
        });
    }
  }
  cancelExcept(keys: Set<string>) {
    this.queue = this.queue.filter((job) => {
      if (keys.has(job.key)) return true;
      this.remove(job);
      job.reject(new Error("Superseded generation"));
      return false;
    });
    for (const job of [...this.running])
      if (!keys.has(job.key)) job.cancel?.(new Error("Superseded generation"));
  }
  dispose() {
    this.closed = true;
    this.cancelExcept(new Set());
  }
}
