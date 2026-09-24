import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { sourceManifest } from "../evidence";
import { editProbe, lightFixture, temporalProbe } from "./probes";

const output = resolve(process.argv[2] ?? "output/compiler-frontiers");
await mkdir(output, { recursive: true });
const before = await sourceManifest("compiler-frontiers-cpu");
const results = {
  lighting: [lightFixture("mixed").summary, lightFixture("above").summary],
  temporal: temporalProbe(),
  edits: editProbe(),
};
const after = await sourceManifest("compiler-frontiers-cpu");
await Bun.write(`${output}/cpu.json`, JSON.stringify(results, null, 2));
await Bun.write(
  `${output}/source-manifest.json`,
  JSON.stringify({ ...before, sourceStable: before.sourceFingerprint === after.sourceFingerprint }, null, 2),
);
console.log(
  JSON.stringify({ output, ...results, temporal: { ...results.temporal, values: undefined } }, null, 2),
);
if (
  results.lighting.some((r) => r.falseExclusions) ||
  results.edits.missed ||
  results.edits.shadows.some((r) => r.missedBySweep)
)
  throw Error("Unsafe compiler candidate");
