import { CLOUD_DOMAIN, compileCloudAdvection } from "@wrela/compiler/cloud-transport";

const path = "packages/render-webgpu/src/atmosphere-cloud-transport.wgsl";
const result = compileCloudAdvection(CLOUD_DOMAIN);
if (process.argv.includes("--check")) {
  if ((await Bun.file(path).text()) !== result.source) throw Error("Compiled cloud transport is stale");
} else await Bun.write(path, result.source);
console.log(`Cloud transport ${process.argv.includes("--check") ? "verified" : "compiled"}: ${path}`);
