// The cases of wrela-abi's manifest tests (runtime/abi/src/manifest/tests.rs), run against the
// TypeScript mirror.

import { describe, expect, test } from "bun:test";
import {
  type Binding,
  CommandError,
  type ComputePipeline,
  inlineUniformSize,
  type Layout,
  Manifest,
  ManifestError,
  type ManifestErrorDetail,
  type Pipeline,
  type RenderPipeline,
} from "../src/manifest.ts";
import type { Command } from "../src/stream.ts";
import { contractManifest, thrown } from "./helpers.ts";

const example = await contractManifest();
const firstLight = () => Manifest.fromJson(example);

const compute = (): ComputePipeline => ({
  kind: "compute",
  id: 1,
  module: "kernels/sample.wgsl",
  compute: "sample",
  workgroup_size: [64, 1, 1],
  bindings: [
    { group: 0, binding: 0, kind: "uniform", visibility: ["compute"], size: 32 },
    {
      group: 0,
      binding: 1,
      kind: "storage_read_write",
      visibility: ["compute"],
      size: 0,
      stride: 16,
    },
  ],
});

/** Parses the first-light manifest's JSON after `edit`; the error's detail, or undefined. */
// biome-ignore lint/suspicious/noExplicitAny: an edit reaches anywhere in the manifest's JSON
type JsonEdit = (value: any) => void;

function edited(edit: JsonEdit): ManifestErrorDetail | undefined {
  const value = JSON.parse(example);
  edit(value);
  try {
    Manifest.fromJson(JSON.stringify(value));
    return undefined;
  } catch (e) {
    expect(e).toBeInstanceOf(ManifestError);
    return (e as ManifestError).detail;
  }
}

/** Validates the parsed first-light manifest after `edit`; the error's detail, or undefined. */
function validated(edit: (m: Manifest) => void): ManifestErrorDetail | undefined {
  const manifest = firstLight();
  edit(manifest);
  try {
    manifest.validate();
    return undefined;
  } catch (e) {
    expect(e).toBeInstanceOf(ManifestError);
    return (e as ManifestError).detail;
  }
}

function render(manifest: Manifest): RenderPipeline {
  const p = manifest.pipelines[0];
  if (p?.kind !== "render") {
    throw new Error("the first pipeline is a render pipeline");
  }
  return p;
}

function binding(manifest: Manifest): Binding {
  const b = render(manifest).bindings[0];
  if (b === undefined) {
    throw new Error("first light has a binding");
  }
  return b;
}

test("the contract's example parses to the first-light manifest", () => {
  const manifest = firstLight();
  expect(manifest.version).toBe(0);
  expect(manifest.wasm).toBe("program.wasm");
  const p = render(manifest);
  expect([p.id, p.vertex, p.fragment]).toEqual([0, "cover", "shade"]);
  expect(p.color_target).toBe("rgba8unorm");
  expect(inlineUniformSize(p)).toBe(16);
  const scene = manifest.layouts[0];
  expect([scene?.name, scene?.size, scene?.align]).toEqual(["Scene", 16, 8]);
  expect(scene?.fields[1]?.type).toBe("f32");
});

// The same cases as wrela-abi's `json_that_serde_rejects_is_rejected`.
test("duplicate keys and non-integer numbers are rejected, as serde rejects them", () => {
  const cases: [string, string][] = [
    ['"version": 0,', '"version": 0, "version": 0,'],
    ['"version": 0,', '"version": 0.0,'],
    ['"version": 0,', '"version": 0e0,'],
    ['"size": 16', '"size": 1.6e1'],
    ['"size": 16', '"size": 16, "size": 16'],
    ['"offset": 8', '"offset": 8.0'],
  ];
  for (const [from, to] of cases) {
    expect(example.includes(from)).toBe(true);
    const error = thrown(() => Manifest.fromJson(example.replace(from, to))) as ManifestError;
    expect(error).toBeInstanceOf(ManifestError);
    expect(error.detail.kind).toBe("Json");
  }
  const duplicate = thrown(() =>
    Manifest.fromJson(example.replace('"version": 0,', '"version": 1, "version": 0,')),
  ) as ManifestError;
  expect(duplicate.message).toContain("manifest: duplicate field `version`");
  const float = thrown(() =>
    Manifest.fromJson(example.replace('"size": 16', '"size": 1.6e1')),
  ) as ManifestError;
  expect(float.message).toContain(
    "pipelines[0].bindings[0].size: expected an integer, found 1.6e1",
  );
  // Escaped keys are the same key, and strings may hold anything.
  expect(
    thrown(() =>
      Manifest.fromJson(example.replace('"version": 0,', '"version": 0, "ver\\u0073ion": 0,')),
    ),
  ).toBeInstanceOf(ManifestError);
  expect(Manifest.fromJson(example.replace('"Scene"', '"S\\"c.e{n}e,1.5"')).layouts[0]?.name).toBe(
    'S"c.e{n}e,1.5',
  );
});

test("unknown fields are rejected at every level", () => {
  const cases: JsonEdit[] = [
    (v) => (v.extra = 1),
    (v) => (v.pipelines[0].blend = "add"),
    (v) => (v.pipelines[0].bindings[0].dynamic = true),
    (v) => (v.layouts[0].packed = true),
    (v) => (v.layouts[0].fields[0].array = 2),
    // A field of the other kind of pipeline is unknown too.
    (v) => (v.pipelines[0].workgroup_size = [1, 1, 1]),
  ];
  for (const edit of cases) {
    const error = edited(edit);
    expect(error?.kind).toBe("Json");
    expect(error?.kind === "Json" && error.message).toContain("unknown field");
  }
});

test("missing fields, mistyped values and unknown kinds are rejected", () => {
  const cases: JsonEdit[] = [
    (v) => delete v.wasm,
    (v) => delete v.pipelines[0].fragment,
    (v) => delete v.pipelines[0].kind,
    (v) => (v.pipelines[0].kind = "mesh"),
    (v) => (v.pipelines[0].color_target = "bgra8unorm"),
    (v) => (v.pipelines[0].bindings[0].kind = "storage"),
    (v) => (v.pipelines[0].bindings[0].visibility = ["geometry"]),
    (v) => (v.pipelines[0].id = -1),
    (v) => (v.pipelines[0].id = 1.5),
    (v) => (v.pipelines[0].id = 2 ** 32),
    (v) => (v.pipelines[0].id = "0"),
    (v) => (v.version = 65536),
    (v) => (v.pipelines = {}),
    (v) => (v.layouts[0].fields[0] = null),
    (v) => (v.pipelines[0].bindings[0].stride = "4"),
    (v) => (v.pipelines[0] = { ...compute(), workgroup_size: [1, 1] }),
    (v) => (v.pipelines[0] = { ...compute(), workgroup_size: [1, 1, 1, 1] }),
  ];
  for (const edit of cases) {
    expect(edited(edit)?.kind).toBe("Json");
  }
  for (const text of ["not json", "", "[]", "null"]) {
    expect((thrown(() => Manifest.fromJson(text)) as ManifestError).detail.kind).toBe("Json");
  }
});

test("a null stride reads as none, as serde reads it", () => {
  expect(edited((v) => (v.pipelines[0].bindings[0].stride = null))).toBeUndefined();
});

test("another version is rejected", () => {
  expect(edited((v) => (v.version = 1))).toEqual({ kind: "UnsupportedVersion", found: 1 });
});

test("paths stay inside the manifest's directory", () => {
  for (const bad of [
    "",
    ".wasm",
    "/abs/program.wasm",
    "../program.wasm",
    "out/../program.wasm",
    "./program.wasm",
    "out//program.wasm",
    "out\\program.wasm",
    "C:program.wasm",
    "https://example.com/x.wasm",
    "first-light.wgsl",
    "first-light",
    // This runtime resolves these as URLs, to other files than the native host opens.
    "%2e%2e/program.wasm",
    "a%20b.wasm",
    "a#.wasm",
    "x?.wasm",
    "a b.wasm",
    "café.wasm",
  ]) {
    const error = validated((m) => (m.wasm = bad));
    expect(error?.kind === "Path" && error.path).toBe(bad);
  }
  expect(validated((m) => (m.wasm = "out/a.b.wasm"))).toBeUndefined();
  expect(validated((m) => (render(m).module = "kernels/sample-1_v2.wgsl"))).toBeUndefined();
  expect(new ManifestError(validated((m) => (m.wasm = "a#.wasm")) as never).message).toBe(
    "the manifest names the file `a#.wasm`: its parts have only letters, digits, `_`, `.` and `-`",
  );
  expect(validated((m) => (render(m).module = "shaders/x.wasm"))?.kind).toBe("Path");
});

test("pipeline ids are unique", () => {
  expect(validated((m) => m.pipelines.push(structuredClone(m.pipelines[0] as Pipeline)))).toEqual({
    kind: "DuplicatePipeline",
    id: 0,
  });
  expect(validated((m) => m.pipelines.push(compute()))).toBeUndefined();
});

test("entry points are WGSL identifiers", () => {
  for (const bad of ["", "_", "__x", "1st", "shade-fs", "café", "a b"]) {
    expect(validated((m) => (render(m).fragment = bad))).toMatchObject({ kind: "Pipeline", id: 0 });
  }
  for (const good of ["shade", "_shade", "shade_f32", "S2"]) {
    expect(validated((m) => (render(m).vertex = good))).toBeUndefined();
  }
});

test("workgroup sizes fit WebGPU's default limits", () => {
  const withSize = (size: [number, number, number]) =>
    validated((m) => m.pipelines.push({ ...compute(), workgroup_size: size }));
  for (const good of [
    [1, 1, 1],
    [256, 1, 1],
    [16, 16, 1],
    [1, 4, 64],
    [8, 8, 4],
  ] as [number, number, number][]) {
    expect(withSize(good)).toBeUndefined();
  }
  for (const bad of [
    [0, 1, 1],
    [1, 0, 1],
    [257, 1, 1],
    [1, 1, 65],
    [16, 16, 2],
    [0xffff_ffff, 0xffff_ffff, 1],
  ] as [number, number, number][]) {
    expect(withSize(bad)).toMatchObject({ kind: "Pipeline", id: 1 });
  }
  expect(withSize([0, 1, 1])).toMatchObject({
    problem:
      "workgroup size [0, 1, 1] is outside WebGPU's default limits: each of x and y in 1..=256, z in 1..=64, and at most 256 invocations",
  });
});

describe("bindings follow WebGPU and version 0", () => {
  const rejected = (error: ManifestErrorDetail | undefined, needle: string) => {
    expect(error?.kind).toBe("Binding");
    expect(error?.kind === "Binding" && error.problem).toContain(needle);
  };
  const edit = (change: (b: Binding) => void) => validated((m) => change(binding(m)));
  const computeEdit = (change: (b: Binding) => void) =>
    validated((m) => {
      const p = compute();
      const b = p.bindings[1];
      if (b !== undefined) {
        change(b);
      }
      m.pipelines.push(p);
    });

  test("render pipelines", () => {
    rejected(
      validated((m) => render(m).bindings.push(structuredClone(binding(m)))),
      "twice",
    );
    rejected(
      edit((b) => (b.group = 4)),
      "limits",
    );
    rejected(
      edit((b) => (b.binding = 1000)),
      "limits",
    );
    rejected(
      edit((b) => (b.visibility = [])),
      "visibility",
    );
    rejected(
      edit((b) => (b.visibility = ["fragment", "fragment"])),
      "visibility",
    );
    rejected(
      edit((b) => (b.visibility = ["compute"])),
      "stage",
    );
    rejected(
      edit((b) => (b.size = 6)),
      "multiples of 4",
    );
    rejected(
      edit((b) => (b.size = 0)),
      "1 to 65536",
    );
    rejected(
      edit((b) => (b.size = 65540)),
      "1 to 65536",
    );
    rejected(
      edit((b) => (b.stride = 16)),
      "runtime-sized",
    );
    rejected(
      edit((b) => (b.binding = 1)),
      "only a uniform at group 0, binding 0",
    );
    rejected(
      edit((b) => (b.kind = "storage_read")),
      "only a uniform at group 0, binding 0",
    );
    rejected(
      edit((b) => {
        b.kind = "storage_read_write";
        b.visibility = ["vertex", "fragment"];
      }),
      "vertex",
    );
    expect(edit((b) => (b.visibility = ["vertex", "fragment"]))).toBeUndefined();
    expect(edit((b) => (b.size = 65536))).toBeUndefined();
    expect(validated((m) => (render(m).bindings = []))).toBeUndefined();
  });

  test("compute pipelines", () => {
    rejected(
      computeEdit((b) => (b.stride = 0)),
      "multiples of 4",
    );
    rejected(
      computeEdit((b) => (b.stride = 6)),
      "multiples of 4",
    );
    rejected(
      computeEdit((b) => delete b.stride),
      "empty",
    );
    rejected(
      computeEdit((b) => (b.visibility = ["fragment"])),
      "stage",
    );
    expect(
      computeEdit((b) => {
        delete b.stride;
        b.size = 1024;
      }),
    ).toBeUndefined();
    expect(computeEdit((b) => (b.size = 16))).toBeUndefined();
  });
});

test("layouts add up", () => {
  const rejected = (error: ManifestErrorDetail | undefined, needle: string) => {
    expect(error?.kind).toBe("Layout");
    expect(error?.kind === "Layout" && error.problem).toContain(needle);
  };
  const layout = (change: (l: Layout) => void) => validated((m) => change(m.layouts[0] as Layout));
  const field = (l: Layout, i: number) => l.fields[i] ?? { name: "", offset: 0, size: 0, type: "" };
  rejected(
    layout((l) => (l.name = "")),
    "name",
  );
  rejected(
    layout((l) => (l.align = 6)),
    "power of two",
  );
  rejected(
    layout((l) => (l.align = 2)),
    "power of two",
  );
  rejected(
    layout((l) => (l.align = 0)),
    "power of two",
  );
  rejected(
    layout((l) => (l.align = 0xc000_0000)),
    "power of two",
  );
  rejected(
    layout((l) => (l.size = 12)),
    "multiple of the alignment",
  );
  rejected(
    layout((l) => (l.size = 0)),
    "multiple of the alignment",
  );
  rejected(
    layout((l) => (field(l, 1).name = "resolution")),
    "twice",
  );
  rejected(
    layout((l) => (field(l, 1).type = "")),
    "type",
  );
  rejected(
    layout((l) => (field(l, 1).offset = 4)),
    "overlaps",
  );
  rejected(
    layout((l) => l.fields.reverse()),
    "overlaps",
  );
  rejected(
    layout((l) => (field(l, 1).offset = 6)),
    "multiples of 4",
  );
  rejected(
    layout((l) => (field(l, 1).size = 0)),
    "multiples of 4",
  );
  rejected(
    layout((l) => (field(l, 1).offset = 16)),
    "past the end",
  );
  rejected(
    layout((l) => (field(l, 1).offset = 0xffff_ffff - 3)),
    "past the end",
  );
  expect(layout((l) => (l.fields = []))).toBeUndefined();
  expect(
    layout((l) => {
      l.align = 0x8000_0000;
      l.size = 0x8000_0000;
    }),
  ).toBeUndefined();
  expect(validated((m) => m.layouts.push(structuredClone(m.layouts[0] as Layout)))).toEqual({
    kind: "DuplicateLayout",
    name: "Scene",
  });
});

test("draws are checked against their pipeline", () => {
  const manifest = firstLight();
  manifest.pipelines.push(compute());
  const draw = (pipeline: number, len: number): Command => ({
    kind: "Draw",
    pipeline,
    vertexCount: 3,
    instanceCount: 1,
    uniforms: new Uint8Array(len),
  });
  const check = (command: Command) => {
    try {
      manifest.checkCommand(command);
      return undefined;
    } catch (e) {
      expect(e).toBeInstanceOf(CommandError);
      return (e as CommandError).detail;
    }
  };
  expect(check(draw(0, 16))).toBeUndefined();
  expect(check(draw(0, 12))).toEqual({ kind: "UniformSize", id: 0, expected: 16, found: 12 });
  expect(check(draw(0, 0))).toEqual({ kind: "UniformSize", id: 0, expected: 16, found: 0 });
  expect(check(draw(9, 16))).toEqual({ kind: "UnknownPipeline", id: 9 });
  expect(check(draw(1, 32))).toEqual({ kind: "NotRender", id: 1 });
  // A pipeline with no inline uniform takes no bytes.
  render(manifest).bindings = [];
  expect(check(draw(0, 0))).toBeUndefined();
  expect(check(draw(0, 16))).toBeDefined();
  expect(check({ kind: "Present" })).toBeUndefined();
  expect(check({ kind: "BeginScreenPass", clear: [0, 0, 0, 0] })).toBeUndefined();
});

test("errors read as messages, in wrela-abi's words", () => {
  const messages = [
    new ManifestError({ kind: "UnsupportedVersion", found: 2 }).message,
    new ManifestError({ kind: "DuplicatePipeline", id: 3 }).message,
    new CommandError({ kind: "UniformSize", id: 0, expected: 16, found: 12 }).message,
    (thrown(() => Manifest.fromJson(example.replace('"program.wasm"', '"../x.wasm"'))) as Error)
      .message,
  ];
  for (const message of messages) {
    expect(message.endsWith(".")).toBe(false);
    expect(message[0]).toBe(message[0]?.toLowerCase());
  }
  expect(messages[2]).toBe(
    "a DRAW with pipeline 0 carries 12 bytes of uniforms; the pipeline takes 16",
  );
  expect(messages[3]).toBe(
    "the manifest names the file `../x.wasm`: it must be relative, with no empty, `.` or `..` parts",
  );
});
