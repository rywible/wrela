import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { referenceProject } from "@wrela/examples";
import { runAuthoring } from "./author";
import { WorkspaceBridge } from "./bridge";

test("headless proposal preview does not publish, apply uses the same source-key publication contract", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-author-"));
  try {
    const bridge = new WorkspaceBridge(directory);
    await bridge.initialize();
    const saved = await bridge.save(referenceProject(), null);
    const file = join(directory, "proposal.json");
    await Bun.write(
      file,
      JSON.stringify({
        workspaceKey: saved.key,
        batch: {
          expectedRevision: 0,
          operations: [{ kind: "document.rename", target: "polar-bunny", name: "Snow scout" }],
        },
      }),
    );
    await runAuthoring([directory, "preview", file]);
    expect((await bridge.read())?.key).toBe(saved.key);
    await runAuthoring([directory, "apply", file]);
    expect(
      (await bridge.read())?.project.documents.find((document) => document.id === "polar-bunny")?.name,
    ).toBe("Snow scout");
    await expect(runAuthoring([directory, "apply", file])).rejects.toThrow("Workspace documents changed");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
