import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createHeroWorkspace, type ResultConstraint } from "@wrela/authoring";
import { type BenchmarkTask, benchmarkSuiteSchema } from "./agent-benchmark/protocol";
import { sourceManifest } from "./evidence";

/** Same wood/pine sources plus independent blank-workspace creation and shell transfer. */
export async function prepareHeroBenchmark(destination: string, previousDirectory: string) {
  const output = resolve(destination),
    previous = resolve(previousDirectory);
  if (await Bun.file(join(output, "suite.json")).exists())
    throw Error("Suite exists; never replace an attempt");
  const old = benchmarkSuiteSchema.parse(await Bun.file(join(previous, "suite.json")).json());
  const tasks = old.tasks.filter((t) => ["wood", "vegetation"].includes(t.id));
  const heroes = [
    {
      id: "lantern",
      family: "expedition lantern",
      brief:
        "Create an original expedition lantern from an empty workspace. Give it a distinctive, plausible silhouette: a hollow metal fuel reservoir, a protective frame, thin glass around a visible warm light source, a vented rain cap and a carrying handle. Compose these yourself using generic construction components; do not import a complete lantern design. Keep it about 0.55 metres tall. Use restrained brass/iron wear and clear material hierarchy. Review beauty, clay, grazing light, detail, motion and gameplay distance. Retain editable source and portable evidence.",
    },
    {
      id: "bell",
      family: "hanging signal bell",
      brief:
        "Create an original hanging signal bell from an empty workspace. Author a real hollow flared bronze body with a thick mouth, a suspended clapper, a forged metal hanger and a weathered timber mount. Compose these yourself using generic construction components; do not import a complete bell design. Keep it about 0.6 metres tall with deliberate proportions and plausible connections. Use restrained patina and material hierarchy. Review beauty, clay, grazing light, detail, motion and gameplay distance. Retain editable source and portable evidence.",
    },
  ];
  for (const hero of heroes) {
    const constraints: ResultConstraint[] = [
      {
        id: "geometry",
        kind: "geometry",
        target: `authored-${hero.id}`,
        minTriangles: 100,
        maxTriangles: 30000,
        minimumExtent: [0.1, 0.3, 0.1],
        maximumExtent: [1, 1.1, 1],
      },
      { id: "preserve-neutral", kind: "source", target: "workshop-neutral", path: [] },
    ];
    tasks.push({
      id: hero.id,
      category: "create",
      brief: hero.brief,
      acceptance: [
        "Recognizable original silhouette",
        "Plausible construction with real cavities",
        "Readable material hierarchy at gameplay distance",
        "No obvious intersections or floating parts",
        "Portable editable source and retained visual evidence",
      ],
      engines: ["wrela"],
      budget: { seconds: 600, tokens: 24000 },
      baseline: `${hero.id}/baseline.json`,
      workId: hero.id,
      subject: `authored-${hero.id}`,
      constraints,
    } satisfies BenchmarkTask);
  }
  const source = await sourceManifest("hero-authoring-benchmark"),
    suite = benchmarkSuiteSchema.parse({
      ...old,
      id: "hero-authoring-loop",
      createdAt: new Date().toISOString(),
      sourceFingerprint: source.sourceFingerprint,
      repetitions: 1,
      tasks,
    });
  for (const task of tasks) {
    await mkdir(join(output, task.id), { recursive: true });
    const baseline = ["wood", "vegetation"].includes(task.id)
      ? await Bun.file(join(previous, task.baseline)).json()
      : createHeroWorkspace();
    await Bun.write(join(output, task.baseline), JSON.stringify(baseline, null, 2));
    await Bun.write(
      join(output, task.id, "brief.json"),
      JSON.stringify(
        {
          task,
          conventions: { units: "metres", up: "+Y", rotation: "radians" },
          assetPolicy:
            "Generic component APIs and schema documentation allowed. Complete starting designs and asset-specific fixtures prohibited for hero creation.",
          artisticAcceptance: null,
        },
        null,
        2,
      ),
    );
  }
  await Bun.write(join(output, "suite.json"), JSON.stringify(suite, null, 2));
  await Bun.write(join(output, "source-manifest.json"), JSON.stringify(source, null, 2));
  return { output, sourceFingerprint: source.sourceFingerprint };
}
if (import.meta.main) console.log(await prepareHeroBenchmark(process.argv[2], process.argv[3]));
