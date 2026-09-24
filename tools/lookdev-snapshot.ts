import { resolve } from "node:path";
import { snapshotSource } from "./source-snapshot";

const captures = {
  scene: "tools/lookdev.ts",
  character: "tools/character-lookdev.ts",
  terrain: "tools/terrain-lookdev.ts",
  materials: "tools/material-lookdev.ts",
  surfaces: "tools/surface-appearance-check.ts",
  foliage: "tools/foliage-lookdev.ts",
  relief: "tools/surface-relief-lookdev.ts",
  vegetation: "tools/vegetation-lookdev.ts",
  environment: "tools/environment-lookdev.ts",
  "environment-check": "tools/environment-authoring-check.ts",
  performance: "tools/performance-lookdev.ts",
  studio: "tools/domain-studio-verification.ts",
  traversal: "tools/world-traversal-lookdev.ts",
  gi: "tools/gi-lookdev.ts",
  lighting: "tools/lighting-upgrade-check.ts",
  "water-reference": "tools/rendering-compiler/water-check.ts",
  variants: "tools/domain-variant-lookdev.ts",
  "thin-coverage": "tools/thin-coverage-lookdev.ts",
  "relief-probe": "tools/surface-relief-probe.ts",
  alpine: "tools/alpine-slice-lookdev.ts",
  "cloud-noise": "tools/cloud-noise-check.ts",
  frontier: "tools/frontier-relief-lookdev.ts",
} as const;
const capture = process.argv.find((arg) => arg.startsWith("--capture="))?.slice(10) ?? "scene";
if (!(capture in captures)) throw new Error(`Unknown lookdev capture ${capture}`);
const destination = resolve("output/lookdev-snapshots", `${Date.now()}-${process.pid}`, "source");
const manifest = await snapshotSource(destination, "lookdev-source-snapshot");
console.log(JSON.stringify({ sourceSnapshot: destination, sourceFingerprint: manifest.sourceFingerprint }));
const child = Bun.spawn(
  [
    process.execPath,
    captures[capture as keyof typeof captures],
    ...process.argv.slice(2).filter((arg) => !arg.startsWith("--capture=")),
  ],
  {
    cwd: destination,
    stdout: "inherit",
    stderr: "inherit",
  },
);
process.exit(await child.exited);
