// Builds the first-light fixture's WASM from its hand-written WAT, so the runtime can be tested
// end to end without the compiler. `--check` fails if the committed .wasm isn't what the .wat
// gives (CI runs it).
//
//   bun scripts/fixture.ts            write tests/first-light-fixture/program.wasm
//   bun scripts/fixture.ts --check    compare it with a fresh build; write nothing

import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { watToWasm } from "../tests/wat.ts";

const dir = join(import.meta.dir, "../tests/first-light-fixture");
const wasm = await watToWasm(join(dir, "program.wat"));
const out = join(dir, "program.wasm");

if (process.argv.includes("--check")) {
  let current: Uint8Array | undefined;
  try {
    current = await readFile(out);
  } catch {
    current = undefined;
  }
  if (current === undefined || Buffer.compare(Buffer.from(current), Buffer.from(wasm)) !== 0) {
    console.error(
      "tests/first-light-fixture/program.wasm doesn't match its .wat: run `bun run fixture` and commit it",
    );
    process.exit(1);
  }
  console.log("the fixture's WASM is up to date");
} else {
  await writeFile(out, wasm);
  console.log(`wrote ${out}: ${wasm.length} bytes`);
}
