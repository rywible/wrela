import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import type { EnvironmentDefinition } from "@wrela/model";
import { evaluateAtmosphere, evaluateEnvironment } from "./environment";

test("physical atmosphere composition is cached across solar and exposure changes", () => {
  const source = referenceProject().documents.find(
    (d): d is EnvironmentDefinition => d.kind === "environment",
  );
  if (!source) throw new Error("Missing environment fixture");
  const midday = evaluateEnvironment(source);
  const sunset = evaluateEnvironment({ ...source, sunElevation: -0.1 }, undefined, 2);
  expect(midday.atmosphere).toBeDefined();
  expect(midday.atmosphere).toBe(sunset.atmosphere);
  expect(midday.sunColor).toEqual([1, 1, 1]);
  expect(sunset.sunColor).toEqual(midday.sunColor);
  expect(sunset.sunIntensity).toBe(midday.sunIntensity);
  expect(sunset.exposure).toBe(2);
  const fog = evaluateEnvironment({ ...source, fogDensity: source.fogDensity + 0.001 });
  expect(fog.atmosphere?.key).not.toBe(midday.atmosphere?.key);
  expect((fog.atmosphere?.data[9] ?? 0) * Math.exp(-20 / 12)).toBeCloseTo(
    (source.fogDensity + 0.001) * 1000,
    5,
  );
  expect(() => evaluateAtmosphere(0, 0.001)).toThrow();
});
