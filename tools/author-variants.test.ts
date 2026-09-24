import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { referenceProject } from "@wrela/examples";
import { runAuthoring } from "./author";
import { WorkspaceBridge } from "./bridge";

test("headless domain alternatives remain unpublished, then a chosen ordinary proposal persists and invalidates siblings", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-variants-"));
  try {
    const bridge = new WorkspaceBridge(directory);
    await bridge.initialize();
    const saved = await bridge.save(referenceProject(), null);
    const request = {
      workspaceKey: saved.key,
      request: {
        id: "sky",
        expectedRevision: 0,
        variants: [-1, 0.5, 1].map((exposureCompensation, index) => ({
          id: `v${index}`,
          recipe: {
            kind: "environment.grade",
            target: "winter-sky",
            grade: { exposureCompensation, tint: [1, 1, 1] },
          },
        })),
      },
    };
    const input = join(directory, "request.json");
    await Bun.write(input, JSON.stringify(request));
    const result = await runAuthoring([directory, "variants", input]);
    if (!("proposals" in result)) throw Error("Missing variant proposals");
    expect(result.published).toBe(false);
    expect(result.proposals).toHaveLength(3);
    expect((await bridge.read())?.key).toBe(saved.key);
    const chosen = join(directory, "chosen.json"),
      sibling = join(directory, "sibling.json");
    await Bun.write(chosen, JSON.stringify(result.proposals[1]));
    await Bun.write(sibling, JSON.stringify(result.proposals[0]));
    await runAuthoring([directory, "preview", chosen]);
    expect((await bridge.read())?.key).toBe(saved.key);
    await runAuthoring([directory, "apply", chosen]);
    const sky = (await bridge.read())?.project.documents.find((document) => document.id === "winter-sky");
    expect(sky?.kind === "environment" && sky.grade?.exposureCompensation).toBe(0.5);
    await expect(runAuthoring([directory, "apply", sibling])).rejects.toThrow("Workspace documents changed");
    await expect(runAuthoring([directory, "variants", input])).rejects.toThrow("Workspace changed");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("generic headless propose reviews non-creature documents without publishing", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-propose-"));
  try {
    const bridge = new WorkspaceBridge(directory);
    await bridge.initialize();
    const saved = await bridge.save(referenceProject(), null);
    const path = join(directory, "request.json");
    await Bun.write(
      path,
      JSON.stringify({
        workspaceKey: saved.key,
        id: "sky-name",
        batch: {
          expectedRevision: 0,
          operations: [{ kind: "document.rename", target: "winter-sky", name: "Neutral sky" }],
        },
      }),
    );
    const result = await runAuthoring([directory, "propose", path]);
    expect("published" in result && result.published).toBe(false);
    expect((await bridge.read())?.key).toBe(saved.key);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
