import { expect, test } from "bun:test";
import { compileVegetation } from "@wrela/compiler";
import { botanicalPreset, createVegetationStand, type VegetationDefinition } from "@wrela/model";

import { vegetationReviewPosition, vegetationReviewTriangles } from "./vegetation-review";

const doc: VegetationDefinition = {
  kind: "vegetation",
  id: "review",
  schemaVersion: 1,
  name: "Review",
  dependencies: [],
  seed: 4,
  height: 3,
  radius: 1,
  branches: 3,
  material: "leaf",
  trunkMaterial: "bark",
  variation: 0.3,
  windResponse: 1,
  botanical: botanicalPreset(),
};
test("review projects actual mesh with camera distance and stand spacing", () => {
  const artifact = compileVegetation(doc, "interactive");
  const options = { distance: 15, stand: false, angle: 0, time: 0, silhouette: true };
  const near = vegetationReviewTriangles(doc, artifact, options, 360, 260);
  const far = vegetationReviewTriangles(doc, artifact, { ...options, distance: 60 }, 360, 260);
  const span = (triangles: typeof near) => {
    const ys = triangles.flatMap((triangle) => triangle.points.map((point) => point[1]));
    return Math.max(...ys) - Math.min(...ys);
  };
  expect(span(far)).toBeLessThan(span(near) * 0.3);
  expect(near.every((triangle) => triangle.color === "#182522")).toBe(true);
  const stand = vegetationReviewTriangles(doc, artifact, { ...options, stand: true }, 360, 260);
  expect(stand.length).toBe(near.length * (doc.botanical?.review.standCount ?? 9));
  expect(stand.flatMap((triangle) => triangle.points.flat()).every(Number.isFinite)).toBe(true);
});
test("review wind anchors roots and stays inside compiler displacement envelope", () => {
  const artifact = compileVegetation(doc);
  for (const time of [0, 1, 7, 20]) {
    expect(vegetationReviewPosition([0, 0, 0], 2, time, [0, 0, 0])).toEqual([0, 0, 0]);
    const moved = vegetationReviewPosition([1, 3, 0], 2, time, [0, 0, 0]);
    expect(Math.abs(moved[0] - 1)).toBeLessThanOrEqual(artifact.maxDisplacement);
  }
});

test("stand review renders the installed seed and maturity variants", () => {
  const artifact = compileVegetation(doc, "interactive");
  const stand = createVegetationStand(doc);
  const population = {
    ...stand,
    artifacts: new Map(
      stand.documents.map((document) => [document.id, compileVegetation(document, "interactive")]),
    ),
  };
  const triangles = vegetationReviewTriangles(
    doc,
    artifact,
    { distance: 20, stand: true, angle: 0.3, time: 1, silhouette: false },
    360,
    260,
    population,
  );
  const expected = population.instances.reduce(
    (sum, instance) =>
      sum +
      (population.artifacts
        .get(instance.definition)
        ?.surfaces.reduce((total, surface) => total + surface.mesh.indices.length / 3, 0) ?? 0),
    0,
  );
  expect(triangles.length).toBe(expected);
  expect(triangles.every((triangle) => triangle.points.flat().every(Number.isFinite))).toBe(true);
});
