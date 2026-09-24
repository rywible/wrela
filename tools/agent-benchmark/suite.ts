import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createDoorwayAssembly, createWorkSession, type ResultConstraint } from "@wrela/authoring";
import { compileAssemblyMesh } from "@wrela/compiler";
import { referenceProject, shape } from "@wrela/examples";
import { contentKey, emptyWorldComposition, type ObjectDefinition, parseProject } from "@wrela/model";
import { WorkFileStore } from "../authoring-work-store";
import { WorkspaceBridge } from "../bridge";
import { sourceManifest } from "../evidence";
import { type BenchmarkSuite, type BenchmarkTask, benchmarkSuiteSchema } from "./protocol";

export async function prepareBenchmark(
  output: string,
  options: { seed?: number; model: string; repetitions?: number; seconds?: number; tokens?: number },
) {
  const directory = resolve(output);
  if (await Bun.file(join(directory, "suite.json")).exists())
    throw Error("Benchmark suite already exists; use a new directory");
  await mkdir(directory, { recursive: true });
  const seed = options.seed ?? 7919,
    width = 1.8 + (Math.abs(seed) % 7) / 10;
  const base = referenceProject();
  const gate: ObjectDefinition = {
    id: "benchmark-gate",
    kind: "object",
    name: "Trail gate",
    schemaVersion: 1,
    dependencies: [],
    material: "bark",
    collision: "mesh",
    assembly: createDoorwayAssembly(width, 2.6, 0.25),
    field: {
      root: "envelope",
      resolution: 24,
      bounds: { min: [-3, 0, -1], max: [3, 3, 1] },
      nodes: [shape("envelope", "Envelope", [0, 1.4, 0], [3, 1.4, 0.4], "box")],
    },
  };
  base.documents.push(gate);
  const world = base.documents.find((d) => d.kind === "world");
  if (!world) throw Error("Benchmark needs a world");
  world.composition = emptyWorldComposition();
  world.composition.paths = [
    {
      id: "trail",
      kind: "path",
      points: [
        [0, 0, -8],
        [0, 0, 8],
      ],
      width: 1.2,
      shoulder: 1,
      flatten: true,
      spacing: 2,
      maxGrade: 0.25,
    },
  ];
  world.composition.review = {
    actorRadius: 0.35,
    actorHeight: 1.8,
    maxStepHeight: 0.3,
    sightline: { from: [0, 1.4, -5], to: [0, 1.4, 5] },
  };
  world.instances = [{ id: "gate", definition: gate.id, position: [0, 0, 0], rotation: [0, 0, 0], scale: 1 }];
  world.populations = [];
  delete world.water;
  delete world.waters;
  const terrain = base.documents.find((d) => d.id === world.terrain);
  if (terrain?.kind === "terrain")
    terrain.interventions = [
      { id: "level", kind: "flatten", center: [0, 0], radius: 100, strength: 1, targetHeight: 0 },
    ];
  const project = parseProject(base);
  const budget = { seconds: options.seconds ?? 600, tokens: options.tokens ?? 16000 };
  const briefs: Omit<BenchmarkTask, "constraints">[] = [
    {
      id: "create",
      category: "create",
      brief: `Create a distinctive trail gateway with a ${width.toFixed(1)} metre clear opening and 2.6 metre headroom. Use a readable original silhouette and two visibly different material regions. Source must remain editable.`,
      acceptance: [
        "Recognizable intentional design in three views",
        "Trail remains clear",
        "Save/reopen preserves authored source",
      ],
      engines: ["wrela", "blender", "unreal"],
      budget,
      baseline: "create/baseline.json",
      workId: "create",
    },
    {
      id: "revise",
      category: "revise",
      brief: `Widen the provided gate opening to ${(width + 0.55).toFixed(2)} metres. Preserve 2.6 metre headroom, beam thickness, material ownership, and attachment relationships.`,
      acceptance: [
        "Required opening dimensions",
        "No floating or disconnected structural parts",
        "Trail remains clear",
      ],
      engines: ["wrela", "blender", "unreal"],
      budget,
      baseline: "revise/baseline.json",
      workId: "revise",
    },
    {
      id: "repair",
      category: "repair",
      brief:
        "The trail gate has a misplaced right support. Diagnose the defect using rendered evidence, restore its connection and doorway access, and preserve the other structural parts. Retain evidence of the diagnosis and repair.",
      acceptance: [
        "Defect visibly repaired",
        "Other parts unchanged",
        "No compensating lighting or camera change",
      ],
      engines: ["wrela", "blender", "unreal"],
      budget,
      baseline: "repair/baseline.json",
      workId: "repair",
    },
    {
      id: "compose",
      category: "compose",
      brief:
        "Create a sheltered approach to this gate using procedural vegetation and a deliberate curved trail. Preserve an open view through the doorway from the approach. Keep the route traversable and the gate visually dominant.",
      acceptance: [
        "Legible route and focal point",
        "Traversal and declared sightline pass",
        "Coherent composition in three fixed views",
      ],
      engines: ["wrela", "unreal"],
      budget,
      baseline: "compose/baseline.json",
      workId: "compose",
    },
    {
      id: "handoff",
      category: "handoff",
      brief:
        "Continue the completed revision using only its source, portable work record and feedback. Make the gateway look weathered while preserving the accepted dimensions and route. Explain which earlier decisions you retained and record new evidence.",
      acceptance: [
        "Fresh agent uses retained decisions",
        "Accepted geometry preserved",
        "Weathering reads in neutral and grazing light",
      ],
      engines: ["wrela", "blender", "unreal"],
      budget,
      baseline: "handoff/baseline.json",
      workId: "handoff",
      prerequisite: "revise",
    },
  ];
  const tasks: BenchmarkTask[] = briefs.map((task) => {
    const opening = task.category === "revise" || task.category === "handoff" ? width + 0.55 : width;
    const constraints: ResultConstraint[] = [
      { id: "trail", kind: "route", target: world.id, route: "trail" },
      { id: "view", kind: "sightline", target: world.id, from: [0, 1.4, -5], to: [0, 1.4, 5] },
      {
        id: "doorway",
        kind: "clearance",
        target: gate.id,
        minimum: [-opening / 2 + 0.002, 0.002, -0.12],
        maximum: [opening / 2 - 0.002, 2.598, 0.12],
      },
    ];
    if (task.category === "revise" || task.category === "repair" || task.category === "handoff") {
      const assembly = createDoorwayAssembly(opening, 2.6, 0.25);
      for (const part of assembly.parts) {
        const { mesh } = compileAssemblyMesh({ ...assembly, parts: [part], clearances: [] }, "bark");
        constraints.push({
          id: `bounds-${part.id}`,
          kind: "dimensions",
          target: gate.id,
          part: part.id,
          minimum: mesh.bounds.min,
          maximum: mesh.bounds.max,
          tolerance: 0.005,
        });
      }
      constraints.push({ id: "material-owner", kind: "source", target: gate.id, path: ["material"] });
      for (let index = 0; index < 3; index++)
        for (const property of ["profile", "rotation", "sockets"])
          constraints.push({
            id: `preserve-${index}-${property}`,
            kind: "source",
            target: gate.id,
            path: ["assembly", "parts", index, property],
          });
    }
    if (task.category === "repair")
      for (const index of [0, 2])
        constraints.push({
          id: `preserve-part-${index}`,
          kind: "source",
          target: gate.id,
          path: ["assembly", "parts", index],
        });
    if (task.category === "compose")
      constraints.push({ id: "preserve-gate", kind: "source", target: gate.id, path: ["assembly"] });
    return { ...task, constraints };
  });
  const source = await sourceManifest("agent-authoring-benchmark");
  const suite: BenchmarkSuite = benchmarkSuiteSchema.parse({
    version: 1,
    id: `authoring-${Math.abs(seed)}`,
    seed,
    createdAt: new Date().toISOString(),
    sourceFingerprint: source.sourceFingerprint,
    repetitions: options.repetitions ?? 3,
    policy: {
      assets: "provided-only",
      track: "content",
      model: options.model,
      engineAccess: "best-available-programmatic",
      artReview: "blind-independent",
      delivery: "separate",
    },
    tasks,
  });
  for (const task of tasks) {
    const dir = join(directory, task.id),
      baseline = structuredClone(project);
    if (task.category === "repair") {
      const object = baseline.documents.find((d) => d.id === gate.id);
      if (object?.kind === "object" && object.assembly) object.assembly.parts[1].position[0] = 0.15;
    }
    await mkdir(dir, { recursive: true });
    await Bun.write(join(dir, "baseline.json"), JSON.stringify(baseline, null, 2));
    const nativeGate = baseline.documents.find((d) => d.id === gate.id);
    if (nativeGate?.kind !== "object" || !nativeGate.assembly)
      throw Error("Native starter is missing its structure");
    const nativeAssembly = nativeGate.assembly;
    const neutral = {
      version: 1,
      units: "metres",
      up: "+Y",
      handedness: "right",
      object: nativeGate.id,
      parts: nativeAssembly.parts.map((part) => {
        const { mesh } = compileAssemblyMesh(
          { ...nativeAssembly, parts: [part], clearances: [] },
          nativeGate.material,
        );
        return {
          id: part.id,
          positions: Array.from(mesh.positions),
          indices: Array.from(mesh.indices),
          bounds: mesh.bounds,
          material: part.material ?? nativeGate.material,
          sockets: part.sockets,
          semantics: part,
        };
      }),
      materials: baseline.documents.filter((d) => d.kind === "material"),
      world: {
        groundHeight: 0,
        route: world.composition.paths[0],
        view: world.composition.review?.sightline,
      },
      note: "Engine-neutral editable starter for native scripting. Keep named structural parts for attribution. Convert coordinates explicitly; do not substitute Wrela renders for native results.",
    };
    await Bun.write(join(dir, "neutral-starter.json"), JSON.stringify(neutral, null, 2));
    const bridge = new WorkspaceBridge(join(dir, "workspace"));
    await bridge.initialize();
    await bridge.save(baseline, null);
    const work = createWorkSession(baseline, {
      id: task.workId,
      brief: task.brief,
      constraints: task.constraints,
    });
    await new WorkFileStore(join(dir, "workspace")).put(work, null);
    await Bun.write(
      join(dir, "brief.json"),
      JSON.stringify(
        {
          suiteKey: contentKey(suite),
          task,
          dimensions: {
            openingWidth: width,
            revisedOpeningWidth: width + 0.55,
            headroom: 2.6,
            beamThickness: 0.25,
          },
          conventions: { units: "metres", up: "+Y", handedness: "right", frameRate: 60 },
          cameras: [
            { position: [5, 4, 7], target: [0, 1.4, 0], fov: 45 },
            { position: [0, 2, 7], target: [0, 1.4, 0], fov: 45 },
            { position: [-5, 3, 4], target: [0, 1.4, 0], fov: 45 },
          ],
          workflow:
            "Use docs/architecture/agent-authoring.md. An agent runner starts a fresh context per attempt. Content track forbids engine edits. Record all failed iterations and review the final source. Artistic acceptance is independent.",
        },
        null,
        2,
      ),
    );
  }
  await Bun.write(join(directory, "suite.json"), JSON.stringify(suite, null, 2));
  await Bun.write(join(directory, "source-manifest.json"), JSON.stringify(source, null, 2));
  return { directory, suiteKey: contentKey(suite), tasks: tasks.map((t) => t.id) };
}
