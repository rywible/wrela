import { emitWovenWGSL } from "@wrela/compiler";

const path = "packages/render-webgpu/src/woven-material.wgsl";
const generated = emitWovenWGSL();
if (process.argv.includes("--check")) {
  if ((await Bun.file(path).text()) !== generated) throw Error("Woven shader is stale: regenerate it");
} else await Bun.write(path, generated);
