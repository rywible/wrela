import { resolve } from "node:path";
import { compileIndirectGeometry } from "@wrela/compiler/indirect-query";
import { traceRadianceSampleSteps } from "@wrela/compiler/radiance-transport";
import { box } from "./fixtures/indirect-scenes";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--frozen")) {
  const destination = resolve("output/lighting-energy", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(destination, "lighting-energy");
  console.log(JSON.stringify({ source: destination, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn([process.execPath, "tools/lighting-energy-check.ts", "--frozen"], {
    cwd: destination,
    stdout: "inherit",
    stderr: "inherit",
  });
  process.exit(await child.exited);
}
const reports = [];
for (const albedo of [0, 0.4, 0.7, 0.9]) {
  // Uniform emission and diffuse reflectance in a closed enclosure have an
  // analytic steady state: L = Le + rho L, independent of the box's shape.
  const surface = box("uniform-enclosure", [-1, -1, -1], [1, 1, 1], [albedo, albedo, albedo]);
  surface.material.emission = { color: [1, 1, 1], intensity: 1 };
  const geometry = compileIndirectGeometry([surface]);
  for (const samples of [64, 256, 4096, 16384, 65536]) {
    let value = 0;
    const started = performance.now();
    const steps = traceRadianceSampleSteps(
      geometry,
      [0, 0, 0],
      0,
      samples,
      [],
      new Float32Array(972),
      new Float32Array(9),
      (direction, incident) => {
        value += (incident[26 * 4] * Math.max(0, direction[1]) * 4) / samples;
      },
    );
    let result = steps.next();
    while (!result.done) result = steps.next();
    const expected = 1 / (1 - albedo);
    const report = {
      albedo,
      samples,
      value,
      expected,
      relativeError: (value - expected) / expected,
      rays: result.value,
      ms: performance.now() - started,
    };
    reports.push(report);
    console.log(JSON.stringify(report));
  }
}
await Bun.write(
  "output/lighting-energy.json",
  JSON.stringify(
    {
      notes:
        "Analytic diffuse enclosure energy check. Uniform unit emission, uniform albedo, no sky or point lights; camera/reference integration is before SH or spatial interpolation. Values are emitted irradiance/pi. This is an independent steady-state physical test, not another implementation of the same bounded path estimator.",
      reports,
    },
    null,
    2,
  ),
);
