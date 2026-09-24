import { integrateOrbit, orbitFixture } from "./coherent-ggx";
import { errorStats } from "./gpu-common";

const reports = [];
for (const roughness of [0.04, 0.06, 0.18]) {
  const gpu = await Bun.file(`output/transport-research/coherent-gpu-${roughness}.json`).json();
  const { width, count } = gpu,
    reference = new Float64Array(count),
    start = performance.now();
  let convergence = 0;
  for (let i = 0; i < count; i++) {
    const { orbit, lighting } = orbitFixture(
      ((i % width) + 0.37) / width,
      (Math.floor(i / width) + 0.61) / width,
      roughness,
    );
    reference[i] = integrateOrbit(orbit, lighting, 8192);
    if (i % 997 === 0)
      convergence = Math.max(convergence, Math.abs(reference[i] - integrateOrbit(orbit, lighting, 32768)));
  }
  // Float32 output comparison is separate from the double-precision convergence check.
  const truth = Float32Array.from(reference),
    errors = Object.fromEntries(
      Object.entries(gpu.images).map(([name, values]) => [
        name,
        errorStats(Float32Array.from(values as number[]), truth),
      ]),
    );
  const result = {
    roughness,
    count,
    referenceSamples: 8192,
    cpuMs: performance.now() - start,
    convergence,
    errors,
  };
  reports.push(result);
  console.log(JSON.stringify(result));
  await Bun.write(
    `output/transport-research/coherent-reference-${roughness}.json`,
    JSON.stringify(Array.from(reference)),
  );
}
await Bun.write("output/transport-research/final-accuracy.json", JSON.stringify({ reports }, null, 2));
