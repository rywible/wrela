// Checks a WASM module against the program ABI before it runs: its one import is
// `wrela.submit: (i32, i32) -> ()`, it exports `memory` (32-bit, not shared) and
// `frame: (f32, i32, i32) -> ()`, and it keeps to the contract's baseline: WebAssembly 2.0 without
// SIMD, relaxed SIMD (whose results V8 leaves to the hardware), threads or 64-bit memory. The
// native host refuses the same modules: its engine has those features turned off.
//
// JavaScript can't see these types: any JS function satisfies any function import, and an
// exported function shows only its parameter count. So this reads them from the binary's type,
// import, function and export sections. It runs on modules the engine has already validated, but
// still checks every bound: it never guesses at bytes it can't read.

import { FRAME, IMPORT_MODULE, MEMORY, SUBMIT } from "./abi.ts";

export class ModuleError extends Error {
  override readonly name = "ModuleError";
}

/** A function type, written as in runtime/command-stream.md: `(f32, i32, i32) -> ()`. */
export interface FuncType {
  readonly params: readonly string[];
  readonly results: readonly string[];
}

export function formatFuncType(type: FuncType): string {
  return `(${type.params.join(", ")}) -> (${type.results.join(", ")})`;
}

const SUBMIT_TYPE: FuncType = { params: ["i32", "i32"], results: [] };
const FRAME_TYPE: FuncType = { params: ["f32", "i32", "i32"], results: [] };

const VALUE_TYPES: ReadonlyMap<number, string> = new Map([
  [0x7f, "i32"],
  [0x7e, "i64"],
  [0x7d, "f32"],
  [0x7c, "f64"],
  [0x7b, "v128"],
  [0x70, "funcref"],
  [0x6f, "externref"],
]);

const EXTERNAL_KINDS = ["function", "table", "memory", "global", "tag"];

class Reader {
  private at = 0;
  constructor(
    private readonly bytes: Uint8Array,
    private readonly what: string,
  ) {}

  done(): boolean {
    return this.at >= this.bytes.length;
  }

  private fail(problem: string): never {
    throw new ModuleError(`the WASM module's ${this.what} can't be read: ${problem}`);
  }

  byte(): number {
    const b = this.bytes[this.at];
    if (b === undefined) {
      this.fail("it ends early");
    }
    this.at += 1;
    return b;
  }

  /** An unsigned LEB128 number of at most 32 bits. */
  u32(): number {
    let result = 0;
    for (let shift = 0; shift < 35; shift += 7) {
      const b = this.byte();
      result += (b & 0x7f) * 2 ** shift;
      if ((b & 0x80) === 0) {
        if (result > 0xffff_ffff) {
          this.fail("a number is too large");
        }
        return result;
      }
    }
    return this.fail("a number is too long");
  }

  take(len: number): Uint8Array {
    if (this.at + len > this.bytes.length) {
      this.fail("it ends early");
    }
    const slice = this.bytes.subarray(this.at, this.at + len);
    this.at += len;
    return slice;
  }

  /** Skips a signed or unsigned LEB128 number (up to 64 bits) whose value isn't needed. */
  leb(): void {
    for (let n = 0; n < 10; n++) {
      if ((this.byte() & 0x80) === 0) {
        return;
      }
    }
    this.fail("a number is too long");
  }

  name(): string {
    const bytes = this.take(this.u32());
    try {
      return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      return this.fail("a name isn't UTF-8");
    }
  }

  valueType(): string {
    const b = this.byte();
    return (
      VALUE_TYPES.get(b) ?? this.fail(`value type 0x${b.toString(16)} isn't one this host reads`)
    );
  }
}

/** A function import, and its type index. */
interface Import {
  readonly module: string;
  readonly name: string;
  readonly type: number;
}

interface Export {
  readonly name: string;
  readonly kind: string;
  readonly index: number;
}

function outsideBaseline(what: string): never {
  throw new ModuleError(
    `the program uses ${what}; programs keep to WebAssembly 2.0 without SIMD, threads or 64-bit memory`,
  );
}

const BLOCK_VALUE_TYPES = new Set([0x40, 0x7f, 0x7e, 0x7d, 0x7c, 0x70, 0x6f]);

/**
 * Reads instructions to the end of `r` (a function body), or to the first `end` (a constant
 * expression, when `constant`), refusing any outside the baseline. Only the baseline's
 * instructions are decoded, so anything else, SIMD's `0xfd` prefix and threads' `0xfe` among it,
 * is refused where it starts: there's no need to know its immediates.
 */
function instructions(r: Reader, constant: boolean): void {
  while (!r.done()) {
    const op = r.byte();
    if (
      op <= 0x01 ||
      op === 0x05 ||
      op === 0x0f ||
      op === 0x1a ||
      op === 0x1b ||
      op === 0xd1 ||
      (op >= 0x45 && op <= 0xc4)
    ) {
      continue; // no immediates
    }
    switch (op) {
      case 0x0b:
        if (constant) {
          return;
        }
        break;
      case 0x02:
      case 0x03:
      case 0x04: {
        const type = r.byte();
        if (type === 0x7b) {
          outsideBaseline("a block of `v128`, which is SIMD");
        }
        if (!BLOCK_VALUE_TYPES.has(type) && (type & 0x80) !== 0) {
          r.leb(); // the rest of a type index
        }
        break;
      }
      case 0x0c:
      case 0x0d:
      case 0x10:
      case 0x20:
      case 0x21:
      case 0x22:
      case 0x23:
      case 0x24:
      case 0x25:
      case 0x26:
      case 0xd2:
        r.u32();
        break;
      case 0x0e:
        for (let n = r.u32(); n >= 0; n--) {
          r.u32(); // the labels, then the default
        }
        break;
      case 0x11:
        r.u32();
        r.u32();
        break;
      case 0x1c:
        for (let n = r.u32(); n > 0; n--) {
          if (r.valueType() === "v128") {
            outsideBaseline("a `select` of `v128`, which is SIMD");
          }
        }
        break;
      case 0x3f:
      case 0x40:
      case 0xd0:
        r.byte();
        break;
      case 0x41:
      case 0x42:
        r.leb();
        break;
      case 0x43:
        r.take(4);
        break;
      case 0x44:
        r.take(8);
        break;
      case 0xfc:
        miscellaneous(r);
        break;
      case 0xfd:
        outsideBaseline("SIMD instructions");
        break;
      case 0xfe:
        outsideBaseline("atomic instructions, which are threads");
        break;
      default:
        if (op >= 0x28 && op <= 0x3e) {
          r.u32(); // a load's or store's alignment and offset
          r.u32();
        } else {
          outsideBaseline(`instruction 0x${op.toString(16)}`);
        }
    }
  }
}

/** The `0xfc` instructions of the baseline: saturating conversions, bulk memory and tables. */
function miscellaneous(r: Reader): void {
  const op = r.u32();
  switch (op) {
    case 8: // memory.init
      r.u32();
      r.byte();
      break;
    case 10: // memory.copy
      r.byte();
      r.byte();
      break;
    case 11: // memory.fill
      r.byte();
      break;
    case 9: // data.drop
    case 13: // elem.drop
    case 15: // table.grow
    case 16: // table.size
    case 17: // table.fill
      r.u32();
      break;
    case 12: // table.init
    case 14: // table.copy
      r.u32();
      r.u32();
      break;
    default:
      if (op > 7) {
        outsideBaseline(`instruction 0xfc ${op}`);
      }
  }
}

/**
 * Checks `bytes`, a valid WASM module, against the program ABI. Throws `ModuleError` saying what's
 * wrong: an import other than `wrela.submit`, a missing export, or a mistyped one.
 */
export function checkProgramModule(bytes: Uint8Array): void {
  const header = new Reader(bytes, "header");
  const magic = header.take(4);
  const version = header.take(4);
  if (String.fromCharCode(...magic) !== "\0asm" || version.join() !== "1,0,0,0") {
    throw new ModuleError("the program isn't a WASM module (version 1)");
  }

  const types: FuncType[] = [];
  const imports: Import[] = [];
  const functions: number[] = []; // type indices of the functions the module defines
  const exports: Export[] = [];
  const sections = new Reader(bytes.subarray(8), "section list");
  while (!sections.done()) {
    const id = sections.byte();
    const body = sections.take(sections.u32());
    switch (id) {
      case 1: {
        const r = new Reader(body, "type section");
        for (let n = r.u32(); n > 0; n--) {
          const form = r.byte();
          if (form !== 0x60) {
            throw new ModuleError(
              `the WASM module's type section has form 0x${form.toString(16)}; this host reads only plain function types`,
            );
          }
          // Counted reads, never preallocated: a count can't make the reader allocate more
          // than the bytes it has.
          const list = (): string[] => {
            const values: string[] = [];
            for (let k = r.u32(); k > 0; k--) {
              values.push(r.valueType());
            }
            return values;
          };
          const params = list();
          const results = list();
          if ([...params, ...results].includes("v128")) {
            outsideBaseline("a function type with a `v128`, which is SIMD");
          }
          types.push({ params, results });
        }
        break;
      }
      case 2: {
        const r = new Reader(body, "import section");
        for (let n = r.u32(); n > 0; n--) {
          const module = r.name();
          const name = r.name();
          const kind = r.byte();
          if (module !== IMPORT_MODULE || name !== SUBMIT) {
            throw new ModuleError(
              `the program imports \`${module}.${name}\`; the only import a program may have is \`${IMPORT_MODULE}.${SUBMIT}\``,
            );
          }
          if (kind !== 0) {
            const what = EXTERNAL_KINDS[kind] ?? `kind ${kind}`;
            throw new ModuleError(
              `the program imports \`${module}.${name}\` as a ${what}, not a function`,
            );
          }
          imports.push({ module, name, type: r.u32() });
        }
        break;
      }
      case 3: {
        const r = new Reader(body, "function section");
        for (let n = r.u32(); n > 0; n--) {
          functions.push(r.u32());
        }
        break;
      }
      case 7: {
        const r = new Reader(body, "export section");
        for (let n = r.u32(); n > 0; n--) {
          const name = r.name();
          const kind = r.byte();
          exports.push({ name, kind: EXTERNAL_KINDS[kind] ?? `kind ${kind}`, index: r.u32() });
        }
        break;
      }
      case 5: {
        const r = new Reader(body, "memory section");
        for (let n = r.u32(); n > 0; n--) {
          const flags = r.byte();
          if ((flags & 0x02) !== 0) {
            outsideBaseline("a shared memory, which is threads");
          }
          if ((flags & 0x04) !== 0) {
            outsideBaseline("a 64-bit memory");
          }
          r.u32();
          if ((flags & 0x01) !== 0) {
            r.u32();
          }
        }
        break;
      }
      case 6: {
        const r = new Reader(body, "global section");
        for (let n = r.u32(); n > 0; n--) {
          if (r.valueType() === "v128") {
            outsideBaseline("a `v128` global, which is SIMD");
          }
          r.byte(); // mutability
          instructions(r, true);
        }
        break;
      }
      case 10: {
        const r = new Reader(body, "code section");
        for (let n = r.u32(); n > 0; n--) {
          const code = new Reader(r.take(r.u32()), "code section");
          for (let k = code.u32(); k > 0; k--) {
            code.u32();
            if (code.valueType() === "v128") {
              outsideBaseline("a `v128` local, which is SIMD");
            }
          }
          instructions(code, false);
        }
        break;
      }
      default:
        break; // custom, table, start, element, data, …: not needed here
    }
  }

  const typeOf = (index: number, what: string): FuncType => {
    const type = types[index];
    if (type === undefined) {
      throw new ModuleError(`${what} has type ${index}, which doesn't exist`);
    }
    return type;
  };
  const sameType = (a: FuncType, b: FuncType) =>
    a.params.join() === b.params.join() && a.results.join() === b.results.join();

  for (const i of imports) {
    const type = typeOf(i.type, `\`${i.module}.${i.name}\``);
    if (!sameType(type, SUBMIT_TYPE)) {
      throw new ModuleError(
        `the program imports \`${IMPORT_MODULE}.${SUBMIT}\` as ${formatFuncType(type)}; it must be ${formatFuncType(SUBMIT_TYPE)}`,
      );
    }
  }

  // Exactly one import, as the contract says and the native host checks: a program that can't
  // submit can't draw, so it's rejected here rather than at its first frame.
  if (imports.length === 0) {
    throw new ModuleError(
      `the program doesn't import \`${IMPORT_MODULE}.${SUBMIT}\`, so it can't draw anything`,
    );
  }
  if (imports.length > 1) {
    throw new ModuleError(
      `the program imports \`${IMPORT_MODULE}.${SUBMIT}\` ${imports.length} times; import it once`,
    );
  }

  const exported = (name: string, kind: string): Export => {
    const e = exports.find((x) => x.name === name);
    if (e === undefined) {
      throw new ModuleError(`the program doesn't export \`${name}\``);
    }
    if (e.kind !== kind) {
      throw new ModuleError(`the program exports \`${name}\` as a ${e.kind}, not a ${kind}`);
    }
    return e;
  };
  exported(MEMORY, "memory");
  const frame = exported(FRAME, "function");
  // Function indices count the imported functions first, then the module's own.
  const frameType =
    frame.index < imports.length
      ? SUBMIT_TYPE
      : typeOf(functions[frame.index - imports.length] ?? -1, `\`${FRAME}\``);
  if (!sameType(frameType, FRAME_TYPE)) {
    throw new ModuleError(
      `the program exports \`${FRAME}\` as ${formatFuncType(frameType)}; it must be ${formatFuncType(FRAME_TYPE)}`,
    );
  }
}
