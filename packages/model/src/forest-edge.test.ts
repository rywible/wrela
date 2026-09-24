import { expect, test } from "bun:test";
import { createForestEdge, forestEdgeCamera, forestTrailX } from "@wrela/examples/forest-edge";
import { parseProject } from "./validation";

test("forest placement is deterministic, editable and clears the complete walking route", () => {
  const project = parseProject(createForestEdge());
  expect(project).toEqual(parseProject(createForestEdge()));
  const world = project.documents.find((d) => d.id === "forest-edge");
  if (world?.kind !== "world") throw Error("Missing forest world");
  expect(world.instances.length).toBeGreaterThan(50);
  for (const i of world.instances)
    expect(Math.abs(i.position[0] - forestTrailX(i.position[2]))).toBeGreaterThan(1.2);
  for (const time of [0, 6, 12, 18, 24]) expect(forestEdgeCamera(time).position[1]).toBe(1.65);
});
