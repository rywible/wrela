import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createWorkSession, domainQuality } from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { callAuthoringAgentTool } from "./authoring-agent";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";
import { transferTimber, transferTree } from "./fixtures/authoring-transfer";

const out = resolve(process.argv[2] ?? `output/authoring-transfer-${Date.now()}`);
await mkdir(out, { recursive: true });
const results = [];
for (const domain of ["timber", "vegetation"] as const)
  for (const heldout of [false, true]) {
    const id = `${domain}-${heldout ? "heldout" : "primary"}`,
      workspace = join(out, id),
      project = domain === "timber" ? transferTimber(heldout) : transferTree(heldout);
    const bridge = new WorkspaceBridge(workspace);
    await bridge.initialize();
    await bridge.save(project, null);
    const store = new WorkFileStore(workspace),
      work = createWorkSession(project, {
        id,
        brief: `${domain} scripted transfer; no art acceptance`,
        constraints: [
          {
            id: "finite-geometry",
            kind: "geometry",
            target: project.entry,
            minTriangles: 10,
            maxTriangles: 10_000_000,
            minimumExtent: [0.1, 0.3, 0.1],
            maximumExtent: [12, 12, 12],
          },
        ],
      });
    await store.put(work, null);
    const start = performance.now();
    const response = await callAuthoringAgentTool("wrela_study", {
      workspace,
      work: id,
      request: {
        id: "appearance",
        expectedKey: contentKey(work),
        brief: {
          domain,
          target: project.entry,
          conditions: { exposure: 0.75, moisture: 0.4, maturity: 0.85, variation: 0.55 },
          quality: domainQuality(domain),
        },
        candidates: heldout ? 1 : 3,
      },
    });
    const text = response.content.find((c) => c.type === "text");
    if (!text || text.type !== "text") throw Error("Missing tool metadata");
    const value = JSON.parse(text.text);
    const image = response.content.find((c) => c.type === "image");
    if (!image || image.type !== "image") throw Error("Native tool did not return an inline image");
    if (!value.candidates.every((c: { passed: boolean }) => c.passed)) throw Error(JSON.stringify(value));
    await Bun.write(join(out, `${id}-gallery.png`), Buffer.from(image.data, "base64"));
    const nativeFinish = await callAuthoringAgentTool("wrela_finish", {
      workspace,
      work: id,
      request: { expectedKey: value.key, proposal: value.candidates[0].proposal },
    });
    const finishText = nativeFinish.content[0];
    if (finishText.type !== "text") throw Error("Missing finish result");
    const finish = JSON.parse(finishText.text);
    if (!finish.published || finish.packagingPending) throw Error("Transfer handoff incomplete");
    const record = {
      id,
      elapsedMs: performance.now() - start,
      gallery: value.gallery,
      output: finish.output,
      candidates: value.candidates,
      inlineBytes: image.data.length,
      sourceKey: (await bridge.read())?.key,
    };
    results.push(record);
    console.log(JSON.stringify(record));
  }
await Bun.write(
  join(out, "results.json"),
  JSON.stringify(
    {
      scope: "Scripted hardware transfer and native tool check, not fresh agents or artistic acceptance",
      results,
    },
    null,
    2,
  ),
);
