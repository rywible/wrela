import { expect, test } from "bun:test";
import type { RenderSurface, Vec3 } from "@wrela/model";
import { quad } from "../../../tools/fixtures/indirect-scenes";
import { compileIndirectGeometry } from "./indirect-query";
import { radianceEmitterSteps } from "./radiance-emission";
import { polygonEmission, polygonIrradiance } from "./radiance-polygon-emission";

const rectangle = (a: number, b: number, h: number) =>
  (2 / Math.PI) *
  ((a / Math.hypot(a, h)) * Math.atan(b / Math.hypot(a, h)) +
    (b / Math.hypot(b, h)) * Math.atan(a / Math.hypot(b, h)));
function plane(id: string, a: number, b: number, h: number, source = false) {
  const s = quad(
    id,
    [
      [-a, h, -b],
      [a, h, -b],
      [a, h, b],
      [-a, h, b],
    ],
    [0, 0, 0],
  );
  if (source) s.material.emission = { color: [1, 0.4, 0.1], intensity: 1 };
  return s;
}
function evaluate(surfaces: RenderSurface[], position: Vec3 = [0, 0, 0], normal: Vec3 = [0, 1, 0]) {
  const geometry = compileIndirectGeometry(surfaces);
  const steps = radianceEmitterSteps(geometry);
  let next = steps.next();
  while (!next.done) next = steps.next();
  return polygonEmission(geometry, next.value, position, normal);
}

test("polygon boundary integral agrees with independent rectangular view factors near and far", () => {
  for (const h of [0.0001, 0.05, 0.2, 1, 5, 100]) {
    const vertices: Vec3[] = [
      [-1, h, -2],
      [1, h, -2],
      [1, h, 2],
      [-1, h, 2],
    ];
    const expected = rectangle(1, 2, h);
    expect(polygonIrradiance(vertices, [0, 1, 0])).toBeCloseTo(expected, 10);
    expect(polygonIrradiance([...vertices].reverse(), [0, 1, 0])).toBeCloseTo(expected, 10);
    expect(polygonIrradiance(vertices, [0, -1, 0])).toBe(0);
    const result = evaluate([plane("source", 1, 2, h, true)]);
    expect(result).toBeDefined();
    expect(result?.value[0]).toBeCloseTo(expected, 7);
    expect(result?.value[1]).toBeCloseTo(expected * 0.4, 7);
    expect(result?.value[2]).toBeCloseTo(expected * 0.1, 7);
  }
});

test("polygon shadows integrate partial and overlapping opaque blockers without counting hidden source area", () => {
  const source = plane("source", 1, 1, 2, true);
  for (const size of [0.05, 0.2, 0.4, 0.6, 2]) {
    const result = evaluate([source, plane("blocker", size, size, 1)]);
    const expected = rectangle(1, 1, 2) - rectangle(Math.min(1, size * 2), Math.min(1, size * 2), 2);
    expect(result).toBeDefined();
    expect(result?.value[0]).toBeCloseTo(expected, 7);
    // A second blocker with the same projected shadow must not be subtracted twice.
    const overlap = evaluate([
      source,
      plane("blocker", size, size, 1),
      plane("overlap", size * 1.5, size * 1.5, 1.5),
    ]);
    expect(overlap?.value[0]).toBeCloseTo(expected, 7);
  }
  for (const h of [-1, 3]) {
    const result = evaluate([source, plane("outside", 3, 3, h)]);
    expect(result?.value[0]).toBeCloseTo(rectangle(1, 1, 2), 9);
  }
});

test("horizon-clipped polygon integral agrees with independent area quadrature", () => {
  const vertices: Vec3[] = [
    [2, -1, -1],
    [2, 1, -1],
    [2, 1, 1],
    [2, -1, 1],
  ];
  let reference = 0;
  const count = 512;
  // Integrate cos(receiver) * cos(emitter) / (pi * r^2) directly in area.
  for (let y = 0; y < count; y++)
    for (let z = 0; z < count; z++) {
      const py = (y + 0.5) / count,
        pz = -1 + ((z + 0.5) * 2) / count;
      const r2 = 4 + py * py + pz * pz;
      reference += (((py * 2) / (Math.PI * r2 * r2)) * 2) / (count * count);
    }
  expect(polygonIrradiance(vertices, [0, 1, 0])).toBeCloseTo(reference, 6);
});

test("analytic source visibility is invariant under world translation and falls back at singular receivers", () => {
  const surfaces = [plane("source", 1, 1, 2, true), plane("blocker", 0.2, 0.3, 1)];
  const expected = evaluate(surfaces);
  if (!expected) throw Error("Expected analytic result");
  const offset: Vec3 = [1e7, -2e7, 3e7];
  const shifted = surfaces.map((s) => {
    const matrix = s.matrix.slice();
    for (let a = 0; a < 3; a++) matrix[12 + a] += offset[a];
    return { ...s, matrix };
  });
  expect(evaluate(shifted, offset)?.value[0]).toBeCloseTo(expected.value[0], 7);
  expect(evaluate(surfaces, [0, 2, 0], [1, 0, 0])).toBeUndefined();
});
