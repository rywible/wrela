import type { StudioController } from "./controller";

/** Clip-local scrubbing is serialized so obsolete slider requests cannot enqueue playback. */
export class DomainMotionPreview {
  private revision = 0;
  private pending: Promise<void> = Promise.resolve();
  constructor(private readonly studio: Pick<StudioController, "seek" | "host" | "authoring" | "patch">) {}

  cancel(): void {
    this.revision++;
  }

  private enqueue(target: string, work: (current: () => boolean) => Promise<void>): Promise<void> {
    const revision = ++this.revision;
    const current = () =>
      revision === this.revision && this.studio.authoring.getSnapshot().selection === target;
    this.pending = this.pending
      .catch(() => {})
      .then(async () => {
        if (current()) await work(current);
      });
    return this.pending;
  }

  seek(target: string, motion: string, seconds: number, playing = false): Promise<void> {
    if (!Number.isFinite(seconds) || seconds < 0 || seconds > 120)
      return Promise.reject(new RangeError("Clip preview time must be between 0 and 120 seconds"));
    return this.enqueue(target, async (current) => {
      await this.studio.seek(0);
      if (!current()) return;
      if (!this.studio.host) throw new Error("Prepare a character preview before scrubbing its motion");
      this.studio.host.playMotion(target, motion, 0);
      // Runtime controls take effect on the next authoritative 60 Hz tick.
      await this.studio.seek(Math.min(120, seconds + 1 / 60));
      if (current()) this.studio.patch({ playing });
    });
  }

  /** Audition the authored transition through fixed-step replay, including constraints and events. */
  transition(
    target: string,
    from: string,
    to: string,
    at: number,
    elapsed: number,
    playing = false,
  ): Promise<void> {
    if (![at, elapsed].every(Number.isFinite) || at < 0 || elapsed < 0 || at + elapsed + 2 / 60 > 120)
      return Promise.reject(new RangeError("Transition preview must fit within 120 seconds"));
    if (from === to) return Promise.reject(new Error("Choose a different destination clip"));
    return this.enqueue(target, async (current) => {
      await this.studio.seek(0);
      if (!current()) return;
      if (!this.studio.host) throw new Error("Prepare a character preview before auditioning a transition");
      this.studio.host.playMotion(target, from, 0);
      await this.studio.seek(at + 1 / 60);
      if (!current()) return;
      this.studio.host.playMotion(target, to);
      await this.studio.seek(at + elapsed + 2 / 60);
      if (current()) this.studio.patch({ playing });
    });
  }
}
