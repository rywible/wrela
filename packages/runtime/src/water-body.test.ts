import { expect, test } from "bun:test";
import { queryWater } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import { createWaterLookdev } from "@wrela/examples/water-lookdev";
import { parseProject, type WaterDefinition, waterSchema } from "@wrela/model";
import { BrowserSceneHost } from "./scene-host";
import { RuntimeSession } from "./session";
import { WaterBodyRuntime } from "./water-body";

function creek(): WaterDefinition {
  return waterSchema.parse(createWaterLookdev().documents.find((d) => d.id === "water-study-creek"));
}
test("the sloping creek stays conservative through currents, rocks, sources and wet/dry transitions", () => {
  const body = new WaterBodyRuntime(creek()),
    simulation = body.simulation!,
    volume = simulation.volume;
  for (let tick = 0; tick < 600; tick++) simulation.step(1 / 60);
  expect(Math.abs(simulation.volume - volume - simulation.exchangedVolume)).toBeLessThan(1e-8);
  const sample = queryWater(body.definition, -2, -11, 10, body.renderState());
  expect(sample.wet).toBe(true);
  expect(sample.velocity[2]).toBeGreaterThan(0.05);
  expect(simulation.state.every(Number.isFinite)).toBe(true);
  expect(simulation.substeps).toBeLessThan(4);
});
test("runtime water impulses replay exactly and source edits retire old beds", async () => {
  const runtime = await RuntimeSession.create(),
    source = creek();
  try {
    runtime.setWater(source);
    expect(runtime.physics.checkpoint().statics.some(([id]) => id === `water-bed:${source.id}`)).toBe(true);
    runtime.seek(60);
    runtime.disturbWater(source.id, 0, 10, 2, 2);
    runtime.seek(120);
    const expected = runtime.waterBodies.get(source.id)!.simulation!.state.slice();
    runtime.seek(60);
    runtime.seek(120);
    expect(runtime.waterBodies.get(source.id)!.simulation!.state).toEqual(expected);
    const saves = JSON.parse(JSON.stringify(runtime.snapshotWaters()));
    runtime.seek(180);
    runtime.restoreEntityStates([], saves);
    expect(runtime.clock.tick).toBe(120);
    expect(runtime.waterBodies.get(source.id)!.simulation!.state).toEqual(expected);
    const invalid = structuredClone(saves);
    invalid[0].state[0] = -1;
    expect(() => runtime.restoreEntityStates([], invalid)).toThrow("Nonphysical");
    expect(runtime.waterBodies.get(source.id)!.simulation!.state).toEqual(expected);
    const mismatch = structuredClone(saves);
    mismatch[0].key = "different-bed";
    expect(() => runtime.restoreEntityStates([], mismatch)).toThrow("incompatible");
    runtime.setWater({ ...source, domain: undefined, spectrum: undefined });
    expect(runtime.waterBodies.size).toBe(0);
    expect(runtime.physics.checkpoint().statics.some(([id]) => id.startsWith("water-bed:"))).toBe(false);
  } finally {
    runtime.dispose();
  }
});
test("scene extraction carries the live fluid and bed at separate elevations", async () => {
  const project = createWaterLookdev(),
    source = creek();
  const host = new BrowserSceneHost(project);
  try {
    await host.prepare(source.id, "neutral-stage");
    await host.seek(1);
    const scene = host.extract({ position: [4, 5, 8], target: [0, 0, 0], fov: 50 });
    expect(scene.surfaces.find((surface) => surface.water)?.waterState?.revision).toBeGreaterThan(0);
    expect(scene.surfaces.some((surface) => surface.id === `${source.id}-bed`)).toBe(true);
    expect(() =>
      parseProject({
        ...project,
        documents: project.documents.map((document) =>
          document.id === source.id
            ? {
                ...source,
                domain: {
                  ...source.domain,
                  basins: [],
                  sources: [{ position: [1e6, 0], radius: 1, rate: 1 }],
                },
              }
            : document,
        ),
      }),
    ).toThrow("outside");
  } finally {
    host.dispose();
  }
});

test("a world restores independent water elevations, fluid state and translated bed collision", async () => {
  const project = referenceProject(),
    lower = creek();
  const upper = waterSchema.parse({
    ...lower,
    id: "upper-lake",
    level: 5,
    spectrum: undefined,
    flow: undefined,
    domain: {
      min: [20, -10],
      size: [20, 20],
      resolution: 32,
      basins: [{ center: [30, 0], radii: [7, 7], depth: 2 }],
    },
  });
  const world = project.documents.find((document) => document.kind === "world"),
    terrain = project.documents.find((document) => document.kind === "terrain");
  if (world?.kind !== "world" || terrain?.kind !== "terrain") throw new Error("World fixture missing");
  world.water = lower.id;
  world.waters = [lower.id, upper.id]; // Legacy primary and collection may name the same body.
  world.instances = [];
  world.populations = [];
  world.composition = undefined;
  terrain.baseHeight = -20;
  terrain.amplitude = 0;
  terrain.interventions = [];
  terrain.geology = undefined;
  project.documents.push(lower, upper);
  const first = new BrowserSceneHost(parseProject(project)),
    second = new BrowserSceneHost(parseProject(project));
  try {
    await first.prepare(world.id);
    const runtime = first.runtime;
    if (!runtime) throw new Error("Runtime missing");
    expect(runtime.waterBodies.size).toBe(2);
    runtime.seek(30);
    const hit = runtime.physics.raycast([30, 10, 0], [0, -1, 0], 40);
    expect(hit?.id).toBe(`water-bed:${upper.id}`);
    expect(hit?.point[1]).toBeCloseTo(3, 1);
    runtime.disturbWater(lower.id, 0, 10, 2, 2);
    runtime.disturbWater(upper.id, 30, 0, 1, 1);
    runtime.seek(60);
    const scene = first.extract({ position: [30, 12, 15], target: [30, 5, 0], fov: 50 });
    expect(scene.surfaces.filter((surface) => surface.water).length).toBe(2);
    const upperState = runtime.waterBodies.get(upper.id)!.renderState();
    expect(queryWater(upper, 30, 0, 1, upperState).height).toBeGreaterThan(4.9);
    const saved = JSON.parse(JSON.stringify(first.saveRuntime()));
    expect(saved.waters.length).toBe(2);
    await second.prepare(world.id);
    await second.loadRuntime(saved);
    expect(second.runtime?.clock.tick).toBe(60);
    expect(second.runtime?.snapshotWaters()).toEqual(saved.waters);
    const invalid = structuredClone(saved);
    invalid.waters[0].key = "edited-bed";
    await expect(second.loadRuntime(invalid)).rejects.toThrow("incompatible");
    expect(second.runtime?.snapshotWaters()).toEqual(saved.waters);
  } finally {
    first.dispose();
    second.dispose();
  }
});
