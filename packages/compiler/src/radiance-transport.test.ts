import { expect, test } from "bun:test";
import type { Vec3 } from "@wrela/model";
import { box, quad } from "../../../tools/fixtures/indirect-scenes";
import { indirectGeometryLayers } from "./indirect-layers";
import { compileIndirectGeometry } from "./indirect-query";
import { radianceEmitterSteps } from "./radiance-emission";
import { emissionBoxOccluded, localEmission } from "./radiance-local-emission";
import { accumulateRadiancePath, compactRadiancePath, type RadiancePath } from "./radiance-path";
import { traceDiffuseReceiverSteps } from "./radiance-surface";
import { traceRadianceSampleSteps } from "./radiance-transport";

function emitterIrradiance(height: number, rays: number, blocked = false) {
  const emitter = quad(
    "emitter",
    [
      [-1, height, -1],
      [1, height, -1],
      [1, height, 1],
      [-1, height, 1],
    ],
    [0, 0, 0],
  );
  emitter.material.emission = { color: [1, 0.4, 0.1], intensity: 1 };
  const geometry = compileIndirectGeometry([
    emitter,
    ...(blocked ? [box("occluder", [-2, height * 0.4, -2], [2, height * 0.6, 2], [0, 0, 0])] : []),
  ]);
  const result: Vec3 = [0, 0, 0];
  const steps = traceRadianceSampleSteps(
    geometry,
    [0, 0, 0],
    0,
    rays,
    [],
    new Float32Array(9 * 27 * 4),
    new Float32Array(9),
    (direction, incident) => {
      for (let c = 0; c < 3; c++) result[c] += (incident[26 * 4 + c] * Math.max(0, direction[1]) * 4) / rays;
    },
  );
  let step = steps.next();
  while (!step.done) step = steps.next();
  return result;
}

test("emitter importance sampling agrees with an analytic rectangular diffuse view factor", () => {
  for (const height of [0.05, 1, 5]) {
    const side = 1 / Math.sqrt(1 + height * height);
    const expected = (4 / Math.PI) * side * Math.atan(side);
    const actual = emitterIrradiance(height, 4096);
    expect(actual[0]).toBeCloseTo(expected, 2);
    expect(actual[1]).toBeCloseTo(actual[0] * 0.4, 7);
    expect(actual[2]).toBeCloseTo(actual[0] * 0.1, 7);
    // The production ray count must resolve a small emitter as well as a
    // near-field source, without relying on a lucky hemisphere direction.
    expect(Math.abs(emitterIrradiance(height, 64)[0] - expected)).toBeLessThan(0.07);
  }
});

test("an opaque blocker rejects sampled emission instead of treating it as an unshadowed light", () => {
  expect(emitterIrradiance(1, 1024, true)).toEqual([0, 0, 0]);
});

test("surface quadrature matches analytic emitter energy and excludes the opposite hemisphere", () => {
  const finish = <T>(steps: Generator<void, T>) => {
    let next = steps.next();
    while (!next.done) next = steps.next();
    return next.value;
  };
  for (const height of [0.05, 1, 5]) {
    const emitter = quad(
      "source",
      [
        [-1, height, -1],
        [1, height, -1],
        [1, height, 1],
        [-1, height, 1],
      ],
      [0, 0, 0],
    );
    emitter.material.emission = { color: [1, 0.4, 0.1], intensity: 1 };
    const geometry = compileIndirectGeometry([emitter]);
    const value = finish(traceDiffuseReceiverSteps(geometry, [0, 0, 0], [0, 1, 0], 4096, [])).transfer;
    const side = 1 / Math.sqrt(1 + height * height);
    expect(value[104]).toBeCloseTo((4 / Math.PI) * side * Math.atan(side), 2);
    expect(value[105]).toBeCloseTo(value[104] * 0.4, 7);
    const away = finish(traceDiffuseReceiverSteps(geometry, [0, 0, 0], [0, -1, 0], 64, [])).transfer;
    expect(away[104]).toBe(0);
    expect(away[0]).toBeCloseTo(0.2820947918, 7);
    expect(() => finish(traceDiffuseReceiverSteps(geometry, [0, 0, 0], [0, 2, 0], 64, []))).toThrow();
    expect(() => finish(traceDiffuseReceiverSteps(geometry, [0, 0, 0], [0, 1, 0], 0, []))).toThrow();
  }
});

test("diffuse transport conserves channel-wise energy in a reflective emitting enclosure", () => {
  // This expectation is analytic, independent of the path implementation:
  // uniform enclosed radiance satisfies L = Le + rho * L in every channel.
  // A short fixed-depth tracer loses most of the blue channel's energy.
  const albedo: Vec3 = [0.4, 0.7, 0.9],
    emission: Vec3 = [0.2, 0.6, 1];
  const surface = box("enclosure", [-1, -1, -1], [1, 1, 1], albedo);
  surface.material.emission = { color: emission, intensity: 1 };
  const geometry = compileIndirectGeometry([surface]);
  const normals: Vec3[] = [
    [0, 1, 0],
    [0, -1, 0],
    [1, 0, 0],
  ];
  for (const position of [
    [0, 0, 0],
    [0.31, -0.27, 0.19],
  ] as Vec3[]) {
    const actual = normals.map(() => [0, 0, 0]);
    const samples = 16384;
    const steps = traceRadianceSampleSteps(
      geometry,
      position,
      0,
      samples,
      [],
      new Float32Array(972),
      new Float32Array(9),
      (direction, incident) => {
        normals.forEach((normal, i) => {
          const cosine = Math.max(
            0,
            direction.reduce((sum, v, axis) => sum + v * normal[axis], 0),
          );
          for (let c = 0; c < 3; c++) actual[i][c] += (incident[26 * 4 + c] * cosine * 4) / samples;
        });
      },
    );
    let result = steps.next();
    while (!result.done) result = steps.next();
    for (const value of actual)
      for (let c = 0; c < 3; c++) {
        const expected = emission[c] / (1 - albedo[c]);
        expect(Math.abs(value[c] / expected - 1)).toBeLessThan(0.04);
      }
  }
});

test("receiver-local emission preserves distance, tangent hemisphere and opaque visibility", () => {
  for (const height of [0.05, 0.2, 1, 5]) {
    const emitter = quad(
      "source",
      [
        [-1, height, -1],
        [1, height, -1],
        [1, height, 1],
        [-1, height, 1],
      ],
      [0, 0, 0],
    );
    emitter.material.emission = { color: [1, 0.4, 0.1], intensity: 1 };
    for (const blocked of [false, true]) {
      const geometry = compileIndirectGeometry([
        emitter,
        ...(blocked ? [box("blocker", [-2, height * 0.4, -2], [2, height * 0.6, 2], [0, 0, 0])] : []),
      ]);
      const steps = radianceEmitterSteps(geometry);
      let result = steps.next();
      while (!result.done) result = steps.next();
      const emitters = result.value;
      const value = localEmission(geometry, emitters, [0, 0, 0], [0, 1, 0]).value;
      const side = 1 / Math.sqrt(1 + height * height);
      const expected = blocked ? 0 : (4 / Math.PI) * side * Math.atan(side);
      expect(Math.abs(value[0] - expected)).toBeLessThan(0.035);
      expect(value[1]).toBeCloseTo(value[0] * 0.4, 7);
      expect(value[2]).toBeCloseTo(value[0] * 0.1, 7);
      expect(localEmission(geometry, emitters, [0, 0, 0], [0, -1, 0])).toEqual({ value: [0, 0, 0], rays: 0 });
    }
  }
});

test("source-box visibility rejects complete shadows but preserves partially visible emitters", () => {
  const source = quad(
    "source",
    [
      [-1, 2, -1],
      [1, 2, -1],
      [1, 2, 1],
      [-1, 2, 1],
    ],
    [0, 0, 0],
  );
  source.material.emission = { color: [1, 1, 1], intensity: 1 };
  // A single large triangle covers the whole source. The small one blocks
  // its center ray but leaves most source area visible.
  for (const size of [0.1, 10]) {
    const blocker = quad(
      "blocker",
      [
        [-size, 1, -size],
        [size * 3, 1, -size],
        [-size, 1, size * 3],
        [-size * 2, 1, 0],
      ],
      [0, 0, 0],
    );
    const geometry = compileIndirectGeometry([source, blocker]);
    const steps = radianceEmitterSteps(geometry);
    let item = steps.next();
    while (!item.done) item = steps.next();
    expect(emissionBoxOccluded(geometry, item.value, [0, 0, 0]).blocked).toBe(size === 10);
    const result = localEmission(geometry, item.value, [0, 0, 0], [0, 1, 0]);
    if (size === 10) expect(result).toEqual({ value: [0, 0, 0], rays: 1 });
    else expect(result.value[0]).toBeGreaterThan(0.15);
  }
});

test("sparse path replay preserves exact transport and supports isolated path retracing", () => {
  const emitter = box("source", [-1, 2, -1], [1, 2.1, 1], [0.7, 0.3, 0.2]);
  emitter.material.emission = { color: [1, 0.4, 0.1], intensity: 2 };
  const geometry = compileIndirectGeometry([
    emitter,
    box("floor", [-3, -0.2, -3], [3, 0, 3], [0.4, 0.7, 0.2]),
  ]);
  const buffers = () => ({
    transfer: new Float32Array(972),
    sky: new Float32Array(9),
    emission: new Float32Array(27),
  });
  const first = buffers(),
    replay = buffers();
  const paths: RadiancePath[] = [];
  const finish = <T>(steps: Generator<void, T>) => {
    let item = steps.next();
    while (!item.done) item = steps.next();
    return item.value;
  };
  finish(
    traceRadianceSampleSteps(
      geometry,
      [0, 0.01, 0],
      0,
      64,
      [{ position: [1, 1, 0], range: 4 }],
      first.transfer,
      first.sky,
      undefined,
      first.emission,
      undefined,
      {
        observe: (i, p) => {
          paths[i] = compactRadiancePath(p);
        },
      },
    ),
  );
  expect(paths).toHaveLength(64);
  expect(paths.reduce((sum, p) => sum + p.incident.byteLength, 0)).toBeLessThan(64 * 108 * 8);
  // Replace several cached paths independently, then rebuild in deterministic
  // path order. Subtract/add delta updates would gradually drift instead.
  for (const r of [0, 19, 47, 63])
    finish(
      traceRadianceSampleSteps(
        geometry,
        [0, 0.01, 0],
        0,
        64,
        [{ position: [1, 1, 0], range: 4 }],
        replay.transfer,
        replay.sky,
        undefined,
        replay.emission,
        undefined,
        {
          first: r,
          end: r + 1,
          project: false,
          observe: (i, p) => {
            paths[i] = compactRadiancePath(p);
          },
        },
      ),
    );
  expect(replay.transfer.every((v) => v === 0)).toBe(true);
  for (const path of paths) accumulateRadiancePath(path, 0, 64, replay.transfer, replay.sky, replay.emission);
  expect(replay).toEqual(first);
  expect(() =>
    finish(
      traceRadianceSampleSteps(
        geometry,
        [0, 0.01, 0],
        0,
        64,
        [],
        replay.transfer,
        replay.sky,
        undefined,
        undefined,
        undefined,
        { first: -1 },
      ),
    ),
  ).toThrow();
  const sources = finish(radianceEmitterSteps(geometry));
  const withDoor = indirectGeometryLayers(geometry, [
    compileIndirectGeometry([box("door", [0, 0, -1], [0.2, 2, 0], [0.2, 0.2, 0.2])]),
  ]);
  expect(finish(radianceEmitterSteps(withDoor))).toBe(sources);
  const extra = box("extra", [2, 1, 0], [3, 2, 1], [1, 1, 1]);
  extra.material.emission = { color: [1, 1, 1], intensity: 1 };
  const withSource = indirectGeometryLayers(geometry, [compileIndirectGeometry([extra])]);
  expect(finish(radianceEmitterSteps(withSource)).power).toBeGreaterThan(sources.power);
});
