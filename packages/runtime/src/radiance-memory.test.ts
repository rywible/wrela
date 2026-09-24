import { expect, test } from "bun:test";
import { compileRadianceLightingSteps } from "@wrela/compiler";
import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { radianceNeighborEstimate, radianceRetainedBytes } from "./radiance-memory";

const finish = <T>(steps: Generator<void, T>): T => {
  let item = steps.next();
  while (!item.done) item = steps.next();
  return item.value;
};
test("retained radiance counts shared immutable geometry and carriers once, never equal independent copies", () => {
  const surfaces = indirectBoxFixture().surfaces;
  const first = finish(compileRadianceLightingSteps(surfaces, { key: "first", center: [0, 1, 0] }));
  const next = finish(
    compileRadianceLightingSteps(surfaces, { key: "next", center: [2, 1, 0], reuse: first }),
  );
  const fresh = finish(compileRadianceLightingSteps(surfaces, { key: "fresh", center: [2, 1, 0] }));
  const sum = first.field.report.bytes + next.field.report.bytes;
  expect(radianceRetainedBytes([first, first])).toBe(first.field.report.bytes);
  expect(radianceRetainedBytes([first, next])).toBeLessThan(sum - first.geometry.report.bytes);
  expect(radianceRetainedBytes([first, fresh])).toBe(first.field.report.bytes + fresh.field.report.bytes);
  expect(radianceNeighborEstimate(first)).toBeGreaterThan(first.field.transfer.byteLength);
  expect(radianceNeighborEstimate(first)).toBeLessThan(
    first.field.report.bytes - first.geometry.report.bytes,
  );
});
