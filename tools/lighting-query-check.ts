import { resolve } from "node:path";
import { compileIndirectGeometry, traceIndirectRay } from "@wrela/compiler/indirect-query";
import { traceRadianceSampleSteps } from "@wrela/compiler/radiance-transport";
import type { Vec3 } from "@wrela/model";
import { lightingRouteScene } from "./fixtures/lighting-route-scene";
import { snapshotSource } from "./source-snapshot";

const comparison = process.argv.find((a) => a.startsWith("--compare="))?.slice(10);
if (!comparison) throw Error("Supply the frozen compiler source to compare");
if (!process.argv.includes("--frozen")) {
  const destination = resolve("output/lighting-query", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(destination, "lighting-query");
  console.log(JSON.stringify({ source: destination, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn(
    [process.execPath, "tools/lighting-query-check.ts", "--frozen", `--compare=${resolve(comparison)}`],
    { cwd: destination, stdout: "inherit", stderr: "inherit" },
  );
  process.exit(await child.exited);
}
const previous = await import(resolve(comparison, "packages/compiler/src/indirect-query.ts"));
const route = lightingRouteScene(true);
const geometry = compileIndirectGeometry(route.scene.surfaces, { maxTriangles: 200000 });
let state = 47219;
const random = () => {
  state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
  return state / 4294967296;
};
const queries = Array.from({ length: 50000 }, (_, i) => {
  const origin: Vec3 = [(random() - 0.5) * 20, random() * 5, random() * 30 - 17];
  const y = random() * 2 - 1,
    angle = random() * Math.PI * 2,
    radius = Math.sqrt(1 - y * y);
  const direction: Vec3 = [radius * Math.cos(angle), y, radius * Math.sin(angle)];
  return {
    origin,
    direction,
    limit: i % 3 ? Infinity : random() * 25,
    ignore: i % 5 ? -1 : i % geometry.triangles.length,
  };
});
const run = (trace: typeof traceIndirectRay, check = false) => {
  const start = performance.now();
  let hits = 0;
  for (const q of queries) {
    const hit = trace(geometry, q.origin, q.direction, q.limit, q.ignore);
    if (hit) hits++;
    if (
      check &&
      JSON.stringify(hit) !==
        JSON.stringify(previous.traceIndirectRay(geometry, q.origin, q.direction, q.limit, q.ignore))
    )
      throw Error("Ray hit changed");
  }
  return { ms: performance.now() - start, hits };
};
run(traceIndirectRay, true);
const previousTransport = await import(resolve(comparison, "packages/compiler/src/radiance-transport.ts"));
const transport = (trace: typeof traceRadianceSampleSteps) => {
  const transfer = new Float32Array(64 * 972),
    sky = new Float32Array(64 * 9),
    emission = new Float32Array(64 * 27);
  for (let sample = 0; sample < 64; sample++) {
    const steps = trace(
      geometry,
      queries[sample].origin,
      sample,
      64,
      route.scene.environment.pointLights ?? [],
      transfer,
      sky,
      undefined,
      emission,
    );
    while (!steps.next().done) {}
  }
  return { transfer, sky, emission };
};
const oldTransport = transport(previousTransport.traceRadianceSampleSteps),
  newTransport = transport(traceRadianceSampleSteps);
for (const key of ["transfer", "sky", "emission"] as const)
  if (oldTransport[key].some((v, i) => v !== newTransport[key][i])) throw Error(`Transport ${key} changed`);
run(previous.traceIndirectRay);
run(traceIndirectRay);
const trials = Array.from({ length: 6 }, (_, i) => {
  if (i % 2) {
    const after = run(traceIndirectRay),
      before = run(previous.traceIndirectRay);
    return { before, after };
  }
  const before = run(previous.traceIndirectRay),
    after = run(traceIndirectRay);
  return { before, after };
});
const report = {
  queries: queries.length,
  identical: true,
  transportCoefficientsIdentical: true,
  transportSamples: 64,
  triangles: geometry.triangles.length,
  trials,
};
await Bun.write("output/lighting-query.json", JSON.stringify(report, null, 2));
console.log(JSON.stringify(report));
