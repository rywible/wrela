import { expect, test } from "bun:test";
import { normalize, type Vec3 } from "@wrela/model";
import { indirectBoxFixture, quad } from "../../../tools/fixtures/indirect-scenes";
import { compileIndirectGeometry, traceSkyDistance } from "./indirect-query";
import { RadianceVisibilityQuery } from "./radiance-visibility-query";

test("occlusion witnesses are retested when receivers move and respect exact segment ends", () => {
  const wall = quad(
    "wall",
    [
      [1, -1, -1],
      [1, 1, -1],
      [1, 1, 1],
      [1, -1, 1],
    ],
    [0.5, 0.5, 0.5],
  );
  const geometry = compileIndirectGeometry([wall]),
    query = new RadianceVisibilityQuery(geometry, 1);
  expect(query.visible([0, 0, 0], [1, 0, 0], 1, 0)).toBe(true);
  expect(query.visible([0, 0, 0], [1, 0, 0], 1.01, 0)).toBe(false);
  expect(query.visible([0, 0, 0], [1, 0, 0], 1.01, 0)).toBe(false);
  expect(query.witnessHits).toBe(1);
  // The triangle test tolerates tiny barycentric edge error; the complete BVH
  // still rejects a parallel ray just outside this leaf's exact bounds.
  expect(query.visible([0, 1 + 1e-10, 0], [1, 0, 0], 1.01, 0)).toBe(true);
  expect(query.visible([0, 2, 0], [1, 0, 0], 1.01, 0)).toBe(true);
  expect(query.visible([0, 0, 0], [-1, 0, 0], 2, 0)).toBe(true);
  expect(query.visible([1, 0, 0], [1, 0, 0], 2, 0)).toBe(true);
  expect(query.visible([2, 0, 0], [-1, 0, 0], 2, 0)).toBe(false);
});
test("coherent visibility matches complete static BVH queries over varying rays and radii", () => {
  const geometry = compileIndirectGeometry(
    indirectBoxFixture({ ceiling: true, front: true, occluder: true }).surfaces,
  );
  const query = new RadianceVisibilityQuery(geometry, 8);
  for (let i = 0; i < 10000; i++) {
    const origin: Vec3 = [Math.sin(i * 0.317) * 5, Math.cos(i * 0.921) * 4 + 1, Math.sin(i * 0.111) * 5];
    const direction = normalize([Math.cos(i * 0.17), Math.sin(i * 0.237), Math.cos(i * 0.911)]);
    const radius = 0.01 + (i % 300) * 0.04;
    expect(query.visible(origin, direction, radius, i % 8)).toBe(
      traceSkyDistance(geometry, origin, direction, radius) >= radius,
    );
  }
  expect(query.witnessHits).toBeGreaterThan(0);
});
