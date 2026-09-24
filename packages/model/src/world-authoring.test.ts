import { expect, test } from "bun:test";
import { emptyWorldComposition, validateWorldComposition } from "./world-authoring";

test("source relationship validation rejects expanded budgets before committing a scene", () => {
  const composition = emptyWorldComposition();
  composition.assemblies.push({
    id: "a",
    name: "A",
    members: Array.from({ length: 128 }, (_, i) => ({
      id: `m${i}`,
      definition: "prop",
      position: [0, 0, 0],
      yaw: 0,
      scale: 1,
    })),
  });
  composition.placements.push(
    ...[0, 1, 2].map((i) => ({
      id: `p${i}`,
      assembly: "a",
      position: [0, 0, 0] as [number, number, number],
      yaw: 0,
      scale: 1,
    })),
  );
  expect(validateWorldComposition(composition).some((issue) => issue.message.includes("384 instances"))).toBe(
    true,
  );
  composition.placements.pop();
  expect(validateWorldComposition(composition)).toHaveLength(0);
  expect(
    validateWorldComposition(composition, [], { instanceIds: ["existing"] }).some((issue) =>
      issue.message.includes("257 instances"),
    ),
  ).toBe(true);
  composition.overrides.push({ id: "layout_assembly_p0_m0", removed: true });
  expect(validateWorldComposition(composition, [], { instanceIds: ["existing"] })).toHaveLength(0);
});
test("source validation detects generated identity conflicts and combined grading budgets", () => {
  const composition = emptyWorldComposition();
  composition.paths.push({
    id: "road",
    kind: "road",
    points: [
      [0, 0, 0],
      [10, 0, 0],
    ],
    width: 2,
    shoulder: 0,
    flatten: true,
    spacing: 2,
    maxGrade: 0.3,
    definition: "paving",
  });
  expect(
    validateWorldComposition(composition, [], { instanceIds: ["layout_path_road_0"] }).some((issue) =>
      issue.message.includes("collides"),
    ),
  ).toBe(true);
  expect(
    validateWorldComposition(composition, [], { interventionCount: 250 }).some((issue) =>
      issue.message.includes("261 terrain interventions"),
    ),
  ).toBe(true);
});
