import { expect, test } from "bun:test";
import { buildGeologyObject } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import { defaultTerrainGeology, type TerrainDefinition, type WorldDefinition } from "@wrela/model";
import { createGeologyPreview } from "./geology-preview";
import { BrowserSceneHost } from "./scene-host";

function fixture() {
  const project = referenceProject();
  const terrain = project.documents.find(
    (document): document is TerrainDefinition => document.kind === "terrain",
  );
  const world = project.documents.find((document): document is WorldDefinition => document.kind === "world");
  if (!terrain || !world) throw new Error("Reference terrain or world missing");
  terrain.amplitude = 0;
  terrain.baseHeight = 0;
  terrain.interventions = [];
  terrain.geology = defaultTerrainGeology();
  terrain.geology.formations = [
    { id: "passage", kind: "cave", position: [0, 0, 0], size: [12, 8, 10], opening: 0.65, resolution: 32 },
  ];
  const published = buildGeologyObject(terrain, terrain.geology.formations[0]);
  project.documents.push(published);
  world.populations = [];
  world.water = undefined;
  world.instances = [
    {
      id: `placed-${published.id}`,
      definition: published.id,
      position: [0, 0, 0],
      rotation: [0, 0, 0],
      scale: 1,
    },
  ];
  return { project, terrain, world, published };
}

test("terrain preview replaces only generated placements in an isolated map and preserves published source", () => {
  const { project, terrain, world, published } = fixture();
  if (!terrain.geology) throw new Error("Missing geology");
  world.instances.push({ ...world.instances[0], id: "manual-copy", position: [40, 0, 0] });
  terrain.geology.formations[0].position = [20, 0, 0];
  terrain.geology.formations[0].size[1] = 12;
  const original = structuredClone(project);
  const preview = createGeologyPreview(project, terrain, world);
  expect(project).toEqual(original);
  expect(preview.world.id).not.toBe(world.id);
  expect(preview.world.instances.length).toBe(2);
  expect(
    preview.world.instances.find((instance) => instance.id === `placed-${published.id}`)?.position,
  ).toEqual([20, 0, 0]);
  expect(preview.world.instances.find((instance) => instance.id === "manual-copy")?.position).toEqual([
    40, 0, 0,
  ]);
  expect(preview.documents.get(published.id)).not.toEqual(published);
  expect(project.documents.find((document) => document.id === published.id)).toEqual(published);
  terrain.geology.formations = [];
  expect(
    createGeologyPreview(project, terrain, world).world.instances.map((instance) => instance.id),
  ).toEqual(["manual-copy"]);
});

test("terrain preview shows unpublished placement and geometry edits while actual worlds retain their bake", async () => {
  const { project, terrain, world, published } = fixture();
  const host = new BrowserSceneHost(project);
  const camera = {
    position: [0, 3, -12] as [number, number, number],
    target: [0, 3, 0] as [number, number, number],
    fov: 50,
  };
  const placementId = `placed-${published.id}`;
  try {
    await host.prepare(terrain.id);
    host.advance(1 / 60);
    const first = host.extract(camera).surfaces.find((surface) => surface.id === placementId);
    expect(first).toBeDefined();
    expect(host.runtime?.physics.raycast([5, 1.5, -10], [0, 0, 1], 20)?.id).toBe(placementId);
    const changed = structuredClone(project);
    const edited = changed.documents.find(
      (document): document is TerrainDefinition => document.id === terrain.id && document.kind === "terrain",
    );
    if (!edited?.geology) throw new Error("Missing edited terrain");
    edited.geology.formations[0].position = [20, 0, 0];
    edited.geology.formations[0].size[1] = 12;
    await host.setProject(changed);
    host.advance(1 / 60);
    const second = host.extract(camera).surfaces.find((surface) => surface.id === placementId);
    expect(second?.mesh.bounds.max[1]).toBeGreaterThan((first?.mesh.bounds.max[1] ?? 0) + 3);
    expect(host.world?.world.instances.find((instance) => instance.id === placementId)?.position).toEqual([
      20, 0, 0,
    ]);
    expect(host.runtime?.physics.raycast([5, 1.5, -10], [0, 0, 1], 20)).toBeNull();
    expect(host.runtime?.physics.raycast([25, 1.5, -10], [0, 0, 1], 20)?.id).toBe(placementId);
    await host.prepare(world.id);
    host.advance(1 / 60);
    const baked = host.extract(camera).surfaces.find((surface) => surface.id === placementId);
    expect(baked?.mesh.bounds.max[1]).toBeCloseTo(first?.mesh.bounds.max[1] ?? -1, 4);
    expect(host.world?.world.instances.find((instance) => instance.id === placementId)?.position).toEqual([
      0, 0, 0,
    ]);
    expect(host.runtime?.physics.raycast([5, 1.5, -10], [0, 0, 1], 20)?.id).toBe(placementId);
    expect(changed.documents.find((document) => document.id === published.id)).toEqual(published);
  } finally {
    host.dispose();
  }
});
