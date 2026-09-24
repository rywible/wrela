import { expect, test } from "bun:test";
import { compileWaterDomain } from "@wrela/compiler";
import { waterSchema } from "@wrela/model";

import { WaterSimulation } from "./water-simulation";

function fixture(sources: unknown[] = []) {
  const water = waterSchema.parse({
    id: "lake",
    name: "Lake",
    kind: "water",
    schemaVersion: 1,
    dependencies: [],
    level: 0,
    color: [0.1, 0.3, 0.2],
    roughness: 0.15,
    waves: [],
    domain: {
      min: [-8, -8],
      size: [16, 16],
      resolution: 32,
      basins: [{ center: [0, 0], radii: [6, 6], depth: 1.5 }],
      obstacles: [{ center: [1, 1], radius: 1, height: 2 }],
      sources,
    },
  });
  const domain = compileWaterDomain(water);
  if (!domain) throw Error("Missing domain");
  return new WaterSimulation(domain, water);
}
test("shallow water preserves a lake at rest over uneven terrain and dry banks", () => {
  const simulation = fixture(),
    initial = simulation.state.slice(),
    volume = simulation.volume;
  for (let i = 0; i < 180; i++) simulation.step(1 / 60);
  expect(Math.abs(simulation.volume - volume)).toBeLessThan(1e-8);
  for (let i = 0; i < initial.length; i++)
    expect(Math.abs(simulation.state[i] - initial[i])).toBeLessThan(1e-9);
});
test("momentum impulses propagate, conserve volume, stay nonnegative and replay exactly", () => {
  const simulation = fixture(),
    volume = simulation.volume;
  simulation.disturb(-2, 0, 1.2, 2);
  for (let i = 0; i < 30; i++) simulation.step(1 / 60);
  const checkpoint = simulation.snapshot();
  for (let i = 0; i < 60; i++) simulation.step(1 / 60);
  const expected = simulation.state.slice();
  expect(Math.abs(simulation.volume - volume)).toBeLessThan(1e-8);
  expect(simulation.state.every(Number.isFinite)).toBe(true);
  for (let i = 0; i < expected.length; i += 4) expect(expected[i]).toBeGreaterThanOrEqual(0);
  simulation.restore(checkpoint);
  for (let i = 0; i < 60; i++) simulation.step(1 / 60);
  expect(simulation.state).toEqual(expected);
  const undisturbed = fixture();
  expect(expected.some((value, i) => i % 4 === 0 && Math.abs(value - undisturbed.state[i]) > 0.001)).toBe(
    true,
  );
});
test("sources and sinks account for all exchanged volume", () => {
  const simulation = fixture([
    { position: [-2, 0], radius: 1, rate: 0.25 },
    { position: [3, 0], radius: 1, rate: -0.1 },
  ]);
  const volume = simulation.volume;
  for (let i = 0; i < 120; i++) simulation.step(1 / 60);
  expect(simulation.volume - volume).toBeCloseTo(simulation.exchangedVolume, 8);
  expect(simulation.exchangedVolume).toBeCloseTo(0.3, 7);
});

test("body drag returns the exact horizontal impulse to water without changing volume", () => {
  const simulation = fixture(),
    volume = simulation.volume;
  const momentum = () => {
    let x = 0,
      z = 0;
    for (let i = 0; i < simulation.state.length; i += 4) {
      x += simulation.state[i + 1];
      z += simulation.state[i + 2];
    }
    const mass = 1000 * simulation.domain.spacing[0] * simulation.domain.spacing[1];
    return [x * mass, z * mass];
  };
  const before = momentum();
  simulation.receiveBodyImpulse(-2, 0, 0.8, 12, -7);
  const after = momentum();
  expect(after[0] - before[0]).toBeCloseTo(12, 9);
  expect(after[1] - before[1]).toBeCloseTo(-7, 9);
  expect(simulation.volume).toBe(volume);
  for (let tick = 0; tick < 60; tick++) simulation.step(1 / 60);
  expect(simulation.state.every(Number.isFinite)).toBe(true);
});

test("local effects leave foam and drying memory, and replay restores their clock", () => {
  const base = fixture();
  const source = waterSchema.parse({
    ...base.definition,
    domain: { ...base.definition.domain, simulate: false },
    effects: [
      {
        id: "break",
        kind: "breaker",
        start: [-3, 0],
        end: [3, 0],
        height: 1,
        width: 4,
        period: 4,
        phase: 0.2,
      },
    ],
  });
  const simulation = new WaterSimulation(compileWaterDomain(source)!, source),
    volume = simulation.volume;
  for (let i = 0; i < 120; i++) simulation.step(1 / 60);
  expect(simulation.state.some((v, i) => i % 4 === 3 && v > 0.01)).toBe(true);
  expect(simulation.volume).toBe(volume);
  const saved = simulation.snapshot();
  for (let i = 0; i < 120; i++) simulation.step(1 / 60);
  const expected = simulation.snapshot();
  simulation.restore(saved);
  for (let i = 0; i < 120; i++) simulation.step(1 / 60);
  expect(simulation.snapshot()).toEqual(expected);
  expect(() => simulation.restore({ ...saved, wetness: new Float32Array([1]) })).toThrow("wetness");
});
