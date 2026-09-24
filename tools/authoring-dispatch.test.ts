import { expect, test } from "bun:test";
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createWorkSession } from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { runAuthoring } from "./author";
import { finishDispatchedJob } from "./authoring-agent";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";
import { transferTimber } from "./fixtures/authoring-transfer";

test("help is read-only and exposes the job and portable handoff workflow without a workspace", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-help-"));
  try {
    const absent = join(directory, "not-created");
    const help = await runAuthoring([absent, "--help"]);
    expect(JSON.stringify(help)).toContain("job <work-id>");
    expect(JSON.stringify(await runAuthoring(["--help"]))).toContain("portability");
    await expect(stat(absent)).rejects.toThrow();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("a dispatch does not bypass proposal selection, source CAS, or retained review verification", async () => {
  const dir = await mkdtemp(join(tmpdir(), "wrela-dispatch-"));
  try {
    const project = transferTimber(),
      bridge = new WorkspaceBridge(dir),
      store = new WorkFileStore(dir);
    await bridge.initialize();
    await bridge.save(project, null);
    const work = createWorkSession(project, { id: "test", brief: "Timber" });
    await store.put(work, null);
    const path = join(dir, "dispatch.json");
    const dispatch = {
      version: 1,
      workspace: dir,
      work: work.id,
      key: "stale",
      brief: "Timber",
      images: [],
      candidates: [{ proposal: "unreviewed", passed: true }],
    };
    await Bun.write(path, JSON.stringify(dispatch));
    await expect(finishDispatchedJob(path, "other")).rejects.toThrow("passing");
    await expect(finishDispatchedJob(path, "unreviewed")).rejects.toThrow("Work changed");
    await Bun.write(path, JSON.stringify({ ...dispatch, key: contentKey(work) }));
    await expect(finishDispatchedJob(path, "unreviewed")).rejects.toThrow();
    expect((await bridge.read())?.key).toBe(contentKey(project));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
