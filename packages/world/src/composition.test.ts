import { describe, expect, test } from "bun:test";
import { terrainHeight } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import {
  emptyWorldComposition,
  type TerrainDefinition,
  validateWorldComposition,
  type WorldDefinition,
  worldCompositionSchema,
} from "@wrela/model";
import { compositionInterests, compositionPopulationDensity, realizeWorldComposition } from "./composition";
import { generatePlacements } from "./population";
import { WorldSession } from "./session";

function fixture() {
  const project = referenceProject();
  const world = structuredClone(project.documents.find((item) => item.kind === "world") as WorldDefinition);
  const terrain = structuredClone(
    project.documents.find((item) => item.kind === "terrain") as TerrainDefinition,
  );
  world.instances = [];
  world.composition = emptyWorldComposition();
  return { world, terrain, composition: world.composition };
}
describe("semantic world composition", () => {
  test("terrain grounded instances follow path grading while explicit exceptions keep their elevation", () => {
    const { world, terrain, composition } = fixture();
    terrain.amplitude = 0;
    terrain.baseHeight = 0;
    terrain.interventions = [];
    world.instances.push({
      id: "grounded-marker",
      definition: "prop",
      position: [0, 99, 0],
      rotation: [0, 0, 0],
      scale: 1,
      grounding: { offset: 0.15 },
    });
    composition.paths.push({
      id: "graded-approach",
      kind: "path",
      points: [
        [-10, 3, 0],
        [10, 3, 0],
      ],
      width: 4,
      shoulder: 0,
      flatten: true,
      spacing: 3,
      maxGrade: 0.3,
    });
    const realized = realizeWorldComposition(world, terrain);
    expect(realized.world.instances[0].position[1]).toBeCloseTo(terrainHeight(realized.terrain, 0, 0) + 0.15);
    expect(world.instances[0].position[1]).toBe(99);
    composition.overrides.push({ id: "grounded-marker", removed: false, position: [2, 7, 1] });
    expect(realizeWorldComposition(world, terrain).world.instances[0].position).toEqual([2, 7, 1]);
    world.composition = undefined;
    expect(realizeWorldComposition(world, terrain).world.instances[0].position[1]).toBeCloseTo(
      terrainHeight(terrain, 0, 0) + 0.15,
    );
  });
  test("assembly instances transform coherently, retain identity after reordering, and accept exceptions", () => {
    const { world, terrain, composition } = fixture();
    composition.assemblies.push({
      id: "house",
      name: "House",
      members: [{ id: "door", definition: "prop", position: [2, 0, 0], yaw: 0, scale: 1 }],
    });
    composition.placements.push({
      id: "first",
      assembly: "house",
      position: [10, 0, 0],
      yaw: Math.PI / 2,
      scale: 2,
    });
    const result = realizeWorldComposition(world, terrain);
    expect(result.world.instances[0].position[0]).toBeCloseTo(10);
    expect(result.world.instances[0].position[2]).toBeCloseTo(-4);
    expect(world.instances).toHaveLength(0);
    composition.placements.push({ id: "second", assembly: "house", position: [20, 0, 0], yaw: 0, scale: 1 });
    const beforeReorder = realizeWorldComposition(world, terrain).world.instances;
    composition.placements.reverse();
    expect(
      realizeWorldComposition(world, terrain).world.instances.sort((a, b) => a.id.localeCompare(b.id)),
    ).toEqual(beforeReorder.sort((a, b) => a.id.localeCompare(b.id)));
    composition.placements = composition.placements.filter((placement) => placement.id === "first");
    const identity = result.world.instances[0].id;
    composition.overrides.push({ id: identity, removed: false, position: [5, 6, 7] });
    expect(realizeWorldComposition(world, terrain).world.instances[0].position).toEqual([5, 6, 7]);
    composition.overrides[0].removed = true;
    expect(realizeWorldComposition(world, terrain).world.instances).toHaveLength(0);
  });
  test("roads change actual terrain heights and exclude populations across region boundaries", () => {
    const { world, terrain, composition } = fixture();
    terrain.amplitude = 0;
    terrain.baseHeight = 0;
    terrain.interventions = [];
    composition.paths.push({
      id: "road",
      kind: "road",
      points: [
        [-10, 5, 0],
        [10, 5, 0],
      ],
      width: 4,
      shoulder: 1,
      flatten: true,
      spacing: 3,
      maxGrade: 0.3,
    });
    const result = realizeWorldComposition(world, terrain);
    expect(terrainHeight(result.terrain, 0, 0)).toBeGreaterThan(4.9);
    expect(terrainHeight(terrain, 0, 0)).toBe(0);
    expect(compositionPopulationDensity(composition, "trees", 0, 1)).toBe(0);
    expect(compositionPopulationDensity(composition, "trees", 0, 8)).toBe(1);
  });
  test("road grading preserves surface module spacing and identity", () => {
    const { world, terrain, composition } = fixture();
    composition.paths.push({
      id: "road",
      kind: "road",
      points: [
        [0, 0, 0],
        [20, 0, 0],
      ],
      width: 2,
      shoulder: 0,
      flatten: false,
      spacing: 10,
      maxGrade: 0.3,
      definition: "paving",
    });
    const original = realizeWorldComposition(world, terrain).world.instances;
    expect(original).toHaveLength(3);
    composition.paths[0].flatten = true;
    expect(realizeWorldComposition(world, terrain).world.instances).toEqual(original);
  });
  test("biomes blend deterministically and respect authored and runtime overrides", () => {
    const { world, terrain, composition } = fixture();
    const rule = world.populations[0];
    rule.density = 1;
    rule.maxSlope = 1;
    rule.minHeight = -1000;
    rule.maxHeight = 1000;
    terrain.interventions = [];
    const region = { minX: -32, minZ: -32, maxX: 32, maxZ: 32 };
    const original = generatePlacements(world, terrain, region);
    expect(original.length).toBeGreaterThan(0);
    composition.biomes.push({
      id: "forest",
      center: [0, 0, 0],
      radius: 20,
      transition: 10,
      populations: [rule.id],
      density: 1,
    });
    expect(compositionPopulationDensity(composition, rule.id, 15, 0)).toBe(0.5);
    expect(compositionPopulationDensity(composition, rule.id, 21, 0)).toBe(0);
    const placement = original[0];
    composition.overrides.push({ id: placement.id, removed: false, position: [0, 7, 0] });
    expect(
      generatePlacements(world, terrain, region).find((item) => item.id === placement.id)?.position,
    ).toEqual([0, 7, 0]);
    expect(
      generatePlacements(world, terrain, region, new Map([[placement.id, { removed: true }]])).some(
        (item) => item.id === placement.id,
      ),
    ).toBe(false);
  });
  test("rooms leave an entrance and encounters realize members", () => {
    const { world, terrain, composition } = fixture();
    composition.rooms.push({
      id: "room",
      center: [0, 0, 0],
      size: [8, 8],
      yaw: 0,
      wallDefinition: "wall",
      moduleWidth: 2,
      doorSide: "south",
      doorWidth: 2,
    });
    composition.spaces.push({
      id: "arena",
      kind: "encounter",
      center: [20, 0, 0],
      radius: 8,
      clearPopulation: true,
      members: [{ id: "enemy", definition: "actor", position: [1, 0, 2], yaw: 0, scale: 1 }],
    });
    const result = realizeWorldComposition(world, terrain);
    expect(
      result.world.instances
        .filter((item) => item.id.startsWith("layout_room_room_2"))
        .every((item) => Math.abs(item.position[0]) >= 2),
    ).toBe(true);
    expect(result.world.instances.find((item) => item.definition === "actor")?.position).toEqual([21, 0, 2]);
    expect(compositionPopulationDensity(composition, "trees", 20, 0)).toBe(0);
    expect(compositionPopulationDensity(composition, "trees", 0, 0)).toBe(0);
  });
  test("budget overflow and conflicting identities fail before publishing", () => {
    const { world, terrain, composition } = fixture();
    composition.rooms.push({
      id: "huge",
      center: [0, 0, 0],
      size: [1000, 1000],
      yaw: 0,
      wallDefinition: "wall",
      moduleWidth: 1,
      doorSide: "south",
      doorWidth: 2,
    });
    expect(() => realizeWorldComposition(world, terrain)).toThrow("budget");
    expect(world.instances).toHaveLength(0);
    composition.rooms = [];
    composition.paths.push({
      id: "road",
      kind: "road",
      points: [
        [0, 0, 0],
        [10, 20, 0],
      ],
      width: 4,
      shoulder: 1,
      flatten: true,
      spacing: 3,
      maxGrade: 0.3,
    });
    expect(validateWorldComposition(composition).some((issue) => issue.code === "world.path.grade")).toBe(
      true,
    );
    expect(
      worldCompositionSchema.safeParse({
        ...composition,
        streaming: [
          { id: "bad", center: [0, 0, 0], radius: 10, preloadDistance: 20, priority: 0, collision: false },
        ],
      }).success,
    ).toBe(false);
  });
  test("streaming zones activate only near observers and enter bounded terrain planning", () => {
    const { world, terrain, composition } = fixture();
    composition.streaming.push({
      id: "castle",
      center: [100, 0, 0],
      radius: 32,
      preloadDistance: 80,
      priority: 3,
      collision: true,
    });
    expect(compositionInterests(composition, [0, 0, 0])).toHaveLength(1);
    expect(compositionInterests(composition, [-100, 0, 0])).toHaveLength(0);
    const session = new WorldSession(world, terrain);
    session.setInterest({ id: "camera", position: [0, 0, 0], visualRadius: 32, collisionRadius: 0 });
    session.update();
    expect(session.metrics.requested).toBeGreaterThan(0);
    session.dispose();
  });
});

test("rounded roads use the same corridor for grading, surface modules, and population clearance", () => {
  const { world, terrain, composition } = fixture();
  terrain.amplitude = 0;
  terrain.baseHeight = 0;
  terrain.interventions = [];
  terrain.geology = undefined;
  composition.paths.push({
    id: "bend",
    kind: "road",
    points: [
      [0, 3, 0],
      [10, 3, 0],
      [10, 3, 10],
    ],
    cornerRadius: 4,
    width: 1,
    shoulder: 0,
    flatten: true,
    definition: "paving",
    spacing: 2,
    maxGrade: 0.3,
  });
  const result = realizeWorldComposition(world, terrain);
  expect(terrainHeight(result.terrain, 9, 1)).toBeGreaterThan(2.9);
  expect(compositionPopulationDensity(composition, "trees", 9, 1)).toBe(0);
  expect(compositionPopulationDensity(composition, "trees", 10, 0)).toBe(1);
  expect(
    result.world.instances.some(
      (instance) => instance.position[0] > 8 && instance.position[2] > 0.2 && instance.position[2] < 2,
    ),
  ).toBe(true);
  expect(validateWorldComposition(composition)).toHaveLength(0);
});

test("grounded reusable groups preserve local height relationships after route grading", () => {
  const { world, terrain, composition } = fixture();
  terrain.amplitude = 0;
  terrain.baseHeight = 4;
  terrain.interventions = [];
  terrain.geology = undefined;
  composition.assemblies.push({
    id: "tower",
    name: "Tower",
    members: [
      { id: "base", definition: "prop", position: [0, 0, 0], yaw: 0, scale: 1 },
      { id: "top", definition: "prop", position: [0, 2, 0], yaw: 0, scale: 1 },
    ],
  });
  composition.placements.push({
    id: "placed",
    assembly: "tower",
    position: [0, 100, 0],
    yaw: 0,
    scale: 2,
    grounding: { offset: 0.25 },
  });
  composition.spaces.push({
    id: "encounter",
    kind: "encounter",
    center: [8, 30, 0],
    radius: 2,
    clearPopulation: true,
    grounding: { offset: 0.5 },
    members: [{ id: "actor", definition: "actor", position: [0, 1, 0], yaw: 0, scale: 1 }],
  });
  const instances = realizeWorldComposition(world, terrain).world.instances;
  expect(instances[0].position[1]).toBe(4.25);
  expect(instances[1].position[1]).toBe(8.25);
  expect(instances[2].position[1]).toBe(5.5);
  composition.overrides.push({ id: instances[1].id, removed: false, position: [0, 12, 0] });
  expect(realizeWorldComposition(world, terrain).world.instances[1].position[1]).toBe(12);
});
