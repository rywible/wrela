import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHeroWorkspace, createWorkSession, toolkitEntrySchema } from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { runAuthoringToolkit } from "./authoring-toolkit-store";
import { WorkFileStore } from "./authoring-work-store";

test("toolkit search returns usable CAS keys without hashing its search ranking", async () => {
  const root = await mkdtemp(join(tmpdir(), "wrela-toolkit-search-"));
  try {
    const result = await runAuthoringToolkit(root, "discovery", { action: "search", query: "hollow shell" });
    if (!("entries" in result) || !result.entries) throw Error("Expected search results");
    expect(result.entries.length).toBeGreaterThan(0);
    const { matchScore, key, ...entry } = result.entries[0];
    expect(matchScore).toBeGreaterThan(0);
    expect(key).toBe(contentKey(toolkitEntrySchema.parse(entry)));
    const registered = await runAuthoringToolkit(root, "discovery", { action: "register", entry });
    expect("key" in registered && registered.key).toBe(key);
    const persisted = await runAuthoringToolkit(root, "discovery", { action: "search", query: "lathe" });
    if (!("entries" in persisted) || !persisted.entries) throw Error("Expected search results");
    expect(persisted.entries[0].key).toBe(key);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("a capability gap keeps its diagnostic attachments after the request folder disappears", async () => {
  const root = await mkdtemp(join(tmpdir(), "wrela-gap-"));
  try {
    const workspace = join(root, "workspace"),
      request = join(root, "request");
    const store = new WorkFileStore(workspace);
    await store.put(
      createWorkSession(createHeroWorkspace(), { id: "gap", brief: "Diagnose a missing construction" }),
      null,
    );
    const preview = join(request, "failure.png");
    await Bun.write(preview, new Uint8Array([137, 80, 78, 71]));
    const result = await runAuthoringToolkit(
      workspace,
      "gap",
      {
        action: "gap",
        gap: {
          version: 1,
          id: "hollow",
          work: "gap",
          desiredResult: "An open cavity",
          attemptedComposition: "An opaque solid obscures the interior",
          evidence: [preview],
          solutionLevel: "compiler",
          reason: "The review shows a closed solid",
          engineeringMs: 0,
        },
      },
      request,
    );
    await rm(request, { recursive: true });
    const bundle = await store.backup("gap");
    const restoredStore = new WorkFileStore(join(root, "restored"));
    const restored = await restoredStore.restore(bundle);
    const event = restored.events.find((e) => e.kind === "tool-development")!;
    const gap = await Bun.file(event.evidence[0]).json();
    expect(gap.evidence[0]).not.toBe(preview);
    expect(Array.from(new Uint8Array(await Bun.file(gap.evidence[0]).arrayBuffer()))).toEqual([
      137, 80, 78, 71,
    ]);
    expect("evidence" in result).toBe(true);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
