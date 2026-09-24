import { expect, test } from "bun:test";
import { compileDocument } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import { emptyWorldComposition, type Vec3 } from "@wrela/model";
import { BrowserSceneHost } from "./scene-host";

function fixture() {
  const project = referenceProject();
  const world = project.documents.find((document) => document.kind === "world");
  const terrain = project.documents.find((document) => document.kind === "terrain");
  const object = project.documents.find(
    (document) => document.id === "river-stone" && document.kind === "object",
  );
  if (!world || !terrain || object?.kind !== "object") throw new Error("Missing world fixture");
  world.populations = [];
  world.instances = [];
  world.water = undefined;
  terrain.amplitude = 0;
  terrain.baseHeight = 0;
  terrain.interventions = [];
  terrain.geology = undefined;
  object.field.resolution = 12;
  const composition = emptyWorldComposition();
  world.composition = composition;
  composition.assemblies.push({
    id: "landmark",
    name: "Landmark",
    members: [{ id: "stone", definition: object.id, position: [1, 0, 0], yaw: 0, scale: 1 }],
  });
  composition.placements.push({ id: "first", assembly: "landmark", position: [4, 1, 0], yaw: 0, scale: 1 });
  composition.paths.push({
    id: "road",
    kind: "road",
    points: [
      [-8, 3, 0],
      [8, 3, 0],
    ],
    width: 4,
    shoulder: 1,
    spacing: 4,
    flatten: true,
    maxGrade: 0.25,
  });
  return { project, world, terrain, object, composition };
}
const camera = { position: [10, 8, 10] as Vec3, target: [0, 0, 0] as Vec3, fov: 42 };
test("scene host compiles composition-only assets and renders generated transforms", async () => {
  const { project, world, object } = fixture();
  const compiled: string[] = [];
  const host = new BrowserSceneHost(project, {
    compile: async (document, quality) => {
      compiled.push(document.id);
      return compileDocument(document, quality);
    },
  });
  try {
    await host.prepare(world.id);
    expect(compiled).toContain(object.id);
    const surface = host
      .extract(camera)
      .surfaces.find((item) => item.instanceId === "layout_assembly_first_stone");
    expect(surface?.source).toBe(object.id);
    expect(surface?.mesh.indices.length).toBeGreaterThan(0);
    expect(surface?.matrix[12]).toBeCloseTo(5);
    expect(surface?.matrix[13]).toBeCloseTo(1);
    const next = structuredClone(project),
      changed = next.documents.find((document) => document.id === world.id);
    if (changed?.kind !== "world" || !changed.composition) throw new Error("Missing edited composition");
    changed.composition.placements[0].position[0] = 9;
    await host.setProject(next);
    expect(
      host.extract(camera).surfaces.find((item) => item.instanceId === "layout_assembly_first_stone")
        ?.matrix[12],
    ).toBeCloseTo(10);
    const editedObject = next.documents.find((document) => document.id === object.id);
    if (editedObject?.kind !== "object") throw new Error("Missing object");
    editedObject.field.resolution = 14;
    const calls = compiled.length;
    await host.setProject(next);
    expect(compiled.length).toBeGreaterThan(calls);
  } finally {
    host.dispose();
  }
}, 20000);

test("live terrain edits preserve graded roads and update resident collision heights", async () => {
  const { project, world, terrain } = fixture();
  const host = new BrowserSceneHost(project);
  try {
    await host.prepare(world.id);
    const session = host.world,
      runtime = host.runtime;
    const first = session?.queryGround(0, 0);
    expect(first?.status).toBe("ready");
    if (first?.status !== "ready") throw new Error("Road collision not prepared");
    expect(first.height).toBeCloseTo(3, 1);
    const next = structuredClone(project),
      changed = next.documents.find((document) => document.id === terrain.id);
    if (changed?.kind !== "terrain") throw new Error("Missing terrain");
    changed.baseHeight = -2;
    await host.setProject(next);
    expect(host.runtime).toBe(runtime);
    expect(host.world).toBe(session);
    const road = host.world?.queryGround(0, 0),
      distant = host.world?.queryGround(0, 10);
    if (road?.status !== "ready" || distant?.status !== "ready")
      throw new Error("Updated terrain collision not prepared");
    expect(road.height).toBeCloseTo(3, 1);
    expect(distant.height).toBeCloseTo(-2, 1);
    expect(road.revision).toBeGreaterThan(first.revision);
    expect(
      host.extract(camera).surfaces.some((surface) => surface.instanceId === "layout_assembly_first_stone"),
    ).toBe(true);
  } finally {
    host.dispose();
  }
}, 20000);
