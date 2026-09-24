import { expect, test } from "bun:test";
import { createAlpineLookdevProject, referenceProject } from "@wrela/examples";
import { emptyWorldComposition } from "@wrela/model";
import { defaultWorldReview, reviewWorldComposition } from "./spatial-review";

function fixture() {
  const project = referenceProject();
  const world = project.documents.find((document) => document.kind === "world"),
    terrain = project.documents.find((document) => document.kind === "terrain"),
    object = project.documents.find((document) => document.kind === "object");
  if (!world || !terrain || !object) throw new Error("Missing fixture");
  world.instances = [];
  world.populations = [];
  world.composition = emptyWorldComposition();
  terrain.amplitude = 0;
  terrain.baseHeight = 0;
  terrain.interventions = [];
  terrain.geology = undefined;
  object.field.bounds = { min: [-0.5, 0, -2], max: [0.5, 3, 2] };
  object.collision = "mesh";
  world.composition.review = defaultWorldReview();
  world.composition.review.sightline = { from: [-5, 1.7, 0], to: [5, 1.7, 0] };
  world.composition.paths.push({
    id: "walk",
    kind: "path",
    points: [
      [-5, 0, 0],
      [5, 0, 0],
    ],
    width: 2,
    shoulder: 0,
    flatten: false,
    spacing: 2,
    maxGrade: 0.3,
  });
  return { project, world, terrain, object, composition: world.composition };
}
test("spatial review identifies actor width/overhead obstacles and static sightline blockers", () => {
  const { project, world, terrain, object } = fixture();
  expect(reviewWorldComposition(world, terrain, project.documents).sightline.clear).toBe(true);
  world.instances.push({
    id: "wall",
    definition: object.id,
    position: [0, 0, 0],
    rotation: [0, 0, 0],
    scale: 1,
  });
  const review = reviewWorldComposition(world, terrain, project.documents);
  expect(review.sightline.obstruction?.id).toBe("wall");
  expect(review.routes[0].samples.some((sample) => sample.blockedBy.includes("wall"))).toBe(true);
  world.instances[0].position[1] = 2;
  expect(reviewWorldComposition(world, terrain, project.documents).routes[0].blockedSamples).toBe(0);
  world.instances[0].position[1] = 1.5;
  expect(reviewWorldComposition(world, terrain, project.documents).routes[0].blockedSamples).toBeGreaterThan(
    0,
  );
});
test("review transforms rotated and scaled instance bounds and detects terrain sightline hills", () => {
  const { project, world, terrain, object } = fixture();
  world.instances.push({
    id: "wall",
    definition: object.id,
    position: [0, 0, 0],
    rotation: [0, Math.PI / 2, 0],
    scale: 2,
  });
  const obstacle = reviewWorldComposition(world, terrain, project.documents).obstacles[0];
  expect(obstacle.bounds.max[0]).toBeCloseTo(4);
  expect(obstacle.bounds.max[2]).toBeCloseTo(1);
  world.instances = [];
  terrain.interventions.push({
    id: "hill",
    kind: "raise",
    center: [0, 0],
    radius: 3,
    strength: 5,
    targetHeight: 0,
  });
  const review = reviewWorldComposition(world, terrain, project.documents);
  expect(review.sightline.obstruction?.id).toBe(terrain.id);
  expect(review.routes[0].steepSamples).toBeGreaterThan(0);
});
test("clearance review honors assembly removal overrides and bounds long route work", () => {
  const { project, world, terrain, object, composition } = fixture();
  composition.assemblies.push({
    id: "a",
    name: "A",
    members: [{ id: "wall", definition: object.id, position: [0, 0, 0], yaw: 0, scale: 1 }],
  });
  composition.placements.push({ id: "p", assembly: "a", position: [0, 0, 0], yaw: 0, scale: 1 });
  expect(reviewWorldComposition(world, terrain, project.documents).sightline.clear).toBe(false);
  composition.overrides.push({ id: "layout_assembly_p_wall", removed: true });
  expect(reviewWorldComposition(world, terrain, project.documents).sightline.clear).toBe(true);
  composition.paths[0].points[1] = [10000, 0, 0];
  const result = reviewWorldComposition(world, terrain, project.documents);
  expect(result.routes[0].samples).toHaveLength(512);
  expect(result.routes[0].sampleSpacing).toBeGreaterThan(1);
});

test("review settings do not change the procedural save fingerprint", async () => {
  const { generatorCompatibility } = await import("./population");
  const { world, terrain, composition } = fixture();
  const before = generatorCompatibility(world, terrain);
  if (!composition.review) throw new Error("Missing review settings");
  composition.review.actorHeight = 2.4;
  composition.review.sightline.to[0] = 200;
  expect(generatorCompatibility(world, terrain)).toBe(before);
});

test("alpine gate approach stays traversable up to its closed door", () => {
  const project = createAlpineLookdevProject();
  const world = project.documents.find(
    (document) => document.id === project.entry && document.kind === "world",
  );
  const terrain = project.documents.find(
    (document) => document.id === (world?.kind === "world" ? world.terrain : undefined),
  );
  if (world?.kind !== "world" || terrain?.kind !== "terrain") throw new Error("Missing alpine world");
  const review = reviewWorldComposition(world, terrain, project.documents);
  expect(review.routes.map((route) => route.blockedSamples)).toEqual([0, 0]);
  expect(review.sightline.clear).toBe(true);
  expect(review.network.reachablePaths).toEqual(["approach", "gate-branch"]);
  expect(review.network.destinations.every((destination) => destination.reachable)).toBe(true);
  expect(review.obstacles.some((obstacle) => obstacle.id.endsWith("/west-jamb-1"))).toBe(true);
});

test("route access connects interior crossings and reports disconnected encounters", () => {
  const { project, world, terrain, composition } = fixture();
  composition.paths.push({
    ...composition.paths[0],
    id: "crossing",
    points: [
      [0, 0, -5],
      [0, 0, 5],
    ],
  });
  composition.paths.push({
    ...composition.paths[0],
    id: "isolated",
    points: [
      [20, 0, -5],
      [20, 0, 5],
    ],
  });
  if (!composition.review) throw new Error("Missing review settings");
  composition.review.entryPath = "walk";
  composition.spaces.push({
    id: "near",
    kind: "encounter",
    center: [0, 0, 5],
    radius: 2,
    clearPopulation: true,
    members: [],
  });
  composition.spaces.push({
    id: "far",
    kind: "landmark",
    center: [20, 0, 5],
    radius: 2,
    clearPopulation: true,
    members: [],
  });
  const network = reviewWorldComposition(world, terrain, project.documents).network;
  expect(network.reachablePaths).toEqual(["crossing", "walk"]);
  expect(network.disconnectedPaths).toEqual(["isolated"]);
  expect(network.destinations.find((destination) => destination.id === "near")?.reachable).toBe(true);
  expect(network.destinations.find((destination) => destination.id === "far")?.reason).toBe(
    "disconnected-route",
  );
  composition.paths.reverse();
  expect(reviewWorldComposition(world, terrain, project.documents).network).toEqual(network);
});

test("room access tests the authored doorway rather than the room center or back wall", () => {
  const { project, world, terrain, object, composition } = fixture();
  object.field.bounds = { min: [-1, 0, -0.1], max: [1, 2, 0.1] };
  composition.paths[0].points = [
    [0, 0, -10],
    [0, 0, -4],
  ];
  composition.rooms.push({
    id: "room",
    center: [0, 0, 0],
    size: [8, 8],
    yaw: 0,
    wallDefinition: object.id,
    moduleWidth: 2,
    doorSide: "north",
    doorWidth: 2,
  });
  expect(reviewWorldComposition(world, terrain, project.documents).network.destinations[0].reachable).toBe(
    true,
  );
  composition.rooms[0].doorSide = "south";
  expect(reviewWorldComposition(world, terrain, project.documents).network.destinations[0].reason).toBe(
    "no-route",
  );
  composition.rooms[0].doorSide = "north";
  if (!composition.review) throw new Error("Missing review settings");
  composition.review.actorRadius = 1.1;
  expect(reviewWorldComposition(world, terrain, project.documents).network.destinations[0].reason).toBe(
    "narrow-entrance",
  );
});
