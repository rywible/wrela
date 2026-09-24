import { expect, test } from "bun:test";
import { alpinePineLookdevDefinition } from "@wrela/examples";
import { botanicalDevelopmentSchema } from "@wrela/model";
import {
  prepareVegetationGrowth,
  validateVegetationGrowth,
  vegetationGrowthDocument,
} from "./vegetation-growth";

const tree = () => {
  const doc = alpinePineLookdevDefinition();
  if (!doc.botanical) throw Error("Fixture");
  doc.botanical.development = botanicalDevelopmentSchema.parse({ species: "lodgepole-pine", steps: 8 });
  return doc;
};
test("persistent growth continues after JSON save and retains source identity", () => {
  const doc = tree();
  const initial = prepareVegetationGrowth(doc, 8);
  const resumed = prepareVegetationGrowth(doc, 12, JSON.parse(JSON.stringify(initial)));
  const direct = prepareVegetationGrowth(doc, 12);
  expect(resumed).toEqual(direct);
  const generated = vegetationGrowthDocument(doc, resumed, "tree-occurrence");
  expect(generated.id).not.toBe(doc.id);
  expect(generated.botanical?.development?.steps).toBe(12);
  expect(doc.botanical?.development?.steps).toBe(8);
  const changed = { ...doc, seed: doc.seed + 1 };
  expect(() => validateVegetationGrowth(changed, resumed)).toThrow("mismatch");
});
test("future pruning extends history atomically, malformed checkpoints fail", () => {
  const doc = tree(),
    original = prepareVegetationGrowth(doc, 8);
  const organ = original.checkpoint.shoots.find((shoot) => shoot.order === 1);
  if (!organ) throw Error("Missing lateral");
  const pruned = prepareVegetationGrowth(doc, 10, original, [
    { kind: "prune", id: "prune", step: 9, organ: organ.id },
  ]);
  expect(pruned.checkpoint.shoots.find((s) => s.id === organ.id)?.state).toBe("pruned");
  expect(original.development.events).toEqual([]);
  const bad = structuredClone(pruned);
  bad.checkpoint.shoots[1].parent = "missing";
  expect(() => validateVegetationGrowth(doc, bad)).toThrow();
});

test("a pruned runtime tree survives save, reload, continued growth and failed edits", async () => {
  const { referenceProject } = await import("@wrela/examples");
  const { BrowserSceneHost } = await import("./scene-host");
  const project = referenceProject(),
    doc = tree();
  const world = project.documents.find((d) => d.kind === "world"),
    terrain = project.documents.find((d) => d.kind === "terrain");
  if (!world || !terrain) throw Error("World fixture");
  world.populations = [];
  world.instances = [
    { id: "persistent-tree", definition: doc.id, position: [0, 0, 0], rotation: [0, 0, 0], scale: 1 },
  ];
  world.water = undefined;
  world.waters = [];
  world.composition = undefined;
  terrain.amplitude = 0;
  terrain.baseHeight = 0;
  terrain.interventions = [];
  terrain.geology = undefined;
  project.documents.push(doc);
  const host = new BrowserSceneHost(project),
    restored = new BrowserSceneHost(project);
  const camera = {
    position: [10, 7, 12] as [number, number, number],
    target: [0, 3, 0] as [number, number, number],
    fov: 42,
  };
  try {
    await host.prepare(world.id, undefined, "interactive");
    const originalSave = host.saveRuntime();
    const originalBytes = host.resourceUsage.installedBytes;
    const grown = await host.growVegetation("persistent-tree", doc.id, 8);
    const organ = grown.checkpoint.shoots.find((s) => s.order === 1);
    if (!organ) throw Error("Missing limb");
    const pruned = await host.growVegetation("persistent-tree", doc.id, 10, [
      { id: "cut", kind: "prune", step: 9, organ: organ.id },
    ]);
    const save = JSON.parse(JSON.stringify(host.saveRuntime()));
    await restored.prepare(world.id, undefined, "interactive");
    await restored.loadRuntime(save);
    const branchMesh = (h: InstanceType<typeof BrowserSceneHost>) =>
      h
        .extract(camera)
        .surfaces.filter((s) => s.instanceId === "persistent-tree")
        .map((s) => ({ id: s.id, positions: [...s.mesh.positions], source: s.source }));
    expect(branchMesh(restored)).toEqual(branchMesh(host));
    const continued = await restored.growVegetation("persistent-tree", doc.id, 12);
    expect(continued).toEqual(prepareVegetationGrowth(doc, 12, pruned));
    const before = restored.saveRuntime();
    await expect(
      restored.growVegetation("persistent-tree", doc.id, 14, [
        { id: "bad", kind: "prune", step: 13, organ: "missing" },
      ]),
    ).rejects.toThrow();
    expect(restored.saveRuntime()).toEqual(before);
    await restored.loadRuntime(originalSave);
    expect(restored.resourceUsage.installedBytes).toBeLessThanOrEqual(originalBytes);
    expect(
      restored
        .extract(camera)
        .surfaces.filter((s) => s.instanceId === "persistent-tree")
        .every((s) => s.source === doc.id),
    ).toBe(true);
  } finally {
    host.dispose();
    restored.dispose();
  }
}, 30000);
