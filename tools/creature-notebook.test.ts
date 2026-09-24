import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCreatureNotebook } from "./creature-notebook";

test("persistent observation writes reject a stale notebook key and survive a new process-style read", async () => {
  const directory = await mkdtemp(join(tmpdir(), "creature-notebook-")),
    file = join(directory, "review.json"),
    request = join(directory, "request.json");
  try {
    const initial = await runCreatureNotebook([file, "init", "ash-warden"]);
    await Bun.write(
      request,
      JSON.stringify({
        expectedKey: initial.key,
        observation: {
          id: "face",
          target: "ash-warden",
          sourceKey: "0123456789abcdef",
          region: "head-region",
          summary: "Nostril lacks depth",
          category: "anatomy",
          priority: "important",
          intent: "Carve a bounded recess",
          evidence: {
            capture: "face.png",
            motion: "idle",
            tick: 0,
            camera: { position: [2, 2, 4], target: [0, 2, 1], fov: 30 },
            channel: "clay",
          },
        },
      }),
    );
    const saved = await runCreatureNotebook([file, "observe", request]);
    expect((await runCreatureNotebook([file, "inspect"])).key).toBe(saved.key);
    await expect(runCreatureNotebook([file, "observe", request])).rejects.toThrow("changed");
    expect(await Bun.file(`${file}.lock`).exists()).toBe(false);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
