import { resolve } from "node:path";
import { compileRadianceLightingSteps, radianceReceiverMixture } from "@wrela/compiler";
import { indirectSH } from "@wrela/compiler/indirect-probes";
import { traceIndirectRay } from "@wrela/compiler/indirect-query";
import { radianceEmitterSteps } from "@wrela/compiler/radiance-emission";
import { localEmission } from "@wrela/compiler/radiance-local-emission";
import { traceDiffuseReceiverSteps } from "@wrela/compiler/radiance-surface";
import {
  compileSurfaceLatticeSteps,
  evaluateSurfaceLattice,
  type RadianceSurfaceLattice,
  surfaceBarycentric,
} from "@wrela/compiler/radiance-surface-lattice";
import { traceRadianceSampleSteps } from "@wrela/compiler/radiance-transport";
import type { RadianceLightingField, Vec3 } from "@wrela/model";
import { lightingRouteScene } from "./fixtures/lighting-route-scene";
import { nativeRadianceReference } from "./lighting-native-reference";
import { snapshotSource } from "./source-snapshot";

const requestedBudget = process.argv.find((a) => a.startsWith("--probe-budget="));
const maxSamples = requestedBudget ? Number(requestedBudget.slice(15)) : undefined;
const latticeExperiment = process.argv.includes("--lattice");
const latticeOptions = process.argv.filter(
  (a) => a.startsWith("--lattice-rays=") || a.startsWith("--lattice-spacing="),
);
const latticeRays = Number(latticeOptions.find((a) => a.startsWith("--lattice-rays="))?.slice(15) ?? 64);
const latticeSpacing = Number(latticeOptions.find((a) => a.startsWith("--lattice-spacing="))?.slice(18) ?? 1);

if (!process.argv.includes("--frozen")) {
  const path = resolve("output/lighting-reference", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(path, "lighting-reference");
  console.log(JSON.stringify({ source: path, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn(
    [
      process.execPath,
      "tools/lighting-reference-check.ts",
      "--frozen",
      ...(requestedBudget ? [requestedBudget] : []),
      ...(latticeExperiment ? ["--lattice"] : []),
      ...latticeOptions,
    ],
    {
      cwd: path,
      stdout: "inherit",
      stderr: "inherit",
    },
  );
  process.exit(await child.exited);
}

const finish = <T>(steps: Generator<void, T>): T => {
  let item = steps.next();
  while (!item.done) item = steps.next();
  return item.value;
};
const points: { name: string; position: Vec3; normal: Vec3 }[] = [
  { name: "room-floor", position: [2.3, 0.003, 0], normal: [0, 1, 0] },
  { name: "room-ceiling", position: [0.2, 2.997, -1.9], normal: [0, -1, 0] },
  { name: "plaster-back-wall", position: [1.8, 1.6, -2.997], normal: [0, 0, 1] },
  { name: "cave-entrance-floor", position: [0.1, 0.003, -4.7], normal: [0, 1, 0] },
  { name: "cave-middle-floor", position: [0.1, 0.003, -8], normal: [0, 1, 0] },
  { name: "cave-deep-floor", position: [0.1, 0.003, -12.8], normal: [0, 1, 0] },
];
const route = lightingRouteScene(true);
const lights = route.scene.environment.pointLights ?? [];
type Channels = { sky: Vec3; local: Vec3; emission: Vec3 };
type LatticeReport = {
  triangle: number;
  admitted: boolean;
  validCells: number;
  cells: number;
  samples: number;
  rays: number;
  ms: number;
  value: Channels;
  replacedEmission: number[];
};
const zero = (): Channels => ({ sky: [0, 0, 0], local: [0, 0, 0], emission: [0, 0, 0] });
const accumulate = (out: Channels, input: ArrayLike<number>, start: number, weight: number) => {
  for (let c = 0; c < 3; c++) {
    out.sky[c] += ((input[start + c] + input[start + 9 * 4 + c]) / 0.2820947918) * weight;
    out.emission[c] += input[start + 26 * 4 + c] * weight;
    for (let l = 0; l < lights.length; l++)
      out.local[c] += input[start + (18 + l) * 4 + c] * lights[l].color[c] * lights[l].intensity * weight;
  }
};
const cached = (field: RadianceLightingField, ids: number[], normal: Vec3): Channels => {
  const out = zero(),
    basis = indirectSH(normal);
  for (let i = 0; i < 4; i++) {
    const id = ids[i];
    if (!id || !ids[i + 4]) continue;
    const sample = zero();
    for (let k = 0; k < 9; k++)
      accumulate(
        sample,
        field.transfer,
        ((id - 1) * 9 + k) * 27 * 4,
        basis[k] * (k === 0 ? 1 : k < 4 ? 2 / 3 : 1 / 4),
      );
    for (const channel of ["sky", "local", "emission"] as const)
      for (let c = 0; c < 3; c++) out[channel][c] += Math.max(0, sample[channel][c]) * ids[i + 4];
  }
  return out;
};
const reports = [];
const builds = [];
for (const door of ["open", "closed"] as const) {
  route.setDoor(door === "open" ? 1.5 : 0, false);
  const product = finish(
    compileRadianceLightingSteps(route.scene.surfaces, { center: [0, 0, 0], key: door, lights, maxSamples }),
  );
  builds.push({ door, ...product.field.report });
  const lattices = new Map<string, RadianceSurfaceLattice>();
  for (const point of points) {
    const mixture = radianceReceiverMixture(product.geometry, product.field.positions, point.position);
    const approximated = cached(product.field, mixture, point.normal);
    const emitters = finish(radianceEmitterSteps(product.geometry));
    const local = localEmission(product.geometry, emitters, point.position, point.normal);
    const replacedEmission: Vec3 = [0, 0, 0];
    const basis = indirectSH(point.normal);
    for (let i = 0; i < 4; i++) {
      if (!mixture[i] || !mixture[i + 4]) continue;
      const sample: Vec3 = [0, 0, 0];
      for (let k = 0; k < 9; k++)
        for (let c = 0; c < 3; c++) {
          const direct = product.field.directEmission![((mixture[i] - 1) * 9 + k) * 3 + c];
          const total = product.field.transfer[(((mixture[i] - 1) * 9 + k) * 27 + 26) * 4 + c];
          sample[c] += (total - direct) * basis[k] * (k === 0 ? 1 : k < 4 ? 2 / 3 : 1 / 4);
        }
      for (let c = 0; c < 3; c++) replacedEmission[c] += Math.max(0, sample[c]) * mixture[i + 4];
    }
    for (let c = 0; c < 3; c++) replacedEmission[c] += local.value[c];
    const references = [];
    for (const rays of [4096, 16384]) {
      const reference = zero();
      finish(
        traceRadianceSampleSteps(
          product.geometry,
          point.position,
          0,
          rays,
          lights,
          new Float32Array(9 * 27 * 4),
          new Float32Array(9),
          (direction, incident) => {
            const cosine = Math.max(
              0,
              direction.reduce((sum, v, a) => sum + v * point.normal[a], 0),
            );
            accumulate(reference, incident, 0, (cosine * 4) / rays);
          },
        ),
      );
      references.push({ rays, value: reference });
    }
    let lattice: LatticeReport | undefined;
    if (latticeExperiment) {
      const hit = traceIndirectRay(
        product.geometry,
        point.position,
        point.normal.map((v) => -v) as Vec3,
        0.02,
      );
      if (!hit) throw Error(`Missing receiver plane for ${point.name}`);
      const key = `${hit.triangle}/${point.normal.join(",")}`,
        started = performance.now();
      const patch =
        lattices.get(key) ??
        finish(
          compileSurfaceLatticeSteps(product.geometry, hit.triangle, point.normal, lights, {
            spacing: latticeSpacing,
            samples: latticeRays,
            maxResolution: 32,
          }),
        );
      lattices.set(key, patch);
      const resolved = evaluateSurfaceLattice(
        patch,
        surfaceBarycentric(product.geometry, hit.triangle, hit.position),
      );
      const value = resolved ? zero() : approximated;
      if (resolved) accumulate(value, resolved.transfer, 0, 1);
      const localReplaced = resolved
        ? value.emission.map((v, c) => Math.max(0, v - resolved.directEmission[c]) + local.value[c])
        : replacedEmission;
      lattice = {
        triangle: hit.triangle,
        admitted: !!resolved,
        validCells: patch.validCells.reduce((sum, valid) => sum + valid, 0),
        cells: patch.validCells.length,
        samples: patch.transfer.length / 108,
        rays: patch.rays,
        ms: performance.now() - started,
        value,
        replacedEmission: localReplaced,
      };
    }
    const result = {
      door,
      point,
      mapped: mixture.some((v, i) => i < 4 && v > 0),
      cached: approximated,
      localEmission: local,
      replacedEmission,
      references,
      lattice,
      native: nativeRadianceReference(product, route.scene.surfaces, point, lights),
      surface: [64, 256].map((samples) => {
        const started = performance.now();
        const result = finish(
          traceDiffuseReceiverSteps(product.geometry, point.position, point.normal, samples, lights),
        );
        const value = zero();
        accumulate(value, result.transfer, 0, 1);
        return { samples, rays: result.rays, ms: performance.now() - started, value };
      }),
    };
    reports.push(result);
    console.log(JSON.stringify(result));
  }
}
await Bun.write(
  "output/lighting-reference.json",
  JSON.stringify(
    {
      maxSamples: maxSamples ?? 192,
      ...(latticeExperiment ? { latticeRays, latticeSpacing } : {}),
      notes:
        "Uniform unit sky, authored point lights and emission; no direct sun. Independent surface-point cosine integration uses the same diffuse path estimator with Russian roulette and representative material albedo. It is a controlled diffuse reference, not full spectral or glossy ground truth.",
      reports,
      builds,
    },
    null,
    2,
  ),
);
