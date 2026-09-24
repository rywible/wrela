import { compileBranchVisibility } from "@wrela/compiler";
import type { BranchPlate } from "@wrela/model";

const plates = (await Bun.file(
  "output/compiler-frontiers/branch-spatial/geometry.json",
).json()) as BranchPlate[];
const start = performance.now(),
  product = compileBranchVisibility(plates);
await Bun.write(
  "output/compiler-frontiers/branch-spatial/product.json",
  JSON.stringify({
    ...product,
    offsets: Array.from(product.offsets),
    candidates: Array.from(product.candidates),
    compileMs: performance.now() - start,
  }),
);
console.log({ plates: plates.length, pairs: product.candidates.length, bytes: product.byteLength });
