// The build manifest: the TypeScript mirror of wrela-abi's `manifest` module
// (runtime/abi/src/manifest.rs). Parsing is as strict as serde's there: an unknown or missing
// field, a wrong type or an out-of-range number is a `Json` error. `validate` makes the same
// checks in the same order, with the same messages.

import { VERSION } from "./abi.ts";
import type { Command } from "./stream.ts";

/** WebGPU's default limits, which every manifest must fit. */
export const limits = {
  MAX_BIND_GROUPS: 4,
  MAX_BINDINGS_PER_GROUP: 1000,
  MAX_UNIFORM_BINDING_SIZE: 65536,
  MAX_WORKGROUP_SIZE: [256, 256, 64],
  MAX_WORKGROUP_INVOCATIONS: 256,
} as const;

export type BindingKind = "uniform" | "storage_read" | "storage_read_write";
export type Stage = "vertex" | "fragment" | "compute";
export type ColorFormat = "rgba8unorm";

export interface Binding {
  group: number;
  binding: number;
  kind: BindingKind;
  /** The stages whose entry points use the binding. */
  visibility: Stage[];
  /** The minimum binding size in bytes. With a `stride`, what comes before the runtime-sized array. */
  size: number;
  /** The element stride of a runtime-sized array at the end of a storage binding. */
  stride?: number;
}

export interface RenderPipeline {
  kind: "render";
  /** What `DRAW` names it by. Unique among the manifest's pipelines. */
  id: number;
  /** The WGSL module holding both entry points, relative to the manifest. */
  module: string;
  vertex: string;
  fragment: string;
  color_target: ColorFormat;
  bindings: Binding[];
}

export interface ComputePipeline {
  kind: "compute";
  id: number;
  module: string;
  compute: string;
  workgroup_size: [number, number, number];
  bindings: Binding[];
}

export type Pipeline = RenderPipeline | ComputePipeline;

export interface FieldLayout {
  name: string;
  offset: number;
  size: number;
  /** The field's type as written in wrela, such as `vec2` or `[f32; 4]`. */
  type: string;
}

/** A `GpuData` type's layout: the same in WASM memory and on the GPU. */
export interface Layout {
  name: string;
  size: number;
  align: number;
  fields: FieldLayout[];
}

/** Why a manifest was rejected. */
export type ManifestErrorDetail =
  /** Not JSON, or not this shape: a missing, unknown or mistyped field. */
  | { readonly kind: "Json"; readonly message: string }
  | { readonly kind: "UnsupportedVersion"; readonly found: number }
  /** A file path that isn't a safe relative path with the right extension. */
  | { readonly kind: "Path"; readonly path: string; readonly reason: string }
  | { readonly kind: "DuplicatePipeline"; readonly id: number }
  | { readonly kind: "Pipeline"; readonly id: number; readonly problem: string }
  | {
      readonly kind: "Binding";
      readonly pipeline: number;
      readonly group: number;
      readonly binding: number;
      readonly problem: string;
    }
  | { readonly kind: "DuplicateLayout"; readonly name: string }
  | { readonly kind: "Layout"; readonly name: string; readonly problem: string };

export class ManifestError extends Error {
  override readonly name = "ManifestError";
  constructor(readonly detail: ManifestErrorDetail) {
    super(manifestErrorMessage(detail));
  }
}

function manifestErrorMessage(d: ManifestErrorDetail): string {
  switch (d.kind) {
    case "Json":
      return `the manifest isn't valid: ${d.message}`;
    case "UnsupportedVersion":
      return `manifest version ${d.found} isn't supported; this host reads version ${VERSION}`;
    case "Path":
      return `the manifest names the file \`${d.path}\`: ${d.reason}`;
    case "DuplicatePipeline":
      return `two pipelines have the id ${d.id}`;
    case "Pipeline":
      return `pipeline ${d.id}: ${d.problem}`;
    case "Binding":
      return `pipeline ${d.pipeline}, group ${d.group}, binding ${d.binding}: ${d.problem}`;
    case "DuplicateLayout":
      return `two layouts are named \`${d.name}\``;
    case "Layout":
      return `layout \`${d.name}\`: ${d.problem}`;
  }
}

/** Why a decoded command doesn't fit the manifest. */
export type CommandErrorDetail =
  | { readonly kind: "UnknownPipeline"; readonly id: number }
  | { readonly kind: "NotRender"; readonly id: number }
  /** The uniform bytes don't match the size of the pipeline's inline uniform. */
  | {
      readonly kind: "UniformSize";
      readonly id: number;
      readonly expected: number;
      readonly found: number;
    };

export class CommandError extends Error {
  override readonly name = "CommandError";
  constructor(readonly detail: CommandErrorDetail) {
    super(commandErrorMessage(detail));
  }
}

function commandErrorMessage(d: CommandErrorDetail): string {
  switch (d.kind) {
    case "UnknownPipeline":
      return `a DRAW names pipeline ${d.id}, which the manifest doesn't have`;
    case "NotRender":
      return `a DRAW names pipeline ${d.id}, which is a compute pipeline`;
    case "UniformSize":
      return `a DRAW with pipeline ${d.id} carries ${d.found} bytes of uniforms; the pipeline takes ${d.expected}`;
  }
}

/**
 * The size of the uniform at group 0, binding 0, whose bytes `DRAW` carries inline; `undefined`
 * when the pipeline has no binding there.
 */
export function inlineUniformSize(pipeline: Pipeline): number | undefined {
  return pipeline.bindings.find((b) => b.group === 0 && b.binding === 0 && b.kind === "uniform")
    ?.size;
}

export class Manifest {
  /** The contract version, `VERSION`. */
  version: number;
  /** The program's WASM module, relative to the manifest. */
  wasm: string;
  pipelines: Pipeline[];
  /** Every `GpuData` type's layout, for tools; hosts don't need them to run a program. */
  layouts: Layout[];

  constructor(fields: { version: number; wasm: string; pipelines: Pipeline[]; layouts: Layout[] }) {
    this.version = fields.version;
    this.wasm = fields.wasm;
    this.pipelines = fields.pipelines;
    this.layouts = fields.layouts;
  }

  /** Parses and validates a manifest. Throws `ManifestError`. */
  static fromJson(text: string): Manifest {
    let value: unknown;
    try {
      value = JSON.parse(text);
    } catch (e) {
      throw new ManifestError({
        kind: "Json",
        message: e instanceof Error ? e.message : String(e),
      });
    }
    // `JSON.parse` keeps the last of two equal keys and reads `1.0` as `1`, where serde rejects
    // both; the scan makes this parser as strict.
    strictScan(text);
    const manifest = readManifest(value);
    manifest.validate();
    return manifest;
  }

  pipeline(id: number): Pipeline | undefined {
    return this.pipelines.find((p) => p.id === id);
  }

  /**
   * Checks everything JSON parsing doesn't: the version, safe relative paths, unique ids and
   * bindings, entry point names, WebGPU's default limits, and layouts that add up. Throws
   * `ManifestError`.
   */
  validate(): void {
    if (this.version !== VERSION) {
      throw new ManifestError({ kind: "UnsupportedVersion", found: this.version });
    }
    checkPath(this.wasm, ".wasm");
    const ids = new Set<number>();
    for (const pipeline of this.pipelines) {
      const id = pipeline.id;
      if (ids.has(id)) {
        throw new ManifestError({ kind: "DuplicatePipeline", id });
      }
      ids.add(id);
      checkPath(pipeline.module, ".wgsl");
      const problem = (p: string | undefined) => {
        if (p !== undefined) {
          throw new ManifestError({ kind: "Pipeline", id, problem: p });
        }
      };
      if (pipeline.kind === "render") {
        problem(checkEntryPoint(pipeline.vertex));
        problem(checkEntryPoint(pipeline.fragment));
      } else {
        problem(checkEntryPoint(pipeline.compute));
        problem(checkWorkgroupSize(pipeline.workgroup_size));
      }
      checkBindings(pipeline);
    }
    const names = new Set<string>();
    for (const layout of this.layouts) {
      if (names.has(layout.name)) {
        throw new ManifestError({ kind: "DuplicateLayout", name: layout.name });
      }
      names.add(layout.name);
      const problem = checkLayout(layout);
      if (problem !== undefined) {
        throw new ManifestError({ kind: "Layout", name: layout.name, problem });
      }
    }
  }

  /**
   * Checks a decoded command against this manifest: a `DRAW` must name a render pipeline and
   * carry exactly the bytes of its inline uniform. Throws `CommandError`.
   */
  checkCommand(command: Command): void {
    if (command.kind !== "Draw") {
      return;
    }
    const id = command.pipeline;
    const pipeline = this.pipeline(id);
    if (pipeline === undefined) {
      throw new CommandError({ kind: "UnknownPipeline", id });
    }
    if (pipeline.kind === "compute") {
      throw new CommandError({ kind: "NotRender", id });
    }
    const expected = inlineUniformSize(pipeline) ?? 0;
    if (command.uniforms.length !== expected) {
      throw new CommandError({ kind: "UniformSize", id, expected, found: command.uniforms.length });
    }
  }
}

// ---- Validation, as in manifest.rs ---------------------------------------------------------------

/**
 * A path the manifest names: relative, `/`-separated, inside the manifest's directory, with only
 * letters, digits, `_`, `.` and `-` in its parts. This runtime resolves it as a URL, so `%`, `?`,
 * `#` or a space would fetch a different file than the native host opens.
 */
function checkPath(path: string, extension: string): void {
  let reason: string | undefined;
  if (path.includes("\\") || path.includes(":")) {
    reason = "use `/` and no drive or scheme";
  } else if (path.split("/").some((part) => part === "" || part === "." || part === "..")) {
    reason = "it must be relative, with no empty, `.` or `..` parts";
  } else if (!/^[A-Za-z0-9_./-]*$/.test(path)) {
    reason = "its parts have only letters, digits, `_`, `.` and `-`";
  } else if (!path.endsWith(extension) || path.length === extension.length) {
    reason =
      extension === ".wasm"
        ? "a WASM module's name ends in `.wasm`"
        : "a WGSL module's name ends in `.wgsl`";
  }
  if (reason !== undefined) {
    throw new ManifestError({ kind: "Path", path, reason });
  }
}

/** An entry point name: a WGSL identifier (ASCII, not `_`, not starting with `__`). */
function checkEntryPoint(name: string): string | undefined {
  const valid = /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) && name !== "_" && !name.startsWith("__");
  return valid ? undefined : `\`${name}\` isn't a valid entry point name`;
}

function checkWorkgroupSize(size: readonly [number, number, number]): string | undefined {
  const within = size.every((n, i) => n >= 1 && n <= (limits.MAX_WORKGROUP_SIZE[i] ?? 0));
  const invocations = size[0] * size[1] * size[2];
  if (within && invocations <= limits.MAX_WORKGROUP_INVOCATIONS) {
    return undefined;
  }
  return (
    `workgroup size [${size.join(", ")}] is outside WebGPU's default limits: each of x and y in ` +
    "1..=256, z in 1..=64, and at most 256 invocations"
  );
}

function checkBindings(pipeline: Pipeline): void {
  const id = pipeline.id;
  const seen = new Set<string>();
  for (const b of pipeline.bindings) {
    const problem = (p: string) =>
      new ManifestError({
        kind: "Binding",
        pipeline: id,
        group: b.group,
        binding: b.binding,
        problem: p,
      });
    const key = `${b.group}/${b.binding}`;
    if (seen.has(key)) {
      throw problem("it appears twice");
    }
    seen.add(key);
    if (b.group >= limits.MAX_BIND_GROUPS || b.binding >= limits.MAX_BINDINGS_PER_GROUP) {
      throw problem("WebGPU's default limits allow groups 0..=3 and bindings 0..=999");
    }
    if (b.visibility.length === 0 || new Set(b.visibility).size !== b.visibility.length) {
      throw problem("its visibility must list each stage once, and not be empty");
    }
    const allowed: readonly Stage[] =
      pipeline.kind === "render" ? ["vertex", "fragment"] : ["compute"];
    if (!b.visibility.every((s) => allowed.includes(s))) {
      throw problem("it's visible to a stage this pipeline doesn't have");
    }
    if (b.kind === "storage_read_write" && b.visibility.includes("vertex")) {
      throw problem("WebGPU forbids writable storage in a vertex shader");
    }
    if (b.size % 4 !== 0 || (b.stride !== undefined && (b.stride === 0 || b.stride % 4 !== 0))) {
      throw problem("sizes and strides are positive multiples of 4");
    }
    if (b.kind === "uniform") {
      if (b.stride !== undefined) {
        throw problem("a uniform can't hold a runtime-sized array");
      }
      if (b.size === 0 || b.size > limits.MAX_UNIFORM_BINDING_SIZE) {
        throw problem("a uniform's size must be 1 to 65536 bytes");
      }
    } else if (b.stride === undefined && b.size === 0) {
      throw problem("a fixed-size binding can't be empty");
    }
    // Version 0's DRAW binds one thing: its inline uniform, at group 0, binding 0.
    if (pipeline.kind === "render" && !(b.group === 0 && b.binding === 0 && b.kind === "uniform")) {
      throw problem("a version 0 render pipeline takes only a uniform at group 0, binding 0");
    }
  }
}

/** For a u32: `&` works on the same 32 bits, whatever sign JavaScript gives them. */
function isPowerOfTwo(n: number): boolean {
  return n > 0 && (n & (n - 1)) === 0;
}

function checkLayout(layout: Layout): string | undefined {
  if (layout.name === "") {
    return "a layout needs a name";
  }
  if (!isPowerOfTwo(layout.align) || layout.align < 4) {
    return `alignment ${layout.align} isn't a power of two of at least 4`;
  }
  if (layout.size === 0 || layout.size % layout.align !== 0) {
    return `size ${layout.size} isn't a positive multiple of the alignment ${layout.align}`;
  }
  let end = 0;
  const names = new Set<string>();
  for (const field of layout.fields) {
    if (field.name === "" || field.type === "") {
      return "every field needs a name and a type";
    }
    if (names.has(field.name)) {
      return `field \`${field.name}\` appears twice`;
    }
    names.add(field.name);
    if (field.size === 0 || field.offset % 4 !== 0 || field.size % 4 !== 0) {
      return `field \`${field.name}\`: offsets and sizes are multiples of 4, and sizes positive`;
    }
    if (field.offset < end) {
      return `field \`${field.name}\` at offset ${field.offset} overlaps the field before it or is out of order`;
    }
    // Both are u32s, so the sum is exact in a double; Rust's checked_add fails exactly when it
    // passes u32::MAX, which also puts it past any layout's end.
    end = field.offset + field.size;
    if (end > layout.size) {
      return `field \`${field.name}\` runs past the end of the ${layout.size}-byte layout`;
    }
  }
  return undefined;
}

// ---- Reading JSON as strictly as serde does ------------------------------------------------------

/**
 * Rejects what `JSON.parse` accepts and serde doesn't in a manifest: an object with a key twice,
 * and a number written with a fraction or an exponent (every number in a manifest is an integer:
 * serde rejects `0.0` and `1e1` for a `u32`). The text is already known to be valid JSON.
 */
function strictScan(text: string): void {
  let i = 0;
  const space = () => {
    while (i < text.length && " \t\n\r".includes(text.charAt(i))) {
      i++;
    }
  };
  // A string token, returned as its value (keys are compared unescaped, as serde does).
  const string = (): string => {
    const start = i;
    i++;
    while (text.charAt(i) !== '"') {
      i += text.charAt(i) === "\\" ? 2 : 1;
    }
    i++;
    return JSON.parse(text.slice(start, i)) as string;
  };
  // Paths as `readManifest` writes them: `manifest` for the root, then `pipelines[0].id`.
  const value = (path: string): void => {
    space();
    const c = text.charAt(i);
    if (c === "{") {
      i++;
      const keys = new Set<string>();
      space();
      while (text.charAt(i) !== "}") {
        const key = string();
        if (keys.has(key)) {
          throw jsonError(path, `duplicate field \`${key}\``);
        }
        keys.add(key);
        space();
        i++; // the `:`
        value(path === "manifest" ? key : `${path}.${key}`);
        space();
        if (text.charAt(i) === ",") {
          i++;
          space();
        }
      }
      i++;
    } else if (c === "[") {
      i++;
      space();
      for (let index = 0; text.charAt(i) !== "]"; index++) {
        value(`${path}[${index}]`);
        space();
        if (text.charAt(i) === ",") {
          i++;
          space();
        }
      }
      i++;
    } else if (c === '"') {
      string();
    } else if (c === "-" || (c >= "0" && c <= "9")) {
      const start = i;
      while (i < text.length && "+-.0123456789eE".includes(text.charAt(i))) {
        i++;
      }
      const lexeme = text.slice(start, i);
      if (/[.eE]/.test(lexeme)) {
        throw jsonError(path, `expected an integer, found ${lexeme}`);
      }
    } else {
      // `true`, `false` or `null`.
      while (i < text.length && /[a-z]/.test(text.charAt(i))) {
        i++;
      }
    }
  };
  value("manifest");
}

function jsonError(path: string, message: string): ManifestError {
  return new ManifestError({ kind: "Json", message: `${path}: ${message}` });
}

function describeJson(value: unknown): string {
  if (value === null) {
    return "null";
  }
  if (Array.isArray(value)) {
    return "an array";
  }
  return typeof value === "object" ? "an object" : `${typeof value} ${JSON.stringify(value)}`;
}

/**
 * An object with exactly the `required` fields, plus any of the `optional` ones; any other field
 * is an error, as with serde's `deny_unknown_fields`.
 */
function readObject(
  value: unknown,
  path: string,
  required: readonly string[],
  optional: readonly string[] = [],
): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw jsonError(path, `expected an object, found ${describeJson(value)}`);
  }
  const object = value as Record<string, unknown>;
  for (const key of Object.keys(object)) {
    if (!required.includes(key) && !optional.includes(key)) {
      const expected = [...required, ...optional].map((k) => `\`${k}\``).join(", ");
      throw jsonError(path, `unknown field \`${key}\`, expected one of ${expected}`);
    }
  }
  for (const key of required) {
    if (!Object.hasOwn(object, key)) {
      throw jsonError(path, `missing field \`${key}\``);
    }
  }
  return object;
}

function readUint(value: unknown, path: string, max: number, type: string): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > max) {
    throw jsonError(path, `expected a ${type}, found ${describeJson(value)}`);
  }
  return value;
}

const readU32 = (value: unknown, path: string) => readUint(value, path, 0xffff_ffff, "u32");

function readString(value: unknown, path: string): string {
  if (typeof value !== "string") {
    throw jsonError(path, `expected a string, found ${describeJson(value)}`);
  }
  return value;
}

function readArray<T>(value: unknown, path: string, item: (v: unknown, path: string) => T): T[] {
  if (!Array.isArray(value)) {
    throw jsonError(path, `expected an array, found ${describeJson(value)}`);
  }
  return value.map((v: unknown, i) => item(v, `${path}[${i}]`));
}

function readEnum<T extends string>(value: unknown, path: string, variants: readonly T[]): T {
  if (typeof value === "string" && (variants as readonly string[]).includes(value)) {
    return value as T;
  }
  const expected = variants.map((v) => `\`${v}\``).join(", ");
  throw jsonError(path, `unknown variant ${describeJson(value)}, expected one of ${expected}`);
}

function readManifest(value: unknown): Manifest {
  const o = readObject(value, "manifest", ["version", "wasm", "pipelines", "layouts"]);
  return new Manifest({
    version: readUint(o.version, "version", 0xffff, "u16"),
    wasm: readString(o.wasm, "wasm"),
    pipelines: readArray(o.pipelines, "pipelines", readPipeline),
    layouts: readArray(o.layouts, "layouts", readLayout),
  });
}

function readPipeline(value: unknown, path: string): Pipeline {
  // An internally tagged enum: read the tag, then the variant's fields (the tag among them).
  const tag = readObject(
    value,
    path,
    ["kind"],
    ["id", "module", "vertex", "fragment", "color_target", "compute", "workgroup_size", "bindings"],
  ).kind;
  const kind = readEnum(tag, `${path}.kind`, ["render", "compute"] as const);
  if (kind === "render") {
    const o = readObject(value, path, [
      "kind",
      "id",
      "module",
      "vertex",
      "fragment",
      "color_target",
      "bindings",
    ]);
    return {
      kind,
      id: readU32(o.id, `${path}.id`),
      module: readString(o.module, `${path}.module`),
      vertex: readString(o.vertex, `${path}.vertex`),
      fragment: readString(o.fragment, `${path}.fragment`),
      color_target: readEnum(o.color_target, `${path}.color_target`, ["rgba8unorm"] as const),
      bindings: readArray(o.bindings, `${path}.bindings`, readBinding),
    };
  }
  const o = readObject(value, path, [
    "kind",
    "id",
    "module",
    "compute",
    "workgroup_size",
    "bindings",
  ]);
  const size = readArray(o.workgroup_size, `${path}.workgroup_size`, readU32);
  if (size.length !== 3) {
    throw jsonError(
      `${path}.workgroup_size`,
      `expected an array of length 3, found ${size.length}`,
    );
  }
  return {
    kind,
    id: readU32(o.id, `${path}.id`),
    module: readString(o.module, `${path}.module`),
    compute: readString(o.compute, `${path}.compute`),
    workgroup_size: size as [number, number, number],
    bindings: readArray(o.bindings, `${path}.bindings`, readBinding),
  };
}

function readBinding(value: unknown, path: string): Binding {
  const o = readObject(value, path, ["group", "binding", "kind", "visibility", "size"], ["stride"]);
  const binding: Binding = {
    group: readU32(o.group, `${path}.group`),
    binding: readU32(o.binding, `${path}.binding`),
    kind: readEnum(o.kind, `${path}.kind`, [
      "uniform",
      "storage_read",
      "storage_read_write",
    ] as const),
    visibility: readArray(o.visibility, `${path}.visibility`, (v, p) =>
      readEnum(v, p, ["vertex", "fragment", "compute"] as const),
    ),
    size: readU32(o.size, `${path}.size`),
  };
  // serde reads an absent `stride` and `"stride": null` alike, as `None`.
  const stride = o.stride;
  if (stride !== undefined && stride !== null) {
    binding.stride = readU32(stride, `${path}.stride`);
  }
  return binding;
}

function readLayout(value: unknown, path: string): Layout {
  const o = readObject(value, path, ["name", "size", "align", "fields"]);
  return {
    name: readString(o.name, `${path}.name`),
    size: readU32(o.size, `${path}.size`),
    align: readU32(o.align, `${path}.align`),
    fields: readArray(o.fields, `${path}.fields`, (v, p) => {
      const f = readObject(v, p, ["name", "offset", "size", "type"]);
      return {
        name: readString(f.name, `${p}.name`),
        offset: readU32(f.offset, `${p}.offset`),
        size: readU32(f.size, `${p}.size`),
        type: readString(f.type, `${p}.type`),
      };
    }),
  };
}
