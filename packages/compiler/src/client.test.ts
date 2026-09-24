import { afterEach, expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { BrowserCompiler } from "./client";

const ActualWorker = globalThis.Worker;
class WorkerDouble {
  static all: WorkerDouble[] = [];
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onerror: ((event: { message: string; preventDefault: () => void }) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  messages: Record<string, unknown>[] = [];
  terminated = false;
  constructor(
    readonly url: string,
    readonly options: WorkerOptions,
  ) {
    WorkerDouble.all.push(this);
  }
  postMessage(message: Record<string, unknown>) {
    this.messages.push(message);
  }
  terminate() {
    this.terminated = true;
  }
}
function install() {
  WorkerDouble.all = [];
  globalThis.Worker = WorkerDouble as unknown as typeof Worker;
}
afterEach(() => {
  globalThis.Worker = ActualWorker;
});
test("an early worker startup failure rejects future jobs instead of hanging", async () => {
  install();
  const compiler = new BrowserCompiler("blob:worker", 1, "classic");
  const worker = WorkerDouble.all[0];
  expect(worker.options.type).toBe("classic");
  worker.onerror?.({ message: "Loading failed", preventDefault() {} });
  await expect(compiler.compile(referenceProject().documents[0], "review")).rejects.toThrow("Loading failed");
  compiler.dispose();
});
test("a failed worker settles queued jobs and a timed-out job terminates its worker", async () => {
  install();
  const compiler = new BrowserCompiler("worker", 1, "module", 15);
  const project = referenceProject(),
    first = compiler.compile(project.documents[0], "review"),
    second = compiler.compile(project.documents[1], "review");
  const results = await Promise.allSettled([first, second]);
  expect(results.every((result) => result.status === "rejected")).toBe(true);
  expect(WorkerDouble.all[0].terminated).toBe(true);
  compiler.dispose();
});
test("healthy remaining workers continue and disposal rejects outstanding jobs", async () => {
  install();
  const compiler = new BrowserCompiler("worker", 2),
    project = referenceProject();
  WorkerDouble.all[0].onerror?.({ message: "First failed", preventDefault() {} });
  const pending = compiler.compile(project.documents[0], "review"),
    worker = WorkerDouble.all[1];
  expect(worker.messages).toHaveLength(1);
  worker.onmessage?.({ data: { id: worker.messages[0].id, artifact: null, error: null } });
  expect(await pending).toBeNull();
  const other = compiler.compile(project.documents[1], "review");
  compiler.dispose();
  await expect(other).rejects.toThrow("disposed");
});

test("rapid edits discard older queued geometry while active work remains valid", async () => {
  install();
  const compiler = new BrowserCompiler("worker", 1),
    doc = referenceProject().documents.find((doc) => doc.kind === "object");
  if (!doc || doc.kind !== "object") throw Error("Missing object");
  const active = compiler.compile(doc, "review"),
    second = structuredClone(doc),
    third = structuredClone(doc);
  second.field.nodes[0].radius = 0.4;
  third.field.nodes[0].radius = 0.5;
  const obsolete = compiler.compile(second, "review"),
    settled = obsolete.catch((error) => error);
  const latest = compiler.compile(third, "review");
  expect(String(await settled)).toContain("superseded");
  const worker = WorkerDouble.all[0];
  expect(worker.messages).toHaveLength(1);
  worker.onmessage?.({ data: { id: worker.messages[0].id, artifact: null, error: null } });
  await active;
  expect(worker.messages).toHaveLength(2);
  expect((worker.messages[1].document as typeof doc).field.nodes[0].radius).toBe(0.5);
  worker.onmessage?.({ data: { id: worker.messages[1].id, artifact: null, error: null } });
  expect(await latest).toBeNull();
  compiler.dispose();
});
test("a timed-out execution gets one replacement worker and settles queued work on the replacement", async () => {
  install();
  const compiler = new BrowserCompiler("worker", 1, "module", 20),
    project = referenceProject();
  const first = compiler.compile(project.documents[0], "review");
  const queued = compiler.compile(project.documents[1], "review");
  await expect(first).rejects.toThrow("execution timed out");
  expect(WorkerDouble.all).toHaveLength(2);
  expect(WorkerDouble.all[0].terminated).toBe(true);
  const replacement = WorkerDouble.all[1];
  expect(replacement.messages).toHaveLength(1);
  replacement.onmessage?.({ data: { id: replacement.messages[0].id, artifact: null, error: null } });
  await expect(queued).resolves.toBeNull();
  await expect(compiler.compile(project.documents[2], "review")).rejects.toThrow("execution timed out");
  expect(WorkerDouble.all).toHaveLength(2);
  await expect(compiler.compile(project.documents[3], "review")).rejects.toThrow("execution timed out");
  compiler.dispose();
});
test("queue residence does not consume the next job's execution deadline", async () => {
  install();
  const compiler = new BrowserCompiler("worker", 1, "module", 100),
    project = referenceProject();
  const first = compiler.compile(project.documents[0], "review"),
    second = compiler.compile(project.documents[1], "review");
  // Attach settlement handlers while intentionally holding the worker.
  const results = Promise.all([first, second]);
  await Bun.sleep(65);
  const worker = WorkerDouble.all[0];
  worker.onmessage?.({ data: { id: worker.messages[0].id, artifact: null, error: null } });
  await Bun.sleep(65);
  expect(worker.terminated).toBe(false);
  worker.onmessage?.({ data: { id: worker.messages[1].id, artifact: null, error: null } });
  expect(await results).toEqual([null, null]);
  compiler.dispose();
});
test("export compilation rejects stale or missing worker identity and accepts the expected build", async () => {
  for (const compilerSource of [undefined, "stale-build", "expected-build"]) {
    install();
    const compiler = new BrowserCompiler("worker", 1, "classic", 30000, "expected-build");
    const document = referenceProject().documents[0];
    const pending = compiler.compile(document, "review"),
      worker = WorkerDouble.all[0];
    worker.onmessage?.({ data: { id: worker.messages[0].id, artifact: null, error: null, compilerSource } });
    if (compilerSource === "expected-build") {
      await expect(pending).resolves.toBeNull();
      expect(worker.terminated).toBe(false);
    } else {
      await expect(pending).rejects.toThrow("source identity");
      expect(worker.terminated).toBe(true);
      await expect(compiler.compile(document, "review")).rejects.toThrow("source identity");
    }
    compiler.dispose();
  }
});
