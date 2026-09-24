/** Research only: no renderer defaults or authored documents are changed. */
import {
  evaluatePhasePolynomial,
  generateTerrainPatch,
  phaseFootprintFactor,
  terrainHeight,
} from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import { type Bounds, normalize, type Vec3 } from "@wrela/model";
import { compileFourier, correlatedExpression } from "./fourier";

export const dot = (a: readonly number[], b: readonly number[]) => a.reduce((s, x, i) => s + x * b[i], 0);
export const clamp = (x: number) => Math.max(-1, Math.min(1, x));
export function random(seed = 137) {
  return () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 4294967296;
  };
}
export function errors(reference: number[], actual: number[]) {
  let squared = 0,
    energy = 0,
    maximum = 0;
  for (let i = 0; i < reference.length; i++) {
    squared += (reference[i] - actual[i]) ** 2;
    energy += reference[i] ** 2;
    maximum = Math.max(maximum, Math.abs(reference[i] - actual[i]));
  }
  return {
    relativeL2: Math.sqrt(squared / Math.max(energy, 1e-30)),
    rms: Math.sqrt(squared / reference.length),
    maximum,
  };
}
export type LightDomain = { center: Vec3; radius: number; axis: Vec3; halfAngle: number; oneSided: boolean };
/** Conservative in real arithmetic for a position ball and a unit-normal cone.
 * The deliberate angular margin is not a general floating-point certificate.
 * No distance cutoff: the production point-light attenuation has infinite support. */
export function excludeLight(domain: LightDomain, light: Vec3, normalMotion = 0): boolean {
  if (!domain.oneSided || domain.halfAngle + normalMotion >= Math.PI / 2) return false;
  const offset = light.map((v, i) => v - domain.center[i]) as Vec3;
  const distance = Math.hypot(...offset);
  if (distance <= domain.radius) return false;
  const separation = Math.acos(clamp(dot(domain.axis, offset) / distance));
  const directionSpread = Math.asin(Math.min(1, domain.radius / distance));
  return separation - domain.halfAngle - normalMotion - directionSpread > Math.PI / 2 + 1e-6;
}
export function winterTerrain() {
  const terrain = referenceProject().documents.find((d) => d.kind === "terrain");
  if (!terrain) throw Error("Winter terrain missing");
  return terrain;
}
export function lightFixture(kind: "mixed" | "above" = "mixed") {
  const terrain = winterTerrain();
  const points: number[] = [],
    domains: LightDomain[] = [],
    lists: number[] = [];
  const lights: Vec3[] = Array.from({ length: 8 }, (_, i) => [
    Math.cos(i * 2.399) * 10,
    kind === "mixed" && i % 2 ? -12 : 15,
    Math.sin(i * 2.399) * 10,
  ]);
  let evaluated = 0,
    rejected = 0,
    falseExclusions = 0,
    tested = 0;
  for (let z = -8; z < 8; z++)
    for (let x = -8; x < 8; x++) {
      const mesh = generateTerrainPatch(terrain, x * 2, z * 2, 2, 7);
      const center = mesh.bounds.min.map((v, a) => (v + mesh.bounds.max[a]) / 2) as Vec3;
      const radius = Math.hypot(...mesh.bounds.max.map((v, a) => v - center[a]));
      center[0] += x * 2;
      center[2] += z * 2;
      const sum: Vec3 = [0, 0, 0];
      for (let v = 0; v < mesh.normals.length; v++) sum[v % 3] += mesh.normals[v];
      const axis = normalize(sum);
      let halfAngle = 0;
      for (let v = 0; v < mesh.normals.length; v += 3)
        halfAngle = Math.max(
          halfAngle,
          Math.acos(clamp(dot(axis, Array.from(mesh.normals.slice(v, v + 3))))),
        );
      const domain = { center, radius, axis, halfAngle: halfAngle + 1e-5, oneSided: true };
      const region = domains.length;
      domains.push(domain);
      const active = lights.flatMap((light, i) => (excludeLight(domain, light) ? [] : [i]));
      lists.push(active.length, ...active, ...Array(8 - active.length).fill(0));
      evaluated += 8;
      rejected += 8 - active.length;
      for (let v = 0; v < mesh.positions.length; v += 3) {
        const p: Vec3 = [mesh.positions[v] + x * 2, mesh.positions[v + 1], mesh.positions[v + 2] + z * 2];
        const n = Array.from(mesh.normals.slice(v, v + 3));
        // generateTerrainPatch stores local x/z; its bounds are also local x/z.
        points.push(...p, region, ...n, 0);
        for (let i = 0; i < 8; i++) {
          tested++;
          if (
            !active.includes(i) &&
            dot(
              n,
              lights[i].map((q, a) => q - p[a]),
            ) > 1e-7
          )
            falseExclusions++;
        }
      }
    }
  return {
    points,
    domains,
    lists,
    lights,
    summary: {
      kind,
      regions: domains.length,
      points: points.length / 8,
      evaluated,
      rejected,
      falseExclusions,
      tested,
    },
  };
}
export function temporalProbe() {
  const rng = random(492);
  const program = compileFourier(correlatedExpression, 2);
  const reference: number[] = [],
    compiled: number[] = [],
    separate: number[] = [],
    point: number[] = [],
    convergence: number[] = [];
  // Correlated factors f=.6+.35cos(phi), g=.5+.45cos(phi+delta).
  // Multiplication creates a DC term and a second harmonic. Filtering factors
  // independently loses those terms even when every factor is filtered exactly.
  for (let test = 0; test < 160; test++) {
    const phase = rng() * Math.PI * 2,
      delta = rng() * Math.PI * 2;
    const f = { origin: [phase], dx: [rng() * 30], dy: [rng() * 8], shutter: [rng() * 20] };
    const average = (mode: number, shift: number) =>
      Math.cos(mode * phase + shift) * phaseFootprintFactor([mode], f);
    compiled.push(
      evaluatePhasePolynomial(program, {
        origin: [phase, delta],
        dx: [f.dx[0], 0],
        dy: [f.dy[0], 0],
        shutter: [f.shutter[0], 0],
      }).value,
    );
    separate.push((0.6 + 0.35 * average(1, 0)) * (0.5 + 0.45 * average(1, delta)));
    point.push((0.6 + 0.35 * Math.cos(phase)) * (0.5 + 0.45 * Math.cos(phase + delta)));
    const integrate = (steps: number) => {
      let result = 0;
      for (let z = 0; z < steps; z++)
        for (let y = 0; y < steps; y++)
          for (let x = 0; x < steps; x++) {
            const p =
              phase +
              ((x + 0.5) / steps - 0.5) * f.dx[0] +
              ((y + 0.5) / steps - 0.5) * f.dy[0] +
              ((z + 0.5) / steps - 0.5) * f.shutter[0];
            result += (0.6 + 0.35 * Math.cos(p)) * (0.5 + 0.45 * Math.cos(p + delta));
          }
      return result / steps ** 3;
    };
    reference.push(integrate(48));
    convergence.push(integrate(24));
  }
  // A curvature counterexample: the affine footprint does not describe accelerated motion.
  const curvedReference: number[] = [],
    affinePrediction: number[] = [];
  for (let i = 0; i < 100; i++) {
    const p = rng() * 6.28,
      velocity = rng() * 12,
      curvature = 30;
    let sum = 0;
    for (let j = 0; j < 4096; j++) {
      const t = (j + 0.5) / 4096 - 0.5;
      sum += 0.5 + 0.5 * Math.cos(p + velocity * t + curvature * t * t);
    }
    curvedReference.push(sum / 4096);
    affinePrediction.push(
      0.5 +
        0.5 * Math.cos(p) * phaseFootprintFactor([1], { origin: [p], dx: [0], dy: [0], shutter: [velocity] }),
    );
  }
  return {
    program,
    cases: reference.length,
    samplesPerReference: 48 ** 3,
    compiled: errors(reference, compiled),
    independentFactors: errors(reference, separate),
    pointSample: errors(reference, point),
    referenceConvergence: errors(reference, convergence),
    curvedMotionFailure: errors(curvedReference, affinePrediction),
    values: { reference, compiled, separate, point },
  };
}
export function intersects(a: Bounds, b: Bounds) {
  return a.min.every((v, i) => v <= b.max[i] && a.max[i] >= b.min[i]);
}
export function editProbe() {
  const terrain = winterTerrain();
  const edit = {
    id: "compiler-probe",
    kind: "raise" as const,
    center: [0, 0] as [number, number],
    radius: 1.2,
    strength: 4,
    targetHeight: 0,
  };
  const modified = { ...terrain, interventions: [...terrain.interventions, edit] };
  let dirty = 0,
    changed = 0,
    missed = 0,
    tested = 0;
  let cachedBytes = 0;
  const start = performance.now();
  const patches = [];
  for (let z = -8; z < 8; z++)
    for (let x = -8; x < 8; x++) {
      const bounds = {
        min: [x * 2, -Infinity, z * 2] as Vec3,
        max: [x * 2 + 2, Infinity, z * 2 + 2] as Vec3,
      };
      const influence = { min: [-1.25, -Infinity, -1.25] as Vec3, max: [1.25, Infinity, 1.25] as Vec3 };
      const affected = intersects(bounds, influence); // Includes the production 5 cm normal stencil.
      const before = generateTerrainPatch(terrain, x * 2, z * 2, 2, 8);
      patches.push({ x, z, before, affected });
      cachedBytes += before.positions.byteLength + before.normals.byteLength + before.indices.byteLength;
    }
  const initialMs = performance.now() - start;
  const fullStart = performance.now();
  const after = patches.map(({ x, z }) => generateTerrainPatch(modified, x * 2, z * 2, 2, 8));
  const fullMs = performance.now() - fullStart;
  const incrementalStart = performance.now();
  const incremental = patches.map(({ x, z, before, affected }) =>
    affected ? generateTerrainPatch(modified, x * 2, z * 2, 2, 8) : before,
  );
  const incrementalMs = performance.now() - incrementalStart;
  for (let i = 0; i < patches.length; i++) {
    if (patches[i].affected) dirty++;
    let different = false;
    for (const property of ["positions", "normals"] as const)
      for (let j = 0; j < after[i][property].length; j++) {
        tested++;
        if (patches[i].before[property][j] !== after[i][property][j]) different = true;
        if (incremental[i][property][j] !== after[i][property][j]) missed++;
      }
    if (different) changed++;
  }
  // Explicit distant-effect counterexample. Orthographic sun along +x on a
  // sampled height field; this is a discrete horizon reference, not production shadows.
  const size = 256,
    spacing = 0.25;
  const heights = (source: typeof terrain) =>
    Float64Array.from({ length: size * size }, (_, i) =>
      terrainHeight(source, ((i % size) - size / 2) * spacing, (Math.floor(i / size) - size / 2) * spacing),
    );
  const a = heights(terrain),
    b = heights(modified);
  const shadow = (h: Float64Array, elevation: number) => {
    const result = new Uint8Array(h.length),
      slope = Math.tan(elevation);
    for (let z = 0; z < size; z++) {
      let horizon = -Infinity;
      for (let x = size - 1; x >= 0; x--) {
        const i = z * size + x,
          value = h[i] - x * spacing * slope;
        result[i] = Number(value < horizon - 1e-9);
        horizon = Math.max(horizon, value);
      }
    }
    return result;
  };
  const shadows = [0.12, 0.6].map((elevation) => {
    const old = shadow(a, elevation),
      current = shadow(b, elevation);
    let changes = 0,
      outsideLocal = 0,
      conservativeInvalidated = 0,
      missedBySweep = 0;
    for (let i = 0; i < old.length; i++) {
      const x = ((i % size) - size / 2) * spacing,
        z = (Math.floor(i / size) - size / 2) * spacing;
      const sweep = x <= 1.25 && Math.abs(z) <= 1.25;
      if (sweep) conservativeInvalidated++;
      if (old[i] !== current[i]) {
        changes++;
        if (Math.abs(x) > 1.25 || Math.abs(z) > 1.25) outsideLocal++;
        if (!sweep) missedBySweep++;
      }
    }
    return { elevation, changes, outsideLocal, conservativeInvalidated, missedBySweep, samples: old.length };
  });
  return {
    patches: patches.length,
    dirty,
    changed,
    missed,
    tested,
    cachedBytes,
    initialMs,
    fullMs,
    incrementalMs,
    shadows,
  };
}
