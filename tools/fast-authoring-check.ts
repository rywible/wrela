import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createAssemblyPart, type EvaluationRequest } from "@wrela/authoring";
import type { Project } from "@wrela/model";
import { runAuthoring } from "./author";
import { WorkspaceBridge } from "./bridge";
import { fastAuthoringProject } from "./fixtures/fast-authoring";

const output = resolve(process.argv[2] ?? `output/fast-authoring-${Date.now()}`);
await mkdir(output, { recursive: true });
const results: unknown[] = [];
async function exercise(name: string, project: Project, intent: EvaluationRequest["intent"]) {
  const workspace = join(output, name),
    bridge = new WorkspaceBridge(workspace);
  await bridge.initialize();
  await bridge.save(project, null);
  const file = join(workspace, "request.json"),
    started = performance.now();
  await Bun.write(
    file,
    JSON.stringify({
      id: name,
      brief: `${name}: scripted integration, no artistic acceptance or agent productivity claim`,
      constraints: [],
    }),
  );
  await runAuthoring([workspace, "work", "init", file]);
  const response = (await runAuthoring([workspace, "work", "context", name, "timber-frame"])) as {
    work: { key: string; views: EvaluationRequest["views"] };
  };
  await Bun.write(
    file,
    JSON.stringify({
      expectedKey: response.work.key,
      proposal: "candidate",
      target: "timber-frame",
      intent,
      views: response.work.views,
    }),
  );
  const reviewed = (await runAuthoring([workspace, "work", "evaluate", name, file])) as {
    work: {
      key: string;
      packet: { contactSheet: string; timings: unknown; report: { results: { status: string }[] } };
    };
  };
  if (reviewed.work.packet.report.results.some((r) => r.status !== "passed"))
    throw Error("A declared constraint did not pass");
  await Bun.write(file, JSON.stringify({ expectedKey: reviewed.work.key, proposal: "candidate" }));
  const finished = (await runAuthoring([workspace, "work", "finish", name, file])) as {
    work: { published: boolean; packagingPending: boolean; output: string };
  };
  if (!finished.work.published || finished.work.packagingPending) throw Error("Handoff was not finalized");
  const bundle = await Bun.file(join(finished.work.output, "handoff-bundle.json")).json();
  const result = {
    name,
    elapsedMs: performance.now() - started,
    contactSheet: reviewed.work.packet.contactSheet,
    timings: reviewed.work.packet.timings,
    constraints: reviewed.work.packet.report.results.length,
    artifacts: bundle.artifacts.length,
    output: finished.work.output,
  };
  results.push(result);
  console.log(JSON.stringify(result));
  return (await bridge.read())!.project;
}
const revised = await exercise("opening", fastAuthoringProject(), {
  kind: "assembly.opening",
  target: "timber-frame",
  width: 3.2,
});
await exercise("timber", revised, {
  kind: "assembly.timber",
  target: "timber-frame",
  material: "aged-oak",
  age: 0.45,
  grainScale: 1.4,
  bevel: 0.005,
});
const trestle = fastAuthoringProject(1.5, 0.7);
const doc = trestle.documents.find((d) => d.id === "timber-frame");
if (doc?.kind === "object" && doc.assembly) {
  doc.name = "Braced workbench trestle";
  doc.assembly.parts.forEach((p, i) => {
    p.id = `member-${i}`;
    p.name = p.id;
  });
  const brace = createAssemblyPart("diagonal-brace");
  brace.path = [
    [-0.7, 0.15, 0.15],
    [0.7, 0.65, 0.15],
  ];
  brace.profile = { kind: "rectangle", width: 0.08, height: 0.09 };
  doc.assembly.parts.push(brace);
}
await exercise("trestle", trestle, {
  kind: "assembly.timber",
  target: "timber-frame",
  material: "workbench-oak",
  age: 0.25,
  grainScale: 1.4,
  bevel: 0.004,
});
await Bun.write(
  join(output, "result.json"),
  JSON.stringify(
    {
      scope: "Scripted public-API integration on a gateway and braced trestle, not a fresh-agent benchmark",
      results,
    },
    null,
    2,
  ),
);
