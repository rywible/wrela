# The world-class engine: the implementation plan

*Every gap the 2026-10-24 audit of the engine found, closed in two pull requests: one in
[rywible/wrela-compiler](https://github.com/rywible/wrela-compiler) that releases toolchain 0.6.0,
and one here that pins it and rebuilds the renderer on it. Written for an implementing agent who
has not read the audit. Each step names its files, what to write, how to test it and the gate it
must pass before the next. Started 2026-10-24 at Ryan's request; reviewed by a council of two
Codex reviewers the same day (its findings are folded in and listed in §8), and revised the same
day for toolchain 0.5.2, which `main` pins since then (§9 lists what moved).*

Ryan owns the goals. A line marked **proposed** is a default the plan chose that Ryan has not
confirmed; the agent builds the default behind a switch where one is cheap, and asks before a
default that can't be switched. A section marked **gate** is a check the branch must pass before
the next step's commit. Numbers the plan names as targets are estimates until a gate measures them.

## 0. Read this first

### 0.1 What "world class" means here

"Fastest" is a claim about a measurement. The claim this plan builds toward, **proposed**:

> At 1080p on a MacBook Air M4 in Chrome, each flagship scene holds 60 fps with a 99th-percentile
> frame interval under 16.7 ms, no shader or streaming hitch, playable within 5 s of a cold page
> open, at a visual tier (dynamic sun, bounce light, local lights, moving shadows on everything,
> dense vegetation, reflections, particles) that native engines run under 30 fps on the same
> machine.

Peak throughput above a native engine is not reachable in a browser: WebGPU has no mesh shaders,
ray tracing hardware, bindless resources or 64-bit atomics in its core, and WASM has no native
code generation. The axes this engine can lead on are frame time at a visual target on integrated
GPUs, smoothness (p99, not the median), time to play, bytes, and bit-exact determinism. Everything
below serves those.

### 0.2 The two repositories and the two pull requests

| Repository | Checkout | Pull request |
|---|---|---|
| `rywible/wrela-compiler` | `~/projects/wrela-compiler` | **Part A.** One PR titled `Release 0.6.0: <summary>`, per `docs/release.md` there. Ryan merges and tags it. |
| `rywible/wrela` | `~/projects/wrela` | **Part B.** One PR, pinned to 0.6.0, holding every engine, studio, content and game change. |

The order is fixed: Part A merges and releases first, then Part B moves the pin. While Part A is a
branch, `WRELA_TOOLCHAIN=~/projects/wrela-compiler` in this checkout runs it in place of the pin
(`docs/toolchain.md`), so Part B can be built against it before the release. Unset it before
running Part B's final checks.

The toolchain knows nothing about the engine (the layering rule). Every Part A feature is phrased
as a general GPU or host capability a non-game program would also want. Nothing in Part A names a
shadow, a probe or a tree.

**Integration before the release.** Part A is tagged only after Part B's first steps (§B0–§B2)
and one consumer of each new capability build and pass against the Part A branch through
`WRELA_TOOLCHAIN` (§A18). A published release is never replaced (`docs/release.md` §5), so a
toolchain fault found after the tag costs a 0.6.1; §A18 is where such faults are meant to
surface.

**Prerequisites, not just order.** Each Part A step names what it needs. The numbering respects
the prerequisites, so following it in order works; an agent that reorders must keep them.

**Where the plan starts.** `main` pins toolchain 0.5.2 (commit `9f1429bf`). The plan was first
written against 0.4.1 and revised when 0.5.0–0.5.2 shipped (§9). Those releases brought the
agent loop's tools (`wrela same`, `wrela migrate`, the `extract-const`, `inline` and
`extract-fn` refactors, lints W0010–W0013, a next step on every error, `wrela pipelines`'
`(under a condition)` marker; rywible/wrela-compiler#28) and two of this plan's steps: the page
starts the render worker's threads, so ticked programs run in the Claude app's browser pane, and
a pointer presses a semantic tree's buttons and menu items (0.5.1 and 0.5.2; #29, #30). §A14 and
§A17 record them as done. Nothing in `std::gpu`, the command stream (`VERSION` 11), the manifest
(`VERSION` 9) or spec §18 changed between 0.4.1 and 0.5.2, so every other step stands as written.
A new feature is a minor release (`docs/release.md` §1), so Part A is 0.6.0 and a fix after its
tag is 0.6.1.

### 0.3 How to work

- **One commit per numbered step,** in the order given, each titled with the step's heading. A
  step that says "gate" ends with the gate's result in its commit message.
- **Both repositories' rules hold:** one PR for one requested scope (this plan is the scope), run
  as few tests as possible to validate a change, cite your sources. Each outside method named in
  §7 gets a row in `docs/citations.md` when its code lands.
- **Nothing lands that the gates don't cover.** The gate kinds are in §0.5. A step whose gate
  fails is fixed before the next step; it is never skipped and never "fixed later".
- **Estimates are labelled.** A cost, a speedup or a budget is `estimated` in code and prose until
  a gate measured it; then the measured number replaces it, with the device and the command.
- **Don't invent design.** The look, the games' feel and their lore are Ryan's. Where a step touches
  them, build what the plan says and mark anything beyond it **proposed**.
- **Keep the docs true:** `docs/toolchain-gaps.md` (each gap this plan closes moves to "Fixed", in
  release 0.6.0), `spec/language.md` §18 (every new GPU capability), `docs/toolchain.md` (new
  commands), the capybara's `design.md` contracts (§5), and the compiler's `docs/release.md` steps.
- **Use 0.5.0's tools.** `wrela same <pkg> --against <saved build>` is the one command that shows
  a change kept what a program does (§0.5); `wrela refactor <pkg> extract-fn|inline|extract-const
  ... --dry-run` makes the mechanical edits checked; `wrela fix --dry-run` shows the compiler's
  own fixes. The lints are on: a copy that is only read is a borrow (W0010), a test that reaches
  no `assert` can't fail and is W0011 (every test this plan adds asserts something), a `Pending`
  polled in the call that made it is W0012, and a counted `while` over a run is a `for` (W0013).
- **An existing form changes only as a migration.** Part A adds forms and removes none: every new
  parameter has a default (`levels`, `mip`, `anisotropy`, `blend`), and a fragment shader
  returning `Over` still draws over. If a step finds it must change a form a program writes today,
  it ships the change as a migration (`docs/release.md` §1: a W01xx warning whose one fix
  `wrela migrate` makes, the old form compiling for two more minor releases), never as a break.

### 0.4 Decisions reserved for Ryan

The agent builds each default; Ryan can flip it in review.

| ID | Decision | Default the plan builds (**proposed**) |
|---|---|---|
| D1 | `Quality::measured` on by default | On. Measured costs are bucketed to damp order churn between runs; a measuring frame's order is **not** guaranteed identical run to run (§B2) |
| D2 | Temporal super-resolution as the default 3D path | On: scene at 0.667 of the output, `Shared` resolver, sharpened (§B5) |
| D3 | The capybara's visible width on wide screens | A fixed 16:9 play area, pillarboxed; `Aspect::Free` kept behind a setting (§B10.5) |
| D4 | A horizontal camera dead zone in the capybara | Not built; the camera stays locked (the current feel). Noted as an option |
| D5 | Dynamic sun in the Last Green | The frame supports it (§B8.2); the Last Green's sun stays put until Ryan moves it |
| D6 | Release WASM optimized for speed or size | Whichever the V8 measurement in §A15 favours, within the size budgets |
| D7 | Keep the clearing on the engine's one wood after §B4 | Yes; §B4's culling phase removes the regression that raised the question |
| D8 | Multi-draw indirect and 64-bit atomics (Chromium-experimental) | Detected and used when present, never required (§A9) |
| D9 | The public claim's wording (§0.1) | As written, once §B1 measures it |

### 0.5 The gate kinds

Each step's gate names one or more of these. The first four compare a candidate build with the
saved baseline build of the same program (`build/base-0.6/<name>`, made at §B0), both run by the
same command on the native host:

```bash
~/.wrela/toolchains/0.6.0/bin/wrela-host <build> --frames N --size 640x360 --workers 1 --input build/gates/<SCRIPT>.json --rgba out.rgba --log out.ticks
```

The scripts and frame counts per program are in `build/gates/` (`CAPY_RUN.json`,
`CLEARING_PATH.json`, `LG_WALK.json`, `LG_SHOT.json`). They test different things; a step names
only the ones it means.

**The one-command form.** Where a step means to record exactly the same commands, 0.5.0's
`wrela same` (`spec/language.md` §23.9; `compiler/cli/src/same.rs`) is G-commands, G-pixels
(identical) and G-pipelines in one run:

```bash
wrela same <pkg> --against build/base-0.6/<name> --frames N --size 640x360 --input build/gates/<SCRIPT>.json --within 0
```

It builds the package now (into `<pkg>/build/same`, under `.gitignore`'s `**/build/`), runs both
builds from `init` on one worker with the same script, and exits 0 only if every frame's command
hash is equal, the last frames' mean channel difference is within `--within` (0 here: identical)
and the pipelines are the same; otherwise it names the first frame whose commands differ. Both
builds must load under the pinned host, so it compares two builds of one toolchain: it can't
bridge §B0's pin move (the stream version in the batch header changes), which keeps the two-host
form above for that step. A step that changes a dispatch, a pass or a draw by design fails `same`
and names the separate gates.

- **G-sim:** the simulation is unchanged. Both builds run the same tick-keyed input script and
  write their tick logs (`--log`); `wrela-host --compare-ticks a.ticks b.ticks` (added in §A12)
  checks that every tick's hash is equal and names the first that isn't. `--replay` is **not**
  used across builds: it checks the log's WASM hash first (`runtime/native/src/lib.rs` ~1191), so
  a rendering-only change would fail it before any tick is compared. Only for a ticked program
  (the capybara, the Last Green). It says nothing about rendering.
- **G-commands:** the two runs' command-stream hashes are equal (the last line `wrela-host`
  prints; it covers every byte the program submits, so equal hashes mean the same GPU work was
  recorded). Only for a step that intends to record exactly the same commands (a refactor in
  §B3; not §B0's pin, whose two builds carry different stream versions). A step that changes a
  dispatch, a pass or a draw fails it by design and must not name it. `wrela same` is its
  one-command form.
- **G-pixels (identical):** the two runs' `--rgba` bytes are equal. Rendering changed nothing the
  eye sees, whatever the commands did.
- **G-pixels (within n/255):** each pixel's channels differ by at most `n`; the step says `n` and
  why the image may change (a strip landing a frame later, a quantization). The commit lists the
  largest difference and where.
- **G-draws:** two counts for a named frame, both stated by the step. The **logical** counts
  (passes, dispatches, draws as the program recorded them) come from `std::tool::drive`'s
  `Commands`. The **physical** counts (compute passes the executor opened, draws it issued after
  any multi-draw expansion, `writeBuffer` calls, bind-group sets) come from the hosts' executor
  counters in test mode (`counters.json`, §A12), in Chrome and natively. A multi-draw is one
  logical draw and, on a device without the feature, `n` physical ones; the recorded command
  stream can't show either physical number, so a step that claims one names the counters.
- **G-suite:** `wrela suite` passes in this repository (fmt, allow, every member's tests), and
  `tools/check.sh` passes in the compiler's.
- **G-golden:** the named `@test(frames:, gpu: true, golden:, pass:)` tests pass; a step that
  changes an image deliberately re-blesses with `wrela test --bless` and says so.
- **G-pipelines:** `engine/tests/frame-pipelines` and `wrela pipelines <pkg>` report the counts
  the step states; a step that adds pipelines says how many and why.
- **G-profile:** `wrela profile <pkg> --compare build/base-0.6/<name> --pairs 3 --size 1920x1080
  --workers 4` on the M4 Air, GPU idle; the step states the change it requires (or "no slower than
  1%"), and the result is pasted in the commit message. From §A12 on, `wrela profile --chrome`
  runs too, and the Chrome numbers are the ones that count.
- **G-both:** the frame's `hosts: both` tests pass (same state hash, same frame within a GPU's
  differences in Chrome and natively).
- **G-size:** the release's runtime ≤ 1 MB, each game's time-to-play ≤ 6 MB and cold start ≤ 8 MB
  as served, playable within 5 s cold in Chrome (`wrela tool release`, `studio/release`).
- **G-bench:** `wrela tool bench` (§B1) runs and its report is pasted into `docs/bench.md`.

### 0.6 What this plan leaves out, and why

- **MSAA.** Temporal super-resolution anti-aliases the 3D frame and the 2D sprites are
  distance fields with analytic edges. MSAA would cost bandwidth on an integrated GPU for no gain
  the gates measure. Reconsider if a 2D game needs geometric edges.
- **Stencil, occlusion queries, render bundles, dual-source blending.** No step needs them: the
  depth pyramid replaces occlusion queries, compute culling replaces stencil masking, and the
  draw counts after §B4 make bundles moot.
- **Rollback netcode and accounts.** The sim is rollback-ready (`engine::timeline` has
  `correct`); wiring it is a game's work when a game needs it.
- **Competitor scenes.** This repository holds only Wrela code, so three.js, Babylon and Unity
  scenes for §B1's comparison live in a sibling repository Ryan makes; §B1 records their numbers
  in `docs/bench.md` by hand with the command that produced them.
- **Safari and Firefox automation.** `std::tool::browser` drives Chrome through its devtools
  protocol. Safari and Firefox are checked by hand (§B11) and recorded.

---

## Part A: toolchain 0.6.0 (`rywible/wrela-compiler`)

Branch `release-0.6.0` from an up-to-date `main` (at or past `v0.5.2`). Follow `docs/release.md`
§2: bump `[workspace.package] version` in `Cargo.toml` to `0.6.0`, run `cargo check --workspace`,
commit `Cargo.toml` and `Cargo.lock` first. Then copy this plan's §0.2–§0.5 and Part A into
`docs/plans/world-class-gpu.md` there, as release 0.5.0's plan lives at
`docs/plans/agent-loop.md`: the compiler's reviewers read the copy in their own repository. The
copy names this file as its source; this file is the one that's edited.

**Where things live** (read these before the first step):

| Concern | Where |
|---|---|
| The command stream: opcodes, payloads, formats | `runtime/abi/src/stream.rs` (the module doc is the spec of the stream; `VERSION` is 11), `runtime/abi/src/stream/` (`TextureFormat`, `Compare`) |
| The manifest: pipelines, targets, bindings | `runtime/abi/src/manifest.rs` (`Pipeline`, `RenderTarget`, the `blend: bool` field at line 154) |
| The browser's copy of the ABI | `runtime/browser/src/abi.gen.ts`, generated by `runtime/abi/src/typescript.rs` (`TS_PATH`); regenerate it rather than editing it |
| The browser's GPU executor | `runtime/browser/src/gpu.ts` (pipelines built at `buildPipeline`, bind groups cached at `#bindGroups`, draws at ~1500, uploads at ~1217) |
| The browser's checker and sequencer | `runtime/browser/src/check.ts` (mirrors WebGPU's usage rules), `runtime/browser/src/program.ts` (`this.checker.check(cmd)` at 608; deferred execution at ~993) |
| The device and its features | `runtime/browser/src/device.ts` (`requiredFeatures`, `requiredLimits`), `runtime/browser/src/check.ts` (`LIMIT_SOURCES`) |
| The page, the render worker, the ticker | `runtime/browser/src/main.ts` (starts the render worker at ~309; starts and stops its threads on its `start-thread` and `stop-thread` messages at ~314–335, since 0.5.1; hands the canvas over at ~384), `runtime/browser/src/worker.ts` (asks the page for each thread: the ticker's channel at ~144, `spawnWorker` at ~796; `normalWorkers()` at ~382), `runtime/browser/src/messages.ts` (the thread messages) |
| The native host's GPU | `runtime/native/src/gpu.rs` (textures at ~922 and ~1180, samplers at ~1208, pipelines at ~2055, device features at ~478) |
| std's GPU API | `compiler/std/gpu.wrela` (formats at 425–570, samplers at 625–750, textures at 690–860, passes at 869–1010, `@intrinsic` functions throughout) |
| The compiler's closed list of std items | `compiler/sema/src/defs.rs` (`Lang`: every std item the compiler lowers itself, e.g. `TextureSample = "std::gpu::texture_sample"` at 369) |
| GPU lowering | `compiler/lower/src/gpu.rs` (resources, `Lang::TextureSample => T::Sample` at 1589, the `Over` blend at 958 and 2318, draw state) |
| WGSL emission | `compiler/wgsl/src/lib.rs` (texture types at 285–335: `arrayed: false`, `multi: false`; storage formats at 309–323; the barrier at ~825) |
| WASM optimization | `compiler/driver/src/optimize.rs` (`-Oz` at ~84, `FEATURES`) |
| The profiler | `compiler/cli/src/profile.rs` (native only), `runtime/browser/src/testmode.ts` (Chrome's `timings.json`, `frames.json`, `saturate`, `inflight`), `runtime/chrome/src/lib.rs` (the headless Chrome driver) |
| What 0.5.0 added | `compiler/cli/src/same.rs` (`wrela same`), `compiler/cli/src/migrate.rs` and `compiler/diag/src/codes.rs` (`MIGRATIONS`, the W01xx codes), `compiler/cli/src/refactor.rs` (`extract-const`, `inline`, `extract-fn`), `compiler/tests/tests/suite/warnings.rs` (W0010–W0013), `docs/plans/agent-loop.md` (the 0.5.0 plan, the precedent for a plan in that repository) |
| The spec | `spec/language.md` §18 (GPU code), §16 (threads), §17 (host services), §23.9 (`wrela same`, `wrela migrate`) |
| Checks | `tools/check.sh` (`python3 tools/check.py`: rustfmt, wrela-fmt, Rust tests, browser tests in Bun, WGSL budgets) |

**The recipe for a new command** (used by several steps): add the opcode and its payload to
`stream.rs`'s table and `Opcode` (never renumber; `VERSION` becomes 12 once in §A1); decode it in
`runtime/abi/src/stream.rs`'s decoder and the sequencer; check it in `runtime/browser/src/check.ts`
and the native checker; execute it in `runtime/browser/src/gpu.ts` and `runtime/native/src/gpu.rs`;
add the std function (an `@intrinsic` in `compiler/std/gpu.wrela`) and its `Lang` entry in
`compiler/sema/src/defs.rs`; lower it in `compiler/lower/src/gpu.rs`; regenerate `abi.gen.ts`; add a
conformance test under `compiler/tests/` and a browser test under `runtime/browser/test/`; document
it in `spec/language.md` §18.

**The recipe for a new texture format:** add it to `TextureFormat::ALL` in `runtime/abi/src/stream/`
with its name, bytes, and `storable`/`filterable`/`is_uint`; regenerate `abi.gen.ts`; add the
`pub struct <Name> {}` and `impl Format`, plus `Filtered`/`Storage`/`Target` as WebGPU allows, in
`compiler/std/gpu.wrela` beside `Rgba16Float`; add it to `ir::TextureFormat` and the WGSL storage
match in `compiler/wgsl/src/lib.rs`; map it in `runtime/native/src/gpu.rs`; test that a pass draws
into it and a kernel writes it where allowed.

### A1. The stream and manifest move to version 12

*Needs: nothing.*

- `runtime/abi/src/stream.rs`: `VERSION = 12`. `runtime/abi/src/manifest.rs`: `VERSION` 9
  becomes 10. Every host rejects the other version, as now. (Both were unchanged from 0.4.1 to
  0.5.2.)
- Nothing else changes in this step.
- **Gate:** G-suite (the compiler's). The frozen programs under `compiler/tests/frozen` rebuild.

### A2. Texture copies and texture readback

*Needs: A1.* Every later texture step's tests read a texture back through these.

- Commands, by the recipe: `CopyTexture` (`source handle, source level, source layer, x, y,
  destination handle, level, layer, x, y, width, height`), `CopyTextureToBuffer` (`texture,
  level, layer, x, y, width, height, buffer, offset`; rows padded to 256 bytes, which std
  unpads), `CopyBufferToTexture` (the reverse). Until §A4 and §A5 land, `level` and `layer` are
  always 0 and the hosts check that.
- std: `copy_texture(from: tex, to: mut other, x, y, width, height, to_x, to_y)`,
  `copy_texture_to_buffer(tex, mut buf, ...)`, `copy_buffer_to_texture(buf, mut tex, ...)`, and
  `read_texture(tex) -> Pending<Vec<F::Texel>>` (nondet, like `read`: it records a copy to a
  buffer the host reads back). The usage rules of §6.5 apply: a texture bound to a pass can't be a
  copy's source in that pass.
- **Tests:** a conformance test clears a 4×4 `Rgba8` texture through a pass, copies it to a
  second texture, reads the second back and checks every texel. A browser test checks
  `copyTextureToTexture` and `copyTextureToBuffer` arguments (row padding).
- **Spec:** §18.8 ("Copies and readback").
- **Gate:** G-suite.

### A3. Texture formats and the format table

*Needs: A2 (the tests read back).*

**The format table.** One table in `runtime/abi/src/stream/` (beside `TextureFormat`) is the
single source for the compiler and both hosts. Per format: WebGPU's name; bytes per texel (or per
block, with the block size); `filterable`; `storable`; `renderable`; `blendable`; the **render
target pixel byte cost** and **component alignment** from the WebGPU specification's "Texture
Format Capabilities" table (not the storage size: `rgba8unorm` costs 8 bytes as an attachment,
`rgba16float` 8, `r32float` 4, `rg32float` 8, `rgba32float` 16, `r8unorm` 1, `rg8unorm` 2,
`rgb10a2unorm` 8, `rg11b10ufloat` 8, `r32uint` 4, `rg32uint` 8); and the feature it needs for
each capability (`rg11b10ufloat-renderable` to render into `rg11b10ufloat`; `float32-blendable` to
blend into `r32float`, `rg32float` or `rgba32float`; `float32-filterable` to filter the three).
`typescript.rs` regenerates `abi.gen.ts` from it. Compiler checks (E0602, E0603) and host checks
read the same table.

**New formats**, by the recipe: `R8` (unorm; filterable, renderable), `Rg8` (unorm; filterable,
renderable), `Rgb10a2` (unorm; filterable, renderable), `Rg32Float` (storable, renderable; not
filterable; blends only with `float32-blendable`), `Rgba32Float` (storable, renderable; likewise),
`Rg32Uint` (storable, renderable; `vec2u` texels, loaded), `Rg11b10Float` (filterable; renderable
only with `rg11b10ufloat-renderable`, §A10; never storable). Texel types: `vec4` for the unorm and
float colour formats, `vec2` for `Rg*` floats, `vec2u` for `Rg32Uint`.

- A fragment shader returning a `vec2u` draws into an `Rg32Uint` target (extend the E0603 rule
  that pairs `u32` with `R32Uint`). `WithDepth<C>` accepts `C = vec2u`.
- A format that needs a feature for a use is checked at run time by the hosts against the
  device's features (the program checks `features()` first, §A10); the compiler can't know the
  device.
- **Tests:** a conformance test per format draws a known value and reads it back (§A2); a test
  asserts the table's byte costs for the eleven formats above against the spec's numbers, written
  out in the test. A browser test checks the names match WebGPU's.
- **Spec:** §18.8's format list and the table's columns.
- **Gate:** G-suite.

### A4. Mip chains, trilinear and anisotropic sampling

*Needs: A2, A3.*

**std (`compiler/std/gpu.wrela`):**
- `texture::<F>(width, height, levels: u32 = 1)` and `writable_texture` alike. `levels` is at
  most `floor(log2(max(width, height))) + 1` (E0603 at the call with a constant, else a trap).
- `Texture<F>::levels() -> u32`.
- `Texture<F>::level(i) -> TextureLevel<F>`: one mip level as a bindable view. A kernel takes a
  `mut Texels<F>` of a level (`writable_texture` only); any GPU code `load`s a level; a pass may
  draw into a level (`begin_pass(mut tex.level(i))`); a copy (§A2) names a level. Two levels of
  one texture bound together in one dispatch, one read and one written, are **allowed**: WebGPU
  validates usage per subresource (a texture's level and layer), and Dawn's checker iterates
  subresources. The compiler's overlap check (§6.5) treats `tex.level(i)` and `tex.level(j)` as
  disjoint when `i ≠ j` are constants, and both hosts' checkers track usage per (texture, level,
  layer).
- `sample(s, uv)` picks the level from the derivatives, as WGSL's `textureSample` does; add
  `sample_grad(s, uv, ddx, ddy)` (`textureSampleGrad`, any GPU code) and `sample_bias(s, uv, bias)`
  (fragment only). `sample_level`'s `level` now means what it says.
- `sampler(filter, address, mip: Filter = Filter::Linear, anisotropy: u32 = 1)`; `anisotropy`
  2–16 needs `filter` and `mip` linear (E0603 otherwise, WebGPU's rule).
- `generate_mips(mut tex, s: Sampler)` in std, as ordinary Wrela: a kernel per level that reads
  level `i − 1` through `sample_level` and writes level `i` as `Texels`. It records `levels − 1`
  dispatches. No host code. Each dispatch binds `tex.level(i − 1)` to read and `tex.level(i)` to
  write, **never the whole chain**: a whole-chain binding includes the written level and WebGPU
  refuses the overlap. The same rule holds for any program that reads one level while writing
  another.
- `DepthTexture` keeps one level. A depth pyramid is an `R32Float` texture with levels, filled by
  the program (the engine does this in §B4 with a **minimum** reduction, not `generate_mips`).

**Stream and hosts:** `CreateTexture` gains a `levels` word; texture bindings become `handle,
level, layer` where `level` is the view's level or `ALL` (`0xFFFF_FFFF`) for the whole chain;
`CreateSampler` gains `mip filter` and `anisotropy` words. The browser creates views per (level,
layer) and caches them on the texture; the native host likewise.

**Compiler:** `Lang` entries for `texture_sample_grad`, `texture_sample_bias`, `texture_level`,
`texture_levels`; lowering emits `textureSampleGrad`, `textureSampleBias`, `textureLoad(tex, xy,
level)`. A `TextureLevel<F>` is a `Texture` resource with a level in its binding.

- **Tests:** a conformance test makes a 4-level 16×16 texture, writes a gradient to level 0, runs
  `generate_mips`, draws a full-screen quad with a trilinear sampler at a size that selects level
  2 and reads back: the centre's value is the gradient's mean within 1/255. Another binds levels 0
  and 1 in one dispatch (one read, one `Texels`) and passes; binding level 0 twice, read and
  written, fails E0506. A browser test checks `mipLevelCount` and `maxAnisotropy` on the created
  objects, and that a dispatch binding two levels of one texture passes WebGPU's validation (no
  `uncapturederror`).
- **Spec:** §18.8 (levels, views, samplers), §18.3 (two levels of a texture don't overlap).
- **Gate:** G-suite.

### A5. Texture arrays and cube maps

*Needs: A4 (levels and per-view bindings).*

- `TextureArray<F>` (`texture_array::<F>(width, height, layers, levels: u32 = 1)`), with
  `layer(i) -> TextureLayer<F>` (a bindable 2D view: a pass's target, a kernel's `Texels`, a
  `load`, a copy's source or destination), `sample(s, uv, layer: u32)`, `sample_level(s, uv,
  layer, level)`, `load(x, y, layer, level)`, `layers()`. WGSL: `texture_2d_array`, so
  `compiler/wgsl/src/lib.rs`'s `arrayed: false` becomes a property of the resource kind.
- `Cubemap<F>`: a six-layer array with `sample_cube(s, dir: vec3)` (`texture_cube`) and
  `face(i) -> TextureLayer<F>` for drawing or writing a face. `cubemap::<F>(side, levels)`.
- `DepthTextureArray` with `layers`, `layer(i)` as a depth pass's target, and
  `sample_compare(s, uv, layer, reference)` (`texture_depth_2d_array`): shadow cascades in one
  texture.
- **Tests:** conformance tests for each: an array of 3 layers written by three kernels and
  sampled by layer; a cube drawn into face by face and sampled by direction; a depth array's layer
  drawn into and compared. A browser test checks `dimension: "2d"`, `depthOrArrayLayers` and the
  `cube` view dimension.
- **Spec:** §18.8.
- **Gate:** G-suite.

### A6. Blend states

*Needs: A3 (the table's `blendable` and feature columns).*

- `std::gpu::Blend: Copy + Eq + Fieldless { Replace, Over, Premultiplied, Add, Multiply, Min,
  Max }`, each defined in the doc comment by its WebGPU `GPUBlendComponent`s:
  - `Over`: colour `src·src.a + dst·(1 − src.a)`, alpha `src.a + dst.a·(1 − src.a)` (what `Over`
    returns today);
  - `Premultiplied`: colour `src + dst·(1 − src.a)`, alpha likewise;
  - `Add`: `src + dst`; `Multiply`: `src·dst`; `Min`, `Max`: the operations.
- A draw takes `blend: Blend = Blend::Replace` beside `cull:`, `depth_bias:` and `depth:`; it is
  part of the pipeline, so a build-time constant (E0603 otherwise), and the manifest's render entry
  gets `"blend": "<name>"` in place of `blend: bool`. A fragment shader returning `std::gpu::Over`
  still draws over, as a default `blend: Blend::Over`: giving both is E0304.
- With several colour targets (§A7), `blend:` applies to all; `blends: [Blend; N]` sets each.
- **Validity comes from the table.** A blend other than `Replace` into a format the table marks
  not blendable is E0603 at build; into `R32Float`, `Rg32Float` or `Rgba32Float` it needs
  `float32-blendable`, which the hosts check at the draw against the device (the program checks
  `features().float32_blendable` first). `R32Uint` and `Rg32Uint` never blend.
- Hosts build the `GPUBlendState` from the name (`runtime/browser/src/gpu.ts` `OVER` becomes a
  table; `runtime/native/src/gpu.rs` likewise).
- **Tests:** a conformance test per blend draws two quads into a cleared `Rgba16Float` target and
  reads back the expected arithmetic (§A2). A test checks each unsupported (blend, format) pair
  from the table is refused. The frozen programs' manifests say `"over"` where they said `true`.
- **Spec:** §18.7.
- **Gate:** G-suite.

### A7. Several colour targets in one pass

*Needs: A3 (byte costs), A6.*

- Pass constructors: `begin_pass2(mut a, mut b, ...)`, `begin_pass3`, `begin_pass4`, and the
  `_with_depth` forms, giving `Pass2<F0, F1>`, `Pass3<...>`, `Pass4<...>` and `DepthPass2..4`,
  all `RenderPass`. Each target has its own `clear:`/`keep:`.
- A fragment shader drawn into a `PassN` returns `std::gpu::Targets2<C0, C1>` (`Targets3`,
  `Targets4`), whose fields are the colours in target order (each a `vec4`, `vec2`, `u32` or
  `vec2u` matching its format), or a `WithDepth<TargetsN<...>>`.
- **The attachment limit is WebGPU's algorithm, not a sum of storage bytes.** Four colour
  targets at most, and their cost by the specification's "calculating color attachment bytes per
  sample": start at 0; for each target in order, **align the running total up to the format's
  component alignment, then add the format's render target pixel byte cost** (both from the §A3
  table); the result must be at most the default `maxColorAttachmentBytesPerSample` of 32. The
  order of targets matters. The compiler computes it at the pass's construction and refuses more
  with E0602 naming the targets and the total. Worked examples the test encodes:
  `[Rgba32Float, Rgba8, Rgba8, Rgba16Float]` is 16 → 24 → 32 → 40, refused, though its storage
  bytes sum to 32; `[Rgba16Float, Rg16Float, R8, R32Uint]` is 8 → 12 → 13 → align(13, 4) + 4 =
  20, accepted. wgpu's `validation.rs` implements the same algorithm and is a second reference.
- The manifest's `targets` already is a list; each pipeline variant's `targets` holds all of
  them. `BeginPass` grows: a count of colour targets, then each's `texture, load, clear r g b a`.
- **Tests:** a conformance test draws into `Rgba16Float` + `Rgb10a2` + `R32Uint` and reads each
  back. The two worked examples above: one E0602, one accepted. A browser test confirms the
  accepted one passes WebGPU's own validation.
- **Spec:** §18.8 (passes), §18.1 (what a fragment shader returns).
- **Gate:** G-suite.

### A8. Scissor and viewport

*Needs: A1.*

- `pass.scissor(x, y, width, height)` and `pass.viewport(x, y, width, height, min_depth: f32 =
  0.0, max_depth: f32 = 1.0)` on every pass type; they apply to the draws recorded after them in
  that pass and reset at the pass's end. Commands `SetScissor` and `SetViewport` (inside a pass
  only; the sequencer rejects them outside).
- **Tests:** a conformance test draws a full-screen triangle with a scissor of the left half and
  reads back both halves (§A2).
- **Spec:** §18.7.
- **Gate:** G-suite.

### A9. Workgroup atomics and shared scatter

*Needs: A1.*

- `SharedAtomics<N>` of `u32` (and `SharedAtomicsI<N>` of `i32`): workgroup memory any
  invocation may `add`, `min`, `max`, `and`, `or`, `xor`, `exchange`, `compare_exchange`, `load`,
  `store` at any index below `N`. It needs no barrier for its own operations (they are atomic) and
  a `barrier` between a phase of atomic writes and a phase of plain `load`s, which the existing
  E0609 analysis extends to. It counts toward the 16 KiB workgroup limit.
- `Shared<T, N>` is **unchanged**: its own-chunk rule stays the only plain write. A general
  scatter is `SharedAtomics::store` or `exchange`, which can't race; a syntactic "one writer per
  index" rule was considered and dropped, because two affine write sites in one phase (`lid` and
  `lid + 1`) satisfy any such rule and still race.
- `workgroup_prefix_sum(shared: mut SharedAtomics<N>, lid, value) -> u32` in std, as Wrela: a
  Hillis–Steele scan over `Shared`, the standard building block for compaction. §A10 adds a
  subgroup form with the same results.
- **Tests:** a conformance test builds a 256-bin histogram of 65,536 values with `SharedAtomics`
  and checks the counts; another compacts a list with the prefix sum and checks order and count.
- **Spec:** §18.5.
- **Gate:** G-suite.

### A10. Optional device features, detected and used, never required

*Needs: A3 (compressed formats use the table), A9 (the subgroup scan).*

- `std::gpu::features() -> Features` (nondet): `f16`, `subgroups`, `subgroup_min: u32`,
  `subgroup_max: u32`, `bc`, `etc2`, `astc`, `float32_filterable`, `float32_blendable`,
  `rg11b10_renderable`, `bgra8_storage`, `timestamps`, `multi_draw_indirect`, `atomic64_min_max`,
  `indirect_first_instance` (always true: still required).
- `runtime/browser/src/device.ts` and `runtime/native/src/gpu.rs` request each of these the
  adapter has: `shader-f16`, `subgroups`, `texture-compression-bc`, `-etc2`, `-astc`,
  `float32-filterable`, `float32-blendable`, `rg11b10ufloat-renderable`, `bgra8unorm-storage`,
  `timestamp-query`, `chromium-experimental-multi-draw-indirect`, and the 64-bit min/max atomics
  extension by whatever name Dawn exposes at the time (check `adapter.features` in Chrome stable
  first; if neither Chrome stable nor wgpu has it, skip it and record that in
  `docs/toolchain-gaps.md`). A program checks `features()` before it records anything that needs
  one.
- **Test mode masks features:** `#test&features=none` (or a comma list of the ones to keep) makes
  `features()` report the rest absent and the device request only `indirect-first-instance`, so a
  program's fallback paths are measured on a device that has the features. `wrela profile
  --chrome --features none` passes it through (§A12).
- **Feature-gated pipelines.** An entry point marked `@gpu(needs: f16)` (and `subgroups`,
  `atomic64`) compiles with `enable f16;` (etc.) and its manifest entry lists `"requires":
  ["shader-f16"]`. The hosts create such a pipeline only when the device has the feature, and a
  command that records it without the feature is a host error naming the feature. Inside such an
  entry point: the `f16` scalar and `vec2h`/`vec3h`/`vec4h` types with conversions `f16(x)`,
  `f32(h)`; `std::gpu::subgroup::{size, invocation_id, ballot, any, all, add, min, max,
  exclusive_add, inclusive_add, broadcast, broadcast_first, shuffle}`; `Atomics<u64>` with `min`
  and `max` only (and `Slots<u64>`). CPU code never sees `f16` or `u64` atomics.
  `workgroup_prefix_sum` gets a `@gpu(needs: subgroups)` twin, `subgroup_prefix_sum`, with the
  same results.
- **Compressed formats** `Bc1`, `Bc4`, `Bc5`, `Bc7`, `Etc2Rgb8`, `Astc4x4`: `Filtered` only (no
  `Target`, no `Storage`); `texture::<Bc7>(w, h, levels)` traps on a device without the feature,
  so a program checks `features()` first; `Texture<Bc7>::write_blocks(level, x, y, w, h, bytes)`
  uploads block rows (`WriteTexture` learns block formats from the table's block size). A kernel
  encodes blocks into a `GpuBuffer<u32>` and `copy_buffer_to_texture` (§A2) moves them: encoding
  is std or engine code.
- **Tests:** a conformance test runs an `f16` kernel when `features().f16` and checks a sum
  within f16's precision; a subgroup test checks `subgroup_prefix_sum` against
  `workgroup_prefix_sum`; a compressed-texture test uploads a known BC1 block and samples it. Each
  test passes trivially (and says so in its output) when the feature is absent. A browser test
  checks `features=none` masks them.
- **Spec:** §18.10 ("Features"), §18.1 (`@gpu(needs:)`), §18.8 (compressed formats).
- **Gate:** G-suite on the M4 Air (f16, subgroups and BC present) and on the Linux CI host.

### A11. Direct indexed draws and multi-draw

*Needs: A10 (the feature flag).*

- `draw(pass, vs, fs, indices: buf, index_count: n, instances: m = 1, first_index: 0,
  base_vertex: 0)` without `indirect:` records a `DrawIndexed` command (new opcode): no argument
  buffer for a static mesh.
- `draw_multi(pass, vs, fs, indirect: buf, count: n)` and the indexed form, with an optional
  `count_buffer: GpuSpan<u32>`: `MultiDrawIndirect` and `MultiDrawIndexedIndirect` commands. On a
  device without `features().multi_draw_indirect` the **host** records `n` indirect draws in a
  loop, so the program's code is the same either way (the command's semantics don't depend on the
  feature; only the host's cost does). The checker treats one multi-draw as `n` draws.
- **Tests:** a conformance test draws an indexed cube directly; another draws three indirect
  draws through `draw_multi` and reads back the same image as three `draw`s, with and without
  `features=none`.
- **Spec:** §18.7.
- **Gate:** G-suite.

### A12. `wrela profile --chrome` and CPU phases in the browser

*Needs: A1.* Built here so §A13 and §A15 can measure in Chrome.

- `compiler/cli/src/profile.rs`: `--chrome` runs the build in headless Chrome through
  `runtime/chrome` in test mode with `timestamps=1&saturate=1&nohash=1` (GPU spans) and a second
  run with `paced=1&inflight=2` (frame intervals as a player sees them), the same warmup and
  frame counts, and reports the same JSON shape as the native run with `"host": "chrome"`.
  `--compare` and `--baseline` work across two Chrome reports. `--features none` passes §A10's
  mask. Full-resolution timestamps need the developer flag the driver already sets
  (`runtime/chrome/src/lib.rs` ~216).
- `std::time::phase` works in the browser: `performance.now()` in the render worker, reported in
  `frames.json` per phase per frame.
- **Executor counters.** Both hosts count, per frame, what they actually did below the recorded
  stream: compute passes opened, render passes, draws issued (a multi-draw expanded by the host
  counts each), `writeBuffer` calls and bytes, bind-group sets, pipeline sets. Test mode writes
  them to `counters.json`; `wrela profile` (native and `--chrome`) reports the frame's medians
  beside the GPU times. These are the physical half of G-draws.
- **`wrela-host --compare-ticks a.ticks b.ticks`:** two tick logs from two builds run on the same
  input; prints the first tick whose hash differs, or "equal" (G-sim). It does not check the logs'
  WASM hashes, which `--replay` keeps checking. 0.5.0's `wrela same` compares each frame's
  command hash, not the ticks', so G-sim still needs this.
- `docs/toolchain.md`'s "Watching and measuring" gains the commands.
- **Tests:** a tool test runs `wrela profile --chrome` on `examples/hello-field` for 30 frames and
  checks the report's shape.
- **Gate:** G-suite.

### A13. The command path: fewer copies, less JavaScript

*Needs: A12 (the measurement).* Each item is measured by `wrela profile --chrome` on the frozen
`herd` and `grazer` programs (`compiler/tests/frozen`) and, through `WRELA_TOOLCHAIN`, on
`examples/clearing` and `games/last-green`: CPU frame time before and after. Keep what helps,
revert what doesn't, and record both in the commit.

**What must not change.** Commands are kept until the export that recorded them returns
(`runtime/browser/src/program.ts` ~618 pushes each to `#pending`; ~996 executes them after the
export succeeds, so a trapping export submits nothing), and a batch's bytes are copied at submit
because the program may overwrite its command buffer before the export returns (~755). The tests
`runtime/browser/test/program.test.ts` ~456 and ~509 guard both; they stay green through every
item below.

1. **One copy fewer per upload.** `runtime/browser/src/gpu.ts` ~1217 copies each upload's bytes
   into the upload ring's staging array (`up.bytes.set`) and later `writeBuffer`s the whole ring.
   Instead, at execution, `queue.writeBuffer(up.buffer, at, batch, offset, length)` straight from
   the retained batch slice (an owned `Uint8Array`), and the ring keeps no staging array. The
   retained slice is the one copy the runtime makes; the browser's own copy into the GPU buffer
   is the second. The same for `WriteTexture` (its rows padded to 256 bytes need a padded source,
   so a texture upload may keep a staging copy). "Two copies" describes the runtime's path, not a
   count of the browser's internal transfers. **The risk:** today one `writeBuffer` carries the
   whole ring (`gpu.ts` ~489); this item makes one call per upload. Measure the call count
   (`counters.json`, §A12) and the CPU time on a small-upload workload (the capybara: many small
   writes) as well as the Last Green; if many calls cost more than one copy saved, keep the
   staging array for uploads under a threshold (say 4 KiB) and go direct above it, or revert.
2. **Decode into a table, not objects.** `program.ts` ~601 decodes each command into a fresh
   object. Instead, decode into a reusable struct-of-arrays table (opcode, offset into the
   retained batch, length) appended per command, which the sequencer, checker and executor read
   through accessors; the batch slice stays the owner of the bytes. Pending commands stay distinct
   entries; nothing is reused until the frame's execution completes.
3. **The checker in debug, test and lifted builds only.** `runtime/browser/src/check.ts` mirrors
   WebGPU's own validation. In a release build's normal run the checker is off and an
   `uncapturederror` is the fatal error (it already is). The checker stays on under `#test`, in
   debug builds and in hot reload, where its messages name the program's mistake. `program.ts`
   ~608 branches on a `checking` option; test mode sets it.
4. **Redundant state.** `gpu.ts` ~1509: skip `setPipeline` when the pipeline is the last set in
   this pass, and `setBindGroup` when the bind group object is the last set.
5. **One compute pass per run of dispatches.** `gpu.ts` ~1391: open a `GPUComputePassEncoder` at
   the first dispatch after a pass or a write, close it at the next non-dispatch command.
6. **NaN canonicalization only for types with floats.** `compiler/wasm/src/func/ops.rs` ~1284: a
   `GpuData` type with no `f32` field skips the pass.
- **Tests:** the browser tests under `runtime/browser/test/` that check uploads, bind-group
  caching, command-memory reuse and trapping exports still pass; a new test submits several
  different `WriteBuffer`s in one call, overwrites the program's command buffer before returning,
  and checks each destination holds its own bytes; a test asserts that a release run with a usage
  error ends in a `GpuError` (from `uncapturederror`), and a test-mode run ends in the checker's
  message.
- **Gate:** G-suite; G-profile (Chrome): the Last Green's CPU frame time does not rise; the
  commit records the change for each item.

### A14. The frame loop: the ticker and helpers start from the page (done in 0.5.1)

*Needs: nothing. Nothing to build.* Shipped in toolchain 0.5.1 (rywible/wrela-compiler#29), kept
in 0.5.2. The Claude app's browser pane (Chrome 152) starts no nested worker, module or classic,
so the render worker now asks the page for each thread and speaks to it over a `MessageChannel`
(`runtime/browser/src/main.ts` ~314–335, `worker.ts` ~796, `messages.ts`); failures and stops go
through the page. `examples/climber-lab` and `games/capybara` run in the pane, and
`docs/toolchain-gaps.md` records the fix with the confirmed cause. The step keeps its number so
the `Needs:` lines stay true.

- **What this plan still owes it:** §A13 changes the command path inside the render worker, not
  the thread start; §A18.4 re-checks the pane on the branch, and §B11 records it.

### A15. Release WASM: speed or size, measured (D6)

*Needs: A12.*

- `compiler/driver/src/optimize.rs`: `--optimize` takes `size` (today's `-Oz`) or `speed` (`-O3`
  with the same `FEATURES`). The default for `wrela build --release` is chosen **in this step** by
  measurement on fixtures Part A has: the frozen `herd` and `grazer` programs' CPU frame time in
  Chrome (`wrela profile --chrome`, 7 alternating pairs) and their gzipped sizes. Through
  `WRELA_TOOLCHAIN`, the same on `games/last-green`. Speed wins if it is at least 3 % faster on the
  Last Green's CPU frame time and keeps every G-size budget; otherwise size stays. Record both
  builds' numbers in the commit. §B1's bench confirms the choice later; if it disagrees, 0.6.1
  flips the default.
- Also measure the CPU inline limit 125 vs 250 (`compiler/lower`), as the 2026-10-21 research
  suggested, on the same numbers.
- **Gate:** G-suite; the measurement in the commit.

### A16. GPU memory accounting

*Needs: A4, A5 (levels and layers count).*

- `std::gpu::memory() -> Memory { buffers: u64, textures: u64, peak_buffers, peak_textures }`:
  std counts every `buffer`, `texture`, `texture3d`, `texture_array` it creates and every drop
  (block formats by their block size; mip chains by their levels). Deterministic: it's the
  program's own arithmetic, so `@deterministic` code may read it.
- Test mode's `memory.json` stays the host's view; a test checks the two agree for a program that
  makes and drops known resources.
- **Gate:** G-suite.

### A17. A pointer presses a semantic tree's buttons (done in 0.5.1 and 0.5.2)

*Needs: nothing. Nothing to build.* Shipped in 0.5.1 and corrected in 0.5.2
(rywible/wrela-compiler#29, #30). Every role a press activates (`Button`, `Checkbox`, `Switch`,
`MenuItem`, a `Link` with an address) takes the pointer where it's drawn
(`runtime/browser/src/semantics.ts` `PRESSED_ROLES` ~194, `wrela-hit` ~390), a mouse press
doesn't take the keyboard's focus, and a click hands it back to the canvas (~338). An `Option` or
a `Tab` stays the program's: 0.5.1 gave those to the pointer too, the docsite reader's search
results stopped opening on a click, this repository's suite caught it, and 0.5.2 took it back.
`games/capybara`'s menu items are `MenuItem`s since `main` 9f1429bf. The gaps entry is "Fixed".

- **What this plan still owes it:** nothing in Part A. §B10's screens keep their `MenuItem`s; a
  new role that needs the pointer (none is planned) is a gaps entry, not a change here.

### A18. Integration against the engine, before the tag

*Needs: A1–A17.* This is the gate that keeps a toolchain fault from becoming a 0.6.1.

With `WRELA_TOOLCHAIN=~/projects/wrela-compiler` (this branch) in `~/projects/wrela`:

1. On `main`'s code, unchanged: `wrela suite` passes, and for every program in §B0's list a build
   with this branch against a build with 0.5.2, each run by its own host, holds **G-pixels
   (identical)** and **G-sim**. G-commands can't hold across toolchains (the batch header carries
   the stream version, 11 against 12, and payload layouts changed), so it is not asked here; if a
   normalized comparison of decoded commands is wanted, `wrela-host --dump-commands` prints each
   frame's decoded commands and `diff` of the two dumps must show only the fields the stream change
   renamed or added. The new toolchain changes nothing a shipped program shows or simulates.
   `wrela migrate --dry-run` on every member reports nothing, or exactly the migrations Part A
   declared (§0.3).
2. On the `engine/world-class` branch, §B0–§B2 as drafted build and pass their gates.
3. A throwaway test package, `engine/tests/toolchain-0.6` (kept in Part B as the capabilities'
   living test), uses every new capability once: a 3-target pass with blends, a mip chain
   generated and sampled trilinear, a cube face drawn and sampled, an array layer written by a
   kernel, a scissored draw, a `SharedAtomics` histogram, a direct indexed draw, a `draw_multi`,
   an `f16` kernel and a `subgroup_prefix_sum` behind `features()`, a BC1 upload, a texture
   readback, and `memory()`. It runs `hosts: both` (G-both) and with `features=none`.
4. `wrela run games/capybara` still starts in the Claude desktop app's browser pane (0.5.1's fix,
   §A14, through §A13's changes inside the render worker), and a mouse press on its `Start`
   starts the game (§A17).

Faults found here are fixed in Part A before the tag. Record what ran in the PR.

- **Gate:** all four, recorded.

### A19. Spec, docs and the release

*Needs: A18.*

- `spec/language.md` §18 covers every addition above with the same voice as the rest. `wrela doc
  std::gpu::Blend` etc. answer. `wrela primer gpu` mentions features, levels and blends.
- `docs/toolchain-gaps.md` in this repository: the entries this part fixes move to "Fixed", each
  naming 0.6.0 and the PR.
- `docs/citations.md` (compiler): rows for any new crate (none expected) and for the WebGPU and
  WGSL specification sections used.
- `tools/check.sh` passes in full on the M4 Air (every Rust test, the browser tests, the WGSL
  budgets) and on a Linux machine with a GPU if one is available; record which ran, with counts.
- `docs/release.md` §2 step 5 (since 0.5.0): run the agent syntax test once with the release's
  compiler and put its headline numbers (first try correct, mean attempts to correct, mean tokens)
  in the PR:

  ```bash
  cargo build --release -p wrela && tools/agent-test/v2/run.py release-0.6.0 --wrela "$PWD/target/release/wrela"
  ```

  It runs an agent per task, 30 tasks, and costs money (0.5.0's run: about $0.71 a task with
  Sonnet 5.5), so it runs once, after §A18 passes; `--dry-run` shows what it would run.
- Open the PR per `docs/release.md` §2; Ryan merges; tag `v0.6.0` per §3; check the release holds
  both tarballs and `SHA256SUMS`.
- **Gate:** the release exists and `wrela toolchain` downloads it in this repository.

---

## Part B: the engine, the studio and the games (`rywible/wrela`)

Branch `engine/world-class` from `main`. Every step below is one commit. Part B's early steps
change no image (G-pixels identical); the first deliberate image change is §B5.

### B0. Pin 0.6.0 and save the baselines

- `wrela.toml`: `toolchain = "0.6.0"`. `wrela toolchain` says so, and downloads it.
- `wrela migrate <member>` on every member (`docs/release.md` §6; `--dry-run` first). Part A
  declares no migration (§0.3), so the expected result is no change; commit whatever it does
  change with the pin, and say so.
- `docs/toolchain.md`: the host path in its examples moves to `0.6.0`, as the 0.5.0 pin's did.
- Build the baselines once, on `main`'s code with 0.6.0, into `build/base-0.6/<name>` for
  `clearing`, `last-green`, `capybara`, `herd`, `climber-lab`, `leapling`, `clusters`
  (`wrela build <pkg> -o build/base-0.6/<name>`). These are the G-pixels, G-profile and
  `wrela same` baselines for all of Part B. `build/` is under `.gitignore`, so nothing here is
  committed.
- **Gate:** G-suite; G-pixels identical and G-sim for every program against the same code built
  with 0.5.2, each build run by its own release's host (the two-host form of §0.5: `wrela same`
  can't load a 0.5.2 build under 0.6.0's host). The pin alone changes nothing the eye sees or the
  sim does. G-commands isn't asked: the batch header carries the stream version, 11 against 12.
  If a pixel moves, the commit says which and why.

### B1. The benchmark harness (`studio/bench`)

A tool, `wrela tool bench`, declared in `wrela.toml` with `browser = true`, `read = ["ws=."]` and
`write = ["out=build/bench"]`, that measures what §0.1 claims.

- **Scenes** (a constant list in `studio/bench/scenes.wrela`): the clearing's path
  (`build/gates/CLEARING_PATH.json`, 600 frames), the Last Green's opening and walk (`LG_SHOT`,
  `LG_WALK`), the capybara's run (`CAPY_RUN`), the herd, the leapling. Each names its package,
  script, frames and size (1920×1080; the capybara also at 2940×1912).
- **Runs**, each through `std::tool::serve` and `std::tool::browser` in test mode, as
  `studio/round` and `studio/lookbook` do (`toolkit::page::run_page`):
  1. GPU: `timestamps=1&saturate=1&nohash=1&workers=N`, 60 warmup + 600 frames: per label p50/p95,
     frame span p50/p95/p99 from `timings.json`.
  2. Play: `paced=1&inflight=2`, the same frames: frame intervals p50/p95/p99 and the count over
     16.7 ms and over 33 ms from `frames.json`; CPU frame time p50/p95.
  3. Cold start: `salt=<n>` (no pipeline cache), `load.json` and `pipelines.json`: time to first
     frame and to the program's "playable" print, bytes loaded.
  4. Memory: `memory.json` at the end and peak.
  Natively too (`std::tool::drive` with `gpu()`), so both hosts' numbers sit side by side.
- **Report:** `build/bench/<machine>-<date>.json` and a Markdown table printed and written to
  `docs/bench.md` under a dated heading (the only committed artefact: the allow list permits
  `docs/bench.md`, added in this step). `--compare <report.json>` prints the deltas. The machine
  is named from `navigator.userAgent` and the adapter's `info`.
- **Competitors:** a section of `docs/bench.md`, "Other engines on this machine", filled by hand
  from the sibling repository (§0.6) with the exact command and date. The plan's claim is checked
  against it in §B13.
- **Tests:** a tool test (`@test(tool, ...)`) runs the bench on `examples/hello-field` for 30
  frames with fake sites and checks the report has every field.
- **Gate:** G-suite; G-bench: the first `docs/bench.md` entry, on `main`'s code, is the baseline
  every later step compares to.

### B2. The planner measures by default (D1)

- `engine/cook.wrela`: `Planner::measure(w, us)` rounds a measured cost **up to the next step of
  a geometric ladder** (steps a factor 1.25 apart from 25 µs: 25, 31, 39, …, 8000) and records
  `Evidence::Measured` with the step. A geometric step damps jitter at every scale without the
  throughput loss a fixed 250 µs bucket would cost the small kinds (the realizer's 120 µs estimate
  would have doubled). Bucketing makes two runs on one device plan the same frames **most of the
  time**; it is a mitigation, not a guarantee (a measurement can cross a step, or arrive a frame
  later), and no test asserts run-to-run identity of a measuring frame. The step measures the
  trade: with and without bucketing on the Last Green's opening, the frame time (G-profile) and
  the **completion latency** of each cook kind (frames from eligible to done, from the frame
  test's records), both recorded in the commit.
- **Tests are deterministic by injection.** `engine/tests/frame` keeps its `measured: false`
  fixture for every assertion about a frame's order. It adds a fixture that calls
  `Planner::measure` itself with fixed numbers (a cost just under a bucket's edge, one just over,
  one ten times the estimate) and asserts the plans that follow: the budget holds, every playable
  frame still runs at least one optional step, the mandatory cooks are never refused, and a
  bucket's edge changes the plan exactly where the arithmetic says.
- `engine/frame.wrela`: `Quality::measured = true`. The live path's test asserts properties, not
  order, and accounts for mandatory work apart from optional: the planner charges every ground
  level and every noted cook whatever the budget (`Planner::begin`, `note`, cook.wrela ~149), so
  the bound is on the **optional** steps: over 300 measuring frames, in every frame the optional
  steps' estimated costs sum to at most `budget − mandatory`, except that the first optional step
  of a frame always runs; the mandatory cooks always complete; and every kind that was eligible
  ran at least once. `Planner` exposes `spent_mandatory()` and `spent_optional()` for the test.
- **Gate:** G-suite; G-sim on the Last Green (the sim is untouched); G-pixels within 2/255 on
  the clearing and the Last Green (a strip may land on another frame); G-both for the
  non-measuring fixture.

### B3. Housekeeping: the cheap wins

Each item is its own commit; all are G-pixels identical unless said.

1. **The cloud dome's scatter covers only the tiles marched.** `engine/sky.wrela` ~839: dispatch
   `scatter` over the phase's tiles (the same list `march_dome` used), not the whole 4096×1280
   dome. G-profile: the clearing's `clouds` label falls; record it.
2. **No moving-shadow pass when nothing moves.** `engine/frame.wrela` ~1112: skip the 1024² pass
   when the scene's moving casters are empty this frame; the `Lighting` group reads a cleared map
   kept from the last empty frame.
3. **One full-screen copy a frame.** `engine/frame.wrela` ~1644 (lines) and `engine/water.wrela`
   ~6 (the water's copy) read one copy, taken once after the opaque passes into `scene_b`; the
   `Effects` group gets `view` from it.
4. **The wood chooses over what's loaded.** `engine/wood.wrela` ~855: `choose` and
   `shift_instances` dispatch over `loaded()` slots' instances, kept compact by a slot map, not
   over `SLOTS × SLOT`. `select_shadow` likewise.
5. **Terrain levels are frustum-tested.** `engine/frame.wrela` ~1543: each clipmap level's ring is
   four quads; a ring whose box is outside the camera's frustum (and the cascade's square, for
   shadows) is not drawn. CPU-side, 12 boxes.
6. **Stones cull on the GPU.** `engine/stone.wrela` ~861: `Rocks::cull`'s loop becomes a kernel
   over the placements writing an `Append` list per (kind, level) and `DrawIndexedArgs`, as the
   wood's `choose` does; no per-frame upload. Folded into §B4's phase when it lands.
7. **Limbs have a level of detail.** `engine/plant.wrela` ~669: card level 2 draws limbs of a
   radius above a threshold only, and impostors draw none (their bake includes them already).
   G-pixels within 2/255 on the clearing; record the far-tree pixels that change.
8. **Creature levels cross-fade.** `engine/realize.wrela` ~1549 (`place`): a finished level is
   drawn beside the old one for 16 frames with the plant's Bayer dither (`engine/lod.wrela` ~168)
   and the old one dropped after. G-golden: re-bless nothing (the clearing's fawn is at one level).
9. **No per-frame allocation in the realizer's pick.** `engine/realize.wrela` ~1256–1395:
   `Realizer::frame` keeps its candidate list and sorts in place; `pick` is one pass.
10. **The probes' replacement allocates nothing new.** `engine/probes.wrela` ~870–907: `Baked::new`
    and `cook` reuse the voxel buffer when the grid's size is unchanged.
- **Gate (whole step):** G-suite; G-pixels as stated; G-profile: the clearing's GPU span is no
  higher than §B1's baseline and the Last Green's CPU frame time is lower; record both.

### B4. One culling phase for everything (`engine::cull`)

The frame gains one compute phase at its start that decides what every pass draws, in two
sub-steps that are measured apart: B4a consolidates culling (frustum only) and bindings; B4b adds
two-phase occlusion against a depth pyramid, both phases together behind one switch, because
phase 1's previous-frame rejection is only correct when phase 2 repairs it the same frame. This is
the GPU-driven pipeline of Haar and Aaltonen (2015) and the cluster-culling half of Karis (2021),
without their mesh shaders.

**B4a. One phase, frustum culling, shared bindings.**

- **Instances.** Every drawable kind registers its instances in one `GpuBuffer<Bound>` (`centre`,
  `radius`, `kind`, `payload`) per frame section: wood instances (from `Wood`'s slots), stones,
  grass cells (one per 8 m level cell), terrain level rings (from §B3.5, now on the GPU), water
  tiles (the mesh split into 64 m tiles at build time, `engine/water.wrela`), creatures and
  characters (one bound each), built things. The scene fills the lists in a new hook,
  `Scene::bounds(mut lists)`.
- **The phase.** `cull` runs in **one compute pass before the first render pass**: a frustum test
  per instance (a sphere against the camera's six planes; for a shadow list, against the
  cascade's square and depth range). It writes per-kind `Append` lists and `DrawIndexedArgs`,
  compacted by `workgroup_prefix_sum` (§A9) or `subgroup_prefix_sum` when present. No occlusion in
  this sub-step, so nothing it draws depends on a previous frame, and nothing culls between
  render passes; that alone removes the clearing's 12.6 % regression (D7).
- **Shadows.** Each cascade and the moving map get their own frustum (the cascade's square and
  depth range) and their own lists from the same bounds buffer, in the same phase, so strips draw
  only the casters in them (`engine/shadow.wrela` strips choose by square today; now by list).
- **Shared bindings, so draws can merge.** A draw collapses across kinds only if every binding
  is the same. The leaf atlas and the bark ridges are already shared (`self.atlas`,
  `self.ridges`); what the wood binds **per kind** is its geometry and its lists
  (`engine/wood.wrela` ~995–1060): each kind's `cards_at(level)`, `orders`, `limbs` span,
  `plant.leaves` and `plant.bark` parameters, and each list's `instances_of(l)` and `marks_of(l)`
  spans, plus each kind's impostor `bake` textures and `bounds`. So, in this order:
  1. **Pooled geometry.** All kinds' cards go in one `GpuBuffer<Card>`, all orders in one, all
     limbs in one, with each kind's offsets and counts in one `GpuBuffer<KindDesc>` (the leaf and
     bark parameters too), indexed by the instance's `kind` in the vertex shader. `cards_at`,
     `orders` and `limbs` become offsets into the pools.
  2. **One instance buffer, ranges by draw argument.** The chosen instances of every list stay in
     one buffer (they are: `self.seen.instances`); a draw's range comes from its
     `DrawIndexedArgs.first_instance`, which the `gather` kernel writes per (kind, way), so no
     per-list span is bound.
  3. **Impostor arrays, moved here from §B6.** Each kind's impostor colour and normal atlases
     become one layer each of two `TextureArray`s (`Rgba8`, `Rg16Float`; §A5), and the bounds one
     buffer; the shader reads the layer from `KindDesc`. §B6 keeps only the compression.
  Then, **with `features().multi_draw_indirect`**, one `draw_multi` per **way** (card level 0–2,
  impostor, volume) with one argument struct per kind draws every kind: 5 logical draws for the
  wood. **Without it**, the host issues one physical draw per (kind, way) with a nonzero count,
  and the program records 5; the fallback's physical count is measured apart and claimed as such.
  Terrain's levels merge into one indirect draw; stones into one per level.
- **The far crowns' 8×8 map** (`engine/wood.wrela` ~202–371) is removed in B4b; in B4a it stays.
- **Tests:** `engine/tests/cull`: a scene of 10,000 spheres, a third outside the frustum, draws
  the instances inside it and no other (counts read back through `read`); the shadow lists hold
  only casters in the cascade's square; a draw per way shades every kind (a scene of three kinds
  gives three kinds' pixels from one recorded draw). The frame's pipeline count test is updated
  with the new kernels (state the number).
- **Gate (B4a):** G-suite; G-pixels within 1/255 on the clearing and the Last Green; G-draws on
  the Last Green's walk frame 600, logical and physical: with multi-draw, under 40 logical draws
  (today about 430 recorded) and one compute pass before the first render pass with none between
  render passes until the sky; with `features=none`, the physical draw count is recorded and
  stated (it will be about the number of nonempty (kind, way) lists); G-profile: the clearing's
  GPU span falls at least 10 % and the Last Green's CPU frame time at least 20 % in Chrome
  (estimated; measure and record); G-pipelines.

**B4b. Two-phase occlusion, behind one switch.**

- **The depth pyramid (HZB).** `engine/cull.wrela`: an `R32Float` texture with levels at the
  scene's size (§A4). Level 0 is written from the frame's `depth` by a kernel (`Texels`); each
  level above takes the **minimum** of the children it covers. Depth is reversed-Z
  (`engine/view.wrela` ~10: 1 at the near plane, 0 at infinity, cleared to 0), so the minimum is
  the **farthest** depth in the cell, and an uncovered pixel (depth 0) keeps the whole cell open,
  exactly as the far crowns' map does today (`engine/wood.wrela` ~206: "Any uncovered pixel keeps
  the block open"). **Odd sizes:** when a child level's width or height is odd, the parent's last
  column or row covers three children, not two (a parent texel at `(x, y)` reads children
  `2x..2x+1` plus `2x+2` where `2x+2` is the child's last index; likewise `y`), so no child texel
  goes unrepresented. The test: an instance is culled only if its **nearest** point's depth is
  **less** than the farthest depth stored over its projected bounds, read at the level whose
  texel covers the bounds (at most 2×2 texels).
- **Phase 1** joins the B4a `cull` pass: after the frustum test, an occlusion test against the
  **previous** frame's pyramid reprojected with last frame's view-projection; an instance visible
  last frame and in the frustum is drawn now; one rejected for occlusion is marked for phase 2.
- **Phase 2.** After the opaque passes, **one** compute pass builds this frame's pyramid from
  `depth` and tests the marked instances; those now visible are drawn in a second set of opaque
  passes (prepass and shading), then the frame continues (sky, effects, resolve) as before. This
  is one deliberate compute interruption between render passes, placed once; a pass can't `join`
  across it (the host flushes an ended pass before compute, `gpu.ts` ~975), and the plan doesn't
  claim it does. Phase 2 is what makes phase 1 correct: an instance revealed this frame is drawn
  this frame.
- **One switch for both.** `Quality::occlusion = true` turns phase 1's occlusion test and phase
  2 on together; off, B4a's frustum-only path runs and nothing depends on the previous frame.
  Phase 1 without phase 2 is never a configuration.
- **The far crowns' 8×8 map** (`engine/wood.wrela` ~202–371) is removed; the pyramid replaces it.
- **Tests:** a scene of 10,000 spheres behind a wall draws only the wall's and the visible
  spheres' instances; a pyramid cell whose four pixels hold depths `[0.9, 0, 0, 0]` does **not**
  occlude a sphere at depth 0.5 behind it; a 5×3 depth image whose only uncovered pixel is in its
  last row and column stays non-occluding through every level; a sphere behind a one-pixel gap
  stays drawn as the camera moves the gap across cell boundaries; a sphere revealed when the wall
  moves is in the frame's `ids` target **that frame**, with the switch on and (trivially) off.
- **Gate (B4b):** G-suite; G-pixels within 1/255 against B4a (what phase 1 rejects, phase 2
  repairs the same frame); G-draws: exactly one compute pass between the opaque passes and the
  sky (physical count); G-profile: the pyramid, the tests and the interruption together cost
  under 0.3 ms GPU on the Last Green and save more than that in overdraw (estimated; measure, and
  if they don't pay, ship with `Quality::occlusion = false` and say so).

### B5. Temporal super-resolution as the default (D2)

- `engine/frame.wrela`: `Quality::scale = 0.667` and the `Shared` resolver as the default type
  the games construct (`games/last-green/main.wrela` ~80, `examples/clearing`). `Gathered` stays
  for stills and tests (`engine/shot` renders at scale 1).
- **Motion vectors for everything.** The `motion` target becomes `Rg16Float` motion plus an
  `R8` reactive mask in a second target of the opaque passes (§A6: the scene pass writes
  `Targets3<vec4, vec2, f32>`: colour, motion, reactive). Every opaque vertex shader passes last
  frame's clip position: terrain (camera-only), stones and built things (camera-only), creatures
  and characters (last frame's bone palette kept in `Cast`/`Realizer`), wood cards and grass (last
  frame's wind, as grass does today), water (camera-only plus its ripple's drift). The separate
  `motion` geometry pass is removed.
- **The resolver** (`engine/temporal.wrela`, `Shared`): add depth-aware dilation of the motion
  vector (the nearest of a 3×3), variance clipping with a reactive-mask-widened box, a
  disocclusion test from the depth pyramid of §B4, and a Lanczos-2 upsample of the current sample
  into the output grid (Karis 2014; AMD FSR 2's structure as a public reference). After the
  resolve, a contrast-adaptive sharpen (Lottes 2021's RCAS form) in `develop`.
- **Pacing.** `Quality::pace = true`; `sheet::LEAST_SCALE` for the 3D frame is 0.5.
- **Tests:** `engine/tests/render/temporal-*` run at scale 0.667 and compare to a scale-1
  `Gathered` render within the tolerance each test states; a new golden of the bare scene at
  0.667, blessed, with the difference image attached to the PR.
- **Gate:** G-suite; G-golden (re-blessed, said so); G-pixels within 6/255 against the scale-1
  baseline on the clearing's path and the Last Green's walk, no pixel over 12/255; G-profile: GPU
  span down at least 35 % on both; G-both.

### B6. A GPU memory budget

- `engine/memory.wrela`: budgets per subsystem (realizer meshes, impostor atlases, wood lists,
  probes, sky, targets), read from `std::gpu::memory()` (§A16) and each subsystem's own count. A
  `Budget` total of 1.0 GB **proposed** (the tab's 1.5 GB less WASM).
- Eviction rules, each a test: a creature unseen for 10 s drops to its coarsest level and frees
  the finer; an impostor kind with no instance in any loaded tile is dropped and rebaked on
  return (the bake is amortized by the planner, `Work::Kind`); wood lists shrink to 1.5× their
  need when under half full for 300 frames.
- **Impostor atlases**, already one `TextureArray` each for colour and normals since §B4a, are
  encoded as `Bc7` (colour) and `Bc5` (normals) layers when `features().bc`, by a kernel after
  the bake (`engine/impostor.wrela` ~354–426): BC1/BC4/BC5 by van Waveren (2006) and van Waveren
  and Castaño (2007), with BC7 mode 6 only (a simple, good-enough encoder). Without BC, the arrays
  stay `Rgba8`/`Rg16Float`. Memory for 26 kinds falls from about 312 MiB to about 78 MiB
  (estimated; the test measures it).
- **Gate:** G-suite; G-pixels within 4/255 on the Last Green's walk (BC quantization on far
  trees); `memory.json` peak in G-bench falls; record the numbers.

### B7. Virtualized field geometry (`engine::virtual`)

Promotes `examples/clusters` (spike S1a) into the engine for creatures, stones and built things.
Trees' cards, grass and terrain keep their own paths. This is the one place the engine has a
structural advantage over a mesh engine: every level is extracted exactly from the field, and
nothing is baked into the build. It lands in two sub-steps: B7a draws clusters with the hardware
rasterizer into a visibility buffer and is complete on its own; B7b adds a software rasterizer
for small clusters **only if B7a's measurement says it pays**.

**B7a. Cluster sets, the cut, a visibility buffer, hardware raster.**

- **Cluster sets.** `engine/virtual.wrela`, from `examples/clusters/clusters.wrela`: levels at
  cells 2^k × level 0's, a cluster per extraction block, errors monotone up the levels, skirts
  for cracks (as the spike). Add a **group DAG**: clusters of level k that share a parent block
  form a group; the cut is decided per group so neighbours in a group change level together, which
  removes most skirt seams (Karis 2021's locked boundaries, adapted to re-extraction).
- **The cut.** Per frame, in §B4's phase: instance cull, then the level-by-level select as the
  spike does (`seed`, `select`, `gather`), with the error compared in pixels at the output's
  scale (so §B5's scale lowers the detail drawn in step).
- **The visibility buffer.** The opaque pass for virtual geometry draws every picked cluster with
  the hardware rasterizer, one indexed indirect draw per level, writing an `R32Uint` visibility id
  (cluster × 128 + triangle) with depth through `WithDepth<u32>`, as the impostors do today, into
  the frame's `ids` target with the frame's depth test (`Compare::Greater`, reversed-Z).
- **Material resolve.** A full-screen pass per material class (from the `ids` class tag the
  frame already keeps) reads the visibility id, fetches the triangle's three vertices by vertex
  pulling, computes barycentrics and their derivatives from the pixel's ray (Burns and Hunt 2013;
  Schied and Dachsbacher 2015), and shades with the existing `lit::` functions. Colours stay
  per-vertex (cooked onto the mesh); this pass replaces the per-material vertex and fragment
  shaders for these kinds.
- **Shadows.** Cascade strips draw the same clusters depth-only with the hardware rasterizer.
- **Skinning.** Vertex pulling applies the bone palette per vertex as now (`character.wrela`
  ~153); the cut is decided on rest-pose bounds grown by the pose's reach.
- **Memory.** Cluster buffers live under §B6's realizer budget; the vertex format shrinks to 16
  bytes (position as 3×u16 in the instance's box, octahedral normal 2×u8, bones 4×u8 packed in the
  payload buffer, weights 4×u8) with a test that the error added is under a tenth of a cell.
- **Tests:** `examples/clusters/tests/cuts` move to `engine/tests/virtual` and extend: a field at
  three distances draws the expected levels; a scene of 500 creatures at 1080p draws a triangle
  count independent of their total (read back the `gather` count); a group's clusters never
  straddle levels; the material resolve of a flat-shaded sphere matches the old forward path
  within 1/255 away from silhouettes.
- **Measurement that decides B7b.** On the herd at 4× instances, record (a) the distribution of
  drawn triangles' projected areas (a histogram of px², read back from a counting kernel over the
  gathered triangles), (b) the GPU time of the visibility raster **alone** (its own label,
  apart from the material resolve's) and (c) the material resolve's time. B7b is built only if
  more than 30 % of drawn triangles are under 4 px² **and** (b) is over 1.5 ms at 1080p on the M4
  Air. These are trigger thresholds for an experiment, not evidence that software raster will
  win; B7b's own gate decides that. Otherwise B7b is skipped and the numbers go in the PR as the
  reason.
- **Gate (B7a):** G-suite; G-pixels within 4/255 on the clearing's fawn and the herd (silhouettes
  move by a cell at most; attach the difference images); G-profile: the herd's GPU span with 4×
  instances is under 1.5× the baseline's with 1× (estimated; measure); G-pipelines (state the
  count).

**B7b. A software rasterizer for small clusters (conditional).**

- **Atomics live in buffers, not textures** (WGSL's atomics take pointers into storage or
  workgroup memory; `Texels` writes only its own texel). Two `GpuBuffer`s of the scene's pixel
  count: `depth_bits: Atomics<u32>` and `ids: Atomics<u32>`, **initialized each frame** by a
  kernel to `depth_bits = 0` (the far plane in reversed-Z) and `ids = 0xFFFF_FFFF` (no triangle;
  the merge skips it).
- **Pass 1:** one invocation per small-cluster triangle rasterizes it (bounding box, edge
  functions, up to 64 px²) and does `depth_bits.max(pixel, bitcast_u32(depth))` per covered
  pixel. Depth is reversed-Z and positive, so its float bits order as integers and the maximum is
  the nearest.
- **Pass 2:** the same triangles again; where `bitcast_u32(depth) == depth_bits.load(pixel)`, do
  `ids.min(pixel, id)`. **Ties are deterministic:** coplanar triangles at equal depth resolve to
  the smallest id, every run, both hosts.
- **Merge.** A full-screen draw with `WithDepth<u32>` into the visibility pass writes each pixel's
  `ids` value at its `depth_bits` depth through the pass's depth test (`Compare::Greater`), so a
  nearer hardware fragment wins and an **equal** depth keeps the hardware fragment (the documented
  tie policy between the two rasterizers).
- With `features().atomic64_min_max` (§A10), pass 1 and 2 become one `Atomics<u64>` `max` of
  `(depth_bits << 32) | (0xFFFF_FFFF − id)`, which picks the nearest and, at a tie, the smallest
  id, the same result as the two passes.
- Shadows: the cascade strips use pass 1 alone (depth only).
- **Tests:** two coplanar overlapping triangles with ids 7 and 3 give id 3 at every shared pixel
  (the **correct** tie, not merely a repeatable one), ten runs, both hosts; a software triangle in
  front of a hardware one and behind one resolves correctly; a pixel no software triangle covers
  keeps the hardware id (the `0xFFFF_FFFF` sentinel is skipped); the 64-bit and 32-bit paths give
  identical ids on a device that has both.
- **Gate (B7b):** G-suite; G-pixels within 1/255 against B7a on the herd (small clusters move
  by less than a pixel); G-profile: the virtual geometry pass's GPU time falls against B7a on
  the herd at 4× (state by how much; if it doesn't fall, revert B7b and record why).

### B8. Lighting

Each item is its own commit. The look is Ryan's: these add light the look can use, with each
effect behind a `Quality` switch defaulting as said.

1. **Ambient occlusion (GTAO).** `engine/occlusion.wrela`: Jimenez et al. (2016) at half the
   scene's size over the depth pyramid and the §B5 normals (a `Rgb10a2` normal target joins the
   opaque pass's targets: `Targets4`), 2 directions × 4 steps a pixel, rotated per frame by the
   TAA jitter index, denoised by the resolver's history. Multiplies the probes' term in
   `lit::ambient`. `Quality::occlusion = true` **proposed**. G-pixels: a deliberate change; attach
   before/after stills of two lookbook shots.
2. **A sun that moves (D5).** `engine/shadow.wrela` ~444: a strip set whose sun moved **finishes
   with the sun it began with**; the next set begins with the new sun; the front map swaps when a
   set completes, so shadows lag a moving sun by one set's duration (near: 12 frames; far: 48).
   `engine/air.wrela`: the air's tables cook in slices through `std::job` over 8 frames instead of
   one 20 ms frame; `engine/frame.wrela` ~1274's 137 ms frame disappears. The probes re-trace at a
   budgeted rate (item 3). `engine/tests/frame` gains a `Sun` step that moves the sun every frame
   for 120 frames and asserts no frame cooks air tables whole and the cascades complete sets.
3. **Dynamic probes.** `engine/probes.wrela` ~853: the `again` path is the default (`Quality::
   probes.again = true`), re-tracing 32 columns a frame at 64 rays through the density volume,
   lit by the current sun and sky, blending with a 0.9 hysteresis, so bounce light follows the
   sun and the clouds (DDGI's update loop, Majercik et al. 2019, over the engine's own volume).
   The frame test's weather and sun steps assert the probes' version advances without a full
   rebake.
4. **Local lights.** `engine/lights.wrela`: `Light` instances (point and spot: position, radius,
   colour, cone) the scene supplies (`Scene::lights() -> [LocalLight]`, at most 256), binned into
   a 16×9×24 froxel grid per frame by a kernel with `SharedAtomics` (Olsson, Billeter and
   Assarsson 2012); `Lighting` gains the grid and the list; `lit::direct` adds the pixel's froxel's
   lights with the GGX model already there. Shadowed lights are out of scope (recorded). A test
   scene with 200 lights reads back each froxel's count against a CPU binning.
5. **Reflections.** Generalize `engine/water.wrela`'s screen-space march to `lit::reflect` for
   surfaces with roughness under 0.3 (the §B8.1 normal target supplies the normal), falling back
   to a **specular probe**: a `Cubemap<Rgba16Float>` of 64² faces baked beside the diffuse probes'
   open sky from the dome and the far terrain (`engine/probes.wrela`), prefiltered per roughness
   into 5 mip levels (§A3), sampled by `sample_cube`. The clearing's water drops its own march for
   the shared one (G-pixels within 2/255 on the water).
6. **Contact shadows.** `lit::sun` adds an 8-step screen-space march toward the sun over the
   depth (2 m), darkening where the ray meets depth; behind `Quality::contact = true` **proposed**.
7. **Exposure and grading.** `engine/paint.wrela` ~184: a 128-bin log-luminance histogram with
   `SharedAtomics` over the pre-TAA scene, exposure from the 50th to 95th percentile band; a
   `Texture3d<Rgba8>` 32³ grading LUT the look may set (`Look::lut`), identity by default; the
   bloom becomes a 4-level mip pyramid (§A3) up and down with tent filters.
- **Gate (whole step):** G-suite; G-golden re-blessed where the step says the image changes, with
  stills in the PR; G-profile: the clearing's GPU span rises by no more than 2.0 ms at 1080p with
  every switch on, and is under the §B1 baseline with them on **and** §B5's scale (record both);
  G-both.

### B9. Particles and transparency

- **GPU particles.** `engine/particles.wrela`: emitters the scene supplies (`Scene::emit(mut
  Emits)`: position, velocity, life, size, colour, kind), a simulate kernel over an `Append`
  double buffer (gravity, wind from `Scene::wind`, drag, a depth-buffer collision test), a radix
  sort by view depth (4 passes of 8 bits with `SharedAtomics` and `workgroup_prefix_sum`; Satish,
  Harris and Garland 2009), one indirect instanced draw of camera-facing quads with
  `blend: Blend::Premultiplied` (or `Add` for sparks), soft-depth fade against the scene's depth,
  lit by the probes. A budget of 262,144 particles **proposed**. The capybara's dust (§B10) uses
  the same module in 2D.
- **Weighted blended order-independent transparency** (McGuire and Bavoil 2013) for the veils
  the frame dithers today (`engine/view.wrela` ~281): an accumulate target `Rgba16Float` with
  `Blend::Add` and a revealage target `R8` with `Blend::Multiply` in one `Pass2` after the opaque
  passes, composited in `develop`. `Quality::transparency = Dithered | Weighted` **proposed**
  default `Weighted`.
- **Tests:** `engine/tests/particles`: 10,000 particles fall under gravity and the sorted order
  read back is monotone in depth; a transparent quad over another gives McGuire's formula within
  1/255.
- **Gate:** G-suite; G-pipelines (state the count); G-profile: a 100k-particle test scene costs
  under 1.0 ms GPU at 1080p on the M4 Air (estimated; measure).

### B10. The 2D path

The sim (`body.wrela`, `feel.wrela`, `ground.wrela`, `timeline`) does not change. These are the
frame's, the camera's and the tooling's.

1. **Targets.** `engine/sheet.wrela`: `Sheet` targets become `Rgba8` (the sheet draws display
   colours; `ink.wrela` says no light is simulated), and `render_size` targets the screen's
   **logical** size × 1.5 capped at the device size (a 2× display renders at 1.5× CSS pixels, the
   page's `finish` stretches). `Scale::pace` keeps stepping below that. G-pixels within 2/255 on
   the capybara at 1280×720; at 2940×1912 the image is a deliberate change (sharper HUD, same
   edges); attach stills.
2. **The water's copy, scissored or skipped.** `Sheet::copy_below` takes the screen rectangles of
   the pools in view (`course.wrela`'s water boxes through `flat_camera`) and copies only them
   with `pass.scissor` (§A7); with no pool in view it records nothing. `main.wrela` ~508.
3. **Cached parallax.** `games/capybara/scenery.wrela` ~656: each of the four layers renders into
   its own `Rgba8` strip texture 1.5 screens wide at its parallax rate, redrawn only when the
   camera has scrolled past a quarter of the margin or the time-driven parts (clouds) are due (a
   per-layer refresh rate); the backdrop pass composites the four strips with `sample`. The sky
   stays procedural. G-pixels within 2/255 (the clouds move in steps of their refresh).
4. **No per-frame allocation.** `main.wrela` ~415 and `dust.wrela` ~79: `Vec`s kept in the state
   and cleared; `present.wrela` ~47's ring indexes instead of `remove(0)`; `dust` becomes a ring;
   one `Renderer` and one `Fonts` upload (`main.wrela` ~199). `waves.wrela` ~64 steps only the
   columns within two screens of the camera.
5. **A fixed play area (D3).** `camera.wrela`: `Aspect::Fixed(16.0 / 9.0)` **proposed** default:
   the view shows `HALF_HEIGHT` by `HALF_HEIGHT × 16/9`; a wider screen is pillarboxed (the page
   draws the margins in the backdrop's sky colour), a taller one letterboxed; `Aspect::Free` is a
   setting in `settings.wrela`. The camera tests (`camera.wrela` ~142) assert the visible width is
   the same at 21:9 and 16:9. `design.md`'s "Movement" gains the contract.
6. **Dust on the GPU.** `dust.wrela` emits into `engine::particles` in its 2D mode (orthographic,
   no sort, `Blend::Premultiplied`); the CPU cap of 600 goes. G-pixels within 2/255.
7. **Text at every size.** `ui/text.wrela` and `ui/ttf.wrela`: glyphs baked as multi-channel
   signed distance fields (Chlumský 2015) at build time; `draw.wrela`'s glyph shader takes the
   median of three channels. The title (`screens.wrela` ~319) renders crisp at 138 px; the test in
   `ui/tests` compares a 128 px glyph's edge against the TTF outline within 0.5 px.
8. **An audio budget.** Test mode's `audio=750` on the capybara's busiest bar (every voice, the
   piano's loudest chord) on the M4 Air with one core pinned low (`taskpolicy -c background`, or
   the Linux CI host): the render time per quantum must be under 1.3 ms (half the 2.67 ms
   quantum). `engine/piano.wrela` gains `MAX_SOUNDING` and drops the quietest string past it;
   `sound.wrela`'s 40 voices stay. The number lands in `design.md` as a contract.
9. **A broadphase for the course.** `course.wrela` ~218: a uniform 8 m grid of piece indices,
   built when the course constant is; `contact` scans the body's cells. A proofs test checks the
   same contacts as the scan over 10,000 random positions.
10. **The course lens.** `studio/tool` (the lens) learns 2D: a course constant's corner chain is
    shown over the running capybara, a drag moves a corner and writes the literal through
    `wrela edit`, as the 3D lens does for a field's literals (`spec/language.md` §23.5). Minimal:
    corners only; slopes and water boxes follow in a later scope. A tool test drags a corner and
    reads the changed literal.
- **Gate (whole step):** G-suite; G-pixels as stated; G-bench: the capybara at 2940×1912, GPU
  span down at least 50 % (the two half-float targets and the unconditional copy were most of it:
  estimated); the audio number recorded.

### B11. Three browsers, one page

- Toolchain 0.5.1 made ticked programs run in the Claude desktop app's browser pane (§A14;
  `docs/toolchain-gaps.md` records it fixed). Re-check it on 0.6.0 with `wrela run games/capybara`
  (the pane opens it, a mouse press on `Start` starts the game, then the keys move the capybara)
  and record the result in `docs/bench.md` under "Browsers"; a regression is a gaps entry for
  0.6.1.
- Safari 26 and Firefox (current stable on macOS): open the Last Green, the clearing and the
  capybara from `wrela run`; record in `docs/bench.md` under "Browsers": does it start, which
  `features()` it reports, the paced frame interval p95 read from the game's own debug readout
  (F7), and any error verbatim. A failure that is the toolchain's becomes an entry in
  `docs/toolchain-gaps.md` for 0.6.1, not a fix here.
- **Gate:** the records exist.

### B12. Records

- `docs/toolchain-gaps.md`: the fixed entries name 0.6.0; new gaps found in Part B are added
  open.
- `docs/citations.md`: a row for each method in §7 whose code landed.
- `games/capybara/design.md`: the aspect contract (D3), the audio budget (§B10.8). (The file is
  allowed by `wrela.toml` as `games/*/design.md`; if it is missing from `main`, restore it from
  the `nostalgic-easley-401100` worktree before editing.)
- `docs/toolchain.md`: `wrela profile --chrome`, `wrela tool bench`.
- `wrela.toml`'s `allow`: `docs/bench.md`, `docs/plan-world-class.md`.
- `AGENTS.md`: a line pointing at `docs/bench.md` as the place performance claims are recorded.

### B13. The final gates and the PR

1. `wrela suite` on 0.6.0 passes, every member.
2. G-pixels: the capybara at 1280×720 within 2/255 of `build/base-0.6`; the herd and climber lab
   within 4/255; the clearing and the Last Green are deliberate changes with lookbook stills before
   and after in the PR (`wrela tool lookbook --package examples/clearing`, and the floor's).
3. G-pipelines: `wrela pipelines games/last-green --json` count in the PR with the per-step
   additions summed.
4. G-profile (Chrome and native) for the clearing's path, the Last Green's opening and walk,
   the capybara's run, the herd: the table in the PR, against `build/base-0.6`.
5. G-size: `wrela tool release` into `build/release`; sizes in the PR.
6. G-bench: `docs/bench.md`'s new entry, beside the §B1 baseline and the competitors' section,
   and whether §0.1's claim holds on the M4 Air, scene by scene, in a sentence each. A scene that
   misses says by how much and what the next lever is.
7. The PR description follows the re-architecture's (commit `37f17e38`): what changed, deviations
   from this plan with reasons, checks with their numbers, review findings, and "For Ryan"
   (the D1–D9 defaults built and anything that needs his call).

---

## 7. Sources to cite when their code lands

Each gets a row in `docs/citations.md` (method, where it's used, licence or publication) when its
step merges. Use them as references, not as code to copy.

| Method | Reference | Used in |
|---|---|---|
| GPU-driven rendering, two-phase occlusion culling | Haar, U., Aaltonen, S. *GPU-Driven Rendering Pipelines.* SIGGRAPH 2015, Advances in Real-Time Rendering | §B4 |
| Cluster LOD, software rasterization, visibility buffer | Karis, B., Stubbe, R., Wihlidal, G. *A Deep Dive into Nanite Virtualized Geometry.* SIGGRAPH 2021 | §B4, §B7 |
| Hierarchical depth buffer | Greene, N., Kass, M., Miller, G. *Hierarchical Z-Buffer Visibility.* SIGGRAPH 1993 | §B4 |
| Visibility buffer and attribute resolve | Burns, C., Hunt, W. *The Visibility Buffer: A Cache-Friendly Approach to Deferred Shading.* JCGT 2013; Schied, C., Dachsbacher, C. *Deferred Attribute Interpolation for Memory-Efficient Deferred Shading.* HPG 2015 | §B7 |
| Temporal supersampling and upscaling | Karis, B. *High Quality Temporal Supersampling.* SIGGRAPH 2014; AMD *FidelityFX Super Resolution 2* (MIT) as a published structure; Lottes, T. *FidelityFX CAS / RCAS* (MIT) | §B5 |
| Ground-truth ambient occlusion | Jimenez, J., Wu, X., Pesce, A., Jarabo, A. *Practical Realtime Strategies for Accurate Indirect Occlusion.* SIGGRAPH 2016 courses | §B8.1 |
| Dynamic diffuse probes | Majercik, Z., Guertin, J.-P., Nowrouzezahrai, D., McGuire, M. *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields.* JCGT 2019 | §B8.3 |
| Clustered shading | Olsson, O., Billeter, M., Assarsson, U. *Clustered Deferred and Forward Shading.* HPG 2012 | §B8.4 |
| Weighted blended OIT | McGuire, M., Bavoil, L. *Weighted Blended Order-Independent Transparency.* JCGT 2013 | §B9 |
| GPU radix sort | Satish, N., Harris, M., Garland, M. *Designing Efficient Sorting Algorithms for Manycore GPUs.* IPDPS 2009 | §B9 |
| Real-time BC compression | van Waveren, J.M.P. *Real-Time DXT Compression.* Intel 2006; van Waveren, J.M.P., Castaño, I. *Real-Time YCoCg-DXT Compression.* 2007 | §B6 |
| Multi-channel signed distance fields | Chlumský, V. *Shape Decomposition for Multi-channel Distance Fields.* Master's thesis, CTU Prague 2015 (msdfgen, MIT) | §B10.7 |
| Workgroup scans | Hillis, W.D., Steele, G.L. *Data Parallel Algorithms.* CACM 1986 | §A8 |
| Software rasterization in WebGPU with 32-bit atomics | Scthe, *nanite-webgpu* (MIT), as prior art for the packing and the two-pass fallback | §B7 |
| WebGPU and WGSL | W3C *WebGPU* and *WGSL* specifications, the sections for each feature | Part A |

## 8. The council's findings

A council of two Codex reviewers (`gpt-6.1-sol`, correctness and verification; `gpt-6-astra`,
architecture and risk; both at high reasoning effort, read-only, through the Codex CLI) reviewed
the plan on 2026-10-24, with a Claude leader. The findings of each round, the decision on each and
what changed.

### Round 1

Both reviewers returned six findings; four were raised by both.

| ID | Finding | Decision | What changed |
|---|---|---|---|
| R1-A-F1, R1-B-F1 | The command-path step would have read uploads from live WASM memory and reused one command object per opcode. Commands are kept until the export returns (`program.ts` ~618, ~996) and bytes are copied because the program may overwrite its command buffer (~755); tests at `program.test.ts` ~456 and ~509 guard this. Reading live memory would upload wrong bytes; executing early would break the rollback a trapping export relies on. | Accepted. | §A13 now states what must not change, writes uploads from the retained batch slice (two copies, not four), decodes into a struct-of-arrays table that keeps every pending command distinct, and adds a test that submits several `WriteBuffer`s then overwrites the command buffer. |
| R1-A-F2, R1-B-F2 | The software rasterizer did `atomicMax` into an `R32Uint` texture, which WGSL can't do (atomics are pointers into storage or workgroup memory; `Texels` writes its own texel only), and equal-depth triangles raced on the id write. | Accepted. | §B7 is split: B7a is the hardware-raster cluster path, complete on its own; B7b is the software rasterizer, built only if B7a's measurement says small clusters pay, with atomics in buffers, `atomicMin` of the id at equal depth (deterministic ties), an explicit merge draw and tie policy against the hardware path, and tests for coplanar triangles and mixed paths. |
| R1-A-F3, R1-B-F3 | The depth pyramid reduced by maximum. Under reversed-Z (`view.wrela` ~10) that hides uncovered pixels in a cell and culls visible geometry behind gaps; the existing map takes the minimum for that reason (`wood.wrela` ~206). | Accepted. | §B4's pyramid takes the minimum (the farthest depth), the cull test is spelled out (nearest point less than the farthest stored), and the `[0.9, 0, 0, 0]` cell test is in the step. |
| R1-A-F4, R1-B-F5 | G-pixels required equal state hashes, but `wrela-host`'s hash covers every command byte, so correct changes to dispatches or passes would fail the gate; and bucketed live measurements can't guarantee run-to-run order. | Accepted. | §0.5 separates G-sim (tick replay), G-commands (hash equality, only where commands are meant to be identical), G-pixels (RGBA bytes) and G-draws (recorded command counts). §B2 tests the planner by injected measurements and asserts properties, not order, of the live path. |
| R1-A-F5 | Blend and attachment validation were wrong: `float32-blendable` is needed to blend any of the three float32 formats, and the attachment limit uses WebGPU's render-target byte cost with alignment (`rgba8unorm` costs 8), not storage bytes. | Accepted. | §A3 adds one format table (capabilities, byte cost, alignment, required features) shared by compiler and hosts; §A6 and §A7 validate from it, with the worked example `[Rgba32Float, Rgba8, Rgba8, Rgba16Float]` = 40 > 32 refused. |
| R1-A-F6, R1-B-F6 | Steps depended on later steps (A2's tests on A4's copies, A11 on A14's profiler, A13 on B1's bench), and the release was tagged before any engine integration, though a published release can't be replaced. | Accepted. | Part A is reordered with a "Needs:" line per step: copies and readback are A2, the Chrome profiler is A12 before the command path (A13) and the WASM choice (A15, decided on Part A's own fixtures), and A18 integrates Part B's first steps and a capability test against the branch before the tag. §0.2 says why. |
| R1-B-F4 | B4 claimed pass joining across the pyramid build (the host flushes an ended pass before compute) and a draw-count collapse the wood's per-kind bindings don't allow. | Accepted. | §B4 is split: B4a consolidates culling into one compute pass before any render pass and makes bindings shared (kind atlases as texture-array layers, kind parameters in one buffer) before claiming the merge, measured by G-draws with and without multi-draw; B4b places the one compute interruption deliberately and claims no join. |

Reviewer A also confirmed the plan's reading that two mip levels of one texture may be bound read
and written in one dispatch under WebGPU's per-subresource usage rules.

### Round 2

Both reviewers read the revised plan. They marked F1, F2, F3 and F6 resolved (with the
initialization values for the software rasterizer's buffers now written into §B7b), U1 and U5
resolved with notes now in the plan, U2 and U3 "needs a check" (the checks are now in §A13.1 and
§B7a), and sustained four objections, each with evidence the leader verified. Three new findings
were raised. No decision-changing dispute remained, so the council stopped at two rounds.

| ID | Finding | Decision | What changed |
|---|---|---|---|
| R2 on F5 (A) | §A7 aligned each attachment's cost, but WebGPU's algorithm aligns the **running total** before adding the next cost; the accepted example costs 20, not 17 (wgpu's `validation.rs` agrees). | Accepted. | §A7 states the algorithm step by step, notes that target order matters, and corrects the example to 8 → 12 → 13 → 20. |
| R2 on F4 (B) | The leaf atlas and bark ridges are already shared; what the wood binds per kind is its cards, orders, limbs, parameters, impostor textures and per-list instance spans (`wood.wrela` ~995–1060), so shared textures alone merge nothing; impostor arrays were in §B6, after the gate; a host loop over multi-draw doesn't reduce physical draws. | Accepted. | §B4a pools geometry into shared buffers with a per-kind descriptor, takes instance ranges from the draw arguments, moves the impostor arrays into B4a, claims the merge only with multi-draw present, and states the fallback's physical count apart. |
| R2 on F4 (A) / F5 (B), R2-N1, R2-N2 | G-sim used `--replay`, which checks the log's WASM hash first, so any rendering change fails it; §A18.1 asked for command-hash equality across 0.4.1 and 0.5.0, impossible since the batch header carries the stream version. | Accepted. | §0.5's G-sim compares two tick logs' hashes with a new `wrela-host --compare-ticks` (§A12); §A18.1 asks for G-pixels and G-sim across toolchains and offers a normalized command dump for inspection; D1 no longer promises identical order. |
| R2-N3 | G-draws counted recorded commands, but compute passes are opened and multi-draws expanded inside the executor, below the recorded stream. | Accepted. | §A12 adds executor counters (`counters.json`: compute and render passes, physical draws, `writeBuffer` calls, bind-group and pipeline sets) on both hosts; §0.5's G-draws names logical and physical counts, and §B4 states both. |
| R2 on U4 | A syntactic single-writer rule for `Shared::scatter` admits a race: `scatter(lid, a)` and `scatter(lid + 1, b)` in one phase both pass it. | Accepted. | §A9 drops `Shared::scatter`; `SharedAtomics::store`/`exchange` is the race-free general scatter and `Shared` keeps its own-chunk rule. |
| R2-F1 (B) | B4a's phase 1 used the previous frame's pyramid; with phase 2 off, a revealed object disappears for a frame, against the plan's own gates. | Accepted. | §B4a is frustum culling and binding consolidation only; §B4b holds both occlusion phases behind one switch, and "phase 1 without phase 2 is never a configuration". |
| R2-F2 (B) | A four-child minimum reduction at odd sizes leaves a last row or column unrepresented, so an uncovered edge pixel can be lost and cause false occlusion. | Accepted. | §B4b's parent texels on an odd edge cover three children; a 5×3 test with the only uncovered pixel in the last row and column is in the step. |
| R2-F3 (B) | B2's live invariant "spent ≤ budget + one step" is false: mandatory levels and noted cooks are charged whatever the budget (`Planner::begin`, `note`). | Accepted. | §B2 bounds the **optional** steps' cost by `budget − mandatory` with the first-step exception, asserts the mandatory cooks complete, and adds `spent_mandatory()` / `spent_optional()`. |
| R2 on U1 | Bucketing is defensible as a mitigation, but a fixed 250 µs step halves the throughput of small kinds (the realizer's 120 µs estimate) and D1 still promised identical order. | Accepted. | §B2 buckets on a geometric ladder (a factor 1.25) and measures the completion latency of each cook kind with and without bucketing; D1's wording says order is not guaranteed. |
| R2 on U2 | Removing the staging copy replaces one batched `writeBuffer` with one call per upload (`gpu.ts` ~489), which may cost more than it saves on small uploads. | Accepted. | §A13.1 measures `writeBuffer` call counts on a small-upload workload (the capybara) and keeps a staging path under a size threshold if direct calls lose. |
| R2 on U3 | Cluster area doesn't establish triangle size or a raster bottleneck; the thresholds are experiment triggers, not evidence. | Accepted. | §B7a records the triangle-area histogram and times the visibility raster apart from the material resolve; the trigger is restated as a trigger, and B7b's own gate decides. |
| R2 on U5 | Resolved, with a caution: the sampled view must exclude the written level. | Accepted. | §A4's `generate_mips` binds `tex.level(i − 1)` and `tex.level(i)`, never the whole chain. |

**Remaining uncertainty, and the check that settles each:** every "estimated" speedup and
memory number (the gates measure them); whether A13's direct uploads win in Chrome (A13's
measure-and-revert); whether B4b's occlusion pays on the Last Green (B4b's gate and switch);
whether B7b is built at all (B7a's measurement); which WASM optimization level wins (A15's
measurement, confirmed by B1). None of these changes the plan's shape; each has a gate.

## 9. Revision for toolchain 0.5.2 (2026-10-24)

The plan was written against toolchain 0.4.1. The same day, `main` moved to 0.5.0 (`ee1f25ad`)
and then 0.5.2 (`9f1429bf`), through rywible/wrela-compiler#28, #29 and #30. What each release
holds and what changed here, so an agent reading the council's record (§8, which names 0.4.1 and
0.5.0 as the versions of its day) can place the numbers:

| Release | What it brought | What changed in this plan |
|---|---|---|
| 0.5.0 | The agent loop: every error carries a next step; `wrela doc` on built-in types; lints W0010–W0013; `wrela pipelines`' `(under a condition)` marker; `wrela migrate` and the migration policy (`docs/release.md` §1); `wrela same`; the `extract-const`, `inline` and `extract-fn` refactors; tighter interval derivations; the agent syntax test v2 as a release step (`docs/release.md` §2.5). The command stream (`VERSION` 11), the manifest (9), `std::gpu` and spec §18 are unchanged. | Part A is **0.6.0** (a feature is a minor bump) and a fix after its tag 0.6.1; the baselines are `build/base-0.6/`. §0.3 names the tools and the lints, and the rule that an existing form changes only as a migration. §0.5 adds the one-command form of G-commands, G-pixels and G-pipelines through `wrela same`, and says why it can't bridge the pin move. §B0 runs `wrela migrate` and moves the docs' host path. §A19 runs the agent syntax test, as the release process now requires. Part A's copy lives in the compiler's `docs/plans/`. |
| 0.5.1 | The page starts the render worker's threads over `MessageChannel`s, so ticked programs run in the Claude app's browser pane (its Chrome starts no nested worker). A pointer presses every role a press activates, and a mouse press doesn't take the keyboard's focus. | §A14 and §A17 are **done**. They keep their numbers for the `Needs:` lines and say what the plan still owes them (a re-check in §A18.4 and §B11). The "Where things live" row for the page and the workers has the new anchors. |
| 0.5.2 | A pointer over an `Option` or a `Tab` is the program's again (0.5.1 broke the docsite reader's search results; this repository's suite caught it). The capybara's menu items are `MenuItem`s. | §A17 records the correction; nothing in §B10 changes. |

Part A's source anchors were re-checked against `v0.5.2`: `program.ts` 608, 618 and 993,
`gpu.ts` 464 (`Ring`), 1033 and ~1223, `manifest.rs` 154–156, `stream.rs` 83 and
`runtime/native/src/gpu.rs` ~478 are as cited; `main.ts` and `worker.ts` moved and are re-cited.
Part B's engine anchors are untouched by the two `main` commits, which changed the docs, the
capybara's screens and 36 `.clone()`s that W0010 flagged into `borrow`s.
