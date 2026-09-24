import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { domainQuality, type ResultConstraint } from "@wrela/authoring";
import { parseProject } from "@wrela/model";
import { type BenchmarkTask, benchmarkSuiteSchema } from "./agent-benchmark/protocol";
import { sourceManifest } from "./evidence";
import { transferTree } from "./fixtures/authoring-transfer";

/** Preregister one existing wood task and one new growth task; no native-engine ranking. */
export async function prepareTransferBenchmark(output: string, previousSuite: string) {
  const root = resolve(output),
    previous = resolve(previousSuite);
  if (await Bun.file(join(root, "suite.json")).exists()) throw Error("Transfer suite already exists");
  const old = benchmarkSuiteSchema.parse(await Bun.file(join(previous, "suite.json")).json());
  const wood = parseProject(await Bun.file(join(previous, "attempts/revise-wrela-0/final.json")).json());
  const vegetation = transferTree();
  const handoff = old.tasks.find((t) => t.id === "handoff");
  if (!handoff) throw Error("Existing wood task required");
  const treeConstraints: ResultConstraint[] = [
    {
      id: "tree-geometry",
      kind: "geometry",
      target: vegetation.entry,
      minTriangles: 100,
      maxTriangles: 10_000_000,
      minimumExtent: [1, 3, 1],
      maximumExtent: [8, 7, 8],
    },
    ...["seed", "height", "radius", "trunkMaterial", "material"].map((key) => ({
      id: `preserve-${key}`,
      kind: "source" as const,
      target: vegetation.entry,
      path: [key],
    })),
    ...["species", "pruning", "branchEdits"].map((key) => ({
      id: `preserve-${key}`,
      kind: "source" as const,
      target: vegetation.entry,
      path: ["botanical", key],
    })),
  ];
  const tasks: BenchmarkTask[] = [
    {
      ...handoff,
      id: "wood",
      workId: "wood",
      subject: "benchmark-gate",
      prerequisite: undefined,
      engines: ["wrela"],
      baseline: "wood/baseline.json",
      brief:
        "Continue the provided accepted gate revision. Make its timber convincingly weathered with restrained silvering, grain following each member, end grain across cut faces and moisture staining near the ground. Preserve the accepted dimensions, attachments, shared source materials and route. Compare bounded alternatives in fixed neutral and grazing light, inspect their images, select the strongest result, and retain an editable portable handoff.",
      acceptance: [...domainQuality("timber").criteria, "Retained source and evidence reopen successfully"],
    },
    {
      id: "vegetation",
      category: "revise",
      workId: "vegetation",
      subject: vegetation.entry,
      engines: ["wrela"],
      budget: handoff.budget,
      baseline: "vegetation/baseline.json",
      brief:
        "Refine the provided trail pine into a mature exposed tree with coherent crown asymmetry, readable branch hierarchy and crown gaps, and enough foliage to feel alive. Preserve its species, seed, source height/radius, material ownership, pruning and manual branch edits. Compare bounded alternatives in fixed neutral and grazing light with a canopy detail view. Inspect the images, select the strongest result and retain an editable portable handoff. These are authored growth controls, not a calibrated botanical simulation.",
      acceptance: [
        ...domainQuality("vegetation").criteria,
        "Retained source and evidence reopen successfully",
      ],
      constraints: treeConstraints,
    },
  ];
  const manifest = await sourceManifest("authoring-domain-transfer");
  const suite = benchmarkSuiteSchema.parse({
    ...old,
    id: "authoring-domain-transfer",
    createdAt: new Date().toISOString(),
    repetitions: 1,
    sourceFingerprint: manifest.sourceFingerprint,
    tasks,
  });
  for (const [i, task] of tasks.entries()) {
    await mkdir(join(root, task.id), { recursive: true });
    await Bun.write(join(root, task.baseline), JSON.stringify(i === 0 ? wood : vegetation, null, 2));
    await Bun.write(
      join(root, task.id, "brief.json"),
      JSON.stringify(
        {
          task,
          quality: domainQuality(i === 0 ? "timber" : "vegetation"),
          views:
            "Use the shared study defaults: neutral whole subject, neutral detail, grazing detail. Same cameras and lighting for every baseline and candidate.",
          scope:
            "One fresh agent per domain; no independent art acceptance. Wood reuses the previous revision source and geometric checks. New study uses stricter fixed-light review than the previous weathering run.",
        },
        null,
        2,
      ),
    );
  }
  await Bun.write(join(root, "suite.json"), JSON.stringify(suite, null, 2));
  await Bun.write(join(root, "source-manifest.json"), JSON.stringify(manifest, null, 2));
  return { root, sourceFingerprint: manifest.sourceFingerprint, tasks: tasks.map((t) => t.id) };
}
if (import.meta.main)
  console.log(JSON.stringify(await prepareTransferBenchmark(process.argv[2], process.argv[3])));
