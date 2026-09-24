import { expect, test } from "bun:test";
import { compileRadianceLightingSteps } from "@wrela/compiler";
import { quad } from "./fixtures/indirect-scenes";
import { nativeRadianceReference } from "./lighting-native-reference";

test("native diagnostic evaluates carrier interpolation and retains disagreements across a shared edge", () => {
  const source = quad("carrier", [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]], [1, 1, 1]);
  const steps = compileRadianceLightingSteps([source], { center: [0, 0, 0], key: "diagnostic", maxSamples: 2, raysPerSample: 16 });
  let next = steps.next();
  while (!next.done) next = steps.next();
  const product = next.value, field = product.field;
  const mesh = { ...source.mesh, radianceProbes: Float32Array.from({ length: 24 }, (_, i) => i % 4 === 0 ? 1024 + i / 4 : 0) };
  product.meshes.set(source.id, mesh);
  product.receivers.set(source.id, { mesh: source.mesh });
  field.transfer.fill(0);
  field.directEmission?.fill(0);
  field.surfaceDiffuse = undefined;
  field.receivers = new Float32Array(Array.from({ length: 6 }, () => [1, 0, 0, 0, 1, 0, 0, 0]).flat());
  field.receiverEmission = new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2]);
  const result = nativeRadianceReference(product, [source], { position: [0.75, 0.25, 0.003], normal: [0, 0, 1] }, []);
  expect(result).toHaveLength(1);
  expect(result[0].coverage).toBe(1);
  expect(result[0].surfaceCoverage).toBe(0);
  expect(result[0].value.emission).toEqual([0.25, 0.5, 0.25]);
  const edge = nativeRadianceReference(product, [source], { position: [0.5, 0.5, 0.003], normal: [0, 0, 1] }, []);
  expect(edge).toHaveLength(2);
  expect(edge[0].value.emission).toEqual([0.5, 0, 0.5]);
  expect(edge[1].value.emission).toEqual([2, 2, 2]);
  const backside = nativeRadianceReference(product, [source], { position: [0.75, 0.25, -0.003], normal: [0, 0, -1] }, []);
  expect(backside[0].coverage).toBe(0);

  const transfer = new Float32Array(3 * 108);
  for (let i = 0; i < 3; i++) for (let c = 0; c < 3; c++) transfer[i * 108 + c] = 0.2820947918 * (i + 1);
  field.surfaceDiffuse = {
    positions: [[0, 0, 0], [1, 0, 0], [1, 1, 0]], transfer,
    receivers: new Float32Array([0, 1, 2, 0, 0.25, 0.5, 0.25, 0, 0.2, 0.3, 0.4, 0]),
  };
  for (let i = 0; i < mesh.radianceProbes.length; i += 4) mesh.radianceProbes[i + 1] = 1;
  const surface = nativeRadianceReference(product, [source], { position: [0.75, 0.25, 0.003], normal: [0, 0, 1] }, []);
  expect(surface[0].surfaceCoverage).toBe(1);
  for (let c = 0; c < 3; c++) {
    expect(surface[0].value.sky[c]).toBeCloseTo(2, 6);
    expect(surface[0].value.emission[c]).toBeCloseTo([0.2, 0.3, 0.4][c], 6);
  }
});
