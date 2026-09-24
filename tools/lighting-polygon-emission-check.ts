import { resolve } from "node:path";
import { compileRadianceLightingSteps } from "@wrela/compiler";
import { radianceEmitterSteps } from "@wrela/compiler/radiance-emission";
import { localEmission } from "@wrela/compiler/radiance-local-emission";
import { polygonEmission } from "@wrela/compiler/radiance-polygon-emission";
import { inverseMatrix, normalize, type Vec3 } from "@wrela/model";
import { radianceNeighborEstimate } from "@wrela/runtime/radiance-memory";
import { lightingRouteScene } from "./fixtures/lighting-route-scene";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--frozen")) {
  const source = resolve("output/lighting-polygon-emission", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(source, "lighting-polygon-emission");
  console.log(JSON.stringify({ source, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn([process.execPath, "tools/lighting-polygon-emission-check.ts", "--frozen"], {
    cwd: source,
    stdout: "inherit",
    stderr: "inherit",
  });
  process.exit(await child.exited);
}
const finish = <T>(steps: Generator<void, T>) => {
  let next = steps.next();
  while (!next.done) next = steps.next();
  return next.value;
};
const route = lightingRouteScene(false);
const product = finish(
  compileRadianceLightingSteps(route.scene.surfaces, {
    key: "polygon-emission",
    center: [0, 0, 0],
    lights: route.scene.environment.pointLights,
  }),
);
const emitters = finish(radianceEmitterSteps(product.geometry));
const sources = new Map(route.scene.surfaces.map((s) => [s.id, s]));
const queries: { position: Vec3; normal: Vec3; source: string }[] = [];
const stride = Math.max(1, Math.ceil(product.field.report.vertices / 2000));
let visited = 0;
for (const [id, mesh] of product.meshes) {
  const source = sources.get(id);
  if (!source) throw Error("Missing source");
  const m = source.matrix,
    inv = inverseMatrix(m);
  if (!inv) throw Error("Singular source matrix");
  for (let i = 0; i < mesh.positions.length; i += 3) {
    if (visited++ % stride || !mesh.radianceProbes?.[(i / 3) * 4]) continue;
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
    queries.push({ position: p.map((v, a) => v + n[a] * 0.003) as Vec3, normal: n, source: id });
  }
}
const analytic = queries.map((q) => polygonEmission(product.geometry, emitters, q.position, q.normal));
const admitted = analytic.flatMap((v, i) => (v ? [i] : []));
const referenceIds = admitted
  .filter((i) => analytic[i]?.value.some((v) => v > 1e-8))
  .filter((_, i, all) => i % Math.max(1, Math.ceil(all.length / 48)) === 0);
const references = referenceIds.map((id) => {
  const q = queries[id];
  return {
    id,
    ...q,
    analytic: analytic[id],
    sampled: localEmission(product.geometry, emitters, q.position, q.normal).value,
    reference: localEmission(product.geometry, emitters, q.position, q.normal, 4096).value,
  };
});
const rounds = [];
for (let round = 0; round < 6; round++)
  for (const usePolygon of round % 2 ? [true, false] : [false, true]) {
    const start = performance.now();
    let checksum = 0,
      fallback = 0;
    for (const q of queries) {
      const result = usePolygon
        ? polygonEmission(product.geometry, emitters, q.position, q.normal)
        : undefined;
      if (usePolygon && !result) fallback++;
      checksum += (result ?? localEmission(product.geometry, emitters, q.position, q.normal)).value[0];
    }
    rounds.push({ round, usePolygon, ms: performance.now() - start, checksum, fallback });
  }
const report = {
  emitters: emitters.entries.length,
  queries: queries.length,
  admitted: admitted.length,
  rounds,
  references,
  memory: {
    productBytes: product.field.report.bytes,
    geometryBytes: product.geometry.report.bytes,
    nextEstimate: radianceNeighborEstimate(product),
  },
};
await Bun.write("output/lighting-polygon-emission.json", JSON.stringify(report, null, 2));
console.log(JSON.stringify(report));
