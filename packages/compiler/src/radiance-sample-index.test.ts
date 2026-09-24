import { expect, test } from "bun:test";
import type { Vec3 } from "@wrela/model";
import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { compileIndirectGeometry } from "./indirect-query";
import { radianceReceiverMixture } from "./radiance-lighting";
import { RadianceSampleIndex } from "./radiance-sample-index";

test("exact receiver index matches exhaustive nearest sets, including equal-distance ties and radius edges", () => {
  const points: Vec3[] = Array.from({ length: 192 }, (_, i) => [
    Math.sin(i * 1.73) * 25,
    Math.cos(i * 0.39) * 12,
    Math.cos(i * 1.17) * 25,
  ]);
  points.push(
    [24, 0, 0],
    [-24, 0, 0],
    [0, 24, 0],
    [0, -24, 0],
    [0, 0, 24],
    [0, 0, -24],
    [0, 0, 0],
    [0, 0, 0],
  );
  const index = new RadianceSampleIndex(points);
  for (let k = 0; k < 240; k++) {
    const p: Vec3 =
      k === 0 ? [0, 0, 0] : [Math.sin(k * 0.117) * 40, Math.cos(k * 0.271) * 15, Math.sin(k * 0.83) * 40];
    const sorted = points
      .map((q, i) => ({ i, d: q.reduce((s, v, a) => s + (v - p[a]) ** 2, 0) }))
      .filter((v) => v.d <= 576)
      .sort((a, b) => a.d - b.d || a.i - b.i);
    for (const limit of [1, 2, 16, 64, 512]) expect(index.nearest(p, limit)).toEqual(sorted.slice(0, limit));
  }
  expect(new RadianceSampleIndex([]).nearest([0, 0, 0], 16)).toEqual([]);
  expect(() => index.nearest([0, 0, 0], 0)).toThrow();
  expect(() => new RadianceSampleIndex([[NaN, 0, 0]])).toThrow();
});

test("indexed visibility produces identical final mixtures on both sides of room walls", () => {
  const geometry = compileIndirectGeometry(indirectBoxFixture({ ceiling: true, front: true }).surfaces);
  const points: Vec3[] = Array.from({ length: 192 }, (_, i) => [
    Math.sin(i * 0.817) * 5,
    Math.cos(i * 0.51) * 3 + 1.5,
    Math.cos(i * 0.337) * 5,
  ]);
  const index = new RadianceSampleIndex(points, geometry);
  for (let i = 0; i < 200; i++) {
    const p: Vec3 = [Math.sin(i * 0.173) * 4, 1 + Math.sin(i * 0.367), Math.cos(i * 0.531) * 4];
    expect(radianceReceiverMixture(geometry, points, p, index)).toEqual(
      radianceReceiverMixture(geometry, points, p),
    );
  }
  expect(() => radianceReceiverMixture(geometry, points.slice(), [0, 1, 0], index)).toThrow();
  expect(index.visibility?.witnessHits).toBeGreaterThan(0);
});
