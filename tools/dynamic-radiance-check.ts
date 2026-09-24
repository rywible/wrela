import { resolve } from "node:path";
import { indirectGeometryLayers } from "@wrela/compiler/indirect-layers";
import { compileIndirectGeometry } from "@wrela/compiler/indirect-query";
import {
  accumulateRadiancePath,
  compactRadiancePath,
  type RadiancePath,
} from "@wrela/compiler/radiance-path";
import { radianceSamplePositions } from "@wrela/compiler/radiance-placement";
import { traceRadianceSampleSteps } from "@wrela/compiler/radiance-transport";
import type { Bounds, Vec3 } from "@wrela/model";
import { lightingRouteScene } from "./fixtures/lighting-route-scene";
import { snapshotSource } from "./source-snapshot";

const granular = process.argv.includes("--paths");

if (!process.argv.includes("--frozen")) {
  const destination = resolve("output/dynamic-radiance", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(destination, "dynamic-radiance");
  console.log(JSON.stringify({ source: destination, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn(
    [process.execPath, "tools/dynamic-radiance-check.ts", "--frozen", ...(granular ? ["--paths"] : [])],
    {
      cwd: destination,
      stdout: "inherit",
      stderr: "inherit",
    },
  );
  process.exit(await child.exited);
}
function finish<T>(steps: Generator<void, T>): T {
  let item = steps.next();
  while (!item.done) item = steps.next();
  return item.value;
}
function crosses(paths: Float64Array, bounds: Bounds): boolean {
  for (let at = 0; at < paths.length; at += 7) {
    let lo = 0,
      hi = paths[at + 6] + 0.0001;
    for (let a = 0; a < 3; a++) {
      const origin = paths[at + a],
        direction = paths[at + 3 + a];
      const min = bounds.min[a] - 0.0001,
        max = bounds.max[a] + 0.0001;
      if (Math.abs(direction) < 1e-15) {
        if (origin < min || origin > max) hi = -1;
      } else {
        const x = (min - origin) / direction,
          y = (max - origin) / direction;
        lo = Math.max(lo, Math.min(x, y));
        hi = Math.min(hi, Math.max(x, y));
      }
    }
    if (hi >= lo) return true;
  }
  return false;
}
const route = lightingRouteScene(true);
const base = compileIndirectGeometry(route.scene.surfaces, { maxTriangles: 200000 });
const positions = finish(radianceSamplePositions(base, [0, 0, 0], 192));
const lights = route.scene.environment.pointLights ?? [];
const empty = () => ({
  transfer: new Float32Array(positions.length * 972),
  sky: new Float32Array(positions.length * 9),
  emission: new Float32Array(positions.length * 27),
  paths: Array.from({ length: positions.length * (granular ? 64 : 1) }, () => new Float64Array()),
  transport: new Array<RadiancePath>(positions.length * 64),
});
const scratch = empty();
const trace = (geometry: typeof base, state: ReturnType<typeof empty>, i: number, path?: number) => {
  const output = path === undefined ? state : scratch;
  if (path === undefined) {
    output.transfer.fill(0, i * 972, (i + 1) * 972);
    output.sky.fill(0, i * 9, (i + 1) * 9);
    output.emission.fill(0, i * 27, (i + 1) * 27);
  }
  const paths = new Map<number, number[]>();
  const rays = finish(
    traceRadianceSampleSteps(
      geometry,
      positions[i],
      i,
      64,
      lights,
      output.transfer,
      output.sky,
      undefined,
      output.emission,
      (p, d, length, r) => {
        const index = granular ? i * 64 + r : i;
        let list = paths.get(index);
        if (!list) {
          list = [];
          paths.set(index, list);
        }
        list.push(...p, ...d, length);
      },
      {
        first: path,
        end: path === undefined ? undefined : path + 1,
        project: path === undefined,
        observe: granular
          ? (r, result) => {
              state.transport[i * 64 + r] = compactRadiancePath(result);
            }
          : undefined,
      },
    ),
  );
  for (const [index, values] of paths) state.paths[index] = new Float64Array(values);
  return rays;
};
let previous: Bounds | undefined;
const state = empty(),
  reports = [];
for (const angle of [1.5, 1, 0.5, 0, 0.5, 1.5]) {
  const started = performance.now();
  route.setDoor(angle);
  const door = route.scene.surfaces.find((s) => s.id === "cave-door");
  if (!door) throw Error("Missing door");
  const dynamic = compileIndirectGeometry([{ ...door, lightingMobility: undefined }]);
  const geometry = indirectGeometryLayers(base, [dynamic]);
  const queryBuildMs = performance.now() - started;
  const oldBounds = previous;
  const changed: Bounds = oldBounds
    ? {
        min: dynamic.bounds.min.map((v, a) => Math.min(v, oldBounds.min[a])) as Vec3,
        max: dynamic.bounds.max.map((v, a) => Math.max(v, oldBounds.max[a])) as Vec3,
      }
    : dynamic.bounds;
  const queryStart = performance.now();
  const dirty = state.paths.flatMap((paths, i) => (!previous || crosses(paths, changed) ? [i] : []));
  const dependencyMs = performance.now() - queryStart;
  const updateStart = performance.now();
  let rays = 0;
  if (!granular || !previous) {
    for (const i of granular ? positions.map((_, i) => i) : dirty) rays += trace(geometry, state, i);
  } else {
    for (const i of dirty) rays += trace(geometry, state, Math.floor(i / 64), i % 64);
    for (const i of new Set(dirty.map((i) => Math.floor(i / 64)))) {
      state.transfer.fill(0, i * 972, (i + 1) * 972);
      state.sky.fill(0, i * 9, (i + 1) * 9);
      state.emission.fill(0, i * 27, (i + 1) * 27);
      for (let r = 0; r < 64; r++)
        accumulateRadiancePath(state.transport[i * 64 + r], i, 64, state.transfer, state.sky, state.emission);
    }
  }
  const updateMs = performance.now() - updateStart;
  const reference = empty(),
    fullStart = performance.now();
  for (let i = 0; i < positions.length; i++) trace(geometry, reference, i);
  const fullMs = performance.now() - fullStart;
  for (const key of ["transfer", "sky", "emission"] as const)
    if (state[key].some((v, i) => v !== reference[key][i]))
      throw Error(`Selective ${key} update disagrees with full recomputation at angle ${angle}`);
  const report = {
    angle,
    samples: positions.length,
    dirty: dirty.length,
    unit: granular ? "path" : "probe",
    rays,
    queryBuildMs,
    dependencyMs,
    updateMs,
    fullMs,
    exact: true,
    dependencyBytes: state.paths.reduce((sum, p) => sum + p.byteLength, 0),
    transportBytes: granular
      ? state.transport.reduce(
          (sum, p) =>
            sum +
            p.incident.byteLength +
            (p.inputs?.byteLength ?? 0) +
            3 * 8 +
            (p.emitter ? 6 * 8 : 0) +
            (p.directHit ? 3 * 8 : 0),
          0,
        )
      : 0,
  };
  reports.push(report);
  console.log(JSON.stringify(report));
  previous = dynamic.bounds;
}
await Bun.write(
  "output/dynamic-radiance.json",
  JSON.stringify(
    {
      notes:
        "Compiler experiment only: rigid door overlay and selective probe transport. Receiver-mixture visibility, local-emission visibility, temporal scheduling, moving lights and skinned occluders are not implemented by this experiment. Timings are synchronous CPU, not GPU or frame times.",
      reports,
    },
    null,
    2,
  ),
);
