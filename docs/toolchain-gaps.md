# Toolchain gaps

*Capabilities this repository needed from the toolchain ([rywible/wrela-compiler](https://github.com/rywible/wrela-compiler)) and found missing or broken. AGENTS.md's rule holds: a gap is fixed in the toolchain, released, and the pin moved; never worked around here in another language. Started 2026-10-21 at Ryan's request.*

Each entry says what was needed, what happens instead, how it was found, the evidence, what this repository does meanwhile, and its status. Add an entry when a gap is found; when the fix ships, say in which release and keep the entry.

## Open

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

## Fixed

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
