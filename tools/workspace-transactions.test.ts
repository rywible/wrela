import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { EditBatch } from "@wrela/authoring";
import { referenceProject } from "@wrela/examples";
import { WorkspaceBridge } from "./bridge";

test("independent process writers rebase disjoint documents and retry durable receipts", async () => {
  const dir = await mkdtemp(join(tmpdir(), "wrela-transactions-"));
  try {
    const bridge = new WorkspaceBridge(dir);
    await bridge.initialize();
    const initial = await bridge.save(referenceProject(), null);
    const batch = (id: string, target: string): EditBatch => ({
      expectedRevision: 0,
      transactionId: id,
      operations: [{ kind: "document.rename", target, name: id }],
    });
    const a = batch("writer-a", "snow-fur"),
      b = batch("writer-b", "daylight");
    // Separate processes share only the filesystem and OS publication lock.
    const calls = [a, b].map(async (value, i) => {
      const file = join(dir, `proposal-${i}.json`);
      await Bun.write(file, JSON.stringify({ workspaceKey: initial.key, batch: value }));
      const process = Bun.spawn(["bun", "tools/author.ts", dir, "apply", file], {
        stdout: "pipe",
        stderr: "pipe",
      });
      const output = await new Response(process.stdout).text(),
        error = await new Response(process.stderr).text();
      if (await process.exited) throw Error(error);
      return JSON.parse(output);
    });
    const [first, second] = await Promise.all(calls);
    const restart = new WorkspaceBridge(dir);
    await restart.initialize();
    expect(await restart.transact(initial.key, a)).toEqual(first);
    expect(await restart.transact(initial.key, b)).toEqual(second);
    await expect(
      restart.transact(initial.key, {
        ...a,
        operations: [{ kind: "document.rename", target: "snow-fur", name: "Different" }],
      }),
    ).rejects.toThrow("transaction ID");
    await expect(restart.transact(initial.key, batch("overlap", "snow-fur"))).rejects.toThrow("changed");
    // Character reads its material: source dependencies invalidate proposals conservatively.
    await expect(restart.transact(initial.key, batch("dependent", "polar-bunny"))).rejects.toThrow("changed");
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
