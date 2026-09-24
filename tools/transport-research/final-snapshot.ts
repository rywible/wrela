import { createHash } from "node:crypto";
import { readdir } from "node:fs/promises";

const root = "output/transport-research";
const read = (name: string) => Bun.file(`${root}/${name}.json`).json();
const gpu = await read("final-gpu"),
  accuracy = await read("final-accuracy"),
  stress = await read("orbit-stress"),
  cpu = await read("coherent-cpu");
const sourceFiles = (await readdir("tools/transport-research")).filter((name) => name.endsWith(".ts")).sort();
const sourceHashes: Record<string, string> = {};
for (const name of sourceFiles)
  sourceHashes[name] = createHash("sha256")
    .update(new Uint8Array(await Bun.file(`tools/transport-research/${name}`).arrayBuffer()))
    .digest("hex");
const check = await Bun.file(`${root}/final-check.log`).text(),
  tests = await Bun.file(`${root}/final-tests.log`).text();
if (
  !check.includes("Workspace boundaries verified.") ||
  !check.includes("No fixes applied.") ||
  /\berror\b/i.test(check) ||
  !tests.includes("245 pass") ||
  !tests.includes("0 fail") ||
  gpu.errors.length
)
  throw new Error("Final validation evidence is incomplete");
const report = {
  created: new Date().toISOString(),
  baseCommit: "13e39e2ca8915f6287a062fb9e6a81e0cc007b2b",
  scope: "Isolated research prototypes; no production integration or whole-frame performance claim.",
  hardware: "Apple M4 / macOS arm64 / Chrome hardware WebGPU / Metal 3; Bun 1.4.2",
  priorEvidence: ["field-realization-results.json", "transport-compilation-results.json"],
  validation: {
    command: "bun run check; bun test",
    tests: 245,
    failures: 0,
    assertions: 986424,
    checkPassed: true,
    gpuErrors: gpu.errors,
  },
  newFindings: {
    phaseWarp:
      "Exact GGX denominator factorization and a Mobius phase warp turn an ideal squared reciprocal-cosine pole into a degree-one transformed integrand; every shifted lattice with at least two nodes integrates that ideal case exactly. Real GGX response tested on moving light/view orbits.",
    lensMoments:
      "The exact first moment of a spherical lens reduces to sin(r1)^2 S(alpha)c1 + sin(r2)^2 S(beta)c2, where S(x)=x-sin(x)cos(x). Small-angle series removes cancellation before GPU evaluation.",
    rejectedMomentEvaluation: {
      reason: "Direct boundary-vector subtraction lost accuracy in FP32",
      originalNormalizedVectorXRms: 0.00027914281128294904,
      stableNormalizedVectorXRms: 4.38907965106368e-9,
    },
    limits: [
      "Coherent complete phase orbit, not an arbitrary finite eight-wave footprint",
      "Thin orbits can make sparse warped quadrature catastrophically noisy",
      "Spherical-sun results assume opaque spherical geometry and uniform distant radiance",
      "No worldwide novelty assertion",
    ],
  },
  sourceHashes,
  gpu,
  accuracy,
  stress,
  cpu,
};
const artifact = "docs/research/rendering-compiler-final-results.json";
await Bun.write(artifact, JSON.stringify(report, null, 2));
const formatted = Bun.spawn(["bun", "x", "biome", "format", "--write", artifact], {
  stdout: "ignore",
  stderr: "inherit",
});
if ((await formatted.exited) !== 0) throw new Error("Evidence formatting failed");
console.log(
  JSON.stringify({
    artifact,
    sources: sourceFiles.length,
    validation: report.validation,
  }),
);
