import { expect, test } from "bun:test";
import type { RenderSurface, Vec3 } from "@wrela/model";

import { indirectAlpineFixture, indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import {
  compileIndirectGeometry,
  type IndirectGeometry,
  type IndirectLighting,
  traceIndirectRay,
} from "./indirect-query";
import {
  indirectReferencePFM,
  renderIndirectReference,
  sampleIndirectReferenceAtHit,
} from "./indirect-reference";

const sky: IndirectLighting = {
  sunDirection: [0, 1, 0],
  sunRadiance: [0, 0, 0],
  skyRadiance: [0.2, 0.4, 0.8],
};
function floorHit(geometry: IndirectGeometry, position: Vec3 = [0.1, 0.1, 0.1]) {
  const hit = traceIndirectRay(geometry, position, [0, -1, 0]);
  if (!hit) throw Error("Fixture floor ray missed");
  return hit;
}
function withColor(surface: RenderSurface, color: Vec3): RenderSurface {
  return { ...surface, material: { ...surface.material, color, secondary: color } };
}

test("reference reproduces the exact Lambertian constant-sky solution on an open floor", () => {
  const floor = indirectBoxFixture({ occluder: false }).surfaces[0];
  const geometry = compileIndirectGeometry([floor]);
  const result = sampleIndirectReferenceAtHit(geometry, floorHit(geometry), sky, { samples: 128, seed: 32 });
  expect(result.direct).toEqual([0, 0, 0]);
  expect(result.bounce).toEqual([0, 0, 0]);
  for (let axis = 0; axis < 3; axis++)
    expect(result.total[axis]).toBeCloseTo(0.7 * sky.skyRadiance[axis], 12);
  const sun = sampleIndirectReferenceAtHit(
    geometry,
    floorHit(geometry),
    {
      sunDirection: [0, 1, 0],
      sunRadiance: [Math.PI, 2 * Math.PI, 3 * Math.PI],
      skyRadiance: [0, 0, 0],
    },
    { samples: 32 },
  );
  expect(sun.total[0]).toBeCloseTo(0.7, 12);
  expect(sun.total[1]).toBeCloseTo(1.4, 12);
  expect(sun.total[2]).toBeCloseTo(2.1, 12);
});

test("a closed opaque enclosure receives neither outside sun nor sky, including through a bounce", () => {
  const fixture = indirectBoxFixture({ ceiling: true, front: true, occluder: false });
  const geometry = compileIndirectGeometry(fixture.surfaces);
  const result = sampleIndirectReferenceAtHit(geometry, floorHit(geometry), fixture.lighting, {
    samples: 512,
    skySamples: 32,
    seed: 71,
  });
  expect(result.direct).toEqual([0, 0, 0]);
  expect(result.sky).toEqual([0, 0, 0]);
  expect(result.bounce).toEqual([0, 0, 0]);
  expect(result.total).toEqual([0, 0, 0]);
});

test("a visible red wall adds red diffuse bounce and darkening it removes that light", () => {
  const fixture = indirectBoxFixture({ occluder: false });
  const red = fixture.surfaces.map((surface) =>
    surface.id === "floor" || surface.id === "left-wall" ? surface : withColor(surface, [0, 0, 0]),
  );
  const black = red.map((surface) => (surface.id === "left-wall" ? withColor(surface, [0, 0, 0]) : surface));
  const redGeometry = compileIndirectGeometry(red),
    blackGeometry = compileIndirectGeometry(black);
  const lighting = { ...sky, skyRadiance: [1, 1, 1] as Vec3 };
  const options = { samples: 2048, skySamples: 32, seed: 43 };
  const bright = sampleIndirectReferenceAtHit(
    redGeometry,
    floorHit(redGeometry, [-0.8, 0.1, 0]),
    lighting,
    options,
  );
  const dark = sampleIndirectReferenceAtHit(
    blackGeometry,
    floorHit(blackGeometry, [-0.8, 0.1, 0]),
    lighting,
    options,
  );
  expect(bright.sky).toEqual(dark.sky);
  expect(bright.bounce[0]).toBeGreaterThan(0.015);
  expect(bright.bounce[0]).toBeGreaterThan(bright.bounce[1] * 20);
  expect(bright.total[0]).toBeGreaterThan(dark.total[0] + 0.015);
  expect(dark.bounce).toEqual([0, 0, 0]);
});

test("open-box and alpine references are deterministic finite linear RGB images", () => {
  for (const fixture of [indirectBoxFixture(), indirectAlpineFixture()]) {
    const geometry = compileIndirectGeometry(fixture.surfaces);
    expect(geometry.report.excluded).toEqual([]);
    const options = { width: 12, height: 8, samples: 8, skySamples: 8, seed: 178 };
    const first = renderIndirectReference(geometry, fixture.camera, fixture.lighting, options);
    const second = renderIndirectReference(geometry, fixture.camera, fixture.lighting, options);
    expect(first.data.length).toBe(12 * 8 * 3);
    expect(first.data.every((value) => Number.isFinite(value) && value >= 0)).toBe(true);
    expect(first.data).toEqual(second.data);
    expect(Math.max(...first.data) - Math.min(...first.data)).toBeGreaterThan(0.1);
  }
});

test("PFM export preserves HDR linear values, channel order and bottom-up scanlines", () => {
  const data = new Float32Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
  const bytes = indirectReferencePFM({ width: 2, height: 2, data });
  const header = new TextEncoder().encode("PF\n2 2\n-1.0\n");
  expect(new TextDecoder().decode(bytes.subarray(0, header.length))).toBe("PF\n2 2\n-1.0\n");
  const view = new DataView(bytes.buffer, bytes.byteOffset + header.length);
  expect(view.getFloat32(0, true)).toBe(7);
  expect(view.getFloat32(5 * 4, true)).toBe(12);
  expect(view.getFloat32(6 * 4, true)).toBe(1);
  expect(view.getFloat32(11 * 4, true)).toBe(6);
});

test("invalid reference sample budgets and camera inputs fail explicitly", () => {
  const fixture = indirectBoxFixture({ occluder: false }),
    geometry = compileIndirectGeometry(fixture.surfaces);
  expect(() => sampleIndirectReferenceAtHit(geometry, floorHit(geometry), sky, { samples: 0 })).toThrow(
    "samples",
  );
  expect(() =>
    sampleIndirectReferenceAtHit(geometry, floorHit(geometry), sky, { samples: 1, skySamples: Number.NaN }),
  ).toThrow("skySamples");
  expect(() =>
    renderIndirectReference(geometry, { ...fixture.camera, target: fixture.camera.position }, sky, {
      samples: 1,
      width: 1,
      height: 1,
    }),
  ).toThrow("look away");
});
