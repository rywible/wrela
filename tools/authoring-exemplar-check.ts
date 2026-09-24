import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { authoringExemplars, createWorkSession } from "@wrela/authoring";
import { callAuthoringAgentTool } from "./authoring-agent";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";
import { sourceManifest } from "./evidence";
import { transferTimber, transferTree } from "./fixtures/authoring-transfer";

/** Rendered, portable starting-point library. No curated preset is labelled art-approved. */
export async function checkAuthoringExemplars(destination: string) {
  const output = resolve(destination);
  await mkdir(output, { recursive: true });
  const source = await sourceManifest("authoring-exemplar-check");
  const rows = [];
  for (const [i, exemplar] of authoringExemplars.entries()) {
    const project = exemplar.domain === "timber" ? transferTimber(i === 2) : transferTree(i === 5);
    const workspace = join(output, exemplar.id),
      bridge = new WorkspaceBridge(workspace),
      store = new WorkFileStore(workspace);
    await bridge.initialize();
    await bridge.save(project, null);
    await store.put(createWorkSession(project, { id: exemplar.id, brief: exemplar.title }), null);
    const started = performance.now();
    const response = await callAuthoringAgentTool("wrela_job", {
      workspace,
      work: exemplar.id,
      request: { exemplar: exemplar.id, candidates: 1, budgetSeconds: 15 },
    });
    const metadata = response.content.find((c) => c.type === "text"),
      image = response.content.find((c) => c.type === "image");
    if (metadata?.type !== "text" || image?.type !== "image")
      throw Error("Native job must return both result and image");
    const result = JSON.parse(metadata.text);
    if (!result.finish || !result.candidates[0].passed || result.exemplar.approval !== "curated-unreviewed")
      throw Error("Invalid exemplar review");
    await Bun.write(join(output, `${exemplar.id}.png`), Buffer.from(image.data, "base64"));
    const finished = await callAuthoringAgentTool("wrela_finish", {
      workspace,
      work: exemplar.id,
      request: result.finish,
    });
    if (finished.content[0].type !== "text") throw Error("Missing finish");
    const handoff = JSON.parse(finished.content[0].text);
    if (!handoff.published || handoff.packagingPending) throw Error("Unfinished exemplar handoff");
    rows.push({
      exemplar,
      elapsedMs: performance.now() - started,
      result,
      handoff: handoff.output,
      image: `${exemplar.id}.png`,
    });
    console.log(JSON.stringify({ id: exemplar.id, elapsedMs: rows.at(-1)?.elapsedMs, passed: true }));
  }
  const report = {
    scope:
      "Scripted native jobs with six curated, unapproved targets. Includes fence and birch transfer. Not independent agent trials or an art score.",
    sourceFingerprint: source.sourceFingerprint,
    rows,
  };
  await Bun.write(join(output, "exemplars.json"), JSON.stringify(report, null, 2));
  await Bun.write(join(output, "source-manifest.json"), JSON.stringify(source, null, 2));
  return { output, targets: rows.length, sourceFingerprint: source.sourceFingerprint };
}
if (import.meta.main)
  console.log(
    JSON.stringify(
      await checkAuthoringExemplars(process.argv[2] ?? `output/authoring-exemplars-${Date.now()}`),
    ),
  );
