import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import {
  AuthoringSession,
  adoptWork,
  attachWorkReview,
  createWorkSession,
  materializeWorkProposal,
  proposeWork,
} from "@wrela/authoring";
import { referenceProject } from "@wrela/examples";
import { contentKey } from "@wrela/model";
import { runAuthoringReview } from "./authoring-review-runner";

const output = resolve(process.argv[2] ?? `output/agent-authoring-smoke-${Date.now()}`);
await mkdir(output, { recursive: true });
const project = referenceProject(),
  camera = {
    position: [5, 3, 6] as [number, number, number],
    target: [0, 1.3, 0] as [number, number, number],
    fov: 45,
  };
let work = createWorkSession(project, {
  id: "hardware-smoke",
  brief: "Change coat response while preserving the realized silhouette",
  constraints: [
    { id: "shape", kind: "source", target: "polar-bunny", path: ["field"] },
    {
      id: "silhouette",
      kind: "silhouette",
      target: "polar-bunny",
      camera,
      width: 320,
      height: 240,
      maxChangedFraction: 0,
    },
    {
      id: "resources",
      kind: "resources",
      target: "polar-bunny",
      camera,
      width: 320,
      height: 240,
      frames: 30,
      maxCpuMs: 100,
      maxGpuMs: 100,
      maxGpuBytes: 512 * 1024 * 1024,
    },
  ],
});
work = proposeWork(work, "matte-coat", [
  { kind: "document.set", target: "snow-fur", path: ["roughness"], value: 0.71 },
]);
const report = await runAuthoringReview(
  work.baseline,
  materializeWorkProposal(work, work.proposals[0].id),
  work.constraints,
  output,
  true,
);
work = attachWorkReview(work, "matte-coat", report);
await Bun.write(join(output, "review.json"), JSON.stringify(report, null, 2));
await Bun.write(join(output, "work.json"), JSON.stringify(work, null, 2));
if (report.results.some((r) => r.status !== "passed"))
  throw Error(`Authoring smoke failed: ${JSON.stringify(report.results)}`);
const session = new AuthoringSession(project);
adoptWork(work, "matte-coat", session);
if (contentKey(session.getSnapshot().project) !== work.proposals[0].candidateKey)
  throw Error("Adoption differs from reviewed source");
console.log(
  JSON.stringify(
    {
      output,
      results: report.results,
      scope:
        "Scripted workflow/hardware integration; permissive resource limits, no artistic or agent-productivity verdict",
    },
    null,
    2,
  ),
);
