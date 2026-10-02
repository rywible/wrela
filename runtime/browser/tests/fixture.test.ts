// The first-light fixture stays what it claims to be: the bootstrap page, the contract's manifest,
// and WASM built from its WAT.

import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { Manifest } from "../src/manifest.ts";
import { contractManifest, FIXTURE, PACKAGE } from "./helpers.ts";
import { watToWasm } from "./wat.ts";

test("the fixture's page is the bootstrap page, loading the runtime from dist/", async () => {
  const bootstrap = await readFile(join(PACKAGE, "src/index.html"), "utf8");
  const page = await readFile(join(FIXTURE, "index.html"), "utf8");
  expect(bootstrap).toContain('<script type="module" src="wrela.js"></script>');
  expect(page).toBe(bootstrap.replace('src="wrela.js"', 'src="../../dist/wrela.js"'));
});

test("the fixture's manifest is the contract's first-light manifest", async () => {
  const fixture = Manifest.fromJson(await readFile(join(FIXTURE, "manifest.json"), "utf8"));
  expect(fixture).toEqual(Manifest.fromJson(await contractManifest()));
});

test("the fixture's WASM is built from its WAT", async () => {
  const committed: Uint8Array = new Uint8Array(await readFile(join(FIXTURE, "program.wasm")));
  expect(committed).toEqual(await watToWasm(join(FIXTURE, "program.wat")));
});
