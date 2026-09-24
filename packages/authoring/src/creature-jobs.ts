import { type CharacterDefinition, canonical, contentKey, idSchema } from "@wrela/model";

import { z } from "zod";
import {
  type CreatureSolveProgress,
  type CreatureSolveResult,
  creatureSolveRequestSchema,
  solveCreatureEditSteps,
} from "./creature-solver";

export const creatureSolveJobRequestSchema = z.strictObject({
  request: creatureSolveRequestSchema,
  deadlineMs: z.number().finite().min(1).max(600000).default(10000),
  evaluationsPerSlice: z.number().int().min(1).max(8).default(1),
});
export type CreatureSolveJobRequest = z.input<typeof creatureSolveJobRequestSchema>;
export type CreatureSolveJobStatus =
  | "queued"
  | "running"
  | "paused"
  | "completed"
  | "cancelled"
  | "deadline-exceeded"
  | "stale"
  | "failed";
export type CreatureSolveJob = {
  id: string;
  status: CreatureSolveJobStatus;
  sourceRevision: number;
  sourceKey: string;
  createdAt: number;
  deadlineAt: number;
  finishedAt?: number;
  progress: CreatureSolveProgress;
  result?: CreatureSolveResult;
  candidateId?: string;
  error?: string;
};
export const CREATURE_JOB_LIMITS = Object.freeze({
  active: 4,
  retained: 16,
  sourceBytesPerJob: 4 * 1024 * 1024,
  activeSourceBytes: 32 * 1024 * 1024,
  retiredIdentities: 1024,
  evaluationsPerSlice: 8,
  millisecondsPerSlice: 8,
});
type Entry = {
  snapshot: CreatureSolveJob;
  fingerprint: string;
  request: z.output<typeof creatureSolveJobRequestSchema>;
  iterator?: ReturnType<typeof solveCreatureEditSteps>;
  sourceBytes: number;
  deadline: number;
  timer?: ReturnType<typeof setTimeout>;
  deadlineTimer?: ReturnType<typeof setTimeout>;
  promise: Promise<CreatureSolveJob>;
  resolve: (snapshot: CreatureSolveJob) => void;
};
const terminal = (status: CreatureSolveJobStatus) => !["queued", "running", "paused"].includes(status);
const clone = <T>(value: T): T => structuredClone(value);

/** Cooperative jobs hold an immutable source version. Timer boundaries occur
 * between real solver evaluations; cancellation never publishes partial edits. */
export class CreatureSolveJobs {
  private entries = new Map<string, Entry>();
  private retired = new Set<string>();
  private pumpTimer?: ReturnType<typeof setTimeout>;
  constructor(
    private readonly hooks: {
      changed: () => void;
      completed: (result: CreatureSolveResult) => string;
    },
  ) {}
  start(
    input: CreatureSolveJobRequest,
    readSource: () => { document: CharacterDefinition; sourceKey: string; sourceRevision: number },
  ): CreatureSolveJob {
    const request = creatureSolveJobRequestSchema.parse(input),
      id = request.request.id,
      fingerprint = contentKey(request);
    const existing = this.entries.get(id);
    if (existing) {
      if (existing.fingerprint !== fingerprint)
        throw Error("A solve job ID cannot be reused for different work");
      return clone(existing.snapshot);
    }
    if (this.retired.has(id)) throw Error("Solve job receipt was released; use a fresh job ID");
    if (this.retired.size >= CREATURE_JOB_LIMITS.retiredIdentities)
      throw Error("Solve job identity budget exhausted for this session");
    if (this.entries.size >= CREATURE_JOB_LIMITS.retained)
      throw Error("Solve job retention budget exhausted; release a terminal job");
    const source = readSource(),
      sourceBytes = canonical(source.document).length * 2;
    if (sourceBytes > CREATURE_JOB_LIMITS.sourceBytesPerJob)
      throw Error("Creature source exceeds per-job byte budget");
    const retainedBytes = [...this.entries.values()].reduce((sum, entry) => sum + entry.sourceBytes, 0);
    if (retainedBytes + sourceBytes > CREATURE_JOB_LIMITS.activeSourceBytes)
      throw Error("Concurrent creature source byte budget exhausted");
    let resolve: (snapshot: CreatureSolveJob) => void = () => {};
    const promise = new Promise<CreatureSolveJob>((done) => {
      resolve = done;
    });
    const now = Date.now();
    const entry: Entry = {
      snapshot: {
        id,
        status: "queued",
        sourceRevision: source.sourceRevision,
        sourceKey: source.sourceKey,
        createdAt: now,
        deadlineAt: now + request.deadlineMs,
        progress: { phase: "initial", evaluations: 0, iterations: 0, bestCost: null },
      },
      request,
      fingerprint,
      sourceBytes,
      deadline: performance.now() + request.deadlineMs,
      iterator: solveCreatureEditSteps(clone(source.document), request.request),
      promise,
      resolve,
    };
    this.entries.set(id, entry);
    entry.deadlineTimer = setTimeout(
      () => this.finish(entry, "deadline-exceeded", "Creature solve deadline elapsed"),
      request.deadlineMs,
    );
    this.requestPump();
    this.hooks.changed();
    return clone(entry.snapshot);
  }
  inspect(id: string) {
    const entry = this.entries.get(idSchema.parse(id));
    return entry ? clone(entry.snapshot) : undefined;
  }
  list() {
    return [...this.entries.values()].map(({ snapshot }) => clone(snapshot));
  }
  usage() {
    return {
      ...CREATURE_JOB_LIMITS,
      retainedJobs: this.entries.size,
      runningJobs: [...this.entries.values()].filter((entry) => entry.snapshot.status === "running").length,
      retainedSourceBytes: [...this.entries.values()].reduce((sum, entry) => sum + entry.sourceBytes, 0),
    };
  }
  private require(id: string) {
    const entry = this.entries.get(idSchema.parse(id));
    if (!entry) throw Error("Solve job is not retained");
    return entry;
  }
  wait(id: string): Promise<CreatureSolveJob> {
    return this.require(id).promise.then(clone);
  }
  cancel(id: string): CreatureSolveJob {
    const entry = this.require(id);
    if (!terminal(entry.snapshot.status)) this.finish(entry, "cancelled", "Creature solve cancelled");
    return clone(entry.snapshot);
  }
  pause(id: string): CreatureSolveJob {
    const entry = this.require(id);
    if (terminal(entry.snapshot.status)) throw Error("A terminal solve job cannot be paused");
    if (entry.timer !== undefined) clearTimeout(entry.timer);
    entry.timer = undefined;
    entry.snapshot.status = "paused";
    this.requestPump();
    this.hooks.changed();
    return clone(entry.snapshot);
  }
  resume(id: string): CreatureSolveJob {
    const entry = this.require(id);
    if (entry.snapshot.status !== "paused") throw Error("Only a paused solve job can be resumed");
    if (performance.now() >= entry.deadline)
      this.finish(entry, "deadline-exceeded", "Creature solve deadline elapsed while paused");
    else {
      entry.snapshot.status = "queued";
      this.requestPump();
      this.hooks.changed();
    }
    return clone(entry.snapshot);
  }
  release(id: string) {
    const entry = this.require(id);
    if (!terminal(entry.snapshot.status)) throw Error("Cancel or complete a solve job before releasing it");
    this.retired.add(id);
    this.entries.delete(id);
    this.hooks.changed();
  }
  reset() {
    if (this.pumpTimer !== undefined) clearTimeout(this.pumpTimer);
    this.pumpTimer = undefined;
    for (const entry of this.entries.values())
      if (!terminal(entry.snapshot.status))
        this.finish(entry, "cancelled", "Project replaced while creature solve was pending");
  }
  private requestPump() {
    if (this.pumpTimer !== undefined) return;
    this.pumpTimer = setTimeout(() => {
      this.pumpTimer = undefined;
      let active = [...this.entries.values()].filter((entry) => entry.snapshot.status === "running").length;
      let changed = false;
      for (const entry of this.entries.values()) {
        if (active >= CREATURE_JOB_LIMITS.active) break;
        if (entry.snapshot.status !== "queued") continue;
        if (performance.now() >= entry.deadline) {
          this.finish(entry, "deadline-exceeded", "Creature solve deadline elapsed in queue");
          continue;
        }
        entry.snapshot.status = "running";
        active++;
        changed = true;
        entry.timer = setTimeout(() => this.slice(entry), 0);
      }
      if (changed) this.hooks.changed();
    }, 0);
  }
  private slice(entry: Entry) {
    entry.timer = undefined;
    if (entry.snapshot.status !== "running") return;
    const start = performance.now();
    try {
      for (let work = 0; work < entry.request.evaluationsPerSlice; work++) {
        if (entry.snapshot.status !== "running") return;
        if (performance.now() >= entry.deadline) {
          this.finish(entry, "deadline-exceeded", "Creature solve deadline elapsed");
          return;
        }
        const step = entry.iterator?.next();
        if (!step) throw Error("Solve job continuation is missing");
        if (performance.now() >= entry.deadline) {
          this.finish(
            entry,
            "deadline-exceeded",
            "Creature solve deadline elapsed during bounded evaluation",
          );
          return;
        }
        if (step.done) {
          entry.snapshot.result = step.value;
          entry.snapshot.progress = {
            ...entry.snapshot.progress,
            evaluations: step.value.evaluations,
            iterations: step.value.iterations,
            bestCost: step.value.residuals.reduce(
              (sum, residual, index) =>
                sum + residual.residual ** 2 * entry.request.request.objectives[index].weight,
              0,
            ),
          };
          // Reserve terminal state before candidate observers run: no observer can
          // cancel a completed computation halfway through its atomic proposal.
          entry.snapshot.status = "completed";
          try {
            entry.snapshot.candidateId = this.hooks.completed(step.value);
          } catch (error) {
            const code = error && typeof error === "object" && "code" in error ? error.code : undefined;
            entry.snapshot.status = code === "authoring.conflict" ? "stale" : "failed";
            entry.snapshot.error = error instanceof Error ? error.message : String(error);
          }
          this.finish(entry, entry.snapshot.status, entry.snapshot.error, true);
          return;
        }
        entry.snapshot.progress = { ...step.value };
        if (performance.now() - start >= CREATURE_JOB_LIMITS.millisecondsPerSlice) break;
      }
      this.hooks.changed();
      // Observers may pause/cancel on this exact progress event.
      if (entry.snapshot.status === "running") entry.timer = setTimeout(() => this.slice(entry), 0);
    } catch (error) {
      this.finish(entry, "failed", error instanceof Error ? error.message : String(error));
    }
  }
  private finish(entry: Entry, status: CreatureSolveJobStatus, error?: string, force = false) {
    if (terminal(entry.snapshot.status) && !force) return;
    if (entry.timer !== undefined) clearTimeout(entry.timer);
    if (entry.deadlineTimer !== undefined) clearTimeout(entry.deadlineTimer);
    entry.timer = undefined;
    entry.deadlineTimer = undefined;
    entry.iterator = undefined;
    entry.sourceBytes = 0;
    entry.snapshot.status = status;
    entry.snapshot.finishedAt = Date.now();
    if (error) entry.snapshot.error = error;
    entry.resolve(clone(entry.snapshot));
    this.requestPump();
    this.hooks.changed();
  }
}
