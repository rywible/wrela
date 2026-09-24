import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createHeroWorkspace, createWorkSession, type HeroDesignInput } from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { invokeAuthoringAgent } from "./authoring-agent";
import { withAuthoringReviewSession } from "./authoring-review-session";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";
import { sourceManifest } from "./evidence";
import { expeditionLantern, hangingBell } from "./fixtures/hero-authoring";

export async function checkHeroAuthoring(destination: string) {
  const output = resolve(destination);
  await mkdir(output, { recursive: true });
  const manifest = await sourceManifest("hero-authoring-check");
  await Bun.write(join(output, "source-manifest.json"), JSON.stringify(manifest, null, 2));
  const rows: ({ id: string; elapsedMs: number } & Awaited<ReturnType<typeof invokeAuthoringAgent>>)[] = [];
  await withAuthoringReviewSession(async () => {
    for (const designs of [
      [expeditionLantern(0), expeditionLantern(1)],
      [hangingBell()],
    ] as HeroDesignInput[][]) {
      const id = designs[0].id,
        workspace = join(output, id),
        bridge = new WorkspaceBridge(workspace),
        store = new WorkFileStore(workspace);
      const baseline = createHeroWorkspace();
      await bridge.initialize();
      await bridge.save(baseline, null);
      const work = createWorkSession(baseline, { id, brief: designs[0].intent });
      await store.put(work, null);
      const target: [number, number, number] = id === "hero-bell" ? [0.03, 0.22, 0] : [0, 0.275, 0];
      const views = [
        { id: "angle", rig: "neutral", camera: { position: [0.72, 0.47, 1.04], target, fov: 38 } },
        {
          id: "detail",
          rig: "neutral",
          camera: { position: [0.32, 0.44, 0.57], target: [0, 0.31, 0], fov: 38 },
        },
        { id: "grazing", rig: "grazing", camera: { position: [0.72, 0.47, 1.04], target, fov: 38 } },
      ];
      const start = performance.now(),
        result = await invokeAuthoringAgent({
          workspace,
          work: id,
          action: "hero",
          input: {
            expectedKey: contentKey(work),
            id: "construction",
            designs,
            views,
            width: 800,
            height: 600,
            budgetSeconds: 60,
          },
        });
      rows.push({ id, elapsedMs: performance.now() - start, ...result });
      console.log(JSON.stringify({ id, elapsedMs: rows.at(-1)!.elapsedMs, images: result.images }));
    }
  });
  await Bun.write(
    join(output, "report.json"),
    JSON.stringify(
      { scope: "Scripted cold/warm creation lookdev, not fresh-agent timings or art approval", rows },
      null,
      2,
    ),
  );
  return { output };
}
if (import.meta.main)
  console.log(await checkHeroAuthoring(process.argv[2] ?? `output/hero-lookdev-${Date.now()}`));
