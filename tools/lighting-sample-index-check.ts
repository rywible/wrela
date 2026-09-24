import { resolve } from "node:path";
import { compileRadianceLightingSteps, radianceReceiverMixture } from "@wrela/compiler";
import { RadianceSampleIndex } from "@wrela/compiler/radiance-sample-index";
import { inverseMatrix, normalize, type Vec3 } from "@wrela/model";
import { lightingRouteScene } from "./fixtures/lighting-route-scene";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--frozen")) {
  const source = resolve("output/lighting-sample-index", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(source, "lighting-sample-index");
  console.log(JSON.stringify({ source, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn([process.execPath, "tools/lighting-sample-index-check.ts", "--frozen"], {
    cwd: source,
    stdout: "inherit",
    stderr: "inherit",
  });
  process.exit(await child.exited);
}
const route = lightingRouteScene(false);
const steps = compileRadianceLightingSteps(route.scene.surfaces, {
  key: "query",
  center: [0, 0, 0],
  lights: route.scene.environment.pointLights,
});
let step = steps.next();
while (!step.done) step = steps.next();
const product = step.value,
  index = new RadianceSampleIndex(product.field.positions, product.geometry);
const sources = new Map(route.scene.surfaces.map((s) => [s.id, s]));
const queries: Vec3[] = [];
const stride = Math.max(1, Math.ceil(product.field.report.vertices / 15000));
let visited = 0;
for (const [id, mesh] of product.meshes) {
  const source = sources.get(id);
  if (!source) throw Error("Missing source");
  const m = source.matrix,
    inv = inverseMatrix(m);
  if (!inv) throw Error("Invalid matrix");
  for (let i = 0; i < mesh.positions.length; i += 3) {
    if (visited++ % stride) continue;
    const p = [0, 1, 2].map(
      (a) =>
        m[a] * mesh.positions[i] +
        m[4 + a] * mesh.positions[i + 1] +
        m[8 + a] * mesh.positions[i + 2] +
        m[12 + a],
    ) as Vec3;
    const n = normalize(
      [0, 1, 2].map(
        (a) =>
          inv[a * 4] * mesh.normals[i] +
          inv[a * 4 + 1] * mesh.normals[i + 1] +
          inv[a * 4 + 2] * mesh.normals[i + 2],
      ) as Vec3,
    );
    for (const side of [-1, 1]) queries.push(p.map((v, a) => v + n[a] * side * 0.003) as Vec3);
  }
}
const exhaustive = queries.map((p) => radianceReceiverMixture(product.geometry, product.field.positions, p));
for (let i = 0; i < queries.length; i++) {
  const value = radianceReceiverMixture(product.geometry, product.field.positions, queries[i], index);
  if (value.some((v, j) => v !== exhaustive[i][j])) {
    console.log(
      JSON.stringify({ mismatch: { i, position: queries[i], actual: value, expected: exhaustive[i] } }),
    );
    throw Error(`Mixture mismatch ${i}`);
  }
}
const rounds = [];
for (let round = 0; round < 6; round++) {
  for (const indexed of round % 2 ? [true, false] : [false, true]) {
    if (index.visibility) {
      index.visibility.queries = 0;
      index.visibility.witnessHits = 0;
    }
    const started = performance.now();
    let checksum = 0;
    for (const p of queries)
      checksum += radianceReceiverMixture(
        product.geometry,
        product.field.positions,
        p,
        indexed ? index : undefined,
      )[0];
    rounds.push({
      round,
      indexed,
      ms: performance.now() - started,
      checksum,
      queries: index.visibility?.queries,
      witnessHits: index.visibility?.witnessHits,
    });
  }
}
const report = {
  queries: queries.length,
  exactMixtures: true,
  coherentVisibility: true,
  probeCount: product.field.positions.length,
  rounds,
  build: product.field.report,
};
await Bun.write("output/lighting-sample-index.json", JSON.stringify(report, null, 2));
console.log(JSON.stringify(report));
