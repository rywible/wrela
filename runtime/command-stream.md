# wrela runtime contract, version 0

Normative for the compiler and for every host (the browser runtime and the native host). It covers
the program ABI, the command stream and the manifest. Its executable form is the `wrela-abi` crate
in `runtime/abi`: constants, the encoder and decoder, the frame sequence check, the manifest types
and their validation, and the state hash. The browser runtime mirrors that crate in TypeScript, and
the two must agree on every case in its tests.

Version 0 is M1's first light: one program draws to the screen. Later versions add buffers,
dispatches and offscreen passes (see Commands).

## Versions

- One version number covers this whole document. It appears in every command buffer's header and
  in the manifest's `version`. Any change a version-0 host would misread bumps it.
- A host supports exactly the versions it was built for and rejects any other. There's no
  negotiation: each game ships with its own pinned runtime (D-099, D-100).

## Build output

`wrela build` writes `manifest.json` and the files it names: one WASM module (`wasm`) and one WGSL
module per pipeline (`module`). Every path in the manifest is relative to the manifest's directory,
uses `/`, has no empty, `.` or `..` component, and has only letters, digits, `_`, `.` and `-` in
its components (a browser resolves the path as a URL, where `%`, `?` and `#` mean something
else). The compiler chooses the file names.

## Program ABI

- **The module** keeps to WebAssembly 2.0 without SIMD (so without relaxed SIMD, whose results
  an engine may leave to the hardware), threads or 64-bit memory. A host rejects a module that
  uses any of them.
- **Exports.** `memory`, the module's linear memory (32-bit, not shared), and
  `frame(time: f32, width: u32, height: u32)`, which in WASM is `(f32, i32, i32) -> ()`. A `u32`
  crosses the boundary as the `i32` with the same bits. Other exports are ignored.
- **Imports.** Exactly one, `submit(ptr: u32, len: u32)` from module `"wrela"`: in WASM,
  `(i32, i32) -> ()`. A module that imports anything else is rejected.
- **The host loop.** The host instantiates the module and reads the manifest, creating every
  pipeline before the first frame. Then, once per frame, it calls `frame` with the time in
  seconds since the first frame and the screen's size in physical pixels.
- **Submitting.** During `frame`, the program calls `submit` any number of times. Each call hands
  over one complete command buffer: `len` bytes at offset `ptr` in `memory`. The host decodes the
  buffer before `submit` returns, so the program may reuse the memory at once. A command never
  spans two buffers, and the host never calls into the program from `submit`.
- **Memory can grow** between calls. A host reads the memory's current extent on every `submit`
  (in JavaScript, take `memory.buffer` afresh: a growth detaches the old one).
- **Padding is zero and NaNs are canonical.** Bytes the program submits are a deterministic
  function of its state: padding inside `GpuData` values is zeroed, as in WASM memory
  (language.md §6.10), and every `f32` NaN in a `DRAW`'s uniform bytes is the quiet NaN
  `0x7fc00000`. WASM leaves an arithmetic NaN's sign and payload to the hardware (x86's
  `0.0 / 0.0` is `0xffc00000`), so without that the state hash would differ between machines.

## Test mode

Both hosts have a test mode, used by CI and the golden comparison:

- The screen is 1920×1080. The host runs N = 60 frames. Frame `i` (from 0) gets
  `time = i / 60` computed in f64 and rounded to the nearest f32 (`Math.fround(i / 60)` in
  JavaScript, `(i as f64 / 60.0) as f32` in Rust), `width = 1920` and `height = 1080`.
- Each frame's GPU work finishes before the next frame starts. A frame that takes longer than
  400 ms, from the `frame` call until its GPU work is done, ends the run with an error
  (`test_mode::FRAME_LIMIT_MS`): four times the ~100 ms a submission may take, so a slow program
  can't hold the GPU for all 60 frames.
- After the last frame is presented and the GPU has finished it, the host captures the screen
  target to a PNG: 1920×1080, 8-bit RGBA, the target's bytes as they are (no colour conversion).
- **The CPU state hash** is the 64-bit FNV-1a hash of every byte passed to `submit`, in order,
  over all N frames: start from `0xcbf29ce484222325`; for each byte, XOR it in, then multiply by
  `0x100000001b3` modulo 2⁶⁴. It's reported as 16 lowercase hex digits. It hashes the buffers as
  submitted, headers included, so it also changes when a program splits its commands differently.
- The host reports the hash, the PNG and any error. The same program must give the same hash in
  every host (AC7), and the two hosts' images must agree channel by channel: for each of red,
  green, blue and alpha, the mean absolute difference over all pixels is at most 0.5/255 (AC1;
  `wrela-host compare` checks it).

## Command buffers

Every number is little-endian; `f32` is IEEE 754 binary32.

```
buffer   = header command*
header   = "W" "R" version:u16          (4 bytes; version 0 reads as the u32 0x00005257)
command  = opcode:u32 length:u32 payload
payload  = length bytes; length is a multiple of 4 and doesn't count the 8 bytes before it
```

A buffer ends exactly at the end of its last command. A buffer of only a header is valid and holds
no commands.

## Commands

| Opcode | Command | Payload | Bytes |
|---|---|---|---|
| 0 | (never valid) | | |
| 1 | `BEGIN_SCREEN_PASS` | `clear: [f32; 4]` (r, g, b, a) | 16 |
| 2 | `DRAW` | `pipeline: u32`, `vertex_count: u32`, `instance_count: u32`, uniform bytes | 12 + uniforms |
| 3 | `PRESENT` | none | 0 |

- **`BEGIN_SCREEN_PASS`** begins a render pass on the screen target, cleared to `clear`. Each
  component must be finite.
- **`DRAW`** draws `vertex_count` vertices for each of `instance_count` instances (first vertex 0,
  first instance 0) with render pipeline `pipeline`, a manifest `id`, in the open screen pass. The
  uniform bytes are the rest of the payload: the whole contents of the pipeline's binding at group
  0, binding 0, which must be a `uniform` binding, so exactly its manifest `size`. A pipeline with
  no binding there takes no uniform bytes. Each draw sees its own bytes: a host must not let a
  later draw's uniforms overwrite an earlier draw's in the same frame. Zero vertices or instances
  draws nothing.
- **`PRESENT`** ends the screen pass and shows the screen target. It ends the frame.

Opcode 0 is never valid, so a buffer of zeroed memory fails loudly. Later versions take opcodes
from 4 up (planned for M1 S13: end pass, create buffer, write buffer, dispatch, and a draw that
binds buffers). A host never skips a command it doesn't know.

**Frame rules.** The commands of one `frame` call, across all its buffers, must be exactly one
`BEGIN_SCREEN_PASS`, then any number of `DRAW`s, then one `PRESENT`. A draw before the pass begins,
a second pass, a `PRESENT` with no pass, any command after `PRESENT`, and a `frame` call that
returns without `PRESENT` are all errors. So is a `DRAW` that takes the frame past
1,048,576 (2²⁰) vertices: each `DRAW` counts `vertex_count × instance_count`, summed over the
frame (`MAX_FRAME_VERTICES`). That stops a garbage count before it reaches the GPU; it doesn't
bound GPU time, which test mode checks.

## The screen target and render state

- The screen target has format `rgba8unorm` and the screen's size. A shader's output is stored as
  it is: no sRGB encoding. The browser configures its canvas with `format: "rgba8unorm"` and
  `alphaMode: "opaque"`; the native host renders to an offscreen `rgba8unorm` texture.
- A render pipeline draws a triangle list with no vertex buffers, no culling, no depth or stencil
  and no blending, into one colour target of its manifest `color_target` format, writing all
  channels.
- Bind group layouts are built from each pipeline's manifest `bindings`, never inferred from the
  shader.

## Errors

A host never guesses at bad input and never lets it cause undefined behaviour. Each of the
following is an error: the host stops calling the program, abandons the frame, and reports what
went wrong with the frame number and, for a command buffer, the byte offset. The native host exits
non-zero; the browser reports a failed run.

- **Manifest:** unreadable, not JSON, an unknown or missing field, another version, or anything
  `Manifest::validate` rejects (duplicate pipeline ids or bindings, an unsafe path, a workgroup
  size beyond WebGPU's default limits, a binding visible to a stage it can't be, …).
- **Module:** a missing or mistyped export, an import other than `wrela.submit`, a feature
  outside the baseline above, or a trap.
- **`submit`:** a range outside `memory`; fewer than 4 bytes; a wrong magic or version; a
  truncated command; a payload length that isn't a multiple of 4 or runs past the end; opcode 0
  or an unknown opcode; a payload length wrong for its opcode; a non-finite clear colour; a `DRAW`
  naming a pipeline that doesn't exist or isn't a render pipeline, or carrying the wrong number of
  uniform bytes. The failing `submit` traps the program: the native host's import returns an
  error, and the browser's throws.
- **Sequence:** a break of the frame rules above.
- **GPU:** a shader that fails to compile, a WebGPU validation error, a lost device, or, in test
  mode, a frame over the 400 ms limit.

## Manifest

`manifest.json`, as `wrela build examples/first-light` writes it (the compiler names the files
`program.wasm` and `program.<id>.wgsl`; a host reads the names from here):

```json
{
  "version": 0,
  "wasm": "program.wasm",
  "pipelines": [
    {
      "kind": "render",
      "id": 0,
      "module": "program.0.wgsl",
      "vertex": "cover",
      "fragment": "shade",
      "color_target": "rgba8unorm",
      "bindings": [
        { "group": 0, "binding": 0, "kind": "uniform", "visibility": ["fragment"], "size": 16 }
      ]
    }
  ],
  "layouts": [
    {
      "name": "Scene",
      "size": 16,
      "align": 8,
      "fields": [
        { "name": "resolution", "offset": 0, "size": 8, "type": "vec2" },
        { "name": "time", "offset": 8, "size": 4, "type": "f32" }
      ]
    }
  ]
}
```

- **`pipelines`** are `render` (`vertex`, `fragment` and `color_target`; the only format in
  version 0 is `rgba8unorm`) or `compute` (`compute` and `workgroup_size: [x, y, z]`). Each has a
  unique `id`, the `module` holding its entry points, and its `bindings`.
- **A binding** has a `group` and `binding` number, a `kind` (`uniform`, `storage_read` or
  `storage_read_write`), the stages whose entry points use it (`visibility`: `vertex`, `fragment`,
  `compute`), and `size`: its minimum size in bytes. A storage binding that ends in a
  runtime-sized array also has `stride`, the array's element stride; then `size` covers what comes
  before the array.
- **`layouts`** describe each `GpuData` type: its `size` and `align` in bytes, and each field's
  `offset`, `size` and `type` as written in wrela. They are the same in WASM memory and on the GPU.
  Hosts don't need them to run a program; tools read them to inspect GPU data.
- Unknown fields are errors, so a manifest a host doesn't fully understand is never half-read.
