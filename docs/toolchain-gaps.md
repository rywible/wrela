# Toolchain gaps

*Capabilities this repository needed from the toolchain ([rywible/wrela-compiler](https://github.com/rywible/wrela-compiler)) and found missing or broken. AGENTS.md's rule holds: a gap is fixed in the toolchain, released, and the pin moved; never worked around here in another language. Started 2026-10-21 at Ryan's request.*

Each entry says what was needed, what happens instead, how it was found, the evidence, what this repository does meanwhile, and its status. Add an entry when a gap is found; when the fix ships, say in which release and keep the entry.

## Open

### A fake site can't answer with a header

- **Needed:** the deploy's Zstandard check (studio/deploy `verify_zstd`, docs/plan-online-play.md step 12) tested end to end on a fake site that answers a WASM with `content-encoding: zstd`, as one with the zone's Compression Rule set does, and the deploy passing with the rule required.
- **What happens:** toolchain 0.17.0's fake sites answer with `content-length` alone, and `std::tool::testing::Fake` sets a status, a 404, a body, a delay or failures, never a header.
- **Found:** 2026-10-31, writing the Zstandard check.
- **Evidence:** `fake` in runtime/native/src/tool/http.rs (wrela-compiler at `a4ad713`, 0.17.0) builds every answer's headers from `length(n)`, a `content-length` only.
- **Meanwhile:** the answer's rule is a test of code (`not_zstd`: only a 200 with `content-encoding: zstd` passes), and a tool test checks that on fake sites every console and game WASM and runtime script is a warning, or with the rule required the deploy's failure (studio/deploy/tests.wrela).
- **Status:** open: a `Fake` of headers (say `Fake::Headers(Vec<Header>)`, added to the file's answer).

### A client can't fetch a public read without its cookies, or with the browser's cache

- **Needed:** the console's public reads (a board's page, a slice, a run's replay by its hash, a search of names: platform/wire.wrela's `PUBLIC` and `REPLAYS`) sent with no cookie, so nothing in them can tie a reader to an account, and kept by the browser's own cache, so a replay asked for again (its path never changes, `immutable` for a year) costs no request at all.
- **What happens:** toolchain 0.17.0's `std::http::fetch` sends the origin's cookies with every request and skips the browser's cache: its browser host calls `fetch` with `credentials: "same-origin"`, `redirect: "error"` and `cache: "no-store"`, for every method. Nothing in `std::http` asks for `credentials: "omit"` or the default cache mode.
- **Found:** 2026-10-31, the council's review of the online-play branch (rywible/wrela, `capybara/design-online-play`).
- **Evidence:** the browser host bundled in 0.17.0's `wrela` (`strings ~/.wrela/toolchains/0.17.0/bin/wrela`) holds `credentials:"same-origin",redirect:"error",cache:"no-store"` in the `fetch` it makes for `http/fetch`; `wrela doc std::http::fetch` (0.17.0) says it is sent "with the origin's cookies".
- **Meanwhile:** a public read needs no account and the server ties none to it: it reads no session for one and sets no cookie on its answer (server.wrela's `sessionless`; server_tests.wrela's `a_public_read_is_no_sessions_and_sets_no_cookie`), so its answer is the same for everyone and Cloudflare's edge cache serves it (`cache-control: public`, server.wrela's `BOARD_AGE`, `RUN_AGE`, `REPLAY_AGE`). The browser asks the edge again each time; the capybara holds the replays it fetched in memory (games/capybara/online.wrela's `HELD`).
- **Status:** open: a fetch that omits credentials and uses the browser's cache (say `std::http::fetch` options, or a `fetch_public(path)` for `GET`).

### A driven build can't be given an address

- **Needed:** the capybara's attract tool test (games/capybara/tests/attract) driving the play build as the console opens it, at `capybara/#attract`, so the test's build decides it plays itself from its address as a browser's does.
- **What happens:** toolchain 0.17.0's native host (and so `std::tool::drive` and `@test(frames: n)`) fails `host::address` (`nav/location`): `drive::Load` has no address, and `wrela-host` takes none.
- **Found:** 2026-10-31, moving the attract start into the play build (docs/plan-online-play.md step 9).
- **Evidence:** a program that polls `host::address()` from `init` under `wrela-host` 0.17.0 gets `Err` on its first frame.
- **Meanwhile:** the game takes a failed address as the game played, and the tool test calls the export `play_itself` after `init`, which does what an address of `#attract` does (games/capybara/main.wrela); the address's own rule (`role_of`) is a test of code.
- **Status:** open: an address in `drive::Load` and `wrela-host --address`, which `nav/location` then answers.

### A driven frame doesn't say its passes' targets or GPU times

- **Needed:** `wrela tool passes` (docs/plan-world-class-2.md §B1.5) shows a frame's passes with their targets (name, format, size), their GPU time, and a thumbnail of what each drew.
- **What happens:** toolchain 0.8.0's `std::tool::drive` records a frame's commands (`Step::Commands`) with each draw's and dispatch's pipeline and bindings, but a `BeginPass` carries no targets and a `CreateTexture` no format or size. A driven build's GPU timings (its `Load::timing`) aren't given back to the tool, and `Step::Capture` gives only the screen, not a labelled pass's target.
- **Found:** 2026-10-26, writing `studio/passes`.
- **Evidence:** `runtime/native/src/tool/drive/child.rs` `Recorded`: only `Dispatch`, `Draw` and the indirect draws fill `pipeline` and `bindings`, and paths fill `target`.
- **Meanwhile:** the viewer lists each pass's label, its pipelines and its draws or dispatches, with the frame's screen. `wrela profile --worst` gives the labels' GPU times.
- **Status:** open: a pass's targets in its record, the timings beside the spans (`Ran`), and a `Capture` of a labelled pass.

### A tool can't read an input script from its JSON

- **Needed:** `wrela tool passes -- games/last-green --frame 300 --input build/gates/LG_WALK.json`: a gate script's file as a driven build's input.
- **What happens:** `std::input::Script`'s fields are private and std has no parser for its JSON (runtime/abi `input`'s format), so a tool can only drive a build with a `Script` constant it was compiled with.
- **Found:** 2026-10-26, writing `studio/passes`.
- **Evidence:** compiler/std/input.wrela: `Script { json, ticks, last, count }`, built only by its methods.
- **Meanwhile:** `--input` names one of the bench's scenes (`last-green-walk` is `LG_WALK`), whose script `studio/bench/scenes.wrela` holds.
- **Status:** open: `Script::from_json(text) -> Option<Script>`.

### A fragment can't say its depth only grows

- **Needed:** the early depth test kept on for a fragment that writes its own depth only further away (or only nearer) than its triangle's: the impostors' leaves and the soft raster's merge (docs/plan-world-class-2.md §A11, §B5.6), which turn early-Z off for their draws today.
- **What happens:** toolchain 0.8.0 has `std::gpu::WithDepthGreater<C>` and `WithDepthLess<C>` to name the intent, but they are `WithDepth<C>`: naga 30.0.1's WGSL writer emits no conservative-depth attribute (its IR has `EarlyDepthTest::Allow { conservative: ConservativeDepth::GreaterEqual }`, which its WGSL front end parses from `@early_depth_test(greater_equal)` and its WGSL back end never writes), and Chrome 155 accepts none either.
- **Found:** 2026-10-25, 0.8.0's §A11.
- **Evidence:** Chrome 155.0.8059.86 on the M4: `@fragment @early_depth_test(greater_equal) fn f() -> @builtin(frag_depth) f32` fails with "expected attribute" (its attributes are `align`, `binding`, `blend_src`, `builtin`, `color`, `compute`, `diagnostic`, `fragment`, `group`, `id`, `input_attachment_index`, `interpolate`, `invariant`, `location`, `must_use`, `size`, `subgroup_size`, `vertex`, `workgroup_size`), and `navigator.gpu.wgslLanguageFeatures` has no depth feature.
- **Meanwhile:** the engine writes `WithDepthGreater` where its depth only grows, and draws as `WithDepth` does.
- **Status:** open: when WGSL and naga can say it, the types emit it and the host asks for it.

### A struct update from a borrowed struct doesn't compile

- **Needed:** a span of the first `n` items of an append buffer (`GpuSpan { count: n, ..a.items() }`), to copy a realized mesh's used part into a mesh fitted to it (`engine::realize::fitted`).
- **What happens:** toolchains 0.6.2 and 0.6.3 stop with an internal error, `error[I0001]: the CPU module's IR is malformed: fn5 first_5: v3 is a *GpuBuffer<u32>, not a GpuBuffer<u32>` ("this is a bug in the wrela compiler"). The struct update's base is a borrow struct (`GpuSpan`), and the copied field is a borrow; naming every field (`GpuSpan { buffer: s.buffer, start: 0, count: n }`) compiles.
- **Found:** 2026-10-25, fitting the Last Green's built things (B13's review).
- **Evidence:** eight lines: `fn first(a: AppendBuffer<u32>, n: u32, to: mut GpuBuffer<u32>) { copy(GpuSpan { count: n, ..a.items() }, to.items_mut()) }`, called from `init`.
- **Meanwhile:** `fitted` names the span's fields.
- **Status:** open.

### No 64-bit atomics on the GPU

- **Needed:** the software rasterizer's single-pass variant (docs/plan-world-class.md §B7b): one `atomicMax` of a 64-bit word holding a fragment's depth and its cluster's id, so a pixel's nearest fragment wins in one operation.
- **What happens:** toolchain 0.6.0 has `Atomics<u32>` and `Atomics<i32>` only; `std::gpu::features().atomic64_min_max` is always false.
- **Found:** 2026-10-24, building 0.6.0's optional features (§A10). Neither WebGPU's core nor Chrome stable exposes a 64-bit atomic feature, and wgpu 30's `SHADER_INT64_ATOMIC_MIN_MAX` has no browser counterpart, so a program couldn't rely on it in a browser.
- **Meanwhile:** §B7b's two-pass form: an `atomicMax` of the depth's bits, then an `atomicMin` of the id at the winning depth, in `Atomics<u32>` buffers.
- **Status:** open until a browser ships 64-bit atomics.

### The native host can't be given a pixel ratio

- **Needed:** profiling a 2D game as a 2× display draws it (docs/plan-world-class.md §B10.1): the capybara draws its scene at most 1.5 times its CSS pixels (`engine::sheet::render_size`), from the environment's `pixel_ratio`.
- **What happens:** the native host's environment has no pixel ratio (`Environment::pixel_ratio` is `None`), and neither `wrela-host` nor `wrela profile` takes one, so a native profile at 2940×1912 draws the scene at the device's full size, as a 1× display.
- **Found:** 2026-10-25, gating §B10.1 at 2940×1912.
- **Meanwhile:** the native profiles measure the 1× case (the larger), and the saving a 2× display gets is a browser's; the B10 gate's span is native, so it understates it.
- **Status:** open: a `--pixel-ratio r` for `wrela-host` and `wrela profile`, which the environment then answers.

### A lifted build takes a storage buffer from every pipeline

- **Needed:** the course lens (§B10.10) lifts the capybara's course in a build that reaches the engine, whose software rasterizer bound eight storage buffers (WebGPU's most).
- **What happens:** a lifted build binds its literal table last in every pipeline (spec/language.md §23.1), also in the pipelines that read no lifted literal, so a kernel at eight buffers fails the lifted build (E0602, "binds 9 storage buffers") though the normal build passes. The error doesn't say the ninth is the table.
- **Found:** 2026-10-25, building the course lens on the capybara.
- **Meanwhile:** the engine's kernels keep to seven storage buffers (the soft rasterizer's depth and id are one buffer).
- **Status:** open: bind the table only in pipelines whose code reads a lifted literal, or say in E0602 that one of the buffers is the lifted table.

### A kernel can't start its dispatch at an offset

- **Needed:** a cook that writes part of a texture or a buffer: a slab of the distance volume (docs/plan-world-class-2.md §B9b), the sights of a strip of probes (§B9a), a band of a fixture's readback (§B14.2).
- **What happens:** `Texels` and `Texels3d` store only the invocation's own texel, and `Slots` only its own element (by `GlobalId`); a dispatch's ids start at 0. A slab of z from 168 to 192 is a dispatch over z from 0 to 192 whose first 168 slices return at once; a buffer's part is a `span_mut` at a 256-byte-aligned offset, which a texture has no counterpart of.
- **Found:** 2026-10-26, writing `engine/distance.wrela` and `engine/probes.wrela`'s sights.
- **Meanwhile:** the distance volume's slab dispatches reach from z 0 (its earlier slices return); the probes' sights live in a buffer, not the maps texture, partly for this.
- **Status:** open: a dispatch's id offset (`dispatch(k, over: n, from: (x, y, z))`), or a store at a computed texel.

### No 4 × 4 matrix inverse in std

- **Needed:** a built thing's tape read in its own space from render space (§B9b's `Shape::to_local`).
- **What happens:** `std` has `Quat::inverse` but no `mat4` inverse.
- **Found:** 2026-10-26, writing `engine/distance.wrela`.
- **Meanwhile:** `distance::rigid_inverse` inverts a rotation and a translation by hand (the built things' transforms are rigid).
- **Status:** open.

### A closure can't capture a function-typed parameter

- **Needed:** a helper inside `blockouts::realize` that reads the ground through the `height` function it was given (§B11d).
- **What happens:** E0702 "a closure can't capture a function-typed parameter yet".
- **Found:** 2026-10-26.
- **Meanwhile:** a plain function taking what the closure would have captured (`rock_look(height(at), water, seed)`). D1's curvature (`substance::field_shape`, a gradient of a gradient of a field) hit it too (2026-10-30): it takes a `std::field::Surface`, whose `distance` its closures call.
- **Status:** open.

### A struct update of the floor's stage is malformed IR

- **Needed:** the Last Green's `init` builds its stage with one field changed: `Stage { neural: NEURAL, ..stage() }`.
- **What happens:** toolchain 0.9.0's build stops with an internal error: "the CPU module's IR is malformed: fn0 `init_0_export`: v1862 is a `Cast<Coat, Body<…>>`, not a `Cast<Coat, Grown, FawnLook>`": the update's copied fields reach a cast of another field's type.
- **Found:** 2026-10-26, adding `Quality::neural` (B14.2c).
- **Evidence:** `wrela build games/last-green` with that line in `games/last-green/main.wrela`'s `init` (the stage holds the floor's creatures' generic bodies).
- **Meanwhile:** `init` makes the stage and sets the field after (`var staged = stage()`, `staged.neural = …`).
- **Status:** open: partly fixed in 0.10.0 (plan three's §A4). Cut down to 38 lines (the compiler's `struct_base_revealed_accept` conformance test, a struct generic over a return-position trait type updated from a function's result), whose cause was `lower_ty` naming an IR struct by the type it was asked for: a type now lowers as the type it reveals, and that program builds. The floor's own update built on 0.10.0 too (B0, 2026-10-28: the same 175 pipelines and shipped files as without it, G-pixels within the floor's noise), until the floor's tree kinds moved to `content/trees` (§B1.1): then `wrela check games/last-green` stops with "the CPU module's IR is malformed: fn0 `init_0`: v136 is a `Cast<Coat, Body<…>>`, not a `Cast<Coat, Grown, FawnLook>`". So a type is still lowered unrevealed somewhere that depends on the order things are lowered in. The floor keeps setting the field after; for 0.10.1.

### A driven export can't return a value of varying length

- **Needed:** `wrela tool export -- water` (docs/plan-world-class-3.md §B7) takes a game's water whole: its meres' shores and its courses, each a list of points of its own length.
- **What happens:** `std::tool::drive`'s `Step::Call` returns its export's results as `Ran.results`, a `Vec<Scalar>` flattened from a fixed-size value, so an export can't return a list, a string or bytes.
- **Found:** 2026-10-28, writing the export's `water`.
- **Evidence:** `std::tool::drive::Ran`; the floor's `test_course_points` returns a `CourseRow { p: [vec4; COURSE_ROW] }`.
- **Meanwhile:** the floor pages its water out in fixed rows (`test_water_count`, `test_mere`, `test_shore`, `test_course`, `test_course_points`), the row's size given by the game (`WaterCount::row`), and the tool batches the calls.
- **Status:** open: a `Call` that returns text or bytes (the game's water as one JSON).

### A declared tool's roots stay inside the workspace

- **Needed:** `studio/compare` (§B4) reads the references from `$WRELA_REFERENCE_DIR`, outside the repository (docs/citations.md's private-reference rule).
- **What happens:** a tool declared in `wrela.toml` is granted roots only inside the workspace, so `wrela tool compare` can't be given the references' directory.
- **Found:** 2026-10-28, writing the compare tool.
- **Evidence:** the declared tool's `--read refs=…` outside the workspace is refused.
- **Meanwhile:** the tool runs as a package with the directory granted on the command line: `wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR -- …` (docs/lookdev.md).
- **Status:** open: a declared tool's grant of a directory outside the workspace, named on the command line.

### An entry point's parameters that its shader never reads are still bound

- **Needed:** a switch that's off binds nothing of its path (docs/plan-world-class-3.md §C1): the clearing's shading, its fog off, shouldn't bind the froxels' volume.
- **What happens:** toolchain 0.10.0 folds a switch's code out of a shader (an associated constant, or a `bool` argument once inlined, give the same WGSL), but every parameter of the entry point stays a binding: a group's fields (`lit::Lighting`'s `fog`, `fogged`, `distance`, `volume`) are declared in the WGSL and bound by the command whether the folded shader reads them or not.
- **Found:** 2026-10-28, C1, a scratch package: `fill<Q>(t: [u32], ...)` whose `Q::ON` false branch is the only reader of `t` still declares `var<storage> t`.
- **Evidence:** the scratch package's two pipelines, `fill::<Yes>` and `fill::<No>`, each with three bindings; the second's code has no read of `t`.
- **Meanwhile:** the frame binds the off paths' small stand-ins (a 1 × 1 × 1 froxel volume, the distance volume's), so the cost is the bindings alone.
- **Status:** open: an entry point's binding that no code of its pipeline reads left out of its layout and its command.

### A generic method can't take one type argument and infer the rest

- **Needed:** `Scene::prepass<P: RenderPass, Q: Switches>(...)` called as `scene.prepass::<_, Q>(pass, ...)`, so the far crowns' cut prepass (`impostor::far_cut_flat`, which takes no lighting to carry `Q`) folds the neural read-through as their shading does.
- **What happens:** toolchain 0.10.0 takes a turbofish whole or not at all: `_` is "expected a type", and a turbofish with fewer arguments is E0322.
- **Found:** 2026-10-28, C1.
- **Evidence:** `twice::<_, Yes>(3)` (E0100: "expected a type, found `_`") and `twice::<Yes>(3)` for `fn twice<Q: Sw, T: Copy>(t: T)` (E0322: "takes 2 generic arguments, not 1").
- **Meanwhile:** the cut prepass reads the wood's net at run time (`Neural::on`, 0 unless a scene gives the wood one, as only the floor's `NEURAL` build does); the lit far shading folds it (`Switches::NEURAL`, through `Lighting<Q>`).
- **Status:** open: `_` in a turbofish, inferred as an omitted argument is.

### A tool's check lists the pipelines a constant's branch leaves out

- **Needed:** `engine/tests/frame-pipelines` counts the frame fixture's pipelines as `wrela pipelines` and a build do.
- **What happens:** toolchain 0.10.1's `std::tool::toolchain::check` lowers without the fold a build makes (`Mode::CHECK`, neither `emit` nor `conditions`), so its `pipelines` hold every branch's, a switch's that's off too: the fixture's 65 against its build's 58 (C1: its sky's `sky_shade`, the fogless branch, and the six the fold drops).
- **Found:** 2026-10-28, C1.
- **Evidence:** the test's check said 65; `wrela pipelines engine/tests/frame` and its build's manifest, 58.
- **Meanwhile:** the test builds the fixture and reads its manifest (as `engine/tests/switches` does).
- **Status:** open: a check's pipelines as a build makes them, or a `CheckOptions` that asks for that.

### A cold start's pipelines compile at the GPU process's own pace

- **Needed:** a cold start (no pipeline in any cache) under 5 s (docs/plan-world-class-3.md §C5): the pipelines the load needs built before it needs them.
- **What happens:** the browser compiles a few pipelines at a time in its GPU process (Chrome on Metal: 17 small ones asked for at once all finished together, 1.5 s later), whatever the host asks. Neither the order the host asks in (largest first, smallest first, both ends, the order the program first uses them) nor how many it asks for at once moves the vale's cold start much below 6.6 s: it needs 48 pipelines before playable, 7.5 s of compiling built one at a time. WebGPU has no way to ship compiled pipelines or to say which matter first.
- **Found:** 2026-10-29, C5 (test mode's `starts.json`, 0.10.2).
- **Evidence:** `wrela tool bench -- --scene vale-walk --starts 3`, cold, playable ms: 8029 with every tier-0 pipeline asked for at once, 6691 six at a time (0.10.2). Single runs: smallest first 6066, alternating ends 6601, in a native run's first-use order 6653.
- **Meanwhile:** fewer and smaller pipelines before playable: the covered frames don't draw the scene (C5a), and C1's switches leave a program's unused paths out.
- **Status:** open: nothing in the toolchain can fix the browser's pace. What's left is the program's: smaller shaders on the load's path (the floor's creature-field realizer, 160–230 KiB of WGSL with its dual-number gradients; the terrain's `cook_level`, 140–166 KiB).
- **More evidence (2026-10-30, the capybara game's nine habitats):** each habitat's backdrop strips are a pipeline of their own (a pipeline is at most 256 KiB, and the eight together are far more), so the game went from 10 pipelines to 17, of which a level's first frame needs 9. Cold, in headless Chrome on Linux (`wrela tool bench -- --scene capybara-run --starts 3`, `starts.json`): every pipeline built by 3,036 to 3,051 ms against `main`'s 1,829 to 1,845 ms, and the first frame waits for its sprite pipeline, built last both times. Warm, every pipeline by 75 to 80 ms. The seven strips no first frame needs are compiled before the one it does.
- **Needed too:** a way for a program to say which pipelines can wait (built after its first frame, or when first drawn): the capybara's other habitats' strips are drawn only when the player goes to another level. Not tried among the orders above.

### A program's `playable` comes before its first frame is on the screen

- **Needed:** the bench's cold `playable` time to be when a player can see and play: the frame on the screen.
- **What happens:** a program prints `playable` from its frame's CPU code; the host then waits for the frame's pipelines before it submits the frame. On a cold start the capybara prints it at 317 ms (median of 3) while its first frame waits for its sprite pipeline until about 3,000 ms (`starts.json`). Warm, the two agree.
- **Found:** 2026-10-30, measuring the capybara game's start.
- **Meanwhile:** read a cold start's `starts.json` with its `playable` time: the frame is on the screen once every pipeline its `waits` name is built.
- **Status:** open.

## Fixed

### A program can't be opened at an address with a fragment

- **Needed:** the capybara's attract start in its play build, opened as `capybara/#attract` and read with `host::address` ("A program can't show another program playing behind its own canvas", fixed), so the console fetches one build of the game, ahead of time, for both.
- **What happens:** toolchain 0.16.0's browser host refuses to open it: `host::open`'s URL goes through `programUrl` (`runtime/browser/src/platform.ts`), which throws "program URLs cannot contain credentials, queries or fragments" for any `#` or `?`, so the open fails.
- **Found:** 2026-10-31, building the console's attract mode (docs/plan-online-play.md step 9).
- **Evidence:** wrela-compiler 36156fa, `runtime/browser/src/platform.ts` `programUrl`: `if (url.username || url.password || url.hash || url.search) throw ...`, called by `#open` before the frame is made.
- **Meanwhile (until 0.17.0):** the attract start was a build of its own: the capybara's package with `ATTRACT` set (`--set ATTRACT=true`), which the release builds beside the play build (studio/release/catalog.wrela's `Build::set`). It cost a second build of the game in each release, and its files fetched apart from the play build's.
- **Status:** closed by toolchain 0.17.0: `host::open` takes an address with a fragment (`capybara/#attract`; no query, credentials or `#test`), and the opened program reads it, its fragment kept, with `host::address`. Done with it: the separate `attract` build is gone, and with it `Build::set` and its `NAME=VALUE` check in studio/release (nothing else set constants); the catalog's `attract` names the play build and its fragment (`capybara#attract`, written in `catalog.tsv` as `.../capybara/#attract`), and `check` holds its build to one of the game's and its fragment to a plain token that isn't `test`. The capybara starts neutral (a frame of sky, no sound, nothing stored or asked), and once its address answers decides once whether it plays itself (games/capybara/attract.wrela's `Role` and `role_of`); playing itself, the frames never start its voice, its storage, the world's boards, its keys, its semantic tree or its link to the console. The console fetches one build of the game for both.

### A program can't show another program playing behind its own canvas

- **Needed:** attract mode on the console's home (Ryan, 2026-10-30: "I really like the idea of the game playing by itself behind the home screen"): the chosen game's own build, playing one of its replays with no input and no sound, behind the home's text in place of its hero. The capybara game can: every run is a replay (`games/capybara/replay.wrela`), its searched runs are constants (`routes_<level>.wrela`), and a start of its own (`#attract` in its address, read with `host::address`) could play one on a loop.
- **What happens:** toolchain 0.11.1's browser host (as 0.10.3's) lays each program a page opens (`host::open`) in an iframe filling the page *behind* the opener's canvas (`runtime/browser/src/platform.ts` `#open`: `position:fixed;inset:0;z-index:0;pointer-events:none`, under the canvas's `z-index:1`), so it is drawn already, but the canvas is configured opaque (`runtime/browser/src/worker.ts:121`, `ctx.configure({ ..., alphaMode: "opaque" })`), so nothing behind it can show: a screen pass's alpha is dropped. `host::show` is the only way to see a surface, and it hides the opener (`surface/show`: `this.canvas.style.visibility = "hidden"`). The native host opens no surfaces at all.
- **Found:** 2026-10-30, redesigning the console's home (the PR that adds hero art, title art, taglines and themes to the catalog).
- **Weighed:**
  - *The game's build behind the console's canvas* (this entry): the game's own renderer, at the device's size and rate, for the cost of a few KB of catalog; the build is the one the player is about to play, so its files are fetched ahead anyway. It needs the toolchain change below, and a start of the game's own.
  - *A clip the release renders* with the game's renderer (`wrela-host --png --every K` frames, as the lookbook does): no video decoder in std or the hosts, so it would be a flipbook of `host::decode_image` frames: 6 s at 12 frames a second at 960×540 is 72 textures, about 150 MB of GPU memory and 3 MB to download, blurry behind a 1080p screen. Rejected.
  - *A `<video>` the host plays*: an encoder in the release (none in `std::tool::image`) and the same transparent canvas. No better than the game itself.
- **Proposed:** `host::underlay(id, on)`: the surface `id` (opened, not shown) is drawn behind this program, which is now composited with premultiplied alpha (the canvas reconfigured `alphaMode: "premultiplied"`, the screen pass's alpha kept), the surface getting no input and no audio (muted, or told so with its `environment`, as reduced motion is); `on: false` puts both back. The opener draws its screen with alpha where the game should show (the console: its hero's gradient over a clear screen). A frame budget should stay the opener's: a host that strains (`strained` in console/main.wrela) closes the surface.
- **Meanwhile:** until 0.16.0, the home showed the game's hero, a still the release ships (`catalog.tsv`'s hero column), drifting a little with the pointer. Nothing was worked around: no flipbook, no frame of the game's in the console.
- **Status:** closed by toolchain 0.16.0's `host::underlay` (the browser host's `surface/underlay`; the program behind muted and given no input, the canvas composited premultiplied). Built with it (docs/plan-online-play.md step 9; the look is proposed until Ryan sees it): `catalog.tsv`'s last column names each game's attract build (studio/release/catalog.wrela's `Game::attract`, empty for none; a catalog without the column reads as none); the console (console/attract.wrela) opens it once the home has rested on a game three seconds with nothing fetched ahead under way, waits for its first frame (`host::wait_ready`), underlays it and fades the hero out over a second, the backdrop's alpha following the fade and its dark gradient kept over the game (console/paint.wrela); it closes as the selection moves or a sheet or the account's panel covers the home (the hero fading back in), and at once when a game starts or shows or the host strains (`strained`). A host that declines (the native one) or a build that fails turns it off for the page's load, and the home is as it was. The capybara's attract build (games/capybara/attract.wrela) plays the route search's human any% run of each level (`HUMAN_ANY`), level after level and round again, with no HUD, menus, sound, input, storage or network. Until 0.17.0 it was a build of its own (`--set ATTRACT=true`), not `capybara/#attract`: 0.16.0's browser host opened no program at an address with a fragment ("A program can't be opened at an address with a fragment", fixed); since, it is the play build opened at `capybara/#attract`.

### A fragment can't return its look's associated type

- **Needed:** one shading shader for every look (docs/plan-world-class-3.md §D1): the scene's colour alone for a look that reads no substance (`type Out = vec4`), and a `Targets4` of it and the substance for one that reads it, so the physical look's frame and shaders are as they were and the others' passes write 24 bytes a sample.
- **What happened:** toolchain 0.10.3 checked a fragment's return type before instantiation, so `-> Y::Out` was E0602 ("returns a colour …"); and an `if` on an associated constant lowered both branches, so the physical look's instance of the four-target branch was a draw of one colour into a pass of four (E0603), though no instance ever took it.
- **Found:** 2026-10-30, D1.
- **Evidence:** `compiler/sema/src/gpu.rs`'s fragment check; `compiler/lower/src/body.rs`'s `region` lowered an `If`'s two branches whatever its condition.
- **Meanwhile:** nothing: fixed before D1 landed.
- **Status:** fixed in 0.11.0: the return type is checked per instantiation, and an `if` whose condition folds (a constant, or comparisons and logic of constants, in the condition itself) lowers only its taken branch.

### A build can't choose a program's look

- **Needed:** the vale built in each look (§D1's costs, D4–D6's stills, D10's `wrela tool styles --check`) from one `main.wrela`.
- **What happened:** `--set` set a `pub const` only; a look is a type (`Frame<R, Q, Y: Style>`), and its fragments' output type is the look's (above), so an enum matched at run time can't choose it.
- **Found:** 2026-10-30, D1.
- **Meanwhile:** nothing: fixed before D1 landed.
- **Status:** fixed in 0.11.0: `--set LOOK=styles::fixtures::Every` points the program's `pub type LOOK` at another type.

### A warm page doesn't time its pipelines

- **Needed:** the bench's warm run (§B1.4) reports `warm_pipelines_ms` beside `warm_playable_ms`.
- **What happens:** test mode writes `results/pipelines.json` only for a salted run (`salt=<n>`), so a warm run, which mustn't be salted, has no pipelines' time.
- **Found:** 2026-10-26, adding the warm run.
- **Evidence:** runtime/browser `worker.ts`: `params.salt > 0 ? putResult(base, "pipelines.json", ...) : null`.
- **Meanwhile:** the warm row's pipelines column is 0, and the report says so.
- **Status:** fixed in 0.10.2 for what a start waits for ([rywible/wrela-compiler#41](https://github.com/rywible/wrela-compiler/pull/41)): every run writes `results/starts.json`, with when each pipeline was built and each time a command waited for one. `pipelines.json` is still a salted run's.

### A branch on an associated constant stays past the CPU inliner's budget

- **Needed:** each switch a compile-time constant of the program's type (docs/plan-world-class-3.md §C1, `engine::switches`): `if Q::CASCADES { ... }` with the program's `CASCADES` false builds none of the radiance cascades.
- **What happens:** toolchain 0.10.0 makes an associated constant a function, and a branch on its call folds only once the call is inlined; the CPU's inliner stops at its budget (4,000 statements a function), so late in a long function (the frame's `draw_scene`) the call stays, and with it the branch and every pipeline behind it. Early in the same function the same constant folded.
- **Found:** 2026-10-28, C1: the clearing (`CASCADES`, `DISTANCE` false) still listed the cascades' `trace`, `merge` and `resolve` and the distance volume's `cook_distance`; the vale and the bare fixture, whose frames are smaller, didn't.
- **Evidence:** `wrela pipelines examples/clearing`: 117 pipelines with them; `if false` in their place, 115; `if Q::CASCADES` moved to guard the fog's call (early in the function) pruned the fog's.
- **Meanwhile:** none needed: fixed in the toolchain.
- **Status:** fixed in 0.10.1 ([rywible/wrela-compiler#40](https://github.com/rywible/wrela-compiler/pull/40), `compiler/lower`'s `assoc_value`): an associated constant is lowered as its value where its initializer folds, so its branch folds before inlining as a literal's does. The clearing went to 113 pipelines; the compiler's `pipelines::a_draw_behind_a_false_associated_constant_is_no_pipeline` holds it.

### A texture can't be a field's data

- **Needed:** the cascades' march (§B14.4) generic over `std::field::Surface`, the distance volume one such field.
- **What happens:** `Surface` needs `Copy + GpuData`, and a `Texture3d` is neither; a `borrow` field can't be part of an ordinary struct (E0519).
- **Found:** 2026-10-26, writing `engine/cascades.wrela`.
- **Meanwhile:** the march is generic over analytic fields (its tests); the volume's march repeats its steps over `distance::distance_at`.
- **Status:** fixed in 0.10.0 (rywible/wrela-compiler, plan three's §A3): `std::gpu::Texture3dHandle<F>` is a 3D texture's handle as `Copy + GpuData` data (`t.to_handle()`), and `std::field::Sampled3d` is a `Surface` over an `R32Float` texture's texels (trilinear, with its own gradient). An entry point that takes a value holding one in a struct's fields binds the texture beside it, so `march<S: Surface>` takes a cooked field as it takes an analytic one. Limits: GPU code builds such a value only as a struct literal, a closure can't capture one, and a handle in a tuple, an array or an enum is E0602.

### A package's const cache can outlive what it was computed from

- **Needed:** a test run that reads what the sources say now.
- **What happens:** with toolchain 0.9.0, `studio/tool`'s `the_server_serves_chrome` failed twice in this checkout ("a page opened after the failed build has the last good one") and passed in a fresh worktree of the same commit. Removing the package's `build/consts/` (512 `.wcv` entries, some from 2026-10-21, before the 0.9.0 pin) made it pass here too. A cached constant from an older build seems to have been read in place of the one the current sources give.
- **Found:** 2026-10-26, in the final `wrela suite` of plan-world-class-2's Part B.
- **Evidence:** `wrela test studio/tool the_server_serves_chrome` before and after `rm -rf studio/tool/build`, on commit cab08093.
- **Meanwhile:** clear a package's `build/` when a test fails here and passes in a clean checkout.
- **Status:** fixed in 0.10.0 (plan three's §A5): a constant's key is now its module's WASM (the initializer and all it reaches, with the constants it reads), its fuel, the build's options, the bytes of each file it embeds, the result type's schema and the toolchain's version, and the cache is `build/consts/<version>`, so another version's entries are never read. `wrela build --report consts` says which constants were computed and which cached.

### A loop body's last call leaked its temporaries

- **Needed:** the lookdev instrument (`wrela tool lookbook --quality`) adds each of a run's thousand frames to running sums: `for _ in 0..SAMPLES { still.add(luma(d.screen()?)) }`.
- **What happens:** toolchains up to 0.8.0 never freed the temporaries of a call that ends a loop's body (its value): each pass kept its `luma` and its screen, about 400 MB a run at 320x180, and the tool ran out of its heap after one shot. Every `for ... { print(f"...") }` leaked its string the same way.
- **Found:** 2026-10-26, the first `--controls` run.
- **Evidence:** the MIR of a loop ending in `sum_into(mut t, made(n))` has no `Drop` of `made(n)`; `end_loop` popped the loop's scope, which held it, without dropping.
- **Status:** fixed in 0.8.1 (rywible/wrela-compiler#37): a loop's body has its own temporaries' scope, ended with each pass.

### A recorded indexed or multi-draw names no pipeline

- **Needed:** the pass viewer names each pass's pipelines (§B1.5).
- **What happens:** toolchain 0.8.0's recorder (`Recorded::of`) fills `pipeline` and `bindings` for `Draw`, `DispatchIndirect`, `DrawIndirect` and `DrawIndexedIndirect` only; a `DrawIndexed` or `MultiDrawIndirect` comes with an empty pipeline, so the Last Green's terrain and late vegetation passes list none.
- **Found:** 2026-10-26, the first `wrela tool passes` of the Last Green's frame 300.
- **Evidence:** `runtime/native/src/tool/drive/child.rs` `Recorded::of`'s match.
- **Meanwhile:** those passes show their draws with no pipeline.
- **Status:** fixed in 0.8.1 (rywible/wrela-compiler#37): `Command::pipeline()` names every draw's and dispatch's pipeline, and the recorder reads it.

### A branch under a false constant still makes its pipelines

- **Needed:** an experimental path behind a switch that costs a build without it nothing: the Last Green's banks drawn through clusters (`games/last-green/stage.wrela` `CLUSTERED`, false), whose 17 pipelines a player without it shouldn't build.
- **What happens:** toolchain 0.6.3's pipeline reachability doesn't fold constants: code under `if CLUSTERED { ... }` with `CLUSTERED` false still reaches its kernels, so the build lists their pipelines (`wrela pipelines games/last-green`: 171 with the path, 154 without) and the page builds them at load.
- **Found:** 2026-10-25, the council's third round on B7c.
- **Evidence:** the Last Green's opening, cold, in Chrome, two alternating pairs: playable 9.90 and 10.08 s without the path's code, 10.14 and 10.12 s with it switched off (+0.14 s); its pipelines 4.44 and 4.39 s against 4.49 and 4.33 s; its first frame +35 ms.
- **Meanwhile:** the cost is measured and kept: the path stays in the build, off.
- **Status:** fixed in 0.8.0 (rywible/wrela-compiler#36, §A6): constants fold after inlining, so what only a false constant's branch reaches isn't built. The Last Green's build went from 130 pipelines to 115 (the 15 behind `CLUSTERED` gone, as `wrela pipelines` counts them), and a pipeline reached only under a runtime condition is built second (§A5).

### A timed ask timed all of a program's work

- **Needed:** the planner measuring this device's costs by default (docs/plan-world-class.md §B2, D1): the frame asks `std::gpu::timed` for five labels' times (the clouds, the cascades' strips, the probes' occluders and bake).
- **What happens:** in toolchain 0.6.1 a program's first ask starts a timer that times every pass and dispatch from then on, on both hosts: the browser gives each timed dispatch a compute pass of its own, and the native host reads every frame's timestamps back before the frame call returns. Measuring five costs paid for timing all the work.
- **Found:** 2026-10-25, benching B13: the clearing's paced 95th percentile in Chrome went from 18.4 ms (B1) to 20.7-29.7 ms with B2's measuring on, and the native frame call from 9.4 to 12.2 ms. With measuring off it was 18.3 ms; with the asks made and their answers ignored, 22.7 ms.
- **Meanwhile:** nothing: the fix shipped the same day.
- **Status:** fixed in 0.6.2 (rywible/wrela-compiler#33): outside a run timed from its start, a host times only the work of a label asked for and not yet answered, from the ask on, so the frame asks at its start (`Frame::measure`). The clearing's paced 95th percentile is 18.1 ms, the native call 9.44 ms. In 0.6.2 the browser took an ask as made when its request started, so while commands waited for a pipeline the label's work recorded before it was timed too; 0.6.3 (rywible/wrela-compiler#34) takes it where its `Timed` command runs in the stream.

### A mip step's load didn't compile

- **Needed:** the depth pyramid (docs/plan-world-class.md §B4b): a kernel reads the level before through `MipStep::load` (unfiltered: the farthest of a cell's depths) and writes the next.
- **What happens:** toolchain 0.6.0 refuses any kernel that calls `MipStep::load`: `error[I0001]: the IR for ... is malformed: ... Load with the wrong arguments` (the lowering gave the texture load no level; `MipStep::sample` passed level 0). Every format alike.
- **Found:** 2026-10-24, building `engine::cull`'s pyramid.
- **Meanwhile:** Part B built against rywible/wrela-compiler's `release-0.6.1` branch (`WRELA_TOOLCHAIN`) from §B4b until the release.
- **Status:** fixed in 0.6.1 (rywible/wrela-compiler#32): the load passes level 0 of the step's view, and the kernels suite tests a 5×3 `R32Float` chain read back.

### A pointer can't press a semantic tree's button, and a tick can't find what the frames drew

- **Needed:** a ticked game's pause menu that a mouse works as the keys and a controller do. The menu must be the sim's, so a restart chosen in it replays from the tick log; then a click on an item has to reach the sim as which item it was.
- **What happens:** a tick's records hold a pointer press in screen pixels, but not the screen's size or anything the frames drew, so the sim can't tell which item is under it. The semantic tree (`std::semantics`, `std::host::publish`) would say so, as `Event::Assist` on the item's node, but toolchain 0.3.3's browser host lets a pointer press only a link with an `href` (`.wrela-semantics a.wrela-node.wrela-hit { pointer-events: auto }`, `runtime/browser/src/semantics.ts`). A `Button` or a `MenuItem` is under the canvas's pointer events: the keyboard's Tab and a screen reader press it, a mouse can't. And a link pressed keeps the keyboard's focus in the overlay, where key presses don't reach the program.
- **Found:** 2026-10-23, building the capybara game's menus (`games/capybara/world.wrela`, `screens.wrela`).
- **Meanwhile:** the capybara game publishes each page's items as links to anchors of the page (`#1-0`), which a pointer presses; and it numbers each item's node anew after every choice (`world::node`), so the host lets go of the pressed node and the keys come back to the game. A screen reader hears the items as links, not menu items.
- **Status:** fixed in toolchain 0.5.2 (rywible/wrela-compiler#29, #30): a pointer presses every node a press activates (a button, a checkbox, a switch, a menu item, a link with an address) where it's drawn, a mouse press doesn't take the keyboard's focus, and a pointer's click hands it back to the canvas. Over an option or a tab the pointer is the program's, as before: 0.5.1 gave it to those too, and the docs' search results stopped opening on a click. Checked in the Claude app's browser pane: a mouse press on the capybara's `Start` starts the game, and the keys move it after. The capybara game's items are `MenuItem`s. They're still numbered anew after each choice: the count is in the sim's state, which recorded runs hash.

### A ticked program doesn't start in the Claude desktop app's browser pane

- **Needed:** an agent playing a ticked game in the browser pane beside the session (and anyone in an embedded Chrome).
- **What happens:** the page stops with "wrela stopped: the ticker's thread failed: it couldn't start". The page is cross-origin isolated, with `SharedArrayBuffer` and WebGPU (the pane's Chrome is 152).
- **Found:** 2026-10-21, with `games/capybara` and `examples/climber-lab`, under `wrela run` and as a static build (`wrela tool serve`). Both fail alike.
- **Likely cause, not confirmed:** the runtime starts the ticker as a module worker from inside the render worker (`new Worker(self.location.href, { type: "module" })` in `worker.js`), and the pane's error event carries no message. Nested module workers may be unavailable there.
- **Meanwhile:** play in Chrome itself. In-Chrome play of the capybara lab is still to be checked: a headless Chrome run in test mode ended before writing results, so it says nothing either way.
- **Status:** fixed in toolchain 0.5.1 (rywible/wrela-compiler#29), which the pin's 0.5.2 holds. The cause, confirmed in the pane: it starts no nested worker, module or classic, and the error event has no message; the page starts one fine. Helper threads (more than one worker) failed alike. The page now starts the render worker's threads and hands each its channel. `examples/climber-lab` and the capybara game run in the pane.

### `wrela trace` and `wrela bisect` don't run a program's ticker

- **Needed:** an agent watching a ticked game's state frame by frame (`wrela trace <package> --watch <export> --input script.json`), and finding the first frame a condition holds (`wrela bisect`).
- **What happened:** the ticker never ticks. Exports that read the sim's snapshots stay at the first world's values for every frame.
- **Found:** 2026-10-21, building the capybara movement lab (`games/capybara`). With the run script, `wrela trace games/capybara --watch capybara,clock` printed tick count 0 for 120 frames, with or without `--no-gpu` and `--workers 2`. The same build, under `wrela-host --frames N --input`, and as a frame test (`@test(frames: 180, gpu: true, input: ...)`), ticks two ticks a frame in lockstep.
- **Cause, from the source:** `compiler/cli/src/trace.rs` steps each frame with the host's `frame()`; the native host's lockstep path (`lockstep_frame` in `runtime/native/src/program.rs`) is what runs the ticks due before a frame.
- **Meanwhile:** traces of the capybara's movement come from tests that step the sim directly; stills come from `wrela-host`.
- **Status:** fixed in toolchain 0.4.1 (rywible/wrela-compiler#27): each frame of a trace or a bisect runs in lockstep with the ticker, as `wrela-host` and frame tests run frames. The toolchain's `trace` suite checks a 60 Hz ticker's tick is `i` at frame `i`.

### A program can't read its own GPU pass timings

- **Needed:** the frame's planner (`engine::cook`) plans each kind of work by its cost on the device it runs on.
- **What happened:** the hosts timed each `label` for `wrela profile`, but std had no timestamp query a program could read. The planner's costs were the Chrome measurements the code carries, and most were estimates (`Evidence::Estimated`).
- **Found:** 2026-10-23, building the planner for the engine's re-architecture.
- **Status:** fixed in toolchain 0.4.0: `std::gpu::timed(label) -> Pending<f32>`, the GPU time of a label's work in microseconds, on both hosts (command stream v11). A frame with `Quality::measured` times the kinds it plans after each frame that ran one step of them, and the planner plans with the device's costs (`Planner::measure`; engine/tests/frame `the_planner_measures_this_devices_costs`). It's off by default, since measured budgets make a frame's order differ from run to run.

### A golden can't be of a pass's target, and a failed one shows nothing

- **Needed:** an engine change that must not change an image, checked by a test against a blessed PNG: the screen, or a target named by its pass's `label`.
- **What happened:** toolchain 0.3.3 compared a GPU frame test's screen with a golden (`golden:`, `within:`, `wrela test --bless`), but not a pass's target, and a failure said only how far the screen was. (The re-architecture's plan first logged this as no golden test kind at all; that was wrong.)
- **Found:** 2026-10-23, gating the re-architecture's commits frame by frame.
- **Status:** fixed in toolchain 0.4.0: `pass: "label"` compares that pass's target (floats clamped, depth grey), and a failure under `wrela test` writes what it saw and each pixel's difference under the package's `build/goldens/`. engine/tests/frame has a golden of the bare scene and of the near cascade.

### A test can't hold a request until it releases it

- **Needed:** the frame's transitions tested where a fetch or a storage read comes late.
- **What happened:** a test could only wait frames, and whether a request had answered by a frame depended on the host.
- **Found:** 2026-10-23, writing `engine/tests/frame`.
- **Status:** fixed in toolchain 0.4.0: `@test(frames: n, hold: [("path", k)])` holds a path's fetches and storage reads until frame `k`, and `std::tool::drive` has `Step::Hold(path)` and `Step::Release(path)`. No engine test uses it yet: the Last Green polls its far layer only once its wood is built, past what a short GPU test of it runs, and the frame's fixture fetches nothing.

### A test can't read the build's pipelines

- **Needed:** tests that say what a build reaches ("no pipeline it didn't reach before").
- **What happened:** the pipeline counts came from `wrela pipelines` run by hand. (The recorded commands were there already, through `std::tool::drive`'s `Commands`, which the clearing's checks read.)
- **Found:** 2026-10-23, checking that the re-architecture adds no pipeline.
- **Status:** fixed in toolchain 0.4.0: `toolchain::check` answers with the pipelines (`Checked::pipelines`, each a `Pipeline`). engine/tests/frame-pipelines checks that a bare scene reaches only the frame's own 28.

### A frame test can't run on both hosts

- **Needed:** the frame's tests run on the native host and in Chrome, their state hashes and last frames compared.
- **What happened:** `engine/tests/hosts` compared the hosts by hand, a test at a time.
- **Found:** 2026-10-23, with the frame's transition fixture.
- **Status:** fixed in toolchain 0.4.0: `@test(frames: n, gpu: true, hosts: both)`. engine/tests/frame's `both_hosts_draw_the_same_frames` runs the bare scene's load, jump and weather in both: the same state hash, and the same frame within a GPU's differences.

### The browser's canvas isn't in the device's preferred format

- **Needed:** the console's opening and library cost as little as they can a frame, on any device, so the page stays cool and saves battery while it is left open.
- **What happens:** toolchain 0.3.1's browser runtime configures the screen's canvas as `rgba8unorm` (`SCREEN_FORMAT`, `runtime/browser/src/abi.gen.ts`). Chrome on a Mac prefers `bgra8unorm` (`navigator.gpu.getPreferredCanvasFormat()`) and warns: "WebGPU canvas configured with a different format than is preferred by this device ("bgra8unorm"). This requires an extra copy, which may impact performance." Every frame a program presents pays a copy of the whole screen.
- **Found:** 2026-10-22, optimizing the console, in the Claude desktop app's browser pane (Chrome 152) on an M4 MacBook Air.
- **Meanwhile:** the console presents fewer frames (it draws only what changes, at most 30 a second while idle), which pays the copy less often.
- **Status:** fixed in 0.3.3 (rywible/wrela-compiler#22): the manifest marks the screen's targets (`"screen": true`, manifest version 9), and a normal run configures the canvas in `getPreferredCanvasFormat()` and builds the screen's pipelines in it; test mode keeps `rgba8unorm`, which it reads back. Checked in Chrome on the M4 Air: no warning.

### A tool's write empties the file before it writes it

- **Needed:** the keys tool (`games/capybara/keys`) writes takes.wrela again, every take Ryan has kept, each time he keeps one; the route tool writes routes.wrela again. A write that fails must leave the file as it was.
- **What happens:** toolchain 0.3.1's `files::write` (and `write_blob`) empties the file, then writes into it (`runtime/native/src/tool/caps.rs`, `open_file` then `write_with`). A write that fails part way, on a full disk say, leaves it empty or half written, and the page says only "Not kept": the takes kept since the last commit are lost.
- **Found:** 2026-10-22, in a council review of the capybara branch, from toolchain 0.3.0's source.
- **Meanwhile:** nothing; commit takes.wrela after keeping takes.
- **Status:** fixed in 0.3.2 (rywible/wrela-compiler#21): a write goes to a new file beside the old, which then takes its place. The pin is 0.3.3, which holds it.

### A failed load can't be told from nothing stored

- **Needed:** the capybara game keeps the player's best runs in its storage (`games/capybara/main.wrela`, `store_runs`). It must store a new run over the stored ones only once it has read them, or knows there are none: storing after a load that merely failed loses every run stored before.
- **What happens:** a `load` of a path nothing was stored at and a `load` that failed (the storage couldn't be read) give the same `IoError`, with one fixed message and no kind. So the game took every failed load as "no runs yet" and stored over them.
- **Found:** 2026-10-22, in a review of the capybara branch, from toolchain 0.3.0's source (`std::io`, the hosts' `request_status`).
- **Meanwhile:** nothing; the game's next finished run could store over runs it couldn't read.
- **Status:** fixed in toolchain 0.3.1 (rywible/wrela-compiler#20): a storage read of a path that holds nothing is answered `REQUEST_MISSING` by both hosts, and `IoError::missing` says so. The game stores its runs only after it has read them or the load was `missing`, and makes a load that failed otherwise again after 2 s.

### No passkeys: no credential API, signature checks or secure random

- **Needed:** passkey accounts for the console (`games/capybara/design.md`, "Accounts and leaderboards"): a page asks the browser for a credential (`navigator.credentials.create` and `get`, WebAuthn), and the server checks each assertion's signature with the platform's crypto through std (the design forbids crypto written here), against a challenge it made from secure random bytes.
- **What happens:** std has none of it. The browser host has no credential service (language.md §17), std has SHA-256 (`std::hash::sha256`) but no signature verification (ECDSA P-256, Ed25519 or RSA, the algorithms authenticators use), and no source of secure random bytes. A room knows a peer only by a 64-bit token the peer chooses (`std::net`, `Hello { token }`; language.md §19.1), so the server can't tell who a player is.
- **Found:** 2026-10-22, planning the capybara's leaderboards and accounts, from toolchain 0.2.2's reference (`wrela doc --export`).
- **Meanwhile:** nothing here depends on accounts yet; replays, ghosts, the watch mode and the route search are local.
- **Status:** fixed in toolchain 0.3.0 (rywible/wrela-compiler#19): the browser host's passkey ceremonies (`std::host::create_passkey`, `use_passkey`), a server's randomness and signature checks (`std::crypto`: ES256, Ed25519 and RS256, by the RustCrypto and dalek crates), and the WebAuthn checks of an authenticator's answer (`std::webauthn`). Binding a connection to an account is the server program's: a connection's first message carries a session token the accounts server issued, and the server checks it against its store.

### A server has no store of its own that outlives its build

- **Needed:** leaderboards: the server keeps every verified run and its replay, serves a slice of them (a board's top runs, one replay to watch), and keeps them through a new server build. A board is tied to a build of the game's sim, not of the server.
- **What happens:** a server's only durable state is a room (language.md §19). A room's state is every peer's: each peer applies the whole log, and a peer far behind is sent the state in one 64 KiB frame, so a board of replays would go to every player and soon past the frame. And a room's journal is read only by the server build that wrote it (`<data>/<build hash>/`, §19.3), so each server deploy would start every board empty.
- **Found:** 2026-10-22, with the gap above.
- **Meanwhile:** the boards are local: a player's best runs and the route search's, kept in the game's own storage.
- **Status:** fixed in toolchain 0.3.0 (rywible/wrela-compiler#19), after Ryan chose persistent storage (SQLite on the server, backed up to a bucket): `std::store`, keys and values in key order in one SQLite file beside the rooms' journals, which every room and every server build shares. Reads are requests; writes commit when a call returns, before its records are kept and its sends go. A client asks a server for a board or a replay as a message on its connection, which the server program answers from the store; a room's log is only for what its peers share.

### A server takes one program's clients

- **Needed:** one accounts server for every wrela game, and each game's runs uploaded to it and verified there (games/capybara/design.md, "Accounts and leaderboards"): the console signs in, and a game it shows submits runs.
- **What happens:** a client's connection carries its program's pair id, a hash of that program's client and server modules, and `wrela-serve` refuses a client of another pair (language.md §15.3, runtime/serve's `Rooms::join`). So only the console's own client can reach a server the console's program builds, a game can't reach it at all, and one server program can't serve several client programs. A server also can't sign anything (std::crypto only verifies), so a token one server issued can't be checked by another server without a shared store, and two `wrela-serve`s can't share one (a data directory is for one process).
- **Meanwhile:** the console's program is the server (`console/server`, `serve_init` and `serve` in console/main.wrela), so its server build holds each game's verifier (the capybara's `boards::Capybara`), and changes with each game's sim. A game reaches the boards through the console's channel to it (`std::host::send`, `receive`, `reply`; platform/link.wrela): the console calls, the game holds the call until it has an ask, and the console carries the ask to the server. The game's token is a record in the store, not a signed token.
- **Status:** open: a server that admits named other pairs (`--pair`), or a client allowed another program's server.

### A server sees no HTTP request

- **Needed:** the account's session in a `__Host-` cookie (HttpOnly, Secure, SameSite), and each request that changes something checked for its Origin and a CSRF token (design.md, "Passkeys only, our own server").
- **What happens:** a server program sees only its rooms' messages (`Event::Message`), over WebTransport. It is never given a request's headers, can't set a cookie, and isn't told a connection's Origin: `wrela-serve --origin` refuses browsers of other origins before a room hears of them, and admits a client with no Origin (not a browser).
- **Meanwhile:** the session's token and its CSRF token are kept in the console's storage (its origin's alone, so a game on its own origin can't read them), and the token is the connection's first message (`Hello`). There is no ambient credential for another site to ride on, which is what a CSRF token guards against, and `--origin https://wrela.dev` keeps other pages' browsers out; every request that changes the account still carries the session's CSRF token, compared in constant time (platform/server.wrela). The server never learns an IP address, which the Privacy Policy relies on.
- **Status:** fixed in toolchain 0.13.0 (rywible/wrela-compiler#46), after Ryan chose (2026-10-31) plain HTTP for sign-in and the boards: `std::http`, a request as a short connection whose message is the request (its method, path, query, allowlisted headers and cookies, and body) and one `respond`, with `session_cookie` for a `__Host-` cookie. The Cloudflare host's router refuses a change from another origin (403), and limits each client by its address (429), which never reaches the program. The console's session is the `__Host-session` cookie and a ceremony's the `__Host-ceremony` cookie (platform/server.wrela); the CSRF token stays, as a second check.

### A server has no wall clock

- **Needed:** sessions that expire after 90 days, game tokens after an hour, challenges after five minutes, and uploads counted by the hour, across server restarts.
- **What happens:** `serve`'s `time` counts from the server process's start (language.md §19.3), and a server has no service for the time of day.
- **Meanwhile:** the server keeps its own clock in its store (platform/server.wrela, `clock_key`): read at its first call and counted on from the host's time, written every 10 s. It never goes back, but stands still while the server is down, so an expiry is late by the server's downtime.
- **Status:** fixed in toolchain 0.13.0 (rywible/wrela-compiler#46): each HTTP request carries the host's wall clock (`std::http::Request::time`), and `std::time::wall_clock` asks for it. The server's clock is the latest request's time (platform/server.wrela), and its stored clock is gone.

### A server runs only on `wrela-serve`

- **Needed:** the console's server on Cloudflare, Workers and Durable Objects (Ryan, 2026-10-31): Cloudflare's protection (floods, firewall rules, rate limits, bot checks, no server address to attack), one vendor with the website, and a server build is wasm, which a Durable Object runs. Sign-in and the boards as plain HTTP requests under wrela.dev (a request in, a response with its cookie out, `std::store` behind it); WebSockets later, for live play.
- **What happens:** a server build runs only on the native `wrela-serve`, and only over WebTransport (language.md §19.3). Cloudflare's proxy can't carry WebTransport: it has no HTTP/3 to an origin (https://developers.cloudflare.com/speed/optimization/protocol/http3/), and its UDP proxy, Spectrum, is an Enterprise add-on (https://developers.cloudflare.com/spectrum/protocols-per-plan/).
- **Found:** 2026-10-31, choosing where the console's server runs; it was set up on Fly.io first, with a UDP relay for `wrela-serve`'s wildcard bind, and taken down unused.
- **Meanwhile:** the release left the console's server build out, and the console was built with `ONLINE = false`.
- **Status:** fixed in toolchain 0.13.0 (rywible/wrela-compiler#46): a server build also writes `server/cloudflare.js` (a Durable Object that runs it beside its SQLite store, and the Worker's router, `api`) and `server/cloudflare.json` (its Wrangler members). The release puts them in the console's Worker (studio/release/site.wrela), on Workers Paid: Free's 10 ms of CPU a Durable Object invocation is too little to verify a run, since the host runs calls for up to 250 ms before it hands on to the object's alarm (Ryan chose Paid, 2026-10-31). WebSocket rooms came in 0.14.0, which the console doesn't use yet.

### A call's reads don't see its writes

- **Needed:** a single-use key taken once, and a unique key claimed once, by the console's server: a passkey ceremony's challenge (used once, even by two requests in one call), an account's name (no two accounts with one), and the verification queue read after an upload writes it.
- **What happens:** `std::store`'s reads are answered on the next call, from the store as it was before the call's writes, which commit when the call returns (language.md §19.4; toolchain 0.13.0's Cloudflare host does the same). So two requests in one call both read a challenge before either's delete commits, and a read made in the call that wrote a key misses it.
- **Found:** 2026-10-31, moving the console's server to HTTP, where a challenge moved from a connection's memory to the store.
- **Meanwhile:** the engine remembers what it used or wrote lately and checks it before trusting a read (platform/server.wrela: `spent` for challenges, `recent_names` for names, `queue_written` for the queue). Each new key of the kind needs its own list.
- **Status:** open: an atomic take (`store::take(key)`: read and delete in one) and claim (`store::put_new(key, value) -> bool`), which one Durable Object or one SQLite file can do in a statement, or a call's reads answered after its writes commit.

### `wrela-serve` keeps a room's printed lines to itself

- **Needed:** debugging the console's server on a real `wrela-serve` (the end-to-end check in docs/bench.md, "The console's server"). On the Cloudflare host, `WRELA_PRINT=1` sends them to the Worker's log.
- **What happens:** a server program's `print`s are kept by its room's host (at most 256 lines a room) and never written out; `wrela-serve` prints only a room's failures.
- **Meanwhile:** a debugging build wrote what it saw into its store, and the store was read with SQLite.
- **Status:** open: `wrela-serve --print`, or a room's lines on its standard error.
