import type {
  BotanicalGrowthState,
  CompiledVegetation,
  Document,
  GrowthEvent,
  MeshData,
  PersistentVegetationGrowth,
  Quality,
  SurfaceArtifact,
  TerrainDefinition,
  VegetationDefinition,
} from "@wrela/model";

import { compilerKey } from "./index";
import type { TerrainStitch } from "./terrain";

type Task = {
  id: string;
  compilationKey?: string;
  payload: Record<string, unknown>;
  resolve: (result: any) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};
type Slot = { worker: Worker; task: Task | null; failed: boolean; restarts: number };
/** Shared bounded worker pool. Startup failures are terminal for that slot;
 * queued requests and requests arriving after failure always settle. Classic
 * bundled workers are supported for portable exports opened through file://. */
export class BrowserCompiler {
  private workers: Slot[] = [];
  private queue: Task[] = [];
  private disposed = false;
  private failure: Error | null = null;
  private pending = new Map<string, Promise<SurfaceArtifact | null>>();
  constructor(
    private readonly url: string,
    count = 2,
    private readonly workerType: WorkerType = "module",
    private timeoutMs = 30000,
    private readonly expectedCompilerSource?: string,
  ) {
    if (!Number.isInteger(count) || count < 1 || count > 8)
      throw new Error("Compiler pool requires 1–8 workers");
    if (!Number.isFinite(timeoutMs) || timeoutMs < 10 || timeoutMs > 300000)
      throw new Error("Compiler timeout must be 10–300000 ms");
    try {
      for (let index = 0; index < count; index++) {
        const slot: Slot = {
          worker: new Worker(url, { type: workerType }),
          task: null,
          failed: false,
          restarts: 0,
        };
        this.connect(slot);
        this.workers.push(slot);
      }
    } catch (error) {
      this.dispose();
      throw error;
    }
  }
  private connect(slot: Slot) {
    const worker = slot.worker;
    worker.onmessage = ({ data }) => {
      if (slot.worker !== worker || slot.failed) return;
      const task = slot.task;
      if (!task) return;
      if (!data || data.id !== task.id) {
        this.fail(slot, new Error("Compiler worker returned an unexpected response"));
        return;
      }
      if (
        !data.error &&
        this.expectedCompilerSource !== undefined &&
        data.compilerSource !== this.expectedCompilerSource
      ) {
        this.fail(
          slot,
          new Error("Compiler worker source identity is missing or incompatible with the requested export"),
        );
        return;
      }
      slot.task = null;
      clearTimeout(task.timer);
      if (data.error) task.reject(new Error(String(data.error)));
      else task.resolve(data.result ?? data.mesh ?? data.artifact);
      this.pump();
    };
    worker.onerror = (event) => {
      if (slot.worker !== worker || slot.failed) return;
      event.preventDefault();
      this.fail(slot, new Error(event.message || "Compiler worker failed to start or execute"));
    };
    worker.onmessageerror = () => {
      if (slot.worker !== worker || slot.failed) return;
      this.fail(slot, new Error("Compiler worker response could not be decoded"), true);
    };
  }
  private fail(slot: Slot, error: Error, recover = false): void {
    if (slot.failed) return;
    slot.failed = true;
    slot.worker.terminate();
    this.failure = error;
    if (slot.task) {
      clearTimeout(slot.task.timer);
      slot.task.reject(error);
      slot.task = null;
    }
    // A stalled execution does not permanently reduce the pool. One replacement
    // per slot is a hard lifetime bound; startup errors remain terminal.
    if (recover && !this.disposed && slot.restarts < 1) {
      slot.restarts++;
      try {
        slot.worker = new Worker(this.url, { type: this.workerType });
        slot.failed = false;
        this.connect(slot);
      } catch {
        /* Fall through to terminal settlement if replacement cannot start. */
      }
    }
    if (this.workers.every((worker) => worker.failed)) {
      for (const task of this.queue) {
        clearTimeout(task.timer);
        task.reject(error);
      }
      this.queue = [];
    } else this.pump();
  }
  private request<T>(payload: Record<string, unknown>, compilationKey?: string): Promise<T> {
    if (this.disposed) return Promise.reject(new Error("Compiler disposed"));
    if (this.workers.every((worker) => worker.failed))
      return Promise.reject(this.failure ?? new Error("Compiler workers are unavailable"));
    if (this.queue.length >= 256)
      return Promise.reject(new Error("Compilation queue is full. Wait for the current preview."));
    return new Promise<T>((resolve, reject) => {
      const task: Task = {
        id: crypto.randomUUID(),
        compilationKey,
        payload,
        resolve,
        reject,
        timer: setTimeout(
          () => {
            this.queue = this.queue.filter((queued) => queued !== task);
            task.reject(new Error("Compilation queue wait timed out; retry when generation is less busy"));
          },
          Math.min(300000, this.timeoutMs * 4),
        ),
      };
      this.queue.push(task);
      this.pump();
    });
  }
  compile = (document: Document, quality: Quality): Promise<SurfaceArtifact | null> => {
    const key = compilerKey(document, quality);
    this.queue = this.queue.filter((task) => {
      const previous = task.payload.document as Document | undefined;
      if (previous?.id !== document.id || task.compilationKey === key) return true;
      clearTimeout(task.timer);
      if (task.compilationKey) this.pending.delete(task.compilationKey);
      task.reject(new Error("Compilation superseded by a newer document revision"));
      return false;
    });
    const existing = this.pending.get(key);
    if (existing) return existing;
    const result = this.request<SurfaceArtifact | null>({ document, quality }, key);
    this.pending.set(key, result);
    void result
      .finally(() => {
        if (this.pending.get(key) === result) this.pending.delete(key);
      })
      .catch(() => {});
    return result;
  };
  growVegetation = (
    document: VegetationDefinition,
    instanceId: string,
    steps: number,
    previous: PersistentVegetationGrowth | undefined,
    events: GrowthEvent[],
    quality: Quality,
  ): Promise<{
    growth: PersistentVegetationGrowth;
    document: VegetationDefinition;
    artifact: CompiledVegetation;
  }> => this.request({ growth: { document, instanceId, steps, previous, events }, quality });
  inspectVegetation = (document: VegetationDefinition): Promise<BotanicalGrowthState> => {
    this.queue = this.queue.filter((task) => {
      if ((task.payload.inspectGrowth as VegetationDefinition | undefined)?.id !== document.id) return true;
      clearTimeout(task.timer);
      task.reject(new Error("Growth inspection superseded"));
      return false;
    });
    return this.request({ inspectGrowth: document });
  };
  generateTerrain = (
    terrain: TerrainDefinition,
    patch: { x: number; z: number; size: number; stitch?: TerrainStitch },
    resolution: number,
  ): Promise<MeshData> => this.request<MeshData>({ terrain, patch: { ...patch, resolution } });
  private pump(): void {
    if (this.disposed) return;
    for (const slot of this.workers)
      if (!slot.failed && !slot.task && this.queue.length) {
        slot.task = this.queue.shift() ?? null;
        if (slot.task)
          try {
            clearTimeout(slot.task.timer);
            const task = slot.task;
            task.timer = setTimeout(() => {
              if (slot.task === task)
                this.fail(slot, new Error("Compilation execution timed out; retry the preview"), true);
            }, this.timeoutMs);
            slot.worker.postMessage({ id: slot.task.id, ...slot.task.payload });
          } catch (error) {
            this.fail(slot, error instanceof Error ? error : new Error(String(error)));
          }
      }
  }
  dispose(): void {
    this.disposed = true;
    for (const slot of this.workers) {
      slot.worker.terminate();
      if (slot.task) {
        clearTimeout(slot.task.timer);
        slot.task.reject(new Error("Compiler disposed"));
        slot.task = null;
      }
    }
    for (const task of this.queue) {
      clearTimeout(task.timer);
      task.reject(new Error("Compiler disposed"));
    }
    this.queue = [];
    this.pending.clear();
  }
}
