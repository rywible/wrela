import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createWorkSession, proposeWork, workHandoff } from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { authoringBenchmarkProject } from "./authoring-benchmark";
import { WorkFileStore } from "./authoring-work-store";

export async function benchmarkAlternatives() {
  const project = authoringBenchmarkProject(),
    directory = await mkdtemp(join(tmpdir(), "wrela-alternatives-bench-"));
  try {
    let work = createWorkSession(project, {
      id: "alternatives",
      brief: "Compare small independent source alternatives",
    });
    const timings: number[] = [];
    for (let i = 0; i < 32; i++) {
      const start = performance.now();
      work = proposeWork(work, `p-${i}`, [
        { kind: "document.rename", target: "benchmark-0", name: `Alternative ${i}` },
      ]);
      timings.push(performance.now() - start);
    }
    const store = new WorkFileStore(directory),
      writeStart = performance.now();
    await store.put(work, null);
    const writeMs = performance.now() - writeStart,
      readStart = performance.now(),
      reopened = await store.get(work.id),
      handoff = workHandoff(reopened),
      readMs = performance.now() - readStart;
    const bytes = Bun.file(join(await store.directory(), "alternatives.json")).size;
    if (
      bytes >= 64 * 1024 * 1024 ||
      handoff.proposals.length !== 32 ||
      contentKey(reopened) !== contentKey(work)
    )
      throw Error("Alternative storage acceptance gate failed");
    return {
      definitions: project.documents.length,
      sourceBytes: new TextEncoder().encode(JSON.stringify(project)).byteLength,
      proposals: 32,
      persistedBytes: bytes,
      proposeMs: {
        p50: [...timings].sort((a, b) => a - b)[15],
        p95: [...timings].sort((a, b) => a - b)[30],
        max: Math.max(...timings),
      },
      writeMs,
      readAndHandoffMs: readMs,
      representation: "one baseline plus operations; no candidate replay for listing or handoff",
    };
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}
if (import.meta.main) console.log(JSON.stringify(await benchmarkAlternatives(), null, 2));
