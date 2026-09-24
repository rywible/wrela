import { resolve } from "node:path";
import { snapshotSource } from "./source-snapshot";

const destination = resolve("output/performance-snapshots", `${Date.now()}-${process.pid}`, "source");
const manifest = await snapshotSource(destination, "performance-source-snapshot");
console.log(JSON.stringify({ sourceSnapshot: destination, sourceFingerprint: manifest.sourceFingerprint }));
const child = Bun.spawn([process.execPath, "tools/performance-lookdev.ts", ...process.argv.slice(2)], {
  cwd: destination,
  stdout: "inherit",
  stderr: "inherit",
});
process.exit(await child.exited);
