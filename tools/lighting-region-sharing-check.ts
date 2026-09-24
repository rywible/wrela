import { resolve } from "node:path";
import { compileRadianceLightingSteps } from "@wrela/compiler";
import { radianceNeighborEstimate, radianceRetainedBytes } from "@wrela/runtime/radiance-memory";
import { lightingRouteScene } from "./fixtures/lighting-route-scene";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--frozen")) {
  const source = resolve("output/lighting-region-sharing", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(source, "lighting-region-sharing");
  console.log(JSON.stringify({ source, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn([process.execPath, "tools/lighting-region-sharing-check.ts", "--frozen"], {
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
const route = lightingRouteScene(false),
  surfaces = route.scene.surfaces;
const first = finish(
  compileRadianceLightingSteps(surfaces, {
    key: "first",
    center: [0, 0, 16],
    lights: route.scene.environment.pointLights,
  }),
);
const second = finish(
  compileRadianceLightingSteps(surfaces, {
    key: "second",
    center: [0, 0, 0],
    lights: route.scene.environment.pointLights,
    reuse: first,
  }),
);
const identical = (a: Float32Array | undefined, b: Float32Array | undefined) =>
  !!a && !!b && a.length === b.length && a.every((v, i) => v === b[i]);
const fields = (
  ["receivers", "receiverEmission", "transfer", "skyVisibility", "directEmission"] as const
).map((field) => ({
  field,
  bytes: second.field[field]?.byteLength,
  identical: identical(first.field[field], second.field[field]),
}));
const bindings = [...second.meshes].map(([id, mesh]) => ({
  id,
  bytes: mesh.radianceProbes?.byteLength,
  identical: identical(first.meshes.get(id)?.radianceProbes, mesh.radianceProbes),
}));
const table = second.field.receivers ?? new Float32Array();
const emission = second.field.receiverEmission;
const unique = new Set<string>();
const uniqueEmission = new Set<string>();
for (let at = 0; at < table.length; at += 8) {
  const e = Array.from(emission?.subarray((at / 8) * 3, (at / 8) * 3 + 3) ?? []);
  unique.add([...table.subarray(at, at + 8), ...e].join(","));
  uniqueEmission.add(e.join(","));
}
const report = {
  first: first.field.report,
  second: second.field.report,
  retainedBytes: radianceRetainedBytes([first, second]),
  neighborEstimate: radianceNeighborEstimate(first),
  fields,
  bindings,
  receiverEntries: table.length / 8,
  uniqueReceiverEntries: unique.size,
  uniqueEmissionEntries: uniqueEmission.size,
};
await Bun.write("output/lighting-region-sharing.json", JSON.stringify(report, null, 2));
console.log(JSON.stringify(report));
