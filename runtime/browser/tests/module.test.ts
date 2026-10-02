// The program ABI's import and export rules, read from the module's bytes.

import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { checkProgramModule, ModuleError } from "../src/module.ts";
import { FIXTURE, thrown } from "./helpers.ts";
import { type Features, wat } from "./wat.ts";

const SUBMIT = `(import "wrela" "submit" (func $submit (param i32 i32)))`;
const MEMORY = `(memory (export "memory") 1)`;
const FRAME = `(func (export "frame") (param f32 i32 i32))`;

async function problem(text: string): Promise<string | undefined> {
  const bytes = await wat(`(module ${text})`);
  try {
    checkProgramModule(bytes);
    return undefined;
  } catch (e) {
    expect(e).toBeInstanceOf(ModuleError);
    return (e as Error).message;
  }
}

test("the fixture and minimal programs pass", async () => {
  checkProgramModule(await readFile(join(FIXTURE, "program.wasm")));
  expect(await problem(`${SUBMIT} ${MEMORY} ${FRAME}`)).toBeUndefined();
  // Other functions and types, before or after, don't confuse the indices.
  expect(
    await problem(`(type (func (param f64) (result f64))) ${SUBMIT} (func $helper (param i64)) ${MEMORY} ${FRAME}
      (func (export "other") (param i32) (result i32) local.get 0)`),
  ).toBeUndefined();
});

// The same features the native host's engine has turned off (program/tests.rs's
// `simd_is_rejected`, `shared_and_64_bit_memories_are_rejected`).
test("programs keep to WebAssembly 2.0 without SIMD, threads or 64-bit memory", async () => {
  const outside = async (text: string, features: Features = {}) => {
    const bytes = await wat(`(module ${text})`, "test.wat", features);
    const error = thrown(() => checkProgramModule(bytes));
    expect(error).toBeInstanceOf(ModuleError);
    return (error as Error).message;
  };
  const body = (code: string) =>
    `${SUBMIT} ${MEMORY} (func (export "frame") (param f32 i32 i32) ${code})`;
  expect(await outside(body("(drop (i32x4.extract_lane 0 (v128.const i32x4 1 2 3 4)))"))).toBe(
    "the program uses SIMD instructions; programs keep to WebAssembly 2.0 without SIMD, threads or 64-bit memory",
  );
  expect(
    await outside(
      body(
        "(drop (f32x4.extract_lane 0 (f32x4.relaxed_madd (v128.const f32x4 1 1 1 1) (v128.const f32x4 1 1 1 1) (v128.const f32x4 1 1 1 1))))",
      ),
      { relaxed_simd: true },
    ),
  ).toContain("uses SIMD instructions");
  expect(await outside(body("(local v128)"))).toContain("a `v128` local");
  expect(
    await outside(`${SUBMIT} ${MEMORY} ${FRAME} (global v128 (v128.const i64x2 0 0))`),
  ).toContain("a `v128` global");
  expect(
    await outside(`${SUBMIT} (memory (export "memory") 1 1 shared) ${FRAME}`, { threads: true }),
  ).toContain("a shared memory");
  expect(
    await outside(`${SUBMIT} (memory (export "memory") i64 1) ${FRAME}`, { memory64: true }),
  ).toContain("a 64-bit memory");
  expect(
    await outside(
      `${SUBMIT} (memory (export "memory") 1 1 shared) (func (export "frame") (param f32 i32 i32) (drop (i32.atomic.load (i32.const 0))))`,
      { threads: true },
    ),
  ).toContain("a shared memory");
  // Everything else in WebAssembly 2.0 passes: blocks with types, branch tables, indirect calls,
  // typed selects, references, bulk memory, saturating conversions, sign extension, constants.
  expect(
    await problem(`${SUBMIT} ${MEMORY} (table 1 funcref) (elem (i32.const 0) $f) (type $t (func (param i32) (result i32)))
      (global $g (mut i32) (i32.const -5))
      (data $d "abc")
      (func $f (type $t) local.get 0)
      (func (export "frame") (param f32 i32 i32) (local i64 f64)
        (block $b (loop $l (br_table $b $l (i32.const 1))))
        (drop (block (result i32) (i32.const 3)))
        (drop (call_indirect (type $t) (i32.const 5) (i32.const 0)))
        (drop (select (result i32) (i32.const 1) (i32.const 2) (i32.const 0)))
        (drop (ref.is_null (ref.func $f)))
        (drop (ref.is_null (ref.null func)))
        (memory.copy (i32.const 0) (i32.const 8) (i32.const 4))
        (memory.fill (i32.const 0) (i32.const 0) (i32.const 4))
        (memory.init $d (i32.const 0) (i32.const 0) (i32.const 3))
        (data.drop $d)
        (drop (table.size))
        (drop (i32.trunc_sat_f32_s (local.get 0)))
        (drop (i32.extend8_s (i32.const 300)))
        (local.set 3 (i64.const -9007199254740993))
        (local.set 4 (f64.const 1.5))
        (global.set $g (i32.load offset=4 (i32.const 0)))
        (if (i32.const 1) (then (i64.store (i32.const 0) (local.get 3))))
        (drop (memory.grow (i32.const 0))))`),
  ).toBeUndefined();
});

test("the only import is wrela.submit: (i32, i32) -> ()", async () => {
  expect(
    await problem(`${SUBMIT} (import "env" "log" (func (param i32))) ${MEMORY} ${FRAME}`),
  ).toBe("the program imports `env.log`; the only import a program may have is `wrela.submit`");
  expect(await problem(`(import "wrela" "time" (func (result f64))) ${MEMORY} ${FRAME}`)).toBe(
    "the program imports `wrela.time`; the only import a program may have is `wrela.submit`",
  );
  expect(await problem(`(import "wrela" "submit" (func (param i32))) ${MEMORY} ${FRAME}`)).toBe(
    "the program imports `wrela.submit` as (i32) -> (); it must be (i32, i32) -> ()",
  );
  expect(
    await problem(
      `(import "wrela" "submit" (func (param i32 i32) (result i32))) ${MEMORY} ${FRAME}`,
    ),
  ).toBe("the program imports `wrela.submit` as (i32, i32) -> (i32); it must be (i32, i32) -> ()");
  expect(
    await problem(`(import "wrela" "submit" (memory 1)) ${FRAME} (export "memory" (memory 0))`),
  ).toBe("the program imports `wrela.submit` as a memory, not a function");
  expect(await problem(`(import "wrela" "submit" (global i32)) ${MEMORY} ${FRAME}`)).toBe(
    "the program imports `wrela.submit` as a global, not a function",
  );
});

test("it imports wrela.submit exactly once", async () => {
  expect(await problem(`${MEMORY} ${FRAME}`)).toBe(
    "the program doesn't import `wrela.submit`, so it can't draw anything",
  );
  expect(
    await problem(`${SUBMIT} (import "wrela" "submit" (func (param i32 i32))) ${MEMORY} ${FRAME}`),
  ).toBe("the program imports `wrela.submit` 2 times; import it once");
});

test("it exports memory and frame: (f32, i32, i32) -> ()", async () => {
  expect(await problem(`${SUBMIT} ${MEMORY}`)).toBe("the program doesn't export `frame`");
  expect(await problem(`${SUBMIT} (memory 1) ${FRAME}`)).toBe(
    "the program doesn't export `memory`",
  );
  expect(await problem(`${SUBMIT} (global (export "memory") i32 (i32.const 0)) ${FRAME}`)).toBe(
    "the program exports `memory` as a global, not a memory",
  );
  expect(await problem(`${SUBMIT} ${MEMORY} (func (export "frame") (param i32 i32 i32))`)).toBe(
    "the program exports `frame` as (i32, i32, i32) -> (); it must be (f32, i32, i32) -> ()",
  );
  expect(
    await problem(
      `${SUBMIT} ${MEMORY} (func (export "frame") (param f32 i32 i32) (result i32) i32.const 0)`,
    ),
  ).toBe(
    "the program exports `frame` as (f32, i32, i32) -> (i32); it must be (f32, i32, i32) -> ()",
  );
  expect(await problem(`${SUBMIT} ${MEMORY} (func (export "frame") (param f64 i32 i32))`)).toBe(
    "the program exports `frame` as (f64, i32, i32) -> (); it must be (f32, i32, i32) -> ()",
  );
  expect(await problem(`${SUBMIT} ${MEMORY} (export "frame" (func $submit))`)).toBe(
    "the program exports `frame` as (i32, i32) -> (); it must be (f32, i32, i32) -> ()",
  );
  expect(await problem(`${SUBMIT} ${MEMORY} (global (export "frame") i32 (i32.const 0))`)).toBe(
    "the program exports `frame` as a global, not a function",
  );
});

test("bytes that aren't a module, or end early, are errors, never guesses", async () => {
  const good = await wat(`(module ${SUBMIT} ${MEMORY} ${FRAME})`);
  expect((thrown(() => checkProgramModule(new Uint8Array())) as Error).message).toContain(
    "ends early",
  );
  expect(
    (thrown(() => checkProgramModule(Uint8Array.of(0, 0x61, 0x73, 0x6d, 2, 0, 0, 0))) as Error)
      .message,
  ).toBe("the program isn't a WASM module (version 1)");
  // A truncation fails with a ModuleError, never anything else. (A cut at a section boundary
  // after the exports reads fine here; the engine's own validation rejects it before this runs.)
  let failures = 0;
  for (let cut = 8; cut < good.length; cut++) {
    try {
      checkProgramModule(good.subarray(0, cut));
    } catch (e) {
      expect(e).toBeInstanceOf(ModuleError);
      failures++;
    }
  }
  expect(failures).toBeGreaterThan(good.length / 2);
  // A type claiming 2^32 - 1 parameters ends early; it doesn't allocate them.
  const huge = Uint8Array.of(
    0,
    0x61,
    0x73,
    0x6d,
    1,
    0,
    0,
    0,
    1,
    7,
    1,
    0x60,
    0xff,
    0xff,
    0xff,
    0xff,
    0x0f,
  );
  expect((thrown(() => checkProgramModule(huge)) as Error).message).toBe(
    "the WASM module's type section can't be read: it ends early",
  );
  // An over-long LEB128 number.
  const leb = Uint8Array.of(0, 0x61, 0x73, 0x6d, 1, 0, 0, 0, 1, 0x80, 0x80, 0x80, 0x80, 0x80, 0);
  expect((thrown(() => checkProgramModule(leb)) as Error).message).toContain("too long");
});
