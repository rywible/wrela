import { expect, test } from "bun:test";
import { AuthoringSession } from "@wrela/authoring";
import { compileAssemblyMesh } from "@wrela/compiler";
import { createAlpineCompositionEdits } from "@wrela/examples/alpine-composition";
import { createArchitectureLookdev } from "@wrela/examples/architecture-lookdev";
import { createAlpineLookdevProject } from "@wrela/examples/lookdev-scene";
import { contentKey, parseProject, validateWorldComposition } from "@wrela/model";
import { alpineSurfaceHistory } from "@wrela/model/surface-history";
import { samplePlantCommunity, worldPathClearance } from "@wrela/model/world-construction";

test("construction recipes keep mechanical gateway and held-out shelter standing clearances open", () => {
  for (const variant of ["gateway", "wayside"] as const) {
    const bundle = createArchitectureLookdev({ variant });
    const object = bundle.documents.find((document) => document.id === bundle.object);
    if (object?.kind !== "object" || !object.assembly) throw new Error("Missing construction");
    const compiled = compileAssemblyMesh(object.assembly, object.material);
    expect(
      compiled.diagnostics.filter((diagnostic) => diagnostic.code === "assembly.clearance-overlap"),
    ).toEqual([]);
    expect(compiled.mesh.indices.length / 3).toBeLessThan(25000);
    expect(object.assembly.parts.length).toBeLessThanOrEqual(128);
    expect(bundle.history.some((sample) => sample.class === "contact")).toBe(true);
    expect(bundle.history.some((sample) => sample.class === "broken")).toBe(true);
    if (variant === "gateway")
      expect(
        object.assembly.parts.some((part) => part.id === "gate-hinge-stile" && part.joint?.kind === "hinge"),
      ).toBe(true);
    else expect(object.assembly.parts.some((part) => part.id === "bench-seat")).toBe(true);
  }
});

test("plant communities remain outside a curved walking core and repeat by stable identity", () => {
  const paths = [
    {
      points: [
        [-3, 0, -3],
        [0, 0, 0],
        [3, 0, -3],
      ] as [number, number, number][],
      width: 1.5,
      cornerRadius: 1.2,
    },
  ];
  const options = {
    id: "test-verge",
    center: [0, 0] as [number, number],
    radius: [3, 3] as [number, number],
    seed: 91,
    counts: { grass: 30, fern: 6, shrub: 3 },
    history: alpineSurfaceHistory(),
    paths,
  };
  const first = samplePlantCommunity(options),
    second = samplePlantCommunity(options);
  expect(first).toEqual(second);
  expect(first.length).toBeGreaterThan(10);
  for (const plant of first)
    expect(worldPathClearance(plant.position, paths)).toBeGreaterThanOrEqual(
      plant.species === "shrub" ? 0.8 : 0.22,
    );
});

test("primary and river-bend compositions use ordinary atomic authoring edits with undo and stable sources", () => {
  const project = parseProject(createAlpineLookdevProject());
  const before = contentKey(project);
  const session = new AuthoringSession(project);
  const operations = createAlpineCompositionEdits(project, "river-bend");
  expect(contentKey(project)).toBe(before);
  session.apply({ expectedRevision: 0, operations });
  const heldout = session.getSnapshot().project;
  const world = heldout.documents.find((document) => document.id === heldout.entry);
  if (world?.kind !== "world" || !world.composition) throw new Error("Missing edited world");
  const errors = validateWorldComposition(
    world.composition,
    world.populations.map((population) => population.id),
    { instanceIds: world.instances.map((instance) => instance.id) },
  ).filter((issue) => issue.severity === "error");
  expect(errors).toEqual([]);
  expect(world.composition.paths[0].points[0][0]).toBe(-3.8);
  expect(contentKey(parseProject(JSON.parse(JSON.stringify(heldout))))).toBe(contentKey(heldout));
  session.undo();
  expect(contentKey(session.getSnapshot().project)).toBe(before);
  session.redo();
  expect(contentKey(session.getSnapshot().project)).toBe(contentKey(heldout));
});

test("history edits keep module identities while changing geometry and visible surface treatment", () => {
  const dry = createArchitectureLookdev({
    history: alpineSurfaceHistory({ damage: 0.05, ageYears: 10, prevailingWetness: 0.02 }),
  });
  const worn = createArchitectureLookdev({
    history: alpineSurfaceHistory({ damage: 0.7, ageYears: 300, prevailingWetness: 0.7 }),
  });
  const a = dry.documents.find((document) => document.id === dry.object),
    b = worn.documents.find((document) => document.id === worn.object);
  if (a?.kind !== "object" || b?.kind !== "object" || !a.assembly || !b.assembly)
    throw new Error("Missing construction");
  expect(a.assembly.parts.map((part) => part.id)).toEqual(b.assembly.parts.map((part) => part.id));
  expect(a.assembly.parts[0].profile).not.toEqual(b.assembly.parts[0].profile);
  const materialA = dry.documents.find((document) => document.id === "gateway-history-masonry-contact");
  const materialB = worn.documents.find((document) => document.id === "gateway-history-masonry-contact");
  if (materialA?.kind !== "material" || materialB?.kind !== "material")
    throw new Error("Missing history material");
  expect(materialB.appearance?.wetness ?? 0).toBeGreaterThan(materialA.appearance?.wetness ?? 0);
  expect(materialB.appearance?.relief?.amplitude ?? 0).toBeGreaterThan(
    materialA.appearance?.relief?.amplitude ?? 0,
  );
  expect(a.assembly.clearances).toEqual(b.assembly.clearances);
});
