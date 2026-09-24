import { expect, test } from "bun:test";
import { createWaterLookdev } from "@wrela/examples/water-lookdev";
import { waterSchema } from "@wrela/model";
import { queryWater } from "./water";
import { compileWaterDomain, oceanWaterMesh, sampleWaterGrid } from "./water-domain";
import { compileWaterSpectrum, queryWaterSpectrum, sampleWaterSpectrum } from "./water-spectrum";

const water = (ocean = false) =>
  waterSchema.parse(
    createWaterLookdev(ocean ? "ocean" : "creek").documents.find(
      (d) => d.id === `water-study-${ocean ? "ocean" : "creek"}`,
    ),
  );
test("water lowering is deterministic, appearance independent, and bounded", () => {
  const source = water();
  const domain = compileWaterDomain(source)!;
  expect(compileWaterDomain({ ...source, color: [0, 0, 1] })).toBe(domain);
  const spectrum = compileWaterSpectrum(source);
  expect(spectrum.carriers.length).toBe(54 * 8);
  expect(spectrum.bandSlopeVariance.reduce((sum, value) => sum + value, 0)).toBeCloseTo(
    spectrum.slopeVariance,
    10,
  );
  expect(compileWaterSpectrum(structuredClone(source))).toBe(spectrum);
  expect(compileWaterSpectrum({ ...source, spectrum: { ...source.spectrum!, seed: 18 } }).key).not.toBe(
    spectrum.key,
  );
  for (let i = 0; i < 100; i++)
    expect(Math.abs(sampleWaterSpectrum(spectrum, i, -i, 3).height)).toBeLessThanOrEqual(
      spectrum.amplitudeBound,
    );
  expect(domain.cells.length).toBe(source.domain!.resolution ** 2 * 4);
  expect(queryWater(source, 100, 100, 0).wet).toBe(false);
  expect(queryWater(source, 0, 11, 0).wet).toBe(true);
  expect(queryWater(source, -5, -21, 0).height).toBeGreaterThan(queryWater(source, 0, 11, 0).height + 0.7);
});
test("bed sampling follows the rendered triangle diagonal and supports closed borders", () => {
  const domain = compileWaterDomain(water())!;
  const data = new Float32Array(domain.cells.length);
  data[0] = 1;
  data[4] = 3;
  data[domain.resolution * 4] = 5;
  data[(domain.resolution + 1) * 4] = 12;
  const at = (x: number, z: number) =>
    sampleWaterGrid(
      domain,
      data,
      domain.min[0] + x * domain.spacing[0],
      domain.min[1] + z * domain.spacing[1],
    )![0];
  expect(at(0.2, 0.3)).toBeCloseTo(1 + 2 * 0.2 + 4 * 0.3, 8);
  expect(at(0.8, 0.7)).toBeCloseTo(12 + (5 - 12) * 0.2 + (3 - 12) * 0.3, 8);
  expect(
    sampleWaterGrid(domain, data, domain.min[0] + domain.size[0], domain.min[1] + domain.size[1]),
  ).toBeDefined();
});
test("ocean queries invert the choppy surface and return its spatial derivative", () => {
  const spectrum = compileWaterSpectrum(water(true));
  for (let i = 0; i < 40; i++) {
    const x = i * 1.73,
      z = i * -0.81,
      time = 3.7;
    const forward = sampleWaterSpectrum(spectrum, x, z, time);
    const px = x + forward.lateralX,
      pz = z + forward.lateralZ;
    const inverse = queryWaterSpectrum(spectrum, px, pz, time);
    expect(inverse.height).toBeCloseTo(forward.height, 7);
    const epsilon = 1e-4;
    const dx =
      (queryWaterSpectrum(spectrum, px + epsilon, pz, time).height -
        queryWaterSpectrum(spectrum, px - epsilon, pz, time).height) /
      (2 * epsilon);
    expect(inverse.dx).toBeCloseTo(dx, 5);
    expect(forward.jxx * forward.jzz - forward.jxz ** 2).toBeGreaterThan(0);
  }
});
test("the ocean has bounded horizon geometry with upward facing triangles", () => {
  const mesh = oceanWaterMesh();
  expect(mesh.positions.length / 3).toBeLessThan(24000);
  for (let i = 0; i < mesh.indices.length; i += 3) {
    const a = mesh.indices[i] * 3,
      b = mesh.indices[i + 1] * 3,
      c = mesh.indices[i + 2] * 3;
    const upward =
      (mesh.positions[b + 2] - mesh.positions[a + 2]) * (mesh.positions[c] - mesh.positions[a]) -
      (mesh.positions[b] - mesh.positions[a]) * (mesh.positions[c + 2] - mesh.positions[a + 2]);
    expect(upward).toBeGreaterThan(0);
  }
});

test("periodic wave cascades retain shared CPU phase and a bounded short-wave tail", () => {
  const source = water(true),
    spectrum = compileWaterSpectrum(source);
  if (!spectrum.tiles) throw Error("Missing periodic shading cascades");
  const period = spectrum.tiles[0];
  for (let i = 0; i < 32; i++) {
    const a = sampleWaterSpectrum(spectrum, i * 0.317, i * -1.327, 3.7);
    const b = sampleWaterSpectrum(spectrum, i * 0.317 + period, i * -1.327, 3.7);
    expect(b.height).toBeCloseTo(a.height, 5);
    // Carrier wavevectors are float32; accumulated phase roundoff across the
    // largest tile is bounded well below the shading-map interpolation error.
    expect(Math.abs(b.dx - a.dx)).toBeLessThan(0.00002);
    expect(Math.abs(b.dz - a.dz)).toBeLessThan(0.00002);
  }
  for (let band = 0; band < 3; band++) {
    let variance = 0;
    for (let i = band * 18 * 8; i < (band + 1) * 18 * 8; i += 8) {
      const data = spectrum.carriers;
      // At least twelve samples per shortest wavelength at the base mip.
      expect((data[i + 5] / spectrum.tiles[band]) * 256).toBeGreaterThan(12);
      variance += data[i + 4] ** 2 * (data[i] ** 2 + data[i + 1] ** 2) * 0.5;
    }
    expect(variance).toBeGreaterThan(0);
    expect(variance).toBeLessThan(0.15);
  }
  const authored = { wavelength: 3.27, amplitude: 0.08, direction: 1.13, phase: 0.4, speed: 2.1 };
  const mixed = compileWaterSpectrum({ ...source, waves: [authored] });
  expect(mixed.tiles).toEqual(spectrum.tiles);
  expect(mixed.carriers[5]).toBeCloseTo(authored.wavelength, 6);
});

test("positive crests compress and orbital velocity follows the travelling wave", () => {
  const source = water(true);
  source.waves = [{ amplitude: 0.3, wavelength: 8, direction: 0, speed: 2, phase: Math.PI / 2 }];
  source.spectrum = { ...source.spectrum!, amplitude: 0, choppiness: 0.5 };
  const spectrum = compileWaterSpectrum(source),
    crest = sampleWaterSpectrum(spectrum, 0, 0, 0),
    trough = sampleWaterSpectrum(spectrum, 4, 0, 0);
  expect(crest.jxx).toBeLessThan(1);
  expect(trough.jxx).toBeGreaterThan(1);
  expect(crest.velocityX).toBeGreaterThan(0);
  expect(trough.velocityX).toBeLessThan(0);
  const dt = 1e-4;
  expect(
    (sampleWaterSpectrum(spectrum, 0, 0, dt).lateralX - sampleWaterSpectrum(spectrum, 0, 0, -dt).lateralX) /
      (2 * dt),
  ).toBeCloseTo(crest.velocityX, 6);
});
test("short ripples cover their wavelength range without octave gaps or broad cross-wave pits", () => {
  const source = water(true);
  const authored = source.spectrum;
  if (!authored) throw Error("Missing ocean spectrum");
  for (const seed of [1, 17, 23, 41, 256]) {
    const spectrum = compileWaterSpectrum({ ...source, spectrum: { ...authored, seed } });
    for (const cascade of [1, 2]) {
      const lengths = Array.from({ length: 18 }, (_, i) =>
        Math.log2(spectrum.carriers[(cascade * 18 + i) * 8 + 5]),
      ).sort((a, b) => a - b);
      // The former narrow wavelength clusters left most of each octave empty.
      for (let i = 1; i < lengths.length; i++) expect(lengths[i] - lengths[i - 1]).toBeLessThan(0.5);
    }
    let along = 0,
      across = 0;
    const wind = authored.direction;
    for (let i = 36 * 8; i < spectrum.carriers.length; i += 8) {
      const [kx, kz, , , amplitude] = spectrum.carriers.subarray(i, i + 8);
      along += (amplitude * (kx * Math.cos(wind) + kz * Math.sin(wind))) ** 2;
      across += (amplitude * (-kx * Math.sin(wind) + kz * Math.cos(wind))) ** 2;
    }
    expect(along).toBeGreaterThan(across * 4);
    // Redistribution must not erase the authored height energy.
    let energy = 0;
    for (let i = 4; i < spectrum.carriers.length; i += 8) energy += spectrum.carriers[i] ** 2;
    expect(energy).toBeCloseTo((authored.amplitude ** 2 * authored.windSpeed) / 6, 7);
  }
});
test("wind retains meaning above six metres per second and sea-state conserves requested variance", () => {
  const source = water(true);
  const at = (speed: number) =>
    compileWaterSpectrum({ ...source, spectrum: { ...source.spectrum!, windSpeed: speed } });
  expect(at(6).carriers).not.toEqual(at(8).carriers);
  expect(at(15).carriers).not.toEqual(at(30).carriers);
  const physical = compileWaterSpectrum({
    ...source,
    spectrum: { ...source.spectrum!, mode: "sea-state", significantHeight: 1.4, peakPeriod: 4 },
  });
  let variance = 0;
  for (let i = 4; i < physical.carriers.length; i += 8) variance += physical.carriers[i] ** 2 / 2;
  expect(4 * Math.sqrt(variance)).toBeCloseTo(1.4, 6);
  expect(physical.carriers.every(Number.isFinite)).toBe(true);
});
test("fine bed triangles and contact remain independent of fluid allocation", () => {
  const source = water();
  source.domain = { ...source.domain!, resolution: 32, renderResolution: 128, bedDetail: 0.05 };
  const domain = compileWaterDomain(source)!;
  expect(domain.cells.length).toBe(32 * 32 * 4);
  expect(domain.contact.length).toBe(128 * 128 * 4);
  expect(domain.surface.positions.length).toBe(32 * 32 * 3);
  expect(domain.bed.positions.length).toBe(128 * 128 * 3);
  for (let i = 0; i < domain.contact.length; i += 4)
    expect(domain.contact[i]).toBe(domain.bed.positions[(i / 4) * 3 + 1]);
});
