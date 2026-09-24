import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { AuthoringSession } from "@wrela/authoring";
import { compileAssemblyMesh, createTerrainSampler } from "@wrela/compiler";
import { createAlpineCompositionEdits } from "@wrela/examples/alpine-composition";
import { createArchitectureLookdev } from "@wrela/examples/architecture-lookdev";
import { createAlpineLookdevProject } from "@wrela/examples/lookdev-scene";
import { contentKey, parseProject, sampleWorldPolyline, worldPathPolyline } from "@wrela/model";
import { realizeWorldComposition } from "@wrela/world";

const revision =
  process.argv.find((argument) => argument.startsWith("--revision="))?.slice(11) ?? "history-v1";
if (!/^[a-z0-9-]+$/.test(revision)) throw new Error("Use a simple revision name");
const output = resolve("output/alpine-construction", revision);
await mkdir(output, { recursive: true });
const source = parseProject(createAlpineLookdevProject());
const session = new AuthoringSession(source);
const operations = createAlpineCompositionEdits(source, "river-bend");
session.apply({ expectedRevision: 0, operations });
const heldout = session.getSnapshot().project;
const before = contentKey(source),
  after = contentKey(heldout);
session.undo();
const undoMatches = contentKey(session.getSnapshot().project) === before;
session.redo();
const redoMatches = contentKey(session.getSnapshot().project) === after;
const reports = [];
for (const [variant, project] of [
  ["primary", source],
  ["river-bend", heldout],
] as const) {
  const world = project.documents.find((document) => document.id === project.entry);
  if (world?.kind !== "world" || !world.composition) throw new Error("Missing composed world");
  const terrain = project.documents.find((document) => document.id === world.terrain);
  const architecture = project.documents.find((document) => document.id === "alpine-lookdev-gateway");
  if (terrain?.kind !== "terrain" || architecture?.kind !== "object" || !architecture.assembly)
    throw new Error("Missing construction source");
  const realized = realizeWorldComposition(world, terrain);
  const sampler = createTerrainSampler(realized.terrain);
  const compiled = compileAssemblyMesh(architecture.assembly, architecture.material);
  const route = world.composition.paths.flatMap((path, index) => {
    const points = worldPathPolyline(path);
    return index ? points.slice(1) : points;
  });
  const probes = sampleWorldPolyline(route, 121).map(({ position }) => ({
    position,
    height: sampler.height(position[0], position[2]),
  }));
  const grades = probes
    .slice(1)
    .map(
      (probe, index) =>
        Math.abs(probe.height - probes[index].height) /
        Math.hypot(
          probe.position[0] - probes[index].position[0],
          probe.position[2] - probes[index].position[2],
        ),
    );
  const materialIds = [...new Set(architecture.assembly.parts.map((part) => part.material))];
  const materialEvidence = materialIds.map((id) => {
    const material = project.documents.find((document) => document.id === id);
    if (material?.kind !== "material") throw new Error(`Missing construction material ${id}`);
    return { id, appearance: material.appearance };
  });
  reports.push({
    variant,
    sourceKey: contentKey(project),
    documents: project.documents.length,
    sourceInstances: world.instances.length,
    realizedInstances: realized.world.instances.length,
    sourceParts: architecture.assembly.parts.length,
    sourceTriangles: compiled.mesh.indices.length / 3,
    materials: materialEvidence,
    bounds: compiled.mesh.bounds,
    structuralDiagnostics: compiled.diagnostics,
    compositionDiagnostics: realized.diagnostics,
    route: {
      points: probes.length,
      maximumSampledGrade: Math.max(...grades),
      maximumHeightDeparture: Math.max(...probes.map((probe) => Math.abs(probe.height - probe.position[1]))),
      probes,
    },
  });
  await Bun.write(resolve(output, `${variant}-project.json`), JSON.stringify(project, null, 2));
}
const history = createArchitectureLookdev();
await Bun.write(
  resolve(output, "report.json"),
  JSON.stringify(
    {
      revision,
      reports,
      operations,
      history: { recipe: history.recipe, parts: history.history },
      sourceEditing: {
        operations: operations.length,
        undoMatches,
        redoMatches,
        parsedSaveMatches: contentKey(parseProject(JSON.parse(JSON.stringify(heldout)))) === after,
      },
      scope:
        "CPU source, assembly-clearance and analytic terrain-route evidence. No rendered appearance, triangle collision traversal, runtime residency, or AAA acceptance is certified.",
    },
    null,
    2,
  ),
);
console.log(
  JSON.stringify({
    output,
    undoMatches,
    redoMatches,
    reports: reports.map(
      ({ variant, sourceParts, sourceTriangles, realizedInstances, route, structuralDiagnostics }) => ({
        variant,
        sourceParts,
        sourceTriangles,
        realizedInstances,
        maximumSampledGrade: route.maximumSampledGrade,
        clearanceWarnings: structuralDiagnostics.length,
      }),
    ),
  }),
);
