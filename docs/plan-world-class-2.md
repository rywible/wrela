# The world-class engine, part two: the implementation plan

*Everything `docs/world-class-next.md` (the 2026-10-25 audit) found, closed in one scope of work:
one toolchain release, `rywible/wrela-compiler` 0.8.0 (0.7.0 shipped on 2026-10-25 while the audit was written: a simplification pass and review fixes, none of this plan's features); one pull request here that pins it and
rebuilds the renderer, the studio and the games on it; and one pull request in a sibling
repository that holds the competitor scenes. Written for an implementing agent who has not read
the audit. Each step names its files, what to write, how to test it and the gate it must pass
before the next step's commit. Started 2026-10-25 at Ryan's request. Not yet reviewed by the
council (§10 is where its findings go).*

Ryan owns the goals. A line marked **proposed** is a default the plan chose that Ryan has not
confirmed; the agent builds the default behind a switch where one is cheap, and asks before a
default that can't be switched. A section marked **gate** is a check the branch must pass before
the next step's commit. A number the plan names as a target is an **estimate** until a gate
measures it; an idea the plan calls a **hypothesis** ships only if its own gate says it pays, and
otherwise ships switched off with the numbers that said so.

## 0. Read this first

### 0.1 What "world class" means now

The first plan's claim (`docs/plan-world-class.md` §0.1) was measured in B13 and missed on two
scenes, and its "99th percentile under 16.7 ms" can't hold in headless Chrome (the leapling, a
2 ms frame, has a p99 of 18.2 ms; `docs/bench.md`). The claim this plan builds toward, **D10,
proposed**:

> On a MacBook Air M4 in Chrome at 1080p, every flagship scene's GPU frame is under 12 ms at the
> median with every `Quality` switch on, no frame of 1,200 paced frames is over 33 ms and the paced
> 99th percentile is under 20 ms; each is playable within 5 s of a cold page open and 3 s of a warm
> one; the Last Green's floor keeps its GPU memory under 512 MiB. On a MacBook Air M1 with 8 GB
> the floor holds 60 fps with the same switches and is playable within 8 s cold. On an Intel Iris
> Xe laptop in Chrome the floor's GPU frame is under 25 ms with no frame over 50 ms. The same
> scenes in three.js, Babylon.js and PlayCanvas, measured by the same harness on the same machines,
> are recorded beside these numbers, whatever they show.

"Every switch on" means the switches this plan adds too: shadow pages, probe visibility, the
distance volume, the froxel volume, the planar reflection where the mere is in view, the field
LOD. The axes are the audit's: frame time at a visual target on integrated GPUs, smoothness (the
p99 and the hitch count, not the median), time to play (cold and warm), bytes, memory and
bit-exact determinism.

### 0.2 The three repositories and the three pull requests

| Repository | Checkout | Pull request |
|---|---|---|
| `rywible/wrela-compiler` | `~/projects/wrela-compiler` | **Part A.** One PR titled `Release 0.8.0: <summary>`, per `docs/release.md` there. Ryan merges and tags it. A fault found after the tag is a 0.8.1, never a replaced release. |
| `rywible/wrela` | `~/projects/wrela` | **Part B.** One PR from `engine/world-class-2`, pinned to 0.8.0, holding every engine, studio, content and game change, and the docs. |
| `rywible/wrela-others` (new) | `~/projects/wrela-others` | **Part C.** The competitor scenes (JavaScript and TypeScript, which this workspace may not hold) and their harness. One PR. Its numbers go into `docs/bench.md` by hand. |

**Where the plan starts.** Toolchain 0.7.0 shipped on 2026-10-25 (rywible/wrela-compiler#35: a cleanup of everything since #18 and the fixes a code review and a council found; the command stream stays at version 12 and the manifest at 10; the store's value limit fell to 1020 KiB; f16 interval derivation is refused with E0700 by a council decision; the pipelines' post-dominator analysis runs only for `wrela pipelines`; an unknown pipeline fails at once in the browser). The branch this plan lives on, `plan/world-class-2`, pins 0.7.0, so Part B starts from it and §B0 moves the pin to 0.8.0. The anchors in §0.8 were read at v0.6.3; the ones 0.7.0 moved are listed under the table.

The order is fixed: Part A merges and releases first, then Part B moves the pin. While Part A is a
branch, `WRELA_TOOLCHAIN=~/projects/wrela-compiler` in this checkout runs it in place of the pin
(`docs/toolchain.md`), so Part B's first steps are built against it before the release (§A16).
Part C is independent and must be done before Part B's final gates (§B16) need its rows.

The toolchain knows nothing about the engine (the layering rule, `docs/vision.md`). Every Part A
feature is phrased as a general GPU, language or host capability a non-game program would also
want. Nothing in Part A names a shadow, a probe, a tree or a tile.

### 0.3 How to work

- **Read first:** §8's worked examples (the shapes the steps expect), `docs/world-class-next.md` (why each step exists, with the audit's numbers),
  `docs/plan-world-class.md` §0.5 (the gate kinds, reused here by name) and §8–§9 (what the
  council changed last time and why), `docs/bench.md`, `docs/toolchain.md`, `docs/toolchain-gaps.md`,
  `spec/language.md` §13, §14, §16, §18, §23 in the compiler, and `AGENTS.md` in both repositories.
- **One commit per numbered step,** in the order given, each titled with the step's heading. A
  step that says "gate" ends with the gate's result in its commit message, numbers included.
- **Both repositories' rules hold:** one PR for one requested scope (this plan is the scope), run
  as few tests as possible to validate a change, cite your sources. Each outside method in §9 gets
  a row in `docs/citations.md` when its code lands.
- **Nothing lands that the gates don't cover.** A step whose gate fails is fixed before the next
  step; it is never skipped and never "fixed later". A **hypothesis** step whose gate fails ships
  switched off, with the measurement in its commit and in `docs/bench.md`, and the plan goes on.
- **Estimates are labelled.** A cost, a speedup or a budget is `estimated` in code and prose until a
  gate measured it; then the measured number replaces it, with the device and the command.
- **Don't invent design.** The look, the games' feel and their lore are Ryan's (`docs/vision.md`,
  "The lore"). Where a step touches them, build what the plan says and mark anything beyond it
  **proposed**. New looks (rock, water, cloth, fog) are engine capabilities with a default that
  the Last Green turns on; their parameters are the content's and Ryan's.
- **Keep the docs true:** `docs/toolchain-gaps.md` (each gap this plan closes moves to "Fixed",
  naming 0.8.0), `spec/language.md` (every new capability), `docs/toolchain.md` (new commands),
  `docs/bench.md` (every measurement), `docs/citations.md`, the games' `design.md`, and the
  compiler's `docs/release.md` steps.
- **Use the tools:** `wrela same <pkg> --against <saved build>` shows a change kept what a program
  does; `wrela profile --chrome --compare` measures; `wrela pipelines <pkg> --json` counts; `wrela
  precision` (new, §A9) says where `f16` is safe; `wrela tool bench` records; `wrela tool passes`
  (new, §B1.5) shows a frame's passes. The lints are on (W0010–W0013).
- **An existing form changes only as a migration** (`docs/release.md` §1): Part A adds forms and
  removes none; every new parameter has a default.
- **Pixels are a first screen in a look step, never the verdict.** A pixel diff against the
  baseline (`wrela same --within n`, the difference image) is cheap and runs first: it says what
  moved, where and by how much, and catches a change that landed somewhere it shouldn't (a
  shadow step that moved the sky). But a step that changes the image on purpose will not match
  the baseline's pixels and must not be made to: an agent that tunes a look step until the
  pixels match has undone the step, and **no work is discarded or reverted because pixels
  differ**. The floor is G-look's numbers (no shot worse by the reference error, the flicker, the
  clipping, the rubric); the aim is a shot Ryan judges better. Pixel gates (G-pixels) remain the
  verdict only for the steps that change no look: pins, refactors, culling, scheduling.
- **Measure on an idle GPU,** nothing else building; report GPU span, CPU frame call and paced
  intervals as the separate things they are (`docs/toolchain.md`, "Watching and measuring").

### 0.4 Decisions reserved for Ryan

The agent builds each default; Ryan can flip it in review.

| ID | Decision | Default the plan builds (**proposed**) |
|---|---|---|
| D10 | The claim's wording | §0.1 as written, once §B16 measures it |
| D11 | HDR and wide-gamut output | Built (§A12, §B13.3); `Look::output = Sdr` by default; `Hdr` is a setting |
| D12 | Local fog in the air | On (`Quality::fog = true`); each floor authors its density field (`Scene::fog`), the Last Green's is a thin ground mist in the hollows at dawn, **proposed** |
| D13 | Shadow pages | Pages 128², a physical pool of 4096² (1,024 pages, 64 MiB), ten clipmap levels from 8 m to 4 km; the moving casters keep their own 1024² map |
| D14 | The cluster path | `ClusterStones` and `CLUSTERED` leave the Last Green; `engine::virtual` stays as `examples/clusters` and its tests; the field LOD (§B11) replaces it for built things, rocks and distant creatures |
| D15 | A planar reflection of the mere | Drawn at half the scene's size only while the mere is in view (`Quality::planar = true`) |
| D16 | Relaxed SIMD in presentation code | Built behind `--relaxed-simd`; on by default in `wrela build --release` only if §B13.5 measures the CPU frame at least 3 % faster on the Last Green with no pixel over 1/255 |
| D17 | The cloud dome's size | 2048×640 marched, upsampled to the 4096×1280 dome, if §B5.4's pixels stay within 2/255; else the dome stays as it is |
| D18 | Neural impostors, the learned rejector, radiance cascades | Experiments (§B14); each ships behind a switch, off, unless its gate says it pays, then on |
| D19 | The competitors | three.js (its WebGPU renderer), Babylon.js and PlayCanvas; Unity Web only if a licence is to hand; the brief in Part C |
| D20 | The device matrix | M4 Air (the reference), M1 Air 8 GB, an Intel Iris Xe laptop with Windows 11 (Chrome and Firefox), an AMD Radeon 780M machine, an iPad Pro in Safari 26; one Android phone's row if a device is to hand |
| D21 | Grass placement cadence | A ring is re-placed when its cell set moves or every 8 frames (the wind's phase), not every frame |
| D22 | Probe visibility | On (`ProbeQuality::visibility = true`) |
| D23 | Pipeline tiers | Tier 1 (pipelines every site of which is under a runtime condition) starts building when tier 0 is built; the arrival cover may lift before tier 1 is done |
| D24 | The memory budget | `memory::TOTAL` stays 1,024 MiB for the program; the floor's own target is 512 MiB (§B7's gate) |
| D25 | Water looks | Flow, foam, falls and wet margins are engine capabilities; their colours and strengths stay in `WaterLook` (`CLEAR`, `PEAT`, `PAINTED`) for Ryan to set |
| D26 | Compressed files | **Decided** (Ryan, 2026-10-25): none are written by a build or held by a release; the toolchain's servers compress at serve time (§A3) and the release relies on Cloudflare's edge, which §B2.1 checks |

### 0.5 The gate kinds

The first plan's gate kinds are reused by name and are not restated: **G-sim, G-commands,
G-pixels (identical / within n/255), G-draws, G-suite, G-golden, G-pipelines, G-profile, G-both,
G-size, G-bench** (`docs/plan-world-class.md` §0.5; the one-command form is `wrela same`). The
baselines for all of Part B are `build/base-0.7/<name>`, made at §B0. New kinds:

A step's gates are its acceptance criteria: the step is done when every gate it names has passed, with the numbers in its commit.

- **G-cold:** `wrela tool bench --scene <s>`'s cold run (`salt`, no cache) three times, alternating
  with nothing else running; the **median** `playable_ms` is under the step's number. The bench
  prints all three.
- **G-warm:** the same, with the new warm run (§B1.4: the same build opened a second time by the
  same browser profile, no `salt`); median under the step's number.
- **G-hitch:** over the paced run's frames, the count over 33 ms is as the step says, and every
  frame over 33 ms is named in the bench's hitch table (§B1.2) with its heaviest labels.
- **G-device:** `docs/bench.md` has the row for each device D20 names, produced by §B8's procedure,
  with the machine's name from the report.
- **G-memory:** the bench's `gpu_peak` for the scene is under the step's number, and the share
  table (§B7.1) is in `docs/bench.md`.
- **G-stills:** `wrela tool lookbook --package <pkg> --stills` before and after, the pairs in the
  PR; a deliberate change is said to be one.
- **G-spike:** a hypothesis step's own numbers (its cost, its quality measure, its memory) against
  the thresholds the step states, pasted into the commit; the switch's default follows the result.
- **G-look:** the lookdev gate for a step that changes the image on purpose. The baseline's
  pixels are diffed first as a cheap screen (the difference image goes in the PR and says where
  the change landed), but the diff decides nothing and discards nothing. Four parts, all
  required: (1) **pairs**: `wrela tool lookbook
  --stills` before and after for every lookbook shot the step names (the floor's 23 and the
  clearing's 3 are the set), in the PR; (2) **the reference error**: `wrela tool lookbook
  --quality` (§B1.8) renders each named shot in **reference mode** (every quality knob at its
  maximum and 64 jittered samples accumulated **with time frozen**: the sim paused, the wind, the
  clouds' drift, the water's drift and the particles held, so only the jitter changes between the
  samples; no TSR, no pacing) and in play mode, and reports PSNR and SSIM of play against
  reference; neither falls, for any shot, by more than 0.5 dB or
  0.01 against the previous step's record, and §B16's record has every shot at least 1 dB above
  B13's; (3) **artifact measures** from the same tool, all with time frozen so that intended motion
  can't count as an artifact: flicker (mean per-pixel temporal variance over 64 play frames with
  the camera still) not up, clipped share (pixels at 1.0 in any channel) not up, popping (the
  largest mean frame-to-frame difference over a 10 s dolly, the camera the only thing moving)
  not up, `trail` (the ghosting length behind a moving edge against the reference) not up, each for every named shot; a rise the commit names as deliberate is allowed only with
  Ryan's acceptance in "For Ryan". The measures are read together: a change that lowers flicker
  while lowering PSNR by over 0.5 dB fails (smoothing is not quality), which the instrument's
  negative controls (§B1.8) enforce; (4) **judgement**: each pair
  judged **blind** on the lookdev rubric (§B1.8: light, colour, value, depth, artifacts) by a
  **separate** agent in a fresh context (not the implementer), the two stills of a pair shown in
  a random order it isn't told, with Ryan's hero anchors beside them when `WRELA_HERO_DIR` names
  them; it scores both and says which it prefers; the scores, the preferences and the order key go
  in the PR, and Ryan's verdict is asked for in "For Ryan". The judge is calibrated once per step
  with a control pair whose "after" is a deliberate degradation (§B1.8); a judge that prefers the
  degraded still is replaced, and every calibration attempt is recorded, not only the last. A step
  that changes temporal behaviour (the TSR, the probes' re-trace, the fog's blend, any switch
  that gathers over frames) is judged on three-frame strips at a moving edge as well as on
  stills. A step is not done while the judge
  preferred "before" on any named shot, or any of (2) and (3) regressed, unless Ryan accepted the
  exception in writing. The numbers in (2) and (3) are a floor, not a proof: the reference shares
  the approximation's algorithm where no better reference exists (the probes, the fog, the
  cascades at higher settings), and only bypasses it where one does (the TSR by supersampling,
  the shadow pages by finest pages everywhere); §B1.8 names which is which per subsystem.
- **G-precision:** an `f16` twin of a kernel against its `f32` original on the same input: pixels
  within the step's n/255 (`wrela same ... --within n` against the step's own saved build, with
  `--features none` forcing the `f32` path), and `wrela precision`'s report attached.

### 0.6 What this plan leaves out, and why

- **Static-or-moving by data flow in the compiler** (audit §5.7). The engine's kinds know what
  moves; §B13.1 gives each a flag. A taint analysis from the sim's snapshot through every draw
  would add nothing the flags don't yet, and it is research for a compiler agent, not a step.
- **Cost models from the compiler** (audit §5.8). The planner measures costs at run time since
  0.6.x; §B4.6 extends the timed labels to every kind. A static model would be a prior for a
  value the device measures in four frames.
- **Immediates (push constants).** The uniform blocks here are larger than the 64-byte immediate
  limit in almost every pipeline (`manifest.json`'s `uniform.size`); nothing to carry in them yet.
- **Compatibility mode as a rendering tier.** The engine needs `indirect-first-instance`,
  storage textures and multi-target passes that the restricted subset limits; reach to OpenGL ES
  3.1 devices is a separate renderer, out of scope. Phones are a non-goal (`docs/vision.md`).
- **Bindless, 64-bit atomics, subgroup matrices, texture atomics.** Not shipped in Chrome stable
  (audit §3.6). The soft rasterizer stays two-pass; the neural steps use `f16` and subgroups.
- **MSAA, stencil, occlusion queries, render bundles.** As in the first plan: the TSR anti-aliases,
  the depth pyramid replaces queries, the draw counts make bundles moot.
- **Strand hair, Gaussian splats, rollback netcode, compute tessellation of the clipmap.** Later
  work (audit §6.9, §7.7, §7.8); the crag's terracing is a representation problem that §B11b
  fixes without tessellation.
- **A render graph in the toolchain.** §B7.6 pools transient targets in the engine; a compiler
  check of pass liveness is a later language feature if the pool ever misbehaves.
- **Safari and Firefox automation.** `std::tool::browser` drives Chrome; the other browsers are
  checked by hand on the device matrix and recorded.

### 0.7 The order, and what needs what

```
Part A (toolchain 0.8.0)            Part B (engine)                       Part C
A1 versions ───────┬─ A2 wasm by URL          B0 pin, baselines
                   ├─ A5 tiers ──────────┐     B1 measurement ◄── A14, A15
A3 compression     ├─ A7 const params ───┤     B2 cold start ◄── A2 A3 A4 A5 A6 A8
A4 data section    ├─ A11 conservative Z │     B3 clipmap split
A6 fold after inl. ├─ A12 screen colour  │     B4 hitch sources
A8 tapes           │                     │     B5 frame ◄── A10 A11
A9 precision       │                     │     B6 shadow pages ◄── A10
A10 gather         │                     │     B7 memory
A13 relaxed SIMD   │                     │     B8 devices ◄── A14, Part C
A14 lan serving    │                     │     B9 GI ◄── A8 (tapes cook the distance volume)
A15 paced labels   │                     │     B10 air
A16 integration ◄──┴─────────────────────┘     B11 field LOD ◄── A7 (override'd levels)
A17 spec, release                              B12 water
                                               B13 image ◄── A9 A12 A13
                                               B14 frontier
                                               B15 records, B16 final gates ◄── Part C
```

An agent that reorders must keep the `Needs:` lines. Part B's steps B3–B7 and B9–B12 each change
what the eye sees and are gated on stills; B1, B2 and B8 change nothing the eye sees.

### 0.8 Where things live

**The toolchain** (`~/projects/wrela-compiler`, tag `v0.6.3`; line numbers are that tag's):

| Concern | Where |
|---|---|
| The command stream and manifest versions | `runtime/abi/src/stream.rs:96` (`VERSION = 12`), `runtime/abi/src/manifest.rs:60` (`VERSION = 10`), `Pipeline` struct `:135-150`, parser `:1045`; the TypeScript mirror `runtime/browser/src/manifest.ts:142-159`, parser `:329-354`; `abi.gen.ts` from `runtime/abi/src/typescript.rs` (`cargo run -p wrela-abi --bin generate`) |
| The page, the hand-over, the loader | `compiler/driver/src/build.rs:236-250` (the preload links), `runtime/browser/src/preload.ts:10-27`, `main.ts:395-398`, `loader.ts:16-99` (`Build`, `Handed`, `loadBuild`), `:192-212` (`startProgram`), `program.ts:469-477` (`WebAssembly.compile`), `:198` (`checkAbi`), `:557` (`instantiate`), `:272-281` (`otherInstance`) |
| Pipeline builds | `runtime/browser/src/gpu.ts:176-326` (`buildPipeline`), `:347-363` (cache, `biggestFirst`), `:408-432` (`buildPipelines`), `:816-847` (the executor's pipelines, `built`), `:993-1060` (waiting commands), `worker.ts:439-443` (frames skipped while waiting), `:727` (`caughtUp`) |
| The servers | `runtime/native/src/loopback.rs:162-174` (`mime`), `:195-198` (`head`), `:213-245` (`get_file`), `:47` (bind); `compiler/driver/src/live.rs:395-403, 595-630` (`wrela run`), `runtime/native/src/tool/serve.rs:441` (`std::tool::serve`), `runtime/native/src/pages.rs:465` |
| The data section | `compiler/wasm/src/lib.rs:371-378, 426-443, 465-490` (one passive segment), `compiler/wasm/src/func.rs:1635-1676` (`init_data`), `runtime/abi/src/memory.rs:227-240` (layout), `compiler/ir/src/lib.rs:945-949` (`ir::Data`) |
| The manifest's emission and the conditional analysis | `compiler/driver/src/build.rs:33, 86, 201-224` (`emit`), `compiler/driver/src/lib.rs:854-867` (`summarize` then `emit`), `:971` (`conditional_commands`), `:998-1045` (`summarize`, `PipelineSummary` `:67-79`), `compiler/cli/src/pipelines.rs:63-95` |
| Inlining and folding | `compiler/ir/src/opt.rs:1485-1569` (`inline_cpu`, `tidy`), `:1929-2032` (`fold_constant_branches`, `fold_block`), `:2296-2303` (`remove_unreached`); `compiler/lower/src/lib.rs:504-528, 569-612` (`recorded_pipelines_only`) |
| Pipelines and their keys | `compiler/lower/src/gpu.rs:20-35` (`PipelineKey`), `:624-636` (dedup), `:749-782` (`recorded_shader`), `:849-863` (`uniforms`), `:2530-2545` (`bind_uniform`); `compiler/sema/src/thir.rs:280-294` (`RenderState`), `compiler/sema/src/check/call.rs:3977-4050` (`render_constant`, E0603) |
| WGSL emission | `compiler/wgsl/src/lib.rs:96-98` (capabilities), `:140-141` (`enable f16;` by naga), `:209` (`workgroup_size_overrides: None`), `:302-311, 1730-1737` (image dims), `:555-563, 626-633` (builtins, `frag_depth`), `:1519` (`gather: None`); naga 30.0.1 |
| Tapes and fields | `compiler/std/stage.wrela:16-99, 112-231, 247-341` (`Tape`, `Op`, `interpret`, builders, `prune`), `compiler/std/field.wrela:1686-1826` (`hash_u32`, `value_noise`, `fbm`), `compiler/std/derive.wrela:201-253` |
| Derivations | `compiler/sema/src/defs.rs:576-582` (`Lang::Gradient` …), `compiler/sema/src/effects.rs:534-551`, `compiler/lower/src/derive.rs:15-60, 88-160, 180-380`, `compiler/lower/src/instance.rs:25-46` (`DeriveKind`), `compiler/ir/src/derive/{mod,ad,interval,exits,literal}.rs` (`Activity` `mod.rs:177-189`, `Builder` `:706-720`; `interval.rs:44, 472-569` (`F32_EPS`, `eps`, `tiny`, `widen*`), `:744-760` (table reads), `:926-1087` (`floor`), `:1234-1283` (`call_fn`)) |
| Half precision | `compiler/sema/src/ty.rs:100-174`, `compiler/lower/src/ty.rs:46, 96-109`, `compiler/ir/src/lib.rs:1098-1106` (`requires`, "shader-f16"), `compiler/wgsl/src/lib.rs:1044-1060` |
| SIMD and determinism | `compiler/wasm/src/func/ops.rs:487-605` (`vec_binary`), `compiler/wasm/src/lib.rs:603-662` (`check_no_relaxed_simd`), `compiler/driver/src/optimize.rs:31-65, 141` (`-Oz`, `FEATURES`), `compiler/sema/src/effects.rs:532` (`DETERMINISTIC`), `compiler/lower/src/lib.rs:1021-1060` (`check_deterministic`) |
| Test mode and timing | `runtime/browser/src/testmode.ts:1-131` (params), `:193-197` (`PUT results/`), `worker.ts:635-741, 806-807` (timing, `frames.json`, `timings.json`), `gpu.ts:627-700, 924-988, 1269-1275, 1504, 1744-1746` (`Timer`, `timeLabel`, pass timestamps), `runtime/native/src/gpu.rs:522-538, 1772-1793, 1996-2004`, `compiler/cli/src/profile.rs:165-211, 460-505`, `compiler/cli/src/profile/chrome.rs:90-116` |
| The canvas | `runtime/browser/src/worker.ts:120-125` (`context`), `:388`, `:603-632` (test mode), `gpu.ts:114-116` (`screenFormat`), `abi.gen.ts:249` (`SCREEN_FORMAT`) |
| std's GPU surface | `compiler/std/gpu.wrela:245-248` (`WithDepth`), `:491-507` (scissor, viewport), `:552-808` (`Storage` impls), `:1069-1074, 1352, 2388-2405` (depth arrays and layer passes), `:1769-1774` (`sample_compare` on layers), `:1826, 2000, 2834-2875` (copies, readback, mips), `:3138, 3293, 3307-3397` (`label`, `timed`, `limits`, `features`), `:3832-3840` (`Texels3d`) |
| std's host surface | `compiler/std/io.wrela:36-50, 139, 208-298` (`Pending`, `fetch`, `Shipped`), `time.wrela:53` (`phase`), `par.wrela:257-355` (`spawn`, `Task`), `job.wrela:14-21`, `cache.wrela:32-124` |
| The CLI | `compiler/cli/src/main.rs:92-216` (modules, usage, dispatch), `pipelines.rs`, `profile.rs`, `same.rs` |
| Release process, spec | `docs/release.md` §1–§3, `spec/language.md` §13 (constants), §14 (derivations), §16 (jobs), §18 (GPU), §23 |

**What 0.7.0 moved.** Checked construct by construct against tag `v0.7.0` (2026-10-25). None of
this plan's features exists in 0.7.0 (no second fold after inlining, no tier field, no
compression, no `compileStreaming`, no `gather`, no conservative depth, no overrides, no canvas
tone mapping, no relaxed SIMD, no LAN bind, no per-frame labels, no `--worst`, no tape change,
no `precision`, no `Screen`, no `wasm_abi`, one data segment). These files are byte-identical
between the tags, so their anchors above stand: `compiler/driver/src/build.rs`, `preload.ts`,
`tool/serve.rs`, `compiler/wasm/src/lib.rs`, `func.rs`, `func/ops.rs`, `abi/src/memory.rs`,
`ir/src/opt.rs`, `lower/src/lib.rs`, `sema/src/thir.rs`, `std/stage.wrela`, `std/field.wrela`'s
noise, `lower/src/derive.rs`, `lower/src/instance.rs`, `ir/src/derive/ad.rs`, `optimize.rs`,
`testmode.ts`, `time.wrela`, `par.wrela`, `job.wrela`, `cache.wrela`, `cli/src/main.rs`. The rest
moved as follows (v0.6.3 → v0.7.0):

| File | Moved anchors | Note |
|---|---|---|
| `runtime/abi/src/manifest.rs` | `Pipeline` 135-150 → 137-152; parser `pipeline` 1045 → 1071 | `SCREEN_FORMAT` now from `SCREEN_TEXTURE` (64-65); `VERSION` still 10 |
| `runtime/abi/src/stream.rs` | `VERSION` unchanged at 96 (still 12) | `PASS_JOIN` removed; `max_levels`/`level_size` (273), `Can` (419) added |
| `runtime/browser/src/manifest.ts` | type 142-159 → 143-160; parser fn 244-354 → 254-341 | |
| `runtime/browser/src/main.ts` | 395-398 → 403-406 | |
| `runtime/browser/src/loader.ts` | 16-99 → 16-104 (`loadBuild` 97-104); `startProgram` 192-212 → 197-217 | `fetchFn` default `networkFetch` |
| `runtime/browser/src/program.ts` | `compile` 469-477 → 474-482 (the call at 477); `instantiate` 557 → 564 | still `compile(bytes)` |
| `runtime/browser/src/gpu.ts` | `buildPipeline` 176-326 → 178-328; cache and `biggestFirst` 347-363 → 349-365; `buildPipelines` 408-432 → 410-434; the executor's pipelines 816-847 → 836-867; waiting commands 993-1060 → 1020-1087; `Timer` 627-700 → 642-724; `timeLabel`/`#timestamps` 924-988 → 946-1011; the `Timed` case 1269-1275 → 1297-1307; `beginComputePass` 1504 → 1535; `beginRenderPass` 1744-1746 → 1778-1780; `screenFormat` 114-116 → 116-118 | `UNTIMED` at 625; an ask's `active` became `from` |
| `runtime/browser/src/worker.ts` | `caughtUp` 727 → 728; timing 635-741 → 635-745; `frames.json` 806-807 → 810-811; `context` 120-125 → 119-124 | 439-443 unchanged |
| `runtime/native/src/loopback.rs` | `mime` 162-174 → 200-212; `head` 195-198 → 233-236; `get_file` 213-245 → 251-283; bind 47 → 85 | `DeadlineIo` at 30-80 |
| `compiler/driver/src/live.rs` | the handler 595-630 → 603-638 | `serve` 395-403 unchanged |
| `runtime/native/src/pages.rs` | 465 → 437 | `pages::run` can serve the build directory alone |
| `compiler/ir/src/lib.rs` | `ir::Data` 945-949 → 925-929; `requires` 1098-1106 → 1078-1086 | |
| `compiler/driver/src/lib.rs` | `summarize` then `emit` 854-867 → 903-917; `conditional_commands` 971 → 1021; `summarize` 998-1045 → 1062-1112; `PipelineSummary` 67-79 → 67-80 | `conditional_commands(checked, &sites)` now runs only through `check_pipelines`, for `wrela pipelines`: §A5 runs it in the build again |
| `compiler/cli/src/pipelines.rs` | 63-95 → 64-97 | calls `check_pipelines` |
| `compiler/lower/src/gpu.rs` | dedup 624-636 → 608-620; `recorded_shader` 749-782 → 744-777; `uniforms` 849-863 → 844-858; `bind_uniform` 2530-2545 → 2546-2561 | `PipelineKey` 20-35 unchanged |
| `compiler/sema/src/check/call.rs` | `render_constant` 3977-4050 → 3942-4015 | |
| `compiler/sema/src/defs.rs`, `effects.rs` | `Lang::Gradient`… 576-582 → 582-588; `DETERMINISTIC` 532 → 539; `DERIVED` 534-551 → 541-558 | |
| `compiler/sema/src/ty.rs`, `lower/src/ty.rs` | F16 100-174 → 100-160; `lower/src/ty.rs:96-109` → 96-100 | `on_cpu`/`on_gpu` became `check_side` (`lower/src/ty.rs:291`) |
| `compiler/std/derive.wrela` | 201-253 → 208-260 | `Box2/Box3::split` keep their ends |
| `compiler/ir/src/derive/mod.rs`, `interval.rs` | `Builder` 706-720 → 717-731; `eps`/`tiny`/`widen*` 472-569 → 511-608; table reads 744-760 → 783-799; `floor` 926-1087 → 965-1126; `call_fn` 1234-1283 → 1273-1322 | the f16 refusal (E0700) at `interval.rs:290-331` |
| `compiler/wgsl/src/lib.rs` | `image_dim` 1730-1737 → 1729-1736 | everything else cited unchanged |
| `runtime/native/src/gpu.rs` | `Timer` 522-538 → 519-535; compute-pass timing 1772-1793 → 1778-1796; render 1996-2004 → 1994-1995 (`timestamp_writes` 2055) | `Timer::next` → `next_if(asked, frame, call, label)`, an `Ask` struct |
| `compiler/cli/src/profile.rs`, `profile/chrome.rs` | `Profile`/`Times` 165-211 → 156-202; `Label`/`by_label` 460-505 → 436-481; `chrome.rs` 90-116 → 88-113 | |
| `compiler/std/gpu.wrela` | `WithDepth` 245-248 → 246-249; scissor/viewport 491-507 → 499-515; `Storage` impls 552-808 → 560-816; `DepthTextureArray` 1069-1074 → 1064-1069; `depth_texture_array` 1352 → 1315; `begin_depth_layer_pass` 2388-2405 → 2346-2363; `sample_compare` 1769-1774 → 1730-1735; `copy_texture` 1826 → 1788; `read_texture` 2000 → 1984; `MipStep`…`generate_mips` 2834-2875 → 2735-2776; `label` 3138 → 3044; `timed` 3293 → 3199; `Limits`…`features` 3307-3397 → 3213-3303; `Texels3d` 3832-3840 → 3737-3745 | no public item renamed or re-signatured; `copy_texture`'s `width: 0`/`height: 0` now mean "what is left after the offset" |
| `compiler/std/io.wrela` | `Pending` 36-50 → 39-53; `fetch` 139 → 138; `Shipped` unchanged | `IoError` lost its `message: String` field; `message()` returns a constant |
| `abi.gen.ts` | `SCREEN_FORMAT` 249 → 250 | `TEXTURE_KINDS` emitted; `PASS_JOIN` and `is_3d` gone |
| `spec/language.md` | §16 1712 → 1713; §18 2068 → 2069; §23 2783 → 2785 | |

**This repository** (`~/projects/wrela`, branch `engine/world-class` at `024d6f77`; line numbers
are the working tree's on 2026-10-25 and may have moved a few lines):

| Concern | Where |
|---|---|
| The frame | `engine/frame.wrela:77-275` (`Scene`), `:293-359` (`Targets`), `:434-444` (`Cooks`), `:580-624` (`Quality`), `:697-724` (`ShadowQuality`, `ProbeQuality`, `Arrival`), `:864-988` (`Frame`), `:1372-1517` (`draw`), `:1460-1468` (the moving map), `:1570-1647` (`memory`, `shares`), `:1651` (`playable`), `:1686` (`strip`), `:1751` (`cook_sky`), `:1887` (`bake_probes`), `:2044-2341` (`draw_scene`: `cull` 2057, `choose` 2103, prepass 2111, `shade` 2121, terrain 2132-2148, characters 2159, reveal 2167-2183, sky, `effects` 2227, particles, veils, composite 2314, `copy_below`, motion 2339), `finish` (after 2341: lines, resolve, meter, cover, dabs, bloom, develop, overlay 2482), `:2489-2512` (`strip_of`) |
| The planner | `engine/cook.wrela:29-59` (`Work`), `:63-78` (`Cost`, the ladder), `:102-136` (`costs`, `Budget`), `:139-297` (`Planner`: `begin` 182, `may` 203, `note` 229, `allowed` 241-250, `measure` 297) |
| Lighting | `engine/lit.wrela:31-54` (`Lighting`), `:58-165` (`sun_at`, `sun_and_sky`, `sun_and_gloss`, `sun_sky_and_gloss`, `reflect`, `through_air`, `shown`); `engine/light.wrela:22-31` (`Light`), `:74-86` (`hazed`), `:95-106` (`Lit`), `:128-193` (BRDF), `:198-206` (`character`), `:209-227` (tonemap) |
| Shadows | `engine/shadow.wrela:24-45` (`Shadows`, `Cascade`), `:87-116` (`shadows`), `:121-149` (`TAPS`, `lookup`), `:154-212` (`sunlit`, `statically_sunlit`), `:260-405` (`Kept`: `new`, `follow` 340, `strip` 371, `drawn` 394), `:410` (`Strip`); `games/last-green/stage.wrela:303-362` (`choose_casters`, `cast`, `cast_moving`), `:1999-2004` (tile arrival marks the near cascade) |
| Probes | `engine/probes.wrela:39-46` (`Probe`, `Held`), `:79-80` (layers), `:112-151` (`ProbeGrid`), `:169-248` (`Occluder`, `cook_occluders`, `density_at`), `:269-363` (`Trace`, `trace`), `:382-393` (`RAYS`, `Strip`), `:403-550` (`bake`, `trace_again`, `gather`), `:658-706` (`PLANES`, the map layout), `:727-806` (`held_at`, `sky_light`), `:829-871` (`Baked`, `STRIP_COLUMNS`, `AGAIN_*`), `:895-1092` (`new`, `renew`, `cook`, `bake_strip`); `games/last-green/stage.wrela:1246-1257` (`probe_grid`), `:2090-2137` (`trace_of`, `loaded_probe_occluders`) |
| Terrain | `engine/terrain.wrela:126-166` (`Cover`, `Texel`, `SIDE`, `MAX_LEVELS`, `ROW`, `STRIDE`), `:183-199` (`Levels`, `FieldsAt`), `:238-275` (`Clipmap`), `:331-407` (`set_origin`, `fields_on`, `write_fields`, `touched`), `:434-525` (`follow`), `:558` (`cast`), `:722-730` (`bound_sides`), `:825-837` (`cook_level`), `:934-942` (`ground`, `ground_shadow`), `:1064-1127` (`Covering`, `ground_cover_at`, `height_at`), `:1203-1250` (`ground_shade`); `games/last-green/stage.wrela:224-238, 1316-1323` |
| Grass | `engine/grass.wrela:39-59` (`Blade`, `Flower`), `:63-101` (constants), `:106-120` (`Place`, `cells`), `:204-375` (`place`), `:1238-1286` (`Grass`, `Grass::place`), `:1392-1431` (`Contact`), `:1609-1617` (`blade_shade`), `:1770-1846` (prepass, motion, shade) |
| Wood, plants, impostors | `engine/wood.wrela`, `engine/plant.wrela:418-432, 669-676`, `engine/impostor.wrela:7-11, 34-36, 130-150, 379-470`, `engine/vegetation.wrela:52-80, 420-460`, `engine/lod.wrela` |
| Sky and air | `engine/sky.wrela:34-78` (constants), `:84-126` (`Weather`, `Clouds`), `:409-523` (`march`, the light loop 504-511), `:565-625` (`march_dome`, `scatter`), `:635-670` (cloud shadow), `:723-825` (`Sky`); `engine/air.wrela:34-52` (sizes), `:291-297` (`aerial_uvw`), `:526-576` (`Tables`, `cook_slice`) |
| Water | `engine/water.wrela:24-63, 193-251, 257-331, 275-316, 357-437, 442-515`; `land/water.wrela:23-83, 395, 585-610` |
| Temporal | `engine/temporal.wrela:28-67` (`Resolve`), `:115-154` (`neighbourhood`, `neighbourhoods`), `:195-234` (`resolve`), `:240-448` (`resolved`), `:454-463` (`motion_of`, `deforming_of`); `engine/frame.wrela:447-572` (`Resolver`, `Gathered`, `Shared`) |
| Realizer, creatures | `engine/realize.wrela:46-61, 160-169` (`BLOCK`, `SkinVertex`), `:190-336, 505-614` (kernels), `:660-722` (`Mesh`), `:766-852` (`solid`, `realize_rigid`, `prepare`, `realizing`), `:1108-1185` (`Realizer`), `:1333, 1389` (`frame`, `realize_all`); `engine/character.wrela:85-121` (`Instance`, `pose_instances`), `:166-262` (`cook_colours`, `cook_materials`, `Realized`), `:440-613` (`Cast`); `engine/look.wrela:14-79`; `engine/stone.wrela:309-334` (`realize_rock`) |
| Clusters | `engine/virtual.wrela:53-97` (constants, `Placed`), `:131-197` (`Cut`, `Level`, `Clusters`), `:227-398` (`cook`), `:737-920` (`own_errors`, `inherit_errors`, `select`, `gather`), `:988-1057` (ids), `:1124-1175` (resolve), `:1689, 1882-1941`; `engine/stone.wrela:1191-1335` (`ClusterStones`); `games/last-green/stage.wrela:62-70, 1474-1487, 2023-2037` (`CLUSTERED`) |
| Particles, memory, culling | `engine/particles.wrela:38-94, 333-459, 625-654`; `engine/memory.wrela:11-94`; `engine/cull.wrela:28-217` |
| The Last Green's stage | `games/last-green/stage.wrela:84-136` (`Stage`, `Thing`), `:140-160` (`stage()`), `:197-299` (the `Scene` overrides), `:367-413` (`choose`, `reveal`), `:457-554` (`shade`, `characters`), `:565-646` (`effects`), `:768-807` (constants), `:1508-1530` (`streamed`), `:1693-1810` (`stream`: fetches, the job loop 1732-1795), `:1887-1965` (the far layer), `:2009-2088` (`cook_load`), `:2195` (`casters_in`); `games/last-green/main.wrela:69-70, 136-189` |
| The studio | `studio/bench/main.wrela:47-67, 183-187, 191-333, 338-391`, `studio/bench/report.wrela:9-56, 126-214`, `studio/bench/scenes.wrela:8-88`, `studio/toolkit/page.wrela:22-78` (`run_page`), `studio/release/site.wrela:49-101, 320-437`, `studio/release/main.wrela:356-360, 445-546`, `studio/deploy/main.wrela:638-725`, `wrela.toml:152-155` (`[tools.bench]`) |

---

### 0.9 The contract: no step is skipped

Every step below ends in **Done when**, a checklist the step's commit must satisfy. Each line is
checked by a command, a test or a number, never by a sentence. The rules:

- **A step is done only when every line of its checklist holds.** The commit pastes the number or
  the command's output beside each line. A step with sub-steps (B6a–c, B14.1–4) has a checklist
  per sub-step and each sub-step is its own commit.
- **A target has a *now* and a *target*.** The *now* is the audit's number (`docs/world-class-
  next.md`) or the number the previous step recorded; the *target* is the line's. After each
  change the implementer measures again and writes the value in the commit, so the commit shows
  the hill it climbed (`playable 10,145 → 7,920 → 6,100 → 4,880 ms`). A trajectory that doesn't
  move after a lever says so and names the next lever before trying it.
- **"Too hard" is not an outcome.** Each step's levers are ordered so the first is reachable in
  a day; the implementer takes them in order and records each. A line that still doesn't hold
  after every lever the step names has been tried becomes a **Blocked** note in the commit, with
  the measurements, what was tried and the next lever; the implementer pushes the branch and
  stops at that step. It does not move to the next step, does not delete or soften the line, and
  does not decide the line was wrong: Ryan does.
- **A hypothesis step's experiment is mandatory.** Its code behind its switch, its measurement
  and its record in `docs/bench.md` are the deliverable; only the switch's default depends on the
  result (D18). An experiment that is never run is a skipped step.
- **Reordering or dropping a step is Ryan's call**, asked for in writing with the reason, before
  the next commit.
- **A step that changes the image satisfies G-look as well as its checklist**; a step that
  changes none holds its pixel gate. Neither is traded for the other, and a pixel diff is run
  on every step as the first, cheapest check: in a look step it only locates the change; work is
  never discarded over it.
- **Every estimate is replaced by a measurement** the first time the step measures it, in the
  code and in this plan's copy in the PR ("the measured value replaces `estimated`").


## Part A: toolchain 0.8.0 (`rywible/wrela-compiler`)

Branch `release-0.8.0` from an up-to-date `main` (at or past `v0.7.0`). Follow `docs/release.md`
§2: bump `[workspace.package] version` in `Cargo.toml` to `0.8.0`, run `cargo check --workspace`,
commit `Cargo.toml` and `Cargo.lock` first. Then copy this plan's §0.2–§0.6 and Part A into
`docs/plans/world-class-2.md` there, as 0.6.0's plan lives at `docs/plans/world-class-gpu.md`; the
copy names this file as its source, and this file is the one that's edited.

**The recipe for a new command** and **for a new texture format** are in
`docs/plan-world-class.md` Part A's preamble and are not repeated. The recipe for a new
**manifest field**: add it to `runtime/abi/src/manifest.rs`'s struct with `#[serde(skip_serializing_if
= ...)]` and a default in its parser (`manifest.rs:1045`, the `flag`/`number` helpers); bump
`VERSION`; mirror it in `runtime/browser/src/manifest.ts`'s type and parser (`:142-159, 329-354`);
regenerate `abi.gen.ts`; add a conformance test that a manifest without the field parses with the
default and one with it parses the value.

### A1. The stream and the manifest move to version 13 and 11

*Needs: nothing.*

- `runtime/abi/src/stream.rs`: `VERSION = 13`. `runtime/abi/src/manifest.rs`: `VERSION = 11`.
  Every host rejects the other version, as now.
- Nothing else changes in this step.
- **Gate:** G-suite (the compiler's). The frozen programs under `compiler/tests/frozen` rebuild.

**Done when:** (1) `stream.rs` says `VERSION = 13` and `manifest.rs` `VERSION = 11`; (2) a test
loads a 0.7.0 build under the 0.8.0 native host and asserts the refusal names both versions;
(3) `cargo test --workspace` passes and `compiler/tests/frozen` rebuilds (paste the counts).
Hill to climb: none; the checklist is the step.


### A2. The WASM arrives by URL, compiles as it streams, and the ABI check reads the manifest

*Needs: A1.* Today the page fetches every preloaded file to bytes and posts them to the worker
(`preload.ts:17-27`); the worker compiles the WASM from bytes with `WebAssembly.compile`
(`program.ts:469-477`), which V8's code cache never serves (the cache keys on a streamed
resource's URL; V8's blog, §9). `checkAbi` (`program.ts:198`) parses the bytes' type and import
sections, so a `Module` alone isn't enough today.

- **The build states the ABI.** `compiler/driver/src/build.rs:201-224`: the manifest gains
  `"wasm_abi": { "memory": { "initial": n, "maximum": m, "shared": true }, "imports": [["wrela",
  "submit"], ...] }` with exactly what `checkAbi` derives from the bytes (`wasm.ts:108`,
  `functionTypes`), by the manifest-field recipe. `checkAbi(module, abi)` compares
  `WebAssembly.Module.imports(module)` and `.exports(module)` with the manifest's lists and builds
  `MemoryLimits` from `wasm_abi.memory`; the byte parser stays only for the native host's tests if
  it is used there (grep; delete it if not).
- **The WASM is handed over as a URL.** `preload.ts`: for the file whose href is the manifest's
  `wasm`, `handOver` posts `{ type: "file", url, bytes: null }` and the worker's `Handed.fetch`
  (`loader.ts:60-89`) falls through to a real `fetch(url)` for it, which the preload has warmed in
  the HTTP cache. `loader.ts:92-99`: the WASM entry of `loadBuild` becomes a `Response`, not
  bytes; `Program.compile` takes `Response | Bytes`: a `Response` whose `Content-Type` is
  `application/wasm` goes to `WebAssembly.compileStreaming(response)`, anything else is read to
  bytes and compiled as today (the fallback for a server that mislabels). The ticker and the
  helpers already receive the `Module` (`worker.ts:826-832`, `messages.ts:42, 125-127`); the
  `wasmHash` for the tick log (`worker.ts:219, 254`) is computed from a `response.clone()`'s bytes
  **only when `settings.hashes`** is on, so a release run reads the bytes once at most.
- **The servers name the type.** `loopback.rs:162-174` already maps `wasm` to `application/wasm`;
  keep it (`compileStreaming` requires it), and §A3's compression keeps the inner type.
- **Tests:** a browser test opens `examples/hello-field` twice in one Chrome profile and asserts
  the second `load.json`'s `wasm` entry shows a compile under half the first's (the code cache
  served it); a test mislabels the WASM (`text/plain`) and asserts the fallback path compiles it;
  the conformance suite's `checkAbi` tests move to the manifest.
- **Spec:** §15 (what the manifest carries). **Docs:** `docs/toolchain.md`'s "Running a program"
  notes that a warm open skips the compile.
- **Gate:** G-suite; the hello-field warm/cold pair recorded in the PR with the numbers.

**Done when:** (1) every frozen program's `manifest.json` carries `wasm_abi` and a test asserts
its fields equal what `functionTypes` derived from the bytes before the change; (2) `grep -rn
functionTypes runtime/browser/src` shows no use on the load path; (3) the browser test opens
`examples/hello-field` twice in one profile and the second `load.json` shows a WASM compile time
at most half the first's (paste both); (4) the mislabelled-type fallback test passes; (5)
`runtime/browser/test` is green; (6) through `WRELA_TOOLCHAIN`, the Last Green's warm compile
in ms is recorded in the PR.
Hill to climb: the warm compile, now equal to the cold (no cache) → at most half; measured by
(3) and by `wrela profile --chrome` twice on the Last Green.


### A3. Compression at serve time, never as files

*Needs: nothing.* No server in the toolchain sets `Content-Encoding` (`loopback.rs:195-198`);
the bench loads 15.7 MB uncompressed of which 92 % is WASM and WGSL; gzip takes the Last Green's
WASM from 8.2 to 3.0 MB and its WGSL from 5.8 to 1.3 MB. **A build writes no compressed files
and a release holds none** (D26, Ryan, 2026-10-25): the release is served by Cloudflare, whose
edge compresses `application/wasm`, `text/plain`, `text/javascript` and `application/json`
responses itself and doesn't properly support pre-compressed objects behind a Worker. What this
step adds is only so the toolchain's own servers show a cold start the way the wire does.

- **The servers compress on the fly.** `loopback.rs:213-245` `get_file`: when the request's
  `Accept-Encoding` lists `gzip` and the file's type is one of `application/wasm`, `text/*`,
  `application/json`, `text/javascript`, compress the bytes with `flate2` (already a dependency,
  `compiler/cli/src/pin.rs:442`) at level 6 and send them with `Content-Encoding: gzip`, `Vary:
  Accept-Encoding` and the inner `Content-Type` (`mime(path)`). The compressed bytes are kept in
  an in-memory cache keyed by `(path, size, mtime)`, bounded at 256 MiB, so a page's second load
  costs no compression; nothing is written to disk. `head` gains the two headers as optional
  parameters. `wrela run`'s hot-reload server (`live.rs:595-630`) keeps `Cache-Control: no-store`
  and compresses the same way.
- **The request's headers are parsed where the path is** (`loopback.rs`'s request reader): read
  `Accept-Encoding` only; the parser stays the same size-bounded reader it is.
- **Nothing in a build directory changes:** `wrela build`'s output is byte-identical to 0.7.0's
  for the same program, and `studio/release` (Part B) gains no compression step that could drift
  into Cloudflare. A test asserts that a build's directory, with and without `--relaxed-simd`
  (A13), holds no `.br` or `.gz` file; §B16's G-size asserts the same of `wrela tool release`'s
  output.
- **Tests:** a Rust test of `get_file` with and without `Accept-Encoding: gzip` checks the headers
  and that the decompressed body equals the file; the browser's `loader.ts` needs no change (the
  browser decodes); a test asserts `load.json`'s `wasm` bytes under test mode are the wire count
  (so the bench's "loaded bytes" means wire bytes, within the gzip-to-Brotli difference of what
  Cloudflare sends).
- **Spec/docs:** `docs/toolchain.md` "Running a program": what the servers send.
- **Gate:** G-suite; `wrela profile --chrome` on the frozen `grazer` shows its loaded bytes fall
  by at least 60 % (recorded).

**Done when:** (1) the `get_file` test shows `Content-Encoding: gzip`, `Vary: Accept-Encoding`
and the inner type with the header, plain bytes without, and the decompressed body equal to the
file; (2) a test builds `examples/hello-field` and asserts no `.br` or `.gz` under its build
directory; (3) a test loads one file twice and the compressor ran once (a counter); (4) `wrela
profile --chrome` on the frozen `grazer` reports loaded bytes at most 40 % of 0.7.0's (paste
both); (5) `docs/toolchain.md` says what the servers send.
Hill to climb: wire bytes of the frozen `grazer`, now 100 % → at most 40 %.


### A4. The data section without its zero runs, and a report of what's in it

*Needs: nothing.* The Last Green's 8.37 MB WASM holds a 4.36 MB data section, 38.8 % zero bytes,
as one passive segment copied by a start function (`compiler/wasm/src/lib.rs:465-490`,
`func.rs:1635-1676`).

- **Segments skip zero runs.** `fn data` returns the blob and a list of `(offset, len)` runs of
  non-zero bytes, splitting at any zero run of 256 bytes or more; the emitter writes one passive
  segment per run (`DataCountSection { count: runs }`), and `init_data` issues one `memory.init`
  per segment and one `data.drop` each, under the same once-only latch. Memory is zero where no
  segment lands, so the result is byte-identical. The active (non-shared) path does the same with
  active segments at their offsets.
- **`wrela build <pkg> --report data`** prints the twenty largest constants by their laid-out size
  (`ir::Data.name` and `ty`), the section's total, and its zero-byte share, as a table; `--json`
  too. It is how §B2.2 decides which constants become files.
- **Tests:** a conformance test builds a program with a `const` holding a 64 KiB array that is
  mostly zeros and asserts the WASM's data sections total under a quarter of it and the program
  reads the array back correctly on both hosts; a test for `--report data`'s shape.
- **Spec:** §13 (what a `const` costs in bytes: "zero runs cost nothing").
- **Gate:** G-suite; G-pixels identical and G-sim on the frozen programs (the data is the same).

**Done when:** (1) the mostly-zero constant test: data sections under a quarter of the array
and correct reads on both hosts; (2) G-pixels identical and G-sim on every frozen program;
(3) `wrela build games/last-green --report data` (through `WRELA_TOOLCHAIN`) prints twenty rows,
the total and the zero share; (4) the Last Green's data section is at most 2.7 MB (now 4.36 MB
with 38.8 % zeros; paste the new size).
Hill to climb: the Last Green's data section, now 4.36 MB → at most 2.7 MB from zero runs alone
(§B2.2 takes it further).


### A5. Pipeline tiers: what's under a condition builds second

*Needs: A1.* The runtime starts all 169 of the Last Green's pipelines at once, biggest first
(`gpu.ts:408-432`), and the first command that needs one that isn't built stalls every command
after it (`gpu.ts:993-1060`). The compiler already knows which pipelines are reached only under a
runtime condition (`driver/src/lib.rs:971-1045`, printed by `wrela pipelines`), 47 of the Last
Green's.

- **The analysis runs in the build again.** 0.7.0 moved the pipelines' post-dominator analysis (`under_condition`) out of the build so only `wrela pipelines` pays for it; the tier needs it at build time. Run it in `wrela build` once per function that records a command, cache it per function, and measure `wrela build games/last-green`'s wall time before and after (paste both; under +5 %, or the analysis is made cheaper first).
- **The manifest says the tier.** By the field recipe: `Pipeline.conditional: bool` (default
  false), true when every reaching site's `conditional` is true (`summarize` runs before `emit`
  at `lib.rs:854-867`; pass `&[bool]` into `emit_in`). `wrela pipelines` prints `tier 1` beside
  such a pipeline and `--json` carries `"tier": 0|1`.
- **The runtime builds tier 0 first.** `buildPipelines`: start tier 0's builds biggest first;
  when `Promise.all` of them settles, start tier 1's. A command that needs a tier-1 pipeline
  before its build has started bumps it (start it at once) rather than waiting for the tier. The
  executor's `built` promise resolves when **tier 0** is built (that is what `pipelines_ms`
  means from now on; `pipelines.json` gains `tier1_ms` for the rest, and `each` keeps every
  pipeline).
- **`features().pipelines_ready()`** is not added: a program can't see tiers and shouldn't; the
  arrival cover (`engine::frame`'s `Arrival`) already hides a scene until it is playable, and a
  conditional pipeline that is reached stalls only its own commands as today.
- **Tests:** a browser test with a program of two pipelines, one under `if false_at_runtime()`,
  asserts `pipelines.json`'s order (tier 0 first) and `tier1_ms > 0`; a test reaches the tier-1
  pipeline in frame 1 and asserts the frame still draws (the bump).
- **Spec:** §18.9 (tiers). **Docs:** `docs/toolchain.md`'s `wrela pipelines` row.
- **Gate:** G-suite; on `games/last-green` through `WRELA_TOOLCHAIN`, `pipelines_ms` (tier 0) is
  under the 0.7.0 number by at least the tier-1 share of WGSL bytes (state both).

**Done when:** (1) the Last Green's manifest marks `conditional: true` on exactly the pipelines
`wrela pipelines --json` lists with every site conditional (now 47; paste the count); (2)
`pipelines.json` orders tier 0 before tier 1 and reports `tier1_ms`; (3) the bump test draws its
frame; (4) `wrela pipelines` prints `tier 1` beside them; (5) through `WRELA_TOOLCHAIN`, the Last
Green's cold `pipelines_ms` (tier 0) is at most 70 % of 0.7.0's in three alternating pairs (now
7.4 s in the cache-defeated bench, 4.3–4.5 s in later runs; paste all six).
Hill to climb: tier-0 `pipelines_ms`, now 100 % → at most 70 %.


### A6. Constants fold after inlining

*Needs: nothing.* `fold_constant_branches` runs once per function before `inline_cpu`
(`opt.rs:1515-1518`) and knows bool literals, `!`, loads of locals that hold known bools and
`if`s on them (`:1942-2032`). A predicate function whose body folds to `false` still returns an
unknown value to its caller until it is inlined, and nothing folds after that. That is why
`CLUSTERED`'s 17 pipelines survive (`docs/toolchain-gaps.md`, "A branch under a false constant").

- **Run the fold again after inlining:** in `inline_cpu`, after the postorder `inline_into` and
  `tidy` and before `remove_unreached`, loop `fold_constant_branches` + `tidy` over every function
  until no function changes (at most 4 rounds; count them in a debug log). Add to `fold_block`:
  `Let(v, Binary(And | Or, a, b))` where one operand is a known bool (`false && x` is `false`;
  `true || x` is `true`; `true && x` is `x`'s value if known), and a known comparison of two
  integer literals (`Eq, Ne, Lt, Le, Gt, Ge`) for the `len() == N` pattern after the length is a
  literal. Nothing else: no arithmetic folding (wasm-opt does it for the CPU; GPU code has
  `flatten_gpu`'s own rules).
- Then `recorded_pipelines_only` (`lower/src/lib.rs:525-528`) drops the pipelines whose records
  are gone, as it does today.
- **Tests:** a conformance test with `const ON: bool = false`, `fn on(s) -> bool { ON && s.n == 3
  }` and a dispatch under `if on(s)` asserts `wrela pipelines --json` lists no such pipeline and
  the manifest has none; the frozen programs' manifests are unchanged (nothing in them was dead).
- **Docs:** `docs/toolchain-gaps.md`'s entry moves to "Fixed" in §B15.
- **Gate:** G-suite; `wrela pipelines games/last-green` through `WRELA_TOOLCHAIN` lists 153, not
  170, with `CLUSTERED` false (the 17 are named in the commit).

**Done when:** (1) the dead-pipeline conformance test passes (no pipeline in the manifest, none
in `wrela pipelines --json`); (2) `wrela pipelines games/last-green` through `WRELA_TOOLCHAIN`
lists 153, not 170, with `CLUSTERED` false, and the 17 are named in the commit; (3) the frozen
programs' manifests are byte-identical; (4) the fold's rounds are at most 4 on every member of
this workspace (a debug counter, pasted).
Hill to climb: the Last Green's pipeline count, now 170 → 153.


### A7. `const` parameters of entry points, as pipeline-overridable constants

*Needs: A1, A5.* `.bind(x: value)` packs every argument into the uniform block
(`lower/src/gpu.rs:849-863`); a `Quality` switch is a uniform branch in every shader that reads
it; the only build-time constants a draw knows are its render state (`render_constant`, E0603).
WGSL has pipeline-overridable constants (`override`, the WGSL spec §9), which naga 30 carries in
`Module.overrides` and WebGPU sets per pipeline through `constants:`.

- **The form.** A GPU entry point's parameter may be `const`: `@compute(64) fn shade(..., ambient:
  const bool, taps: const u32)`. A `const` parameter is a `bool`, `u32`, `i32` or `f32`. At a
  `bind`/dispatch/draw, its argument must be a literal or a `const` item (the `render_constant`
  rule, extended: E0603's message names the parameter). Inside the entry point it is an ordinary
  value; a branch on it is a uniform branch today and a dead branch after this step.
- **Lowering.** `PipelineKey` gains `consts: Vec<(String, ConstValue)>` for both `Compute` and
  `Render`; two records with different values are two pipelines; `recorded_shader` evaluates the
  argument as `render_constant` does. The WGSL writer emits `override <name>: <type>;` for each
  (`naga::Override` **without** an `@id`, so WebGPU's `constants` map is keyed by the name, as the
  manifest is; WGSL §7.2.2 would otherwise require numeric keys), and the manifest's `Pipeline`
  gains `"constants": {"name": value}` (the field recipe). The branch on a `const` parameter
  stays in the WGSL text; the driver's compiler folds it at pipeline creation, which is where the
  value is fixed. Both hosts pass them: `createComputePipelineAsync({ compute: {
  module, entryPoint, constants } })` and `renderDescriptor`'s `vertex.constants`/
  `fragment.constants` (`gpu.ts:282-286, 127-139`); native `compilation_options.constants`
  (`runtime/native/src/gpu.rs:1143-1149, 2674, 2691`). The shader module is created once per
  pipeline today and stays so; the win is the dead code, not module sharing.
- **The pipeline key in `pipelineKey`** (`gpu.ts:352-356`) covers the constants since they are in
  the manifest entry's JSON.
- **`wrela pipelines`** lists each constant set as an instantiation: `shade::<ambient = true,
  taps = 4>`.
- **Tests:** a conformance test dispatches a kernel with `flag: const bool` as `true` and
  `false`, reads back both (the results differ as the kernel's arithmetic says) and asserts two
  pipelines in the manifest with their `constants`, on both hosts with no validation error; a
  test passes a runtime value and gets E0603; a microbench dispatches the `const` form and a
  uniform-branch twin of the same kernel 1,000 times and records both times (no threshold: the
  record shows the driver folded it, or didn't). A browser test asserts the pipeline is created
  with name-keyed `constants`.
- **Spec:** §18.1 (`const` parameters), §18.9 (what makes a pipeline).
- **Gate:** G-suite; `compiler/tests/frozen` unchanged.

**Done when:** (1) the two-variant test passes with two manifest entries carrying `constants`;
(2) the runtime-value test gets E0603 naming the parameter; (3) the microbench's two times are
recorded; (4) the browser test sees name-keyed `constants` in the pipeline descriptor and the
native host passes them (a native test reads back the kernel's result for both values with no
validation error); (5) `wrela pipelines` prints the instantiation with its constants;
(6) the frozen programs are unchanged.
Hill to climb: none; §B5 and §B13 use it.


### A8. Tapes that can hold a creature, and a tape derived from a compiled field

*Needs: nothing (std and the IR).* `std::stage::Tape` is 128 scalar ops of 18 kinds
(`stage.wrela:16-36`), interpreted on the GPU as a 2,064-byte uniform; it has no floor, select,
mix, table read or noise, so no creature or built thing can be a tape. The audit's §5.2 and
§3.1.4: one tape-driven extraction pipeline set serves every kind at load, and the per-kind
kernels (2.4 MB of the Last Green's WGSL, the four slowest compiles) move out of the cold start.

**A8a. The tape grows.**

- `TAPE_LEN = 4096`; `Op` keeps 16 bytes: `code: u32` holds the opcode in its low 16 bits and a
  third register in its high 16; `a`, `b` registers; `c: f32`. A `Tape` of 4,096 ops is 64 KiB +
  16 bytes, over the uniform limit (`MAX_UNIFORM_BUFFER_BINDING_SIZE`), so it binds as read-only
  storage, which `bind_uniform` already does for a block over 64 KiB (`gpu.rs:2540-2545`); a short
  tape costs the same bytes (the type is fixed-size). A second type `ShortTape` (`TAPE_SHORT =
  256`, 4 KiB, uniform) keeps the lens's run-time fields cheap; both implement `Surface`.
- New opcodes, each defined by one line in `apply` (`stage.wrela:72-93`): `FLOOR`, `FRACT`,
  `STEP` (`a < b ? 1 : 0`), `SELECT` (`a > 0 ? b : third`), `MIX` (`a + (b - a) * third`),
  `CLAMP01`, `POW` (`pow(a, c)`), `LOG`, `MOD` (`a - b * floor(a / b)`), `LOAD` (`table[u32(c) +
  u32(clamp(a, 0, b))]`, a constant table read; `b` is a register holding the count), `LERP_LOAD`
  (linear between two entries at `a`), `NOISE3` (`std::field::value_noise(vec3(r[a], r[b],
  r[third]) * c)`, calling std's own noise, so the tape and the compiled field agree bit for bit
  on the GPU), `FBM3` (`fbm` with `c` holding `octaves` in its integer part and `gain` in its
  fraction, `0.5` by default), `HASH` (`hash_u32` of `bitcast_u32(floor(a))` combined with
  `floor(b)`, `floor(third)`, as `lattice` does), `DOT3` (`r[a]·r[b]` over three consecutive
  registers each), `LEN3`, `NORM3` (writes three registers). The tape gains a constant table
  `consts: [f32; 2048]` for `LOAD` (lofts' radii, harmonics). Register `COPY` and the rest stay.
- `interpret` loops to `tape.len` as today; `prune` learns the new ops' intervals through
  std's `interval` of `apply`, as now (`stage.wrela:247-265`), and prunes `SELECT`/`STEP` whose
  condition's interval is one-signed.
- **Tests:** each new op against its std twin at 10⁶ random inputs on both targets, bit-exact
  for `NOISE3`/`FBM3`/`HASH` (they call the same code) and within 1 ulp else; a 4,096-op tape
  binds and runs on both hosts; `prune` on a `SELECT` tape.

**A8b. `std::derive::tape`: a compiled field becomes a tape.**

- std: `@intrinsic pub fn tape(f: fn(vec3) -> f32) -> Tape {}` in `derive.wrela` beside
  `gradient`, and `Surface::to_tape(self) -> Tape` in `field.wrela` as `tape(|p| self.distance(p))`.
  Also `tape_with<T: GpuData>(f: fn(vec3) -> (f32, T)) -> Tape`, the analogue of
  `value_gradient_with`: the tape's last `1 + n` registers are the distance and `T`'s `n` floats
  (`T` is scalars and vectors of `f32`, flattened; anything else is E0708), and
  `interpret_with::<T>(tape, p) -> (f32, T)` reads them. That is how a field's channels (a
  colour, a material) travel with its distance, so a tape can cook colours as a compiled field
  does.
  `Lang::TapeOf = "std::derive::tape"` in `sema/src/defs.rs:576-582`; it joins
  `derived_callables` (`effects.rs:542-551`) so the derived effects apply.
- lower: `DeriveKind::Tape` (`instance.rs:34-46`); `derive.rs:22-27` maps it; `signature()` returns
  the lowered `Tape` ADT; `build()` calls `ir::derive::tape(m, cache, inner, ncap, target)`; add
  the arm at `lower/src/gpu.rs:1180`.
- ir: `derive/tape.rs`, a `Builder` driven by the AD activity analysis with `p` active
  (`mod.rs:177-189, 320, 706-720`). It runs **at build time on the CPU target only** (a tape is
  made once and uploaded): inactive statements run as ordinary code (they compute the concrete
  values: a part's placement, a radius, a seed), and each active value becomes a register. The
  emitted ops: `Binary` add/sub/mul/div; `Builtin` min, max, abs, sqrt, sin, cos, exp, log, pow,
  floor, fract, step, mix, clamp (as `CLAMP01` after scaling, or `MIN`/`MAX`), `length`,
  `dot`, `normalize` on `vec3`s (three registers each: a `vec3` active value is three registers,
  a `vec4` four); `smooth_min` (std's `smin`, recognized by path) → `SMIN`; `select`/`if` on an
  active condition → both sides emitted and `SELECT` (an `if` whose both sides are straight-line
  code; a branch with side effects on inactive state is E0708, "a tape can't hold this branch");
  a call to `std::field::value_noise`/`Noise::value` on an active vector → `NOISE3`/`FBM3`; a
  table read at an active index → the table's values copied into `consts` and `LOAD`/`LERP_LOAD`;
  loops with a count known at tape time (a loft's sections, a creature's parts) unrolled; any
  other active construct (an active loop count, recursion, an active write to captured state) →
  E0708 naming the construct and the line. A tape over 4,096 ops or 2,048 constants → E0709 with
  the counts.
- **Tests:** the derivation corpus's fields (`compiler/tests`: the ones `interval` is tested on)
  each `to_tape()` and agree with their compiled twin within 1e-5 at 10⁶ points on both targets
  (the spec's existing tape test, now over every corpus field, not three); the tape's `gradient`
  (derived from `interpret`) agrees with the field's within 3.4e-4 relative; E0708 and E0709
  tests; a tape of a corpus creature (the herd's grazer, if the corpus has it; else a loft of 20
  sections with a displacement) is under 2,048 ops and the number is recorded in the test.
- **Spec:** §14.4 (fields built at run time; tapes from compiled fields; the op set; the errors),
  §20.3 (`to_tape`).
- **Gate:** G-suite. §B2.5 measures what it buys; if a creature of the Last Green can't tape
  (E0708), the op set grows in 0.8.0 before the tag, not in a 0.8.1.

**Done when, A8a:** (1) `TAPE_LEN = 4096`, `ShortTape`, `consts` and every new opcode exist
with a test per opcode against its std twin at 10⁶ inputs on both targets (bit-exact for
`NOISE3`, `FBM3`, `HASH`; within 1 ulp otherwise; paste the worst error per op); (2) a 4,096-op
tape binds (the manifest shows a storage binding) and runs on both hosts; (3) `prune` folds a
`SELECT` whose condition is one-signed; (4) the cost of `interpret` is measured: nanoseconds per
op per point on the M4 for a 1,024-op tape over 10⁶ points (paste it; it is §B2.5's budget).
**Done when, A8b:** (5) `std::derive::tape`, `tape_with`, `Surface::to_tape` exist and every
corpus field's tape agrees with its compiled twin within 1e-5 at 10⁶ points on both targets
(paste the count of fields and the worst difference); (6) the tape's `gradient` agrees with the
field's within 3.4e-4 relative; (7) E0708 and E0709 tests pass and name the construct; (8) a
loft of 20 sections with a displacement tapes under 2,048 ops (paste the count); (9) through
`WRELA_TOOLCHAIN`, a test in this workspace lists every creature and built thing of
`games/last-green` (the fawn, the wolf, the tower, the posts, the egg stones, the crag beds, the
three bank prototypes, and whatever else `blockouts::built` yields) with its tape's op and
constant counts, and none fails with E0708 (paste the table). A kind that fails names the
construct, and the opcode set grows in this release until the table is complete.
Hill to climb: the count of the Last Green's kinds that tape, now 0 → all; the longest tape,
→ under 4,096 ops.


### A9. `wrela precision`: where `f16` is safe, from the interval derivation

*Needs: nothing.* The interval walker widens every result by its target's rounding
(`interval.rs:44, 472-569`: `F32_EPS`, per-op ulps). An `f16` build of the same walk says, per
operation, how wide the result gets at half precision.

- **The command:** `wrela precision <pkg> <fn> --box "<lo>..<hi>[,...]" [--tol 1e-3] [--json]`,
  registered in `compiler/cli/src/main.rs:92-216`. `<fn>` is a GPU-callable function of scalars
  and vectors (an entry point's helper, a field's `distance`); `--box` gives each input's range
  (the box the interval is taken over; several boxes may be given, comma-separated, for a sampled
  domain).
- **The walk:** a new `Mode::Half` beside `Interval`'s `gpu` widening: `eps = 2⁻¹¹`, `tiny =
  2⁻¹⁴` (f16's smallest normal), `inf = 65504`, and each op's ulps as the GPU table has them;
  every `widen_k` call records `(span, op, width_f32, width_f16)` through a hook on the `Ivx`
  (`:331-358`). The derived function is built for the CPU target and run at build (as `wrela
  solve` runs code) over the boxes.
- **The report:** per operation, in source order: file:line:col, the op, the value's f32 width,
  its f16 width, their ratio, and a verdict: `safe` (f16 width ≤ `tol` × max(|value|, 1)),
  `watch` (≤ 10×tol), `unsafe`; and a summary: the share of ops that are `safe`, and the
  **first unsafe op** on each path. A value that overflows 65504 or underflows `tiny` anywhere
  in the box is `unsafe` with "range".
- **It decides nothing by itself:** the engine's §B13.4 writes `f16` twins by hand where the
  report says `safe`, and G-precision measures them. 0.7.0's council refused *deriving* intervals
  in f16 (E0700: the rounding margins aren't proven sound); this report doesn't derive in f16. It
  walks the f32 interval as today and adds f16's rounding as extra widening per op, and its
  verdicts are advisory: the measured twin (G-precision) is the proof, never the report.
- **Tests:** `x * x + y * y` over `[-1, 1]²` is `safe` at `tol 1e-3`; `length(p) - 1000.0` over
  a box near 1000 is `unsafe` (cancellation at f16's 2⁻¹¹); a value noise's lattice hash is
  `unsafe` by range (its integer path), while `mix` of its corner values is `safe`; the report's
  JSON shape.
- **Docs:** `docs/toolchain.md`, "Watching and measuring".
- **Gate:** G-suite.

**Done when:** (1) `wrela precision` is registered and documented; (2) the four tests pass
(`safe` for `x*x + y*y`, `unsafe` cancellation, `unsafe` range for the lattice hash with `safe`
`mix`, the JSON shape); (3) a run on `std::field::value_noise` over `[-8, 8]³` prints at least
one `unsafe` op by range and names its line; (4) a run on a corpus field's `distance` finishes in
under a minute on the M4 (paste the time).
Hill to climb: none; §B13.4 climbs with it.


### A10. `gather`

*Needs: nothing.* Four depth compares of one texel quad are four `sample_compare_level`s today
(`shadow.wrela:125-149`); WGSL's `textureGather` fetches the quad's one component in one
instruction, and naga's writer supports it (`gather: Some(component)`, `wgsl/src/lib.rs:1519`).

- std: `Texture<F>::gather(s: Sampler, uv: vec2, component: u32 = 0) -> vec4` (the four texels'
  one channel, in WGSL's order), `TextureArray::gather(s, uv, layer, component)`,
  `DepthTexture::gather(s: Sampler, uv) -> vec4` (the four depths) and
  `DepthTexture::gather_compare(s: ComparisonSampler, uv, reference) -> vec4`
  (`textureGatherCompare`), the array forms too. Any GPU code (no derivatives involved).
  `component` is a build-time constant (E0603 otherwise; WGSL requires it).
- lower: `Lang` entries and `T::Gather { component } / T::GatherCompare`; WGSL emission sets
  `gather: Some(SwizzleComponent::…)`.
- **Tests:** a conformance test gathers a 2×2 of known values and asserts the order; a depth
  gather-compare against a known depth.
- **Spec:** §18.8.
- **Gate:** G-suite.

**Done when:** (1) the gather-order and gather-compare tests pass on both hosts; (2) the emitted
WGSL of the test contains `textureGather`; (3) spec §18.8 names the four forms.
Hill to climb: none; §B6c uses it.


### A11. Conservative depth, if WGSL can say it

*Needs: A1.* A fragment that writes `WithDepth` turns early-Z off for its draw (the impostors,
the soft raster's merge). WGSL's `fragment_depth` language extension lets a shader declare its
depth only ever grows (or shrinks) so early-Z stays on; Chrome 155–156 has it temporarily
disabled on every platform for a Windows compiler bug (§9).

- std: `WithDepthGreater<C>` and `WithDepthLess<C>` beside `WithDepth<C>` (`gpu.wrela:245-248`),
  same fields; the draw's manifest entry gains `"depth_mode": "greater" | "less"` (the field
  recipe). Lowering treats them as `WithDepth` for every check (`sema/src/gpu.rs:326`
  `writes_depth`).
- WGSL: **if naga 30.0.1 can emit the attribute** (check `naga::ir` for a conservative-depth
  binding on `FragDepth`; if absent, check whether a newer naga in the 30.x line can, and bump
  within the lockfile's policy), emit it and the `requires fragment_depth;` directive, and the
  browser requests the extension only when `navigator.gpu.wgslLanguageFeatures.has("fragment_depth")`,
  else strips the directive and attribute from the source before `createShaderModule` (a text
  rewrite of two known lines). If naga can't emit it at all, this step records a gap in
  `docs/toolchain-gaps.md` ("conservative depth") and ships the std types as plain `WithDepth`
  so the engine's code can name its intent now.
- **Tests:** a browser test on a device with the feature asserts the shader module compiles with
  the directive; the conformance test draws with `WithDepthGreater` and reads back the same image
  as `WithDepth`.
- **Spec:** §18.1.
- **Gate:** G-suite; the commit says which of the two outcomes happened.

**Done when:** either (a) the browser test on a device with `fragment_depth` compiles the module
with the directive and the conformance test draws `WithDepthGreater` as `WithDepth` does, or
(b) `docs/toolchain-gaps.md` has the "conservative depth" entry with naga's version and the
missing binding named, and the types compile as plain `WithDepth`; (c) in both cases the commit
says which outcome and why.
Hill to climb: none; §B5.6 uses it if (a).


### A12. The screen's colour: HDR and wide gamut

*Needs: A1.* The canvas is configured `{ format, alphaMode: "opaque" }` (`worker.ts:120-125`)
with no colour space or tone mapping; Chrome has had `toneMapping: { mode: "extended" }` with an
`rgba16float` canvas since 129 and ships `srgb-linear`/`display-p3-linear` canvas colour spaces in
155–156 (§9).

- std: `std::gpu::screen(Screen { hdr: bool = false, gamut: Gamut = Gamut::Srgb })` (`Gamut::{Srgb,
  DisplayP3}`), a command (`ConfigureScreen`, by the command recipe) a program may record at any
  time; `std::gpu::screen_is() -> Screen` (nondet: what the host configured, since a display may
  refuse HDR).
- browser: on `ConfigureScreen`, reconfigure the canvas: `format: hdr ? "rgba16float" :
  preferred`, `colorSpace: gamut == DisplayP3 ? "display-p3" : "srgb"`, `toneMapping: { mode: hdr
  ? "extended" : "standard" }`, keep `alphaMode`. The screen's pipelines (manifest `screen: true`
  targets, built in the device's format at load, `gpu.ts:298-320`) get a **variant** for the new
  format, created async; the swap happens when every screen pipeline has it (until then the old
  configuration stays). Test mode ignores the command (it reads back `rgba8unorm`). Native:
  wgpu's surface configure with `TextureFormat::Rgba16Float` where the surface supports it, else
  the command is a no-op and `screen_is()` says so.
- **What a program must do:** in HDR, the fragment of a screen pass returns **linear** values
  with headroom (1.0 is SDR white; the browser maps "extended" to the display); the program's
  tonemap moves from an sRGB curve to a linear output with its own highlight roll-off (§B13.3).
- **Tests:** a browser test configures HDR and asserts `canvas.getContext` reports the format
  and tone mapping (`getConfiguration()`), and that a 2.0 white reads back as 2.0 in an
  `rgba16float` readback of the screen texture; `screen_is()` returns what was set.
- **Spec:** §18.8 ("The screen").
- **Gate:** G-suite.

**Done when:** (1) `std::gpu::screen` and `screen_is` exist; (2) the browser test reads
`rgba16float`, `extended` and `display-p3` from `getConfiguration()` after the command and a
2.0 white back from the screen texture; (3) `screen_is()` reports what was set and `false` for
`hdr` where the device refuses; (4) test mode's goldens are unchanged (`wrela test` on
`engine/tests/frame` passes untouched); (5) the native host configures `Rgba16Float` where the
surface allows and reports otherwise.
Hill to climb: none; §B13.3 uses it.


### A13. Relaxed SIMD outside deterministic code

*Needs: nothing.* Every CPU function uses standard SIMD; relaxed SIMD (fused multiply-add,
relaxed lane ops) is refused module-wide (`wasm/src/lib.rs:603-662`) because `@deterministic`
code must be bit-exact across engines. The frame's CPU time (4–5 ms on the Last Green) is
presentation code, which is not held to bit-exactness.

- **Which functions:** `check_deterministic` (`lower/src/lib.rs:1021-1060`) already walks the call
  graph from every `@deterministic` root; its reach set is the deterministic set. Every function
  outside it, **and not reachable from a job the sim resumes nor from the audio thread** (both
  are deterministic by construction: `@audio` and `@deterministic` roots), may be relaxed. The
  set is computed per instance after inlining (`inline_cpu` merges bodies: a relaxed callee
  inlined into a deterministic caller must not relax it, so the flag is the **caller's**: a
  function is relaxed only if it is outside the set, and inlining into it inherits its flag).
- **The option:** `wrela build --relaxed-simd` (and `wrela profile --relaxed-simd`), default off
  (D16 decides the release default after §B13.5 measures). On: `Options.relaxed: HashSet<FuncId>`
  threaded into `vec_binary` (`ops.rs:487-550`): a `Mul` followed by `Add`/`Sub` on `f32x4`
  lanes in a relaxed function emits `F32x4RelaxedMadd`/`RelaxedNmadd`; `min`/`max` emit the
  relaxed forms; nothing else. `FEATURES` gains `--enable-relaxed-simd` when on.
  `check_no_relaxed_simd` runs on the **emitted** module per function (the emitter knows each
  function's flag) and is skipped after wasm-opt when the option is on (wasm-opt may move code
  across functions; the emitter's check is the guarantee). `@deterministic` code's bytes are
  unchanged: a test compares the tick log (`--compare-ticks`) of a build with and without the
  option.
- **Browsers:** Chrome 114+ and Firefox 146+ run relaxed SIMD; Safari doesn't (§9). The build's
  loader checks `WebAssembly.validate` of a one-function relaxed module at start and, failing
  that, loads the **standard** build: a relaxed build is written beside the standard one
  (`game.relaxed.wasm`), and the manifest names both; no compressed file is written for either (D26). The size budget counts the
  larger.
- **Tests:** a conformance test marks `f` deterministic and `g` not, both with `a * b + c` on
  vectors; the WASM has a relaxed op in `g` and none in `f`; `--compare-ticks` of a ticked program
  with and without the option is equal; a browser test loads the fallback when the validate fails
  (mocked).
- **Spec:** §11.2 (SIMD: "relaxed outside deterministic code, when asked").
- **Gate:** G-suite; the frozen `herd`'s CPU frame time with and without, recorded (no claim).

**Done when:** (1) the conformance test finds a relaxed op in `g`'s WASM and none in `f`'s;
(2) `wrela-host --compare-ticks` of a ticked program built with and without the option prints
"equal"; (3) the fallback-load browser test passes; (4) `optimize.rs` passes `--enable-relaxed-
simd` only when the option is on and a build without it is byte-identical to 0.7.0's; (5) the
frozen `herd`'s CPU frame time with and without, seven alternating pairs, is in the PR (no
threshold here; D16 decides in §B13.5).
Hill to climb: none here.


### A14. Serving on the LAN

*Needs: nothing.* The servers bind `127.0.0.1` (`loopback.rs:47`), and the test page writes its
results back with `PUT results/<name>` (`testmode.ts:193-197`), so a device on the LAN can run a
bench page and report, if the server listens.

- `std::tool::serve` gains `lan: bool = false` (and `wrela run --lan`, `wrela tool serve --lan`):
  bind `0.0.0.0`, print each interface's `http://<ip>:<port>/` with the page's hash, and accept
  `PUT results/` from any peer **only under `--lan`** (the live server's existing size and
  connection limits hold: 64 open requests, 10 s per request, 256 MiB per result). COOP/COEP
  headers stay, so WebGPU and `SharedArrayBuffer` work over plain HTTP only for `localhost`:
  document that a LAN device needs the page served over HTTPS or Chrome's
  `--unsafely-treat-insecure-origin-as-secure=<origin>` flag for the device's run (the bench's
  procedure in §B8 says which). The tool capability `browser = true` is unchanged (the agent's
  own browser is local; a remote browser is a person's).
- `std::tool::browser` gains `navigate(b, url)`: the open browser loads `url` in its tab (the
  same profile, so its caches are warm) and a following `run(b, Script::new().wait(Until::done(),
  …))` waits for that page's `DONE`. The bench's warm run (§B1.4) is this: the same page a second
  time. If the host's driver already exposes a navigation, wrap it; if not, add it to
  `runtime/native/src/tool/browser.rs` through the devtools protocol's `Page.navigate`.
- Test mode gains `show=1`: at the run's end the page draws its summary (the paced intervals'
  p50/p95/p99, the count over 33 ms, `playable_ms`, `pipelines_ms`, the adapter's description) as
  text on the canvas and leaves it there, for a device that can't `PUT` (a phone or a tablet on a
  served release page; §B8).
- **Tests:** a tool test starts `serve` with `lan: true` and GETs the page from the machine's
  own non-loopback address; a PUT from it lands in `results/`; a tool test opens hello-field,
  navigates to it again and reads a second `results/`; a browser test with `show=1` reads the
  summary's pixels back as not black.
- **Docs:** `docs/toolchain.md`, "Running a program".
- **Gate:** G-suite.

**Done when:** (1) `wrela run --lan` and `std::tool::serve(lan: true)` bind `0.0.0.0` and print
every interface's URL; (2) the tool test GETs the page from a non-loopback address and a PUT
lands in `results/`; (3) the navigate test reads a second `results/`; (4) the `show=1` test
reads non-black summary pixels; (5) `docs/toolchain.md` has the LAN notes (the secure-context
flag for Chrome, the release page for Safari).
Hill to climb: none; §B1.4 and §B8 use it.


### A15. Per-frame labels in a paced run, and the worst frames named

*Needs: nothing.* In a `timestamps=1` run every pass already gets timestamp writes
(`gpu.ts:984-988`, `Timer.keep`), and nothing rejects `timestamps=1&paced=1&inflight=2`
(`worker.ts:635-741`); `timings.json`'s rows carry `frame` (`GpuTiming`, `gpu.rs:59-73`). What is
missing is the join and the report.

- `wrela profile --chrome` gains `--worst n` (default 5): after the paced run, read
  `timings.json` (the paced run now requests `timestamps=1` too) and `frames.json`, and print the
  `n` frames with the longest intervals, each with its interval, its CPU ms, its `phases` and its
  labels' GPU ms sorted, largest first. `--json` carries them as `"worst": [...]`.
- `frames.json` rows gain `"labels": [[name, ms], ...]` when timestamps are on, written by the
  worker from the same `GpuTiming`s (`worker.ts:806`), so a tool reads one file per frame.
- The native host's `wrela profile --worst` does the same from its own timer.
- **Tests:** a tool test runs `wrela profile --chrome --worst 2` on `examples/hello-field` and
  asserts the shape; a frame row has `labels`.
- **Docs:** `docs/toolchain.md`.
- **Gate:** G-suite.

**Done when:** (1) `wrela profile --chrome --worst 2` on `examples/hello-field` prints two frames
with their intervals, CPU ms, phases and labels; (2) a paced run's `frames.json` rows carry
`labels` when `timestamps=1`; (3) the native profile prints the same; (4) `docs/toolchain.md`
names the flag.
Hill to climb: none; §B1.2 uses it.


### A16. Integration against the engine, before the tag

*Needs: A1–A15.* With `WRELA_TOOLCHAIN=~/projects/wrela-compiler` (this branch) in `~/projects/wrela`:

1. On `main`'s code, unchanged: `wrela suite` passes; for every program in §B0's list a build with
   this branch against a build with 0.7.0 holds **G-pixels (identical)** and **G-sim**, each run
   by its own release's host. `wrela migrate --dry-run` on every member reports nothing (Part A
   declares no migration).
2. On the `engine/world-class-2` branch, §B0–§B2 as drafted build and pass their gates.
3. `engine/tests/toolchain-0.7` (kept in Part B as the capabilities' living test) uses every new
   capability once: a program opened twice shows the warm compile (A2), a page served
   compressed on the fly (A3, read `load.json`'s wire bytes), a zero-run constant (A4), a tier-1 pipeline bumped (A5), a folded
   predicate (A6), a `const` parameter in two variants (A7), a corpus field's `to_tape()` realized
   through `interpret` and compared with the compiled field (A8), a `wrela precision` run on one
   of its functions (A9, a tool test), a `gather` (A10), a `WithDepthGreater` draw (A11), a
   `screen(Screen { hdr: true })` round trip (A12), a relaxed build's tick log equal to the
   standard's (A13), a `--lan` serve (A14, tool test), a `--worst` profile (A15). It runs `hosts:
   both` and with `features=none`.
4. `wrela run games/capybara` starts in the Claude app's browser pane and a mouse press on
   `Start` starts the game (no regression on 0.5.1's fix).

Faults found here are fixed in Part A before the tag. Record what ran in the PR.

- **Gate:** all four, recorded.

**Done when:** the four items above are recorded in the PR with their numbers: (1) the suite,
the per-program G-pixels and G-sim hashes; (2) B0–B2's gates as drafted; (3)
`engine/tests/toolchain-0.7` passing with `hosts: both` and with `features=none`, and the A8b
kind table complete; (4) the browser-pane check. A fault found here is fixed in Part A before
the tag; the PR lists each.


### A17. Spec, docs and the release

*Needs: A16.*

- `spec/language.md` covers every addition above in the same voice as the rest: §11.2 (relaxed
  SIMD), §13 (zero runs), §14.4 (tapes), §15 (the manifest's ABI), §18.1 (`const` parameters,
  conservative depth), §18.8 (`gather`, the screen), §18.9 (tiers, constants), §23.9 (`wrela
  precision`, `--worst`, `--lan`, `--report data`). `wrela doc std::stage::Tape`, `std::gpu::Screen`
  etc. answer. `wrela primer gpu` mentions tiers, `const` parameters and tapes.
- `docs/toolchain-gaps.md` in this repository: "A branch under a false constant still makes its
  pipelines" moves to "Fixed" (A6), naming 0.8.0 and the PR; a "conservative depth" gap is added
  if A11 found naga can't emit it.
- `docs/citations.md` (compiler): the WGSL specification's sections for overrides,
  `textureGather`, `fragment_depth`; the WebGPU specification's canvas configuration;
  the V8 code-caching note; the relaxed SIMD proposal.
- `tools/check.sh` passes in full on the M4 Air and on the Linux CI host; record which ran.
- `docs/release.md` §2 step 5: the agent syntax test once with the release's compiler; its
  headline numbers in the PR.
- Open the PR per `docs/release.md` §2; Ryan merges; tag `v0.8.0` per §3; check the release holds
  both tarballs and `SHA256SUMS`.
- **Gate:** the release exists and `wrela toolchain` downloads it in this repository.

---

**Done when:** (1) every spec section named above mentions its new item (grep each item's name
in `spec/language.md`); (2) `wrela doc` answers for `std::stage::Tape`, `std::gpu::Screen`,
`std::derive::tape`, `Texture::gather`; (3) the gaps entries moved; (4) `tools/check.sh` passes in
full on the M4 Air and on the Linux host, counts pasted; (5) the agent syntax test's headline
numbers are in the PR; (6) `v0.8.0` is tagged, the release holds both tarballs and `SHA256SUMS`,
and `wrela toolchain` in this repository downloads it.


## Part B: the engine, the studio and the games (`rywible/wrela`)

Branch `engine/world-class-2` from `engine/world-class` once B13's review fixes are committed
there (the checkout held uncommitted edits to `engine/frame.wrela`, `cull.wrela`, `virtual.wrela`
and others on 2026-10-25; they are part of B13, not of this plan). Every step below is one
commit. Part B's B0–B2 and B8 change no image (G-pixels identical or within the number stated);
every other step says what it changes and attaches stills.

### B0. Pin 0.8.0 and save the baselines

- `wrela.toml`: `toolchain = "0.8.0"`. `wrela toolchain` says so, and downloads it.
- `wrela migrate <member>` on every member (`--dry-run` first); Part A declares no migration, so
  the expected result is no change; commit whatever it does change with the pin, and say so.
- `docs/toolchain.md`: the host path in its examples moves to `0.8.0`.
- Build the baselines once, on the branch's code with 0.8.0, into `build/base-0.7/<name>` for
  `clearing`, `last-green`, `capybara`, `herd`, `climber-lab`, `leapling`, `clusters` (`wrela
  build <pkg> -o build/base-0.7/<name>`). `build/` is under `.gitignore`.
- **Gate:** G-suite; G-pixels identical and G-sim for every program against the same code built
  with 0.7.0, each run by its own release's host (the two-host form). The pin alone changes
  nothing the eye sees or the sim does; if a pixel moves, the commit says which and why.

**Done when:** (1) `wrela toolchain` names 0.8.0; (2) `wrela migrate --dry-run` reports nothing
on every member (or the commit lists what it changed); (3) `build/base-0.7/` holds the seven
builds; (4) for each of the seven, the two-host G-pixels identical and G-sim hashes are pasted
in the commit.


### B1. Measurement first

Nothing in B3–B14 is claimed without this step's instruments. Each item is its own commit.

1. **The bench keeps every run.** `studio/bench/main.wrela:191-333` runs four pages into one
   `results/` that each run overwrites. After each `run_page`, copy `results/` to
   `out/<report-slug>/<scene>/<run>/` (`gpu`, `play`, `cold`, `warm`), and write the paths into
   the report JSON (`report.wrela:133`). `timings.json`, `frames.json`, `counters.json`,
   `memory.json`, `pipelines.json`, `load.json` and `log.txt` survive every run from now on.
2. **The hitch table.** The play run adds `timestamps=1` to its hash (A15 makes a paced run
   carry labels per frame). `report.wrela`: `Hitch { frame: u32, interval_ms, cpu_ms: f64,
   phases: Vec<(String, f64)>, labels: Vec<(String, f64)>, draws: u32, written: u64 }` for every
   frame whose interval is over 33 ms, from `frames.json` and `counters.json`; `markdown` prints a
   "Hitches" table under each scene with the five heaviest labels of each; `docs/bench.md` gets
   it. A frame with no label over 2 ms and a CPU time under 2 ms is marked "unattributed" (a
   browser stall), and the count of those is a number the gates watch.
3. **Phases.** `engine/frame.wrela`: `std::time::phase("cook")` at the start of `draw`
   (`:1372`), `phase("choose")` before `scene.choose` (`:2103`), `phase("passes")` before the
   prepass, `phase("finish")` at `finish`'s start, `phase("present")` before the develop; the
   games' `frame` exports call `phase("sim")` round their snapshot read. A `PHASES` const in
   `frame.wrela` names them. `frames.json`'s `phases` stop being `{}`.
4. **The warm run.** `studio/toolkit/page.wrela:22-78` `run_page` gains `again: bool`: after the
   first `DONE`, `browser::navigate(b, url)` (A14) and a second wait; the bench's fourth run opens
   the cold build again **without** `salt` and reads the second `load.json` and `pipelines.json`
   into `warm_playable_ms`, `warm_pipelines_ms`, `warm_loaded_bytes` (new `Measured` fields and
   columns).
5. **The pass viewer: `wrela tool passes`.** A new tool package `studio/passes` (declared in
   `wrela.toml` with `read = ["ws=."]`, `write = ["out=build/passes"]`, `browser = true`): `wrela
   tool passes -- <pkg> --frame <n> [--input <script.json>] [--size WxH]` builds the package,
   drives it `n` frames natively with `std::tool::drive` (`Step::Frames`, then `Commands` for the
   last frame, and the frame's GPU timings with timestamps on), and writes
   `build/passes/<pkg>/index.html` plus a JSON: for the frame, every pass and dispatch group in
   order with its label, its pipelines' names (from the manifest), its targets (name, format,
   size), its draw or dispatch counts and its GPU time; render passes whose target a capture can
   give (the golden mechanism's `pass: "label"`, through `drive`'s capture of a labelled pass if
   `std::tool::drive` exposes it; else the final frame only, and a `docs/toolchain-gaps.md` entry
   asking for per-pass captures) get a thumbnail. The page is served by `std::tool::serve` and
   opened in the browser pane; a row's thumbnail enlarges on click. Minimal and static; the
   profiler timeline overlay the audit wished for is this page's second version, later.
6. **Per-pipeline compile times in the table.** From `pipelines.json`'s `each`: the ten slowest,
   with their WGSL sizes, under each scene's cold row in `docs/bench.md`.
7. **The claim's section.** `docs/bench.md`'s "Whether §0.1's claim holds" becomes "Whether D10
   holds", restated per device and per scene, filled by §B16.
8. **`wrela tool lookbook --quality`: the lookdev instrument.** The lookbook gains a reference
   mode and measurements, so a look step has a number to climb and a floor it may not fall under:
   - `Quality::reference()` in `engine/frame.wrela`: a preset with `scale 1.0`, `pace false`,
     every switch on, probes at `RAYS` 128 and filling, the shadow maps at their finest (after B6,
     level 0 pages everywhere in view and `Pcss` at 64 taps; before it, the cascades at 4096²),
     fog at twice the froxels and four times the samples once B10 lands, the TSR off; the capture
     path (`engine/shot.wrela`) accumulates 64 jittered frames into one still (the jitter grid is
     `view.wrela`'s 4×4 extended to 8×8).
   - **Time is frozen for every measurement**: the capture path's still mode (`engine/shot.wrela`,
     `Debug.still`) pauses the sim and holds the frame's time, the wind, the clouds' drift, the
     water's drift and the particles, so the reference's 64 samples differ only by their jitter
     (an instantaneous truth, not a motion blur) and the measures below count only what the
     renderer does, never intended motion.
   - `--quality [--shots a,b,c]`: for each named shot (all 26 by default), renders the reference
     still and the play still (the play mode is the frame as a player sees it, settled for 64
     frames), then: 64 more play frames with the camera still for **flicker** (mean over pixels
     of the temporal variance of luma); a 10 s dolly at 0.5 m/s along the shot's look direction,
     the camera the only thing moving, for **pop** (the largest mean absolute frame-to-frame luma
     difference) and **still_pop** (the same measure over the 64 still frames: pop's comparator,
     in pop's units); **trail** (at four camera positions along the dolly where the reference is
     rendered too, the distance in pixels behind the sharpest moving edge over which the play
     frame's luma differs from the reference's by more than 2/255: a ghosting measure that
     frame-to-frame differences can't give, since history blending lowers them while it trails);
     **clipped** (the share of pixels with any channel at 1.0 in the play still); **PSNR** and
     **SSIM** (Wang et al. 2004, 8×8 windows on luma) of play against reference.
     Writes `quality.json` per shot and prints a table; `docs/bench.md` gets a "Lookdev" table
     under the scene.
   - **What the reference bypasses, and what it can't.** The reference mode replaces the TSR by a
     64-sample supersample and the shadow pages' caching by the finest pages everywhere with a
     64-tap PCSS: for those two, play against reference measures the approximation. For the
     probes, the fog and the cascades the reference is the same algorithm at higher settings
     (more rays, more froxels, more samples), so it measures convergence, not bias; the rubric and
     Ryan carry those. The table in `docs/lookdev.md` says which is which.
   - **Negative controls**, tests that keep the instrument honest: a play render of `02-beech-high-forest` (a
     detail-rich fixture shot, named so the control can't be run on a flat scene) blurred by a
     3×3 box must lose at least 1 dB of PSNR although its flicker falls; a play render with a
     deliberate ghost trail (the resolver's blend forced to 0.02) must lengthen `trail` to at
     least 4 px and lose PSNR at the dolly's reference positions, while `pop` may stay or fall; a sharp moving feature in the frozen-time reference must stay sharp (its
     edge width within 1 px of the play still's); and a **detail-removal control**, the grass
     density halved in both play and reference, which PSNR may reward: it is the blind judge's
     calibration pair (G-look (4)), and the control's standing is "degraded" in `quality.json`
     whatever its PSNR says, so a step can't cite a PSNR gain from removed detail.
   - **The rubric** (`docs/lookdev.md`, new, allowed; **proposed** until Ryan edits it), scored
     blind by a separate judging agent on each pair shown in a random order (G-look (4)): five
     rows scored 0–3 per shot, before and after, and a preference: **light** (direction read, falloff, bounce
     where the eye expects it), **colour** (harmony, saturation held, haze toward the distance),
     **value** (a full range, forms readable by value alone: judge the still desaturated), **depth**
     (overlap, scale cues, atmosphere), **artifacts** (aliasing, flicker, leaks, popping, tiling;
     3 is none seen). The judging agent writes the scores and one sentence per row; when
     `WRELA_HERO_DIR` names Ryan's private hero set (the 83 images `docs/citations.md` describes),
     it views them beside the pairs first. The scores go in the PR; Ryan's verdict is final.
   **Done when:** the tool runs over the 26 shots on B13's state and `docs/bench.md` records the
   numbers as the **lookdev baseline**; a test with a deliberate quarter-resolution play render
   lowers PSNR by at least 3 dB and a deliberate per-frame noise raises flicker (the instrument
   reacts); the three negative controls pass; `docs/lookdev.md` exists.
- **Tests:** a tool test runs the bench on `examples/hello-field` for 30 frames with fake sites
  and checks the report has the new fields, the hitch table (possibly empty) and the kept run
  directories; `wrela tool passes` on hello-field writes a page with its passes.
- **Gate:** G-suite; one bench run of the Last Green's walk whose hitch table names each of its
  frames over 33 ms with a heaviest label (recorded in `docs/bench.md` as the "B1 attribution";
  it is the evidence B4 works against); `wrela tool passes games/last-green --frame 300 --input
  build/gates/LG_WALK.json` opens and lists the 22 passes.

**Done when:** (1) after `wrela tool bench --scene last-green-walk`, `out/<slug>/last-green-walk/
{gpu,play,cold,warm}/` each hold `frames.json`, `timings.json`, `counters.json`, `memory.json`,
`log.txt` (and `load.json`, `pipelines.json` for the cold and warm runs), and the report JSON
names the paths; (2) the walk's hitch table in `docs/bench.md` names every frame over 33 ms with
its five heaviest labels, and the "unattributed" count is printed (the B1 attribution); (3) the
Last Green's `frames.json` rows carry at least five phase names with non-zero sums; (4) the
report and `docs/bench.md` have the warm columns with numbers; (5) `wrela tool passes
games/last-green --frame 300 --input build/gates/LG_WALK.json` writes a page whose pass count
equals the frame's recorded passes (22 today; paste it) with labels, targets and times, and
opens in the browser pane; (6) the ten slowest compiles are under the cold row; (7) the
"Whether D10 holds" section exists with its per-device, per-scene shape; (8) the lookdev baseline
row is in `docs/bench.md` and the instrument's two reaction tests pass.
Hill to climb: the "unattributed" count in the walk's hitch table, → 0 (every hitch named).


### B2. Cold start

The chain today (`docs/world-class-next.md` §2.1): 7.4 s of pipelines (frame 1 waits for the
last), then 26 plant kinds at two a frame, `Wood::new`, three bank prototypes one a frame, every
built thing one a frame with a readback, "casters complete", the probes' first bake in eleven
strips: "playable at frame 220" while "floor ready" prints at frame 41. Each item is a commit.

1. **Compressed at the edge, checked.** No pre-compressed file is added to the release (D26):
   Cloudflare's edge compresses responses whose `Content-Type` is on its list (`application/wasm`,
   `text/plain`, `text/javascript` and `application/json` are; §9) when the client accepts it and
   the response isn't already encoded. Check it: deploy the branch's release to a test worker and
   run `curl -sI -H 'Accept-Encoding: br, gzip' <origin>/releases/<id>/last-green/game.wasm`, and
   the same for a `pipeline_N.wgsl` and for `manifest.json`; each must answer `content-encoding:
   br` or `gzip`. If one doesn't, the fix is the Worker's `Content-Type` for that suffix
   (`studio/release/site.wrela:326, 408`, the `CONTENT_TYPES` table) or a Cloudflare compression
   rule, never a file; a type Cloudflare won't compress is recorded for Ryan in the commit and in
   `docs/bench.md`. Paste the headers and the wire sizes (`content-length` of the encoded
   responses) in the commit. The bench's local server already shows wire bytes (A3).
2. **The big constants become files.** Run `wrela build games/last-green --report data` (A4).
   For each constant over 256 KB whose readers are presentation or load-time code (not the tick's
   first frames: check with `wrela query games/last-green "callers <NAME>"`), move it behind
   `std::io::Shipped`: the build keeps the constant's bytes in a shipped file (`ship("data",
   [(name, bytes)])` from a `const` of the serialized value, as `BAKED` ships the floor's tiles,
   `games/last-green/history.wrela:82`), and the program loads it with `Shipped::fetch` on first
   use, decoded with `std::serialize`. Candidates from the audit: `OBSTACLES`, `VIEWS_GRID`,
   `TOUR`, `COURSES` (`history.wrela:173-615`), the UI's baked glyph fields (`ui/text.wrela:25-28`,
   `ui/brand/manrope.wrela:6`: ship them per font, loaded when the first glyph is drawn; the
   title screen waits for its font, which the cover hides). Record the section's size before and
   after in the commit; target under 1 MB (**estimated**).
3. **Tiers and the fold.** With 0.8.0's A5 and A6: `wrela pipelines games/last-green --json`
   lists the tier of each pipeline; paste the count of tier 0 and tier 1 and the 17 `CLUSTERED`
   pipelines' absence in the commit. Make the floor's optional systems reach their pipelines
   under conditions by structure where they don't yet (the shafts, the mist, the harness's map
   overlay already are; particles' `admit` is conditional; check the probes' `trace_again` and
   the lights' `bin`: the floor supplies no lights, so `bin` can be under `if !lights.is_empty()`
   in `frame.wrela:2059`).
4. **The load chain, un-serialized.** `games/last-green/stage.wrela:2009-2088` (`cook_load`) and
   the `Scene` hooks:
   - kinds cook as many a frame as the planner allows (`Work::Kind` measured, B4.6), not
     `KINDS_PER_FRAME = 2`; the loading budget is 60 ms (`cook.wrela:132-136`);
   - built things and bank prototypes no longer wait for the wood: move their branches above
     the `None` arm of the wood match; they need only pipelines (through tapes after item 5);
   - `Wood::new` runs the frame the last kind lands; the first placed tile's `w.load` happens the
     same frame (today a frame later);
   - the near cascade's first set starts when the first tile within 60 m is placed (`Kept::new`
     already starts at once; make `casters_ready(far: false)` true then, `stage.wrela:285-289`),
     and the far cascade's when the far layer is in;
   - the probes' first bake needs casters complete, the near map and the air and dome settled
     (`frame.wrela:1811-1832` in 0.6.3's numbering); none of those changes, but each is now
     reached earlier; `STRIP_COLUMNS` (`probes.wrela:862`) rises to 1,280 while loading (the
     loading budget allows it) and returns to 640 when playable;
   - `streamed` (`:1508-1530`) is unchanged.
   Write the order as a dependency table in a doc comment above `cook_load`, so the next reader
   sees it: `kinds → wood → tiles placed; pipelines → built, prototypes; built ∧ prototypes ∧
   tiles(60 m) → near cascade; near ∧ air ∧ dome ∧ casters → probes → playable`.
   **Tests:** `games/last-green/tests`: a `@test(frames: 400, hold: [("files/floor/far.bin",
   200)])` asserts built things are realized before the wood is complete and that `playable`
   prints by frame 150 **estimated** (replace with the measured frame, then assert it doesn't
   regress by more than 20 frames).
5. **The realizer through tapes.** `engine/realize.wrela` gains the tape path beside the generic
   one: `realize_rigid_tape(tape: Tape, parts: GpuSpan<Tape>, bounds: Box3, surface: look::Mesh)
   -> Mesh` and, for creatures, `realizing_tape` (a `@job` like `realizing`, `:832-852`) over the
   same slices, with kernels `cull_blocks_tape`, `place_vertices_tape`, `skin_vertices_tape`,
   `cook_colours_tape`, `cook_materials_tape` that take `Tape`s where the generic kernels take
   `F: Parts<C>`: the union tape (`field.to_tape()`) for the surface; one tape per part
   (`part_distance` of each, `spec/language.md` §23.3) for the skin weights and the part masks;
   `tape_with` of the look's channels (A8b) for colours and materials. Each tape is pruned per
   block with `tape.prune(block)` in `cull_blocks_tape` (a pruned tape per live block, stored in
   a `GpuBuffer<ShortTape>` when it fits, else the whole tape), which is the engine's part masks
   in tape form. `Cast::realize` (`character.wrela:483-508`) and `realize_rock`
   (`stone.wrela:323-334`) take a `through: Through { Compiled | Tape }`; the game chooses `Tape`
   for the first realization of every kind at load and `Compiled` for later re-realizations (an
   edit in the lens, a level change), so the compiled per-kind kernels are reached only under a
   runtime condition and fall to tier 1 (A5). A kind whose field is E0708 under `to_tape` keeps
   `Compiled`, and the commit names it.
   **Tests:** `engine/tests/realize-tape` (new): the herd's grazer realized through `Compiled`
   and `Tape` gives meshes whose vertices agree within 1e-4 m (the tape agrees with the field
   within 1e-5) and the same quad count; the fawn's tape is under 4,096 ops (print the count);
   the time of the fawn's tape realization on the M4 in frames (print it).
   **Gate:** G-pipelines: tier 0 of the Last Green holds no `cull_blocks`/`place_vertices`/
   `skin_vertices`/`cook_*` instantiation but the tape ones (state the counts); G-cold: the
   opening's `pipelines_ms` (tier 0) under 2.5 s (**estimated**); the fawn's tape realization
   completes within 90 frames at load (**estimated**; if over 120, the fawn keeps `Compiled` in
   tier 0 and the commit says so with the numbers).
6. **Warm open, measured.** With B1.4: the Last Green's warm `playable_ms`, three runs, in
   `docs/bench.md`.
- **Gate (whole step):** G-cold: the Last Green's opening and walk playable within 5 s, the
  clearing within 2.5 s; G-warm: the Last Green within 3 s; G-pixels identical on every scene
  against `build/base-0.7` (nothing here changes an image; the first frames may differ in *when*
  things appear, so compare frame 600 of the walk and 480 of the shot); G-sim.

**Done when, per item:** (1) the three `curl -sI` answers from the test worker show
`content-encoding` on the WASM, a WGSL file and the manifest, pasted with their encoded
`content-length`s; (2) the data section is at most 1 MB (now 4.36 MB; A4's zero runs took it to
at most 2.7; paste `--report data` before and after) and the `playable` frame moved by at most
5 frames; (3) tier 0 and tier 1 counts pasted, the 17 `CLUSTERED` pipelines absent, and `lights::bin`
conditional; (4) the dependency table sits above `cook_load`, the held-fetch test passes, and
"playable at frame N" is at most 120 (now 220; paste every intermediate value as the chain's
links are moved); (5) tier 0 holds no per-kind realizer instantiation except the kinds the
commit names, the fawn's tape op count is pasted, the fawn's tape realization completes within
90 frames at load (paste the frames), and the tape and compiled meshes agree within 1e-4 m;
(6) three warm `playable_ms` values are in `docs/bench.md`.
**Done when, the step:** (7) G-cold: the Last Green's opening and walk `playable_ms` median of
three at most 5,000 ms (now 10,100–14,100) and the clearing's at most 2,500 (now 4,000);
(8) G-warm: the Last Green at most 3,000 ms (now unmeasured); (9) G-pixels identical at frame
480 of the shot and 600 of the walk against `build/base-0.7`, and on every other scene; (10)
G-sim on the Last Green and the capybara.
Hill to climb: `playable_ms` cold, 10,100 → 5,000; after every item, run the cold bench once and
paste `playable_ms`, `pipelines_ms`, `loaded_bytes` and "playable at frame N".


### B3. The clipmap's cover apart from its height

A tile's field write re-cooks every level whose square it overlaps, height and normal included
(`engine/terrain.wrela:364-407, 440-467`), though fields enter only `cover_with` (`:48-52`).

- **Two texels.** `Texel` (`:141-150`) splits into `Relief: Copy + GpuData { height: f32, normal:
  Oct32 }` (8 bytes) and `Cover: Copy + GpuData { cover, meadow, flowers, under, flora, materials:
  Unorm8x4 }` (24 bytes) in two buffers, `relief` and `covers`, with the same `STRIDE` indexing
  (`:162`). `Levels` is unchanged. Every reader takes both spans: `ground` and `ground_shadow`
  (`:934-942`: `relief` only), `ground_shade` (`:1203`: both), `ground_cover_at` and `height_at`
  (`:1106-1127`), the grass's `place` (`grass.wrela:204-217`), the probes' `trace`
  (`probes.wrela:291-307`: `relief` only), the clipmap's `cast` (`:558`), the water, and
  `Lighting.levels`' users through a new `Lighting.covers: GpuSpan<Cover>` beside the relief span
  the frame already passes as `texels`.
- **Two cooks.** `cook_level` (`:825-837`) becomes `cook_relief<T>` (height, `normal`,
  `substrate_height`) and `cook_cover<T>` (`cover_seen` with the fields), each over a rectangle:
  `moved: vec2i` becomes `rect: vec4i` (lo, hi in texels). `Clipmap` keeps per level a dirty
  rectangle for the cover (`dirty: [Option<vec4i>; MAX_LEVELS]`, the union of the frame's
  `touched` rectangles in that level's texel grid) and the old `cooked`/moved logic for the
  relief. `follow` dispatches `cook_relief` only on a move or `again` (the two-cell copy as today)
  and `cook_cover` only over the dirty rectangle (groups `ceil(w/8) × ceil(h/8)`), and over the
  moved rows and columns on a move. `touched(lo, hi)` (`:398-407`) sets the rectangles, not
  `cooked[l] = false`. `Change::Ground` dirties both, whole.
- **A field write costs its own texels.** A 256 m tile at level 5 (spacing 4 m) is 64×64 cover
  texels; at level 11 (256 m) one texel; the sum over levels is under 6,000 texels of cover,
  against ~116k texels of relief plus cover today.
- **Tests:** `engine/tests/frame`: the fixture writes one tile's fields and asserts
  `Clipmap::cooked_texels()` (a new counter: relief and cover texels dispatched this frame) is
  the tile's cover texels only; a move of two cells still cooks 258 relief texels; G-pixels
  identical on the clearing's path and the Last Green's walk against `build/base-0.7` (the
  representation changes; the image must not).
- **Gate:** G-suite; G-pixels identical (clearing, Last Green walk and shot); G-profile (Chrome):
  the opening's `terrain cook` label p50 falls by at least 70 % (**estimated**; record the number
  and the new absolute); G-hitch: no frame of the walk names `terrain cook` as its heaviest label.

**Done when:** (1) `Relief` and `Cover` exist, every reader listed compiles against both spans,
and `Lighting.covers` is bound; (2) the fixture's one-tile field write dispatches cover texels
only, at most 6,000 of them, and 0 relief texels (`Clipmap::cooked_texels()`, pasted); (3) a
two-cell move still cooks 258 relief texels; (4) G-pixels identical on the clearing's path and
the Last Green's walk and shot; (5) the opening's `terrain cook` label p50 is at most 1.4 ms
(now 4.65; paste `wrela profile --chrome --compare` three pairs); (6) the walk's hitch table
names no frame with `terrain cook` as its heaviest label.
Hill to climb: `terrain cook` p50, 4.65 → at most 1.4 ms; if a measurement stalls above it,
check in order: the cover kernel's groups equal the rectangle, `bound_sides` runs once a frame,
no level is dirtied by two writes in one frame, the relief isn't cooked on a field write.


### B4. The hitch sources

Each item is a commit. The evidence per item is the B1 attribution table.

1. **The far layer, gated.** `games/last-green/stage.wrela:1897-1965`: the 65 baked tiles' far
   crowns are placed and loaded in one frame. Place at most `FAR_PER_FRAME = 8` a frame, each
   under `c.planner.may(Work::Tile)`, nearest first; `casters_ready(far: true)` waits for the
   last.
2. **The sky, sliced.** `engine/sky.wrela`: the clouds' noise (`shape` 128³, `detail` 32³) and
   the weather and shadow maps (1024²) cook whole ("a frame that cooked the air and the clouds
   together took 137 ms", `frame.wrela:1721`). Cook them as `std::job`s like the air's
   `cook_slice` (`air.wrela:568`): the shape in 8 slabs of 16, the detail whole, the weather map
   in 4 bands, the shadow map in 4 bands; `Work::SkyNoise` and `Work::SkyWeather` are asked per
   slice (`Planner::may`, not `note`), with measured costs (item 6). The first frame that needs
   the sky waits for the slices as it waits for the air's today.
3. **Built things as jobs.** `blockouts::realize` → `realize::realizing_rigid`, a `@job` over
   `SLICE_BLOCKS` (`realize.wrela:1108`) like `realizing`, one slice per `may(Work::Built)`; the
   `read(mesh.tally)` (`stone.wrela:323-334`) becomes a `Pending` polled the next frame and the
   `fit` applied when it arrives. Through tapes after B2.5, the same job.
4. **One field write a frame.** The `while` over finished tile jobs (`stage.wrela:1732-1795`)
   consumes one job per frame when playable (all of them while loading or capturing); the
   neighbours' refresh jobs it spawns count against `IN_FLIGHT`.
5. **The far strips.** Their 11 ms (`cook.wrela:112-115`) go with B6; until then a frame whose
   heaviest label is `shadows far` is excused from this step's gate and listed.
6. **Measured costs for every kind.** `frame.wrela:679-685`'s `MEASURED` labels gain `"terrain
   cook"`, `"air"`, `"sky noise"`, `"sky weather"`, `"kind"`, `"built things"`, `"tile"`,
   `"realize"` (the labels exist or are added where the work is recorded); `Planner::measure`
   runs for each; `costs()` (`cook.wrela:102-127`) keeps estimates only as first guesses and the
   commit pastes the ladder's measured values from the opening and the walk.
7. **The first optional step, bounded.** `Planner::may` (`:203-216`): the rule that the first
   optional step of a frame runs whatever its size holds only for a step whose cost is at most
   twice the playing budget; a kind over that must be sliced (after items 2 and 3, every kind
   is), and a `debug_assert` says which if one isn't.
- **Tests:** `engine/tests/frame`'s load fixture asserts no frame's planned work exceeds
  `2 × budget` once playable, and that the sky's cook spans at least 8 frames; the Last Green's
  test asserts the far layer lands over at least 8 frames.
- **Gate:** G-hitch on the walk and the opening: at most 2 frames of 1,200 over 33 ms, each
  named, none attributed to `terrain cook`, `built things`, `kind`, `sky noise`, `sky weather`
  or a tile's placement (`shadows far` excused until B6); G-pixels within 2/255 (work lands on
  other frames); G-sim.

**Done when, per item:** (1) the far layer lands over at least 8 frames (test) and
`FAR_PER_FRAME` is 8; (2) the sky's noise and weather cook over at least 8 frames (test) and no
frame's sky labels sum over 6 ms in the opening (paste the maximum); (3) `realizing_rigid` is a
`@job`, the tally readback is a polled `Pending`, and no frame of the load has a `built things`
label over 2× the loading budget's share (paste the maximum); (4) the one-write-a-frame test
passes; (5) frames whose heaviest label is `shadows far` are listed, not counted (B6 removes
them); (6) `costs()` holds measured rows for every kind with the ladder's values pasted, and
`MEASURED` names them all; (7) the bounded-first-step assertion is in and never fires on the
opening and the walk.
**Done when, the step:** (8) G-hitch: the walk has at most 2 frames of 1,200 over 33 ms and the
opening at most 2 of 480, excluding `shadows far` frames, each named (now 9 and 8); (9) the walk's
paced p99 is at most 22 ms (now 29; B6 finishes it); (10) G-pixels within 2/255 on the clearing
and the Last Green; (11) G-sim.
Hill to climb: frames over 33 ms in the walk, 9 → at most 2; paste the hitch table after each item.


### B5. The frame

Each item is a commit with its own numbers; the step's gate is the opening's GPU frame.

1. **Grass: a prepass, a pyramid test, a kept placement.**
   - Blades join the depth prepass: `grass.wrela:1770-1778` draws the three tiers with a
     `blade_cut` fragment (the frond `discard` of `blade_shade`, `:1627-1634`, nothing else)
     into `ids` and depth; the shade pass draws them at `depth: view::PREPASSED` as the flowers
     are.
   - Cells are tested against the pyramid: `Place` (`:106-115`) gains `hzb: Hzb`, the kernel
     binds `pyramid: Texture<R32Float>`, and a cell whose bounding sphere (its 8 m square, the
     tallest blade high) `cull::occluded` (`cull.wrela:173-217`) says is hidden places nothing;
     `Stage::choose` passes `pyramid` and `hzb` through (`stage.wrela:386-398`).
   - A ring is kept: five per-ring `AppendBuffer<Blade>`s (the three tiers' buffers split by
     ring; `tier_of(k)` says which shader) and per-ring `DrawIndexedArgs`; `Grass::place` re-places
     ring `k` only when `cells(k, cam)` moved since its last placement, when 8 frames have passed
     (D21: the wind's phase and the probes' light are sampled at placement), or when the probes'
     version changed. The frame exposes `Frame::probes_version()`.
   - Footprints (`stones`, `:248-255`) and the cloth contact are read at placement today and
     stay so; a kept ring applies the cloth contact in the vertex shader already
     (`cloth_contact`, `:1406`).
   **Tests:** a frame fixture counts `place` dispatches over 64 still frames (at most 5 × 8 plus
   one per probe version); G-pixels within 2/255 on the clearing's path.
2. **One bounds list; culling for every kind.** `engine/cull.wrela` gains `Bound: Copy +
   GpuData { centre: vec3, radius: f32, kind: u32, payload: u32 }`, `Bounds` (a `GpuBuffer<Bound>`
   the scene fills in `choose` through `Cooks`-like access), and `@compute(64) cull_bounds(cam,
   hzb, pyramid, bounds: [Bound], lists: mut Append<u32> per kind …, args: mut Slots<
   DrawIndexedArgs>)`: frustum then pyramid (`occluded`), writing per-kind lists and arguments.
   The Last Green registers: built things (`Thing`, one bound each, `stage.wrela:485-505`), bank
   placements (`Instances`, `:513-528`: today a CPU draw per placement), rocks (already culled
   on the GPU: route through the one kernel), water tiles (the water mesh laid in 64 m tiles at
   `lay` time, `land/water.wrela:69`, each a `WaterTile { first, count, bound }`), characters
   (one bound each; the CPU `meets_view` stays as the early-out). Draws: built things one
   `draw_multi` per material class; bank stones one `draw_multi`; water one `draw_multi` over its
   visible tiles; the water's `copy_below` and its pass are skipped when no tile is in the
   frustum (a CPU test over at most 64 tile bounds) and otherwise the copy is scissored to the
   union of the visible tiles' screen rectangles (`pass.scissor`). The terrain's sides and the
   wood keep their own paths (they are already GPU-driven).
   **Tests:** `engine/tests/cull` extends: a scene of 10,000 bounds of four kinds draws only the
   visible of each kind, read back; the water copy is absent from the recorded commands when the
   camera looks away from every tile (`std::tool::drive`'s `Commands`).
3. **Fewer full-screen passes.** `lines` (`finish`) folds into the resolve: `Shared::resolve`'s
   fragment calls the lines helper on its own output (`lines`'s fragment becomes a function of
   `(scene, depth, px)`), and the `scene_b` swap for lines goes; the water's copy is scissored
   (item 2); `mist_over` (`stage.wrela:590-612`) waits for B10 (it folds into the froxel apply);
   the bloom's chain skips levels narrower than 64 px (none at 1080p; a 720p phone has three).
   Record the pass counts before and after with `wrela tool passes`.
4. **Cloud rays (D17).** `engine/sky.wrela`: the march runs over a `marched` buffer of
   `SKY_W/2 × SKY_H/2` (2048×640) with the same 32 phases (40,960 rays a frame, not 163,840);
   `scatter` (`:616-625`) upsamples bilinearly into the 4096×1280 dome with the IGN jitter's
   offset so the temporal blend (`:825`) still converges; the light loop (`:504-511`) exits early
   when the optical depth passes 4 (transmittance under 2 %) and takes the last four light
   samples at twice the step. **Hypothesis:** pixels within 2/255 on the clearing's sky.
5. **A probe clipmap.** `engine/probes.wrela`: `ProbeGrid` gains a toroidal origin
   (`wrap: vec2u`); `index(i, j, k)` (`:151`) and `held_at` (`:727-782`) apply it; on an eye move
   past `ProbeQuality::moves` the grid shifts by whole columns (`Baked::shift(dc, dr)`): the
   slots and the maps keep their contents, the density volume shifts by voxels the same way,
   `cook_occluders` (`:183`) runs over the new slab only, and only the new columns are queued for
   `bake` strips. `renew` (`:927`) stays for a size change. The first bake is unchanged.
   **Tests:** `engine/tests/probes-renew` extends: a 30 m move bakes `new columns × LAYERS`
   probes, not all 52,488 (read back `baked`), and `held_at` at a point that stayed in the grid
   returns the same `Held` before and after the shift within 1/1024.
6. **Early-Z for depth-writing fragments.** If A11 shipped the attribute: a draw may declare
   `WithDepthGreater` only where every fragment's written depth is nearer than or equal to its
   rasterized depth (reversed-Z: numerically greater), or WGSL may give an indeterminate depth.
   Establish the bound per draw, never blanket: the soft raster's merge draws its full-screen
   triangle at `z = 0.0`, the far plane (`virtual.wrela:1869` draws it at 0.5 today), so every
   stored depth is at least it; an impostor card is drawn at its instance's **far** bound (the
   bounding sphere's far plane along the view) and its fragment's depth only moves nearer, so the
   promise holds. `engine/tests/render/conservative-depth` (new) draws each such draw twice, with
   `WithDepth` and with `WithDepthGreater`, over 10,000 fragments and asserts identical depth
   results and `written ≥ rasterized` for every fragment (both depths through a debug target).
   If A11 recorded a gap, nothing to do.
- **Gate (whole step):** G-profile (Chrome): the opening's GPU frame p50 under 12 ms and the
  walk's under 9 ms with every switch on (**estimated**; if the opening misses, say by how much
  and which label); G-pixels within 2/255 on the clearing and the Last Green (item 4 may move
  the sky within 2/255; attach the difference image); G-draws: the Last Green's walk frame 600
  records under 30 logical draws (state the number); G-hitch holds; G-stills.

**Done when, per item:** (1) the still-frame fixture counts at most 5 × 8 `place` dispatches
in 64 frames plus one per probe version, blades appear in the prepass's `ids`, and the opening's
`vegetation` label p50 is at most 2.4 ms (now 3.93; paste after the prepass, after the pyramid
test, after the kept placement: three numbers); (2) the four-kind cull test passes, the walk's
frame 600 records at most 30 logical draws (paste), bank stones are one `draw_multi` (now one
CPU draw per placement), and `Commands` shows no water copy when the camera looks away;
(3) `wrela tool passes` shows at most 20 passes and at most 11 full-screen (now 22 and 13);
(4) the `clouds` label p50 is at most 1.2 ms (now 2.35) with the clearing's sky within 2/255,
or, if the half-size march breaks 2/255, the early-out alone at most 1.9 ms and D17 recorded as
off; (5) a 30 m move bakes `new columns × 8` probes (read back `baked`) and `held_at` is stable
within 1/1024; (6) if A11 shipped it, the merge draws at `z = 0.0`, the impostors at their far bound, the
conservative-depth test passes for both, and both return `WithDepthGreater`; else a note.
**Done when, the step:** (7) G-profile: the opening's GPU frame p50 at most 12.0 ms (now 18.9)
and the walk's at most 9.0 (now 12.6), three pairs each, every switch on; (8) G-pixels within
2/255 on the clearing and the Last Green except the sky's change under (4), with difference
images attached; (9) G-draws as (2); (10) G-hitch as B4's; (11) G-stills.
Hill to climb: the opening's GPU frame p50, 18.9 → 12.0 ms; paste the span after every item,
with the label that fell.


### B6. Shadow pages

The two cascades are cached, strip-replaced 2048² maps (`engine/shadow.wrela:260-405`); a tile
arriving marks the near one stale whole (`stage.wrela:1999-2004`), the far strips cost 11 ms in
Chrome, and the near texel is 7 cm. Pages (Epic's virtual shadow maps, §9; UnityVSM and Sakmary
2025 as implementations outside Epic) replace both cascades in three sub-steps, each a commit;
the moving casters keep their own 1024² map (`frame.wrela:1460-1468`) throughout.

**B6a. A page table and footprint invalidation over the maps we have.**

- `engine/shadow.wrela`: `PAGE = 128`; a cascade's map becomes a **pool** of `(side / PAGE)²`
  pages (16×16 for 2048²) with a `table: GpuBuffer<u32>` per cascade: per page, bit 31 valid,
  bit 30 requested, bits 0–15 the pool page it lives in (identity in B6a). `mark_pages`, a
  `@compute(8, 8)` over the previous frame's depth (`last_depth`) at scene size: each pixel's
  world position projects into the cascade (`sun_view`, `:53`) and sets its page's requested bit
  (an `Atomics<u32>` or per page). `invalidate(lo: vec2, hi: vec2)` (a CPU call that writes a
  small list the GPU applies, or a kernel over the rect) clears the valid bit of every page whose
  world rectangle overlaps `[lo, hi]`; the Last Green calls it with a tile's **caster bounds**
  (the union of its placed trees' crown bounds, computed when the tile is placed) in place of
  marking the cascade stale (`stage.wrela:1999-2004`), and with a changed caster's bounds on
  `Change::Casters`.
- Each frame, under `Work::ShadowPages` (new; measured), at most `PAGES_PER_FRAME = 16`
  requested-and-invalid pages are drawn, nearest the eye first: for each, `pass.viewport` and
  `pass.scissor` to the page's rectangle in the pool and `scene.cast` with a view matrix of the
  page's world square (`sun_view` with the page's centre and span); `choose_casters(cam, lo,
  hi)` (`stage.wrela:303-311`) selects the casters in the page's rectangle (it already selects by
  a z-band; make it a rectangle). `Kept`'s strips, `strip`, `drawn` and the front/back swap
  retire; `front` is the pool.
- `lookup` (`:125-149`) reads the page table: a page that isn't valid returns "lit, unknown" and
  `sunlit` falls back to the other cascade (near → far → lit), as the `near_share` mix does.
- **Tests:** `engine/tests/render/shadow-pages` (new): a wall and a post; after one frame only
  the pages under visible pixels are requested (read `table` back); `invalidate` of a rectangle
  re-draws exactly the overlapping pages (count the draws through `Commands`); a caster added
  inside a page shows in the next frame's shadow; the stability test (`shadow-stability`) holds.
- **Gate:** G-pixels within 2/255 on the clearing and the walk against B5 (the same casters, the
  same resolution; pages land a frame or two later); G-profile: `shadows near` + `shadows far`
  labels replaced by `shadow pages` at p50 under 1.2 ms in the opening (**estimated**); G-hitch:
  no frame's heaviest label is shadows.

**B6b. A physical pool and ten clipmap levels (D13).**

- The pool becomes one 4096² `DepthTexture` (1,024 pages, 64 MiB) shared by ten levels; level
  `k` (0..9) is a 2048² virtual square of extent `8 · 2^k` m (8 m to 4,096 m; texels 3.9 mm to
  2 m) centred on the eye and snapped to its page grid; the table holds `10 × 256` entries; the
  moving map stays. Level choice per pixel in `mark_pages` and in `lookup`: the finest level
  whose texel is at most the pixel's world footprint (its `fwidth` of world position) times 1.5,
  so a near pixel reads millimetre pages and the horizon reads metre pages. Page allocation on
  the GPU: `allocate`, a kernel over the requested-and-unbacked pages that pops free pool pages
  from an `Atomics<u32>` stack, and `release`, which pushes pages no level has requested for 60
  frames (an age per page); a frame that needs more pages than the pool has draws the nearest
  and leaves the rest to fall back to the coarser level. **Coarse pages make the fallback real**
  (Epic's VSM reserves coarse pages for exactly this: receivers and fog samples that no visible
  pixel requested). Reserved, always requested, never released: **all 256 pages of level 9**
  (the whole 4 km square at 2 m texels, 16 MiB), so every receiver and every fog sample within
  the sampling domain has a page at level 9 at least; and the central 4×4 pages of each of
  levels 0–8 (144 pages), so a point inside level k's square falls back at most **three** levels
  coarser than its ideal for k ≤ 6 (level k+3's central 4×4 spans twice level k's width, which
  leaves half a width of margin each side for the levels' differently snapped centres and the
  filter's footprint; k+2 would have no margin at all), and levels 7 and 8 fall back to level 9. 400
  of the 1,024 pages are reserved and 624 serve requests; if B6b measures the requested pages'
  p95 above what 624 hold, the pool grows to 4096×6144 (`POOL_PAGES` 1,536, 96 MiB) and the
  commit says so. `POOL_PAGES` is one constant; every capacity assertion and memory gate below
  reads it: at 1,024 pages the `shadows` share is at most 68 MiB (pool 64, the moving map 4,
  the tables under 1), at 1,536 at most 101 MiB. After a cold start, a teleport (the arrival's jump), a recentre or a sun
  change, the reserved pages are drawn **first**, coarsest level first, under the loading budget
  (64 pages a frame while the arrival cover is up, 16 after), and `Frame::playable` requires every
  reserved page valid (this replaces `near.ready`). A query that finds no valid page at any level
  (only possible before the reserved pages are drawn) returns lit, and the frame test asserts
  that never happens after `playable`. The two `Kept` maps (64 MiB) go.
- Casters per page come from the one bounds list (B5.2): the cull kernel tests each caster's
  bounds against the pages to draw this frame and writes per-page draw lists, so a page draws
  only what overlaps it.
- **Tests:** the pages test extends: a camera at 1.6 m above ground requests level 0 or 1 under
  its feet and level 6 or more at 300 m; the pool never exceeds 1,024 pages (assert the stack);
  a frame requesting 2,000 pages draws 16 and shows the coarser level elsewhere without a hole.
- **Gate:** G-stills (a deliberate change: near shadows sharper; attach the clearing's
  `00-the-opening` and the Last Green's `02-beech-high-forest` pairs); G-memory: the frame's
  `shadows` share at most 68 MiB (pool + moving map + tables); G-profile: `shadow pages` p50 under
  1.5 ms in the walk with the eye moving at 3 m/s (**estimated**).

**B6c. Filtering across pages.**

- `lookup`: four taps become one `gather_compare` (A10) when the four texels lie in one page,
  and four single compares with their own page lookups across a page edge; `soft` keeps its
  meaning in texels of the level. `ShadowQuality::soft: Soft = Soft::Pcf` gains `Soft::Pcss`:
  an 8-tap blocker search at the level's texel scale, the penumbra from the blocker distance
  and `Light.sun`'s angular size (0.53° default, a `Light` field), then a 12-tap PCF of that
  width (Fernando 2005, §9); **proposed** default `Pcf`.
- **Tests:** a quad over a plane at two heights: PCSS gives a wider penumbra for the higher
  quad (measure the 10–90 % transition width in texels, read back).
- **Gate:** G-pixels within 2/255 against B6b with `Pcf`; G-profile: `Pcss` costs under 0.4 ms
  more at 1080p (**estimated**; if more, it stays a switch with the number).

**Done when, B6a:** (1) the pages test shows only pages under visible pixels requested, an
invalidation re-drawing exactly the overlapping pages, a new caster showing the next frame, and
`shadow-stability` passing; (2) `stage.wrela` calls `invalidate` with a tile's caster bounds and
never marks a cascade stale; (3) G-pixels within 2/255 against B5 on the clearing and the walk;
(4) the opening's `shadow pages` label p50 is at most 1.2 ms and the `shadows near`/`shadows
far` labels are gone; (5) the walk's hitch table names no shadow label as a heaviest.
**Done when, B6b:** (6) a camera 1.6 m up requests level 0 or 1 underfoot and level 6 or
coarser at 300 m; (7) the pool stack never exceeds `POOL_PAGES` (1,024, or 1,536 after the growth rule) and a
2,000-page request frame draws 16 and shows no hole; from an empty pool, after a 30 m teleport and after a sun step, every
receiver pixel and every fog sample point (a 32³ grid over the froxel volume's extent, read
through the same lookup) finds a valid page within 10 frames and `playable` waits for the
reserved pages (the test reads the lookup's "no page" flag back and asserts zero after frame 10); (8) the `shadows` share is at most 68 MiB at 1,024 pages or 101 MiB at 1,536 (now 64 for the
cascades plus the moving map), whichever `POOL_PAGES` the commit chose; (9) `shadow pages` p50 at most 1.5 ms in the walk at 3 m/s (paste, and the
pages drawn per frame p50/p95, the requested pages' p95 against the 624 free, and the eviction
rate per frame); (9b) the coverage test projects the whole view frustum (its FOV and aspect, to
200 m, the fog's extent) into shadow space and asserts, for every projected point after page
snapping and the filter's footprint, a **successful fallback with its level bound**: a page at
level 9 at least anywhere in the 4 km domain, and a page at most three levels coarser than the
point's ideal inside each of levels 0–6's squares (finding some page is not enough: the
returned level is asserted), from an empty pool, after a recentre and after a sun step; (10) G-stills with `00-the-opening` and `02-beech-high-forest`
pairs.
**Done when, B6c:** (11) the PCSS penumbra test shows the wider penumbra for the higher quad;
(12) G-pixels within 2/255 against B6b with `Pcf`; (13) `Pcss` costs at most 0.4 ms more at
1080p, or ships as a switch with its number.
**G-look:** shots `00`–`22` of the floor and the clearing's three: pairs, `--quality` numbers (PSNR/SSIM not down by 0.5 dB/0.01, flicker, clipped and pop not up) and rubric scores, against the previous step's record; B6b's sharper near shadows are the deliberate change.
Hill to climb: `shadow pages` p50 in the walk, → at most 1.5 ms, with pages per frame beside it;
and the near texel under the eye, 7 cm → 3.9 mm (a fact of B6b's levels, pasted).


### B7. Memory

Each item is a commit; the gate is the walk's peak.

1. **The breakdown.** `Frame::shares` (`frame.wrela:1607-1647`) and `Stage::shares`
   (`stage.wrela:682-717`) are printed by the game once at `playable` as `memory shares: name
   bytes, ...`; the bench parses the line and `markdown` prints a "Memory" table per scene;
   `docs/bench.md` gets it. The game's `shares()` adds `"realizer"` (the casts' and built things'
   mesh bytes), so `memory::evict_after` (`memory.wrela:82-84`) can fire.
2. **Impostor kinds, streamed.** `engine/wood.wrela`: a kind's atlas layers are baked when the
   first instance of the kind lands in a loaded slot (queue `Work::Kind`), and dropped when no
   loaded slot holds one for `SHRINK_AFTER` frames (`:407`), rebaked on return; the seven far
   kinds stay resident. The Last Green's `cook_load` no longer bakes all 26 up front; it bakes the
   kinds its first tiles need (the planner's order), which also shortens B2's chain.
3. **The realizer's eviction, wired.** With item 1's share, `Realizer::evict_after` drops the
   finer levels of creatures unseen for `EVICT_AFTER` frames (`realize.wrela:1135`); the herd
   test asserts the bytes fall after 600 still frames with a creature behind the camera.
4. **The dome.** With B5.4 the march buffer is a quarter; the dome texture stays 4096×1280
   `Rgba16Float` (42 MB) unless D17 is flipped; nothing to do here but record it.
5. **The occluder volume and the pages.** The probes' `volume` (`u32` atomics, 161×71×161) is
   dropped after `cook_occluders` writes `density` and re-made on a renew (today kept: check
   `Baked`, `probes.wrela:829-858`); the shadow pool is B6b's.
6. **A pool for transient targets.** `engine/frame.wrela`'s `Targets` (`:293-359`) takes its
   intermediates from a `Pool` keyed by `(width, height, format)`: `shafts`, `dabs`, the
   see-through `accum`/`reveal`/`parts` and the bloom chain are acquired when first written and
   released after their last read in the frame (explicit calls in `draw_scene`/`finish`, a
   `debug_assert` that an acquired texture isn't acquired again before release); `scene_b` and
   `below` stay (they are live across most of the frame). **Estimated** saving 30–60 MiB at
   1080p.
7. **The history and scene targets** stay `Rgba16Float`; a note in `Targets::bytes` says what
   each costs at 1080p (the audit's §3.4 numbers), so the next budget discussion starts from it.
- **Tests:** a frame fixture asserts the pool's peak is under the sum of its clients' sizes; the
  wood test asserts a kind with no instance is dropped and rebaked.
- **Gate:** G-memory: the Last Green's walk `gpu_peak` under 512 MiB (D24) and the opening under
  560 MiB; the share table in `docs/bench.md`; G-pixels within 4/255 on the walk (a far kind may
  arrive a frame later).

**Done when, per item:** (1) the game prints `memory shares:` once at `playable`, the bench
parses it, and `docs/bench.md` has the table with a `realizer` row; (2) the wood test drops a
kind with no instance and rebakes it, and `cook_load` bakes only the kinds its first tiles need;
(3) the herd test shows the realizer's bytes falling after 600 still frames; (4) the dome's
bytes are recorded; (5) the occluder `volume` buffer is dropped after `cook_occluders` (bytes
pasted before and after); (6) the pool test holds and the pool's peak is pasted; (7) the notes in
`Targets::bytes`.
**Done when, the step:** (8) G-memory: the walk's `gpu_peak` at most 512 MiB (now 781) and the
opening's at most 560; (9) the share table in `docs/bench.md`; (10) G-pixels within 4/255 on the
walk, 2/255 elsewhere.
Hill to climb: the walk's `gpu_peak`, 781 → at most 512 MiB; paste it after every item.


### B8. Devices and the others

Nothing here changes code the player runs; it makes the claim mean something.

1. **The bench on the LAN.** `studio/bench/main.wrela` gains `--lan`: it builds each scene,
   serves `build/bench/builds/` with `serve::start(dir, lan: true)` (A14) and prints, per scene,
   the four URLs (gpu, play, cold, warm) with their hashes and `show=1`; it waits for `PUT`s into
   each scene's `results/` (the same files the local runs produce) and writes the same report
   with the machine's name taken from the posted `device.json` (the adapter's description and
   the user agent). A device that can't PUT (a tablet) shows its summary on the page (A14's
   `show=1`); the person types the five numbers into `wrela tool bench --row "<machine>;<scene>;
   ..."`, which appends a row marked "by hand".
2. **The procedure**, written into `docs/toolchain.md` ("Benching another device"): install the
   toolchain's Chrome flags for the device (`--unsafely-treat-insecure-origin-as-secure=http://
   <ip>:<port>` for a page that needs cross-origin isolation over plain HTTP, or serve the
   bench's builds through the release's worker on a test subdomain for Safari and phones), open
   each URL in order, let it finish, and read the report; three cold runs per scene, the median
   kept.
3. **The rows.** For every device in D20 that is to hand: the Last Green's opening and walk and
   the clearing's path at 1080p (the device's own resolution noted if it can't), in
   `docs/bench.md` under "Devices", each with the adapter, the OS, the browser and its version,
   the date, and the command. A device not to hand gets a row saying so and why.
4. **The others' numbers.** Part C's three scenes measured on the M4 Air and on every device the
   rows above have, transcribed into `docs/bench.md`'s "Other engines on this machine" with the
   exact command and date, by hand, as the first plan's §B1 said.
- **Tests:** a tool test runs `bench --lan` with fake sites and asserts the printed URLs and the
  row's shape from a posted `results/`.
- **Gate:** G-device: the rows exist for the M1 Air, the Iris Xe laptop and the 780M machine at
  least (D20), and the competitor rows exist for the M4 Air; what the rows show is recorded in
  `docs/bench.md`'s "Whether D10 holds" by §B16, not here.

**Done when:** (1) the `--lan` tool test passes and `wrela tool bench --lan` prints the URLs;
(2) `docs/toolchain.md` has "Benching another device"; (3) `docs/bench.md` has a row for the M1
Air, the Iris Xe laptop and the 780M machine at least, each with adapter, OS, browser, version,
date and command (a device not to hand has a row saying so and why), and the iPad's and the
phone's rows if the devices are to hand; (4) the three competitor rows for the M4 Air from Part C
are transcribed with their commands and dates.
Hill to climb: devices with a row, 1 → at least 4.


### B9. Light with visibility

Each sub-step is a commit; each changes the image on purpose and attaches stills.

**B9a. A visibility term for the probes (D22).** The probes store L1 spherical harmonics with no
notion of what is between a probe and the point it lights (`engine/probes.wrela:37-49, 727-782`),
so bounce light leaks through trunks and walls and interiors are "unsolved". DDGI's answer
(Majercik et al. 2019, already cited) is a depth map per probe and a Chebyshev test.

- `Baked` (`:829-858`) gains `visibility: Texture<Rgba16Float>` (writable), one octahedral tile
  per probe: 8×8 texels for layers 0–3 and 4×4 for layers 4–7 (about 17 MB for 52,488 probes),
  `.r` the mean hit distance, `.g` the mean squared distance, `.b` the share of rays that hit
  within `Trace.reach`. `bake` and `gather` (`:403-421, 533-550`) write it: each ray's hit
  distance (the point where the march stopped: the ground, or where transmittance fell under
  0.01, `:308-361`) is splatted into the texels within the ray's cosine lobe (weight `pow(max(dot
  (texel_dir, ray_dir), 0), 50)`, DDGI's choice), blended with `keep` on a re-trace, clamped at
  `reach` for a miss.
- `held_at` (`:727-782`): each of the ten samples (two layers × five planes read as one probe
  each) gets a weight: `w = chebyshev × backface × trilinear`, `chebyshev = var / (var + max(d −
  mean, 0)²)` cubed, `var = max(mean2 − mean², 1e-4)`, `d` the distance from the probe to the
  lit point along the direction read from the tile, `backface = (dot(n, to_probe) + 1) / 2`
  squared plus 0.2 (DDGI's smooth backface term); the weights are normalized, and when their sum
  is under 1e-3 the unweighted mean is used (open sky). The probes-inside-solids exclusion stays.
- `ProbeQuality::visibility: bool = true`; off, the lookup is today's.
- **Tests:** `engine/tests/probes-visibility` (new): a wall between a lit half and a dark half of
  a box: a point 1 m into the dark half receives under 10 % of the lit half's irradiance (read
  back `sky_light` at both; today it receives about half); a closed box's inside is under 2 % of
  outside; the open meadow's `sky_light` at 50 points is within 1/255 of the unweighted value.
- **Gate:** G-stills (deliberate: trunks' undersides and the canopy's base darker; attach
  `02-beech-high-forest` and `17-the-fawn-under-the-beeches`); G-profile: `probes bake`, `probes
  again` and the lookup's share of `vegetation` rise by under 0.3 ms together (**estimated**);
  G-memory: the `probes` share rises by the tile's bytes only.

**B9b. The signed-distance volume.** The probes' rays march a density volume in 64 fixed
0.75 m steps (`probes.wrela:308-361`); the contact shadows march the screen (`reflect.wrela:
483-551`), which fails at its edges; the ambient occlusion sees only what is on screen. The scene
is made of distance fields, and a tape cooks any of them in one kernel (A8).

- `engine/distance.wrela` (new): `Distance { volume: Texture3d<R32Float> }`, 192×64×192 voxels
  of 0.5 m (96 × 32 × 96 m, 9.4 MB) round the eye with a toroidal origin (recentred by whole
  voxels when the eye moves 8 m), and `Solids`, what a scene says is solid: `capsules:
  GpuBuffer<Capsule>` (trunks and limbs: the wood's placed instances' `Tube<4>` axes, `plant.
  wrela:669-676`, as `(a, b, radius)`), `tapes: GpuBuffer<(Tape, mat4)>` (built things, bank
  prototypes, rocks, crags, through the tapes B2.5 made), and the ground as the clipmap's relief
  (distance ≈ `(y − h) / sqrt(1 + |∇h|²)`, `∇h` from the relief's normal). A new `Scene::solids
  (self) -> Solids` hook (default empty) supplies the lists; the Last Green fills them when a tile
  or a built thing lands.
- `cook_distance`, a `@compute(4, 4, 4)` over a slab of voxels: `min` over the solids whose
  bounds meet the voxel's 8 m bin (bins and their lists built on the CPU at recentre and when
  the lists change), tapes interpreted with `stage::interpret` (pruned per bin with
  `tape.prune`), capsules analytically, the ground last. A whole cook is 2.4 M voxels and runs
  in 8 slabs under `Work::Distance` (new, measured); a recentre cooks the new slab only.
- **Users, each its own commit:**
  1. `probes::trace` (`:291-363`): sphere-trace the solids through the volume (`step = max
     (d(p), 0.25)`) while marching the density volume for leaves with the same `x`; a solid hit
     (`d < 0.1`) ends the ray as the ground does today. Fewer steps on open air, exact hits on
     trunks and walls.
  2. `reflect::contact_shadow`: march the volume toward the sun for 2 m (8 steps, `step = max
     (d, 0.1)`), not the screen; no edge failure; the result multiplies the sun as today.
  3. `ambient`: beyond GTAO's 1 m radius, four cones (the normal and three at 60°) sample the
     volume at 0.5, 1, 2, 4 m, occlusion `1 − d / r` per sample clamped, averaged, multiplied
     into `ao` at half the scene's size in the same kernel; `Quality::ambient` covers both.
- **Tests:** `engine/tests/distance` (new): a sphere's volume distance against the analytic one
  within half a voxel at 1,000 points; a tape of a cuboid likewise; a probe ray toward a trunk
  stops at it (the ray's returned radiance is the trunk's, not the sky's); a contact shadow at
  the screen's edge matches one at its centre for the same geometry.
- **Gate:** G-profile: `probes again` p50 falls by at least 30 % at equal rays (**estimated**;
  record), `contact` no higher; `distance` label under 0.25 ms p50 while playable (slabs
  amortized); G-stills (deliberate: contact under the fawn's belly and at the climber's hem;
  attach `16`, `18`); G-memory: `+9.4 MB` in a new `distance` share.

**B9c. Variable-rate re-trace.** `probes again` traces 32 columns a frame on a golden spiral
(`:869-871, 1054-1091`), whatever changed. Infinity Ward's variable-rate ray tracing (SIGGRAPH
2026, §9) steers a fixed ray count to where the image changed.

- `Baked` gains `priority: GpuBuffer<f32>` per column (6,561): each frame a kernel sets
  `priority = age × (1 + 4 × change)`, `age` the frames since the column's last trace, `change`
  the sum of: the sun's movement since the trace (`|sun − traced_sun|`), the weather's version
  delta, 1 if an invalidation rectangle (B6a's, the casters' and tiles') met the column's
  reach since the trace, and 1 if the column lies under a visible pixel (marked by the same
  `mark_pages` pass B6a runs, with a second bitset for columns). A select kernel takes the 32
  highest priorities (a two-pass bucket select over 6,561 values: 64 buckets then the top bucket
  sorted) and `trace_again` runs those columns; the ray count per frame is constant.
- **Tests:** `engine/tests/probes-renew`: after a sun step, the columns under the eye re-trace
  within 12 frames (today within 205); with nothing changing, every column is traced within 205
  frames as before (age wins).
- **Gate:** G-pixels within 2/255 at steady state on the clearing; the `Sun` fixture's frames to
  convergence (the probes' version settles) halve (record both numbers).

**Done when, B9a:** (1) the three visibility tests pass (dark half under 10 %, closed box under
2 %, meadow within 1/255); (2) `ProbeQuality::visibility` defaults on and off restores today's
lookup (G-pixels identical with it off); (3) the probe labels and the lookup's share rise by at
most 0.3 ms together (paste three pairs); (4) the `probes` share rises by the tile's bytes only
(about 17 MB, pasted); (5) G-stills with `02` and `17`.
**Done when, B9b:** (6) the four distance tests pass; (7) `Scene::solids` is filled by the Last
Green when tiles and built things land (a test counts capsules and tapes after the load);
(8) `probes again` p50 falls by at least 30 % at equal rays (paste before and after), `contact`
no higher, `distance` at most 0.25 ms p50 while playable; (9) `+9.4 MB` in a `distance` share;
(10) G-stills with `16` and `18`.
**Done when, B9c:** (11) after a sun step the columns under the eye re-trace within 12 frames;
(12) every column is traced within 205 frames with nothing changing; (13) G-pixels within
2/255 at steady state on the clearing; (14) the `Sun` fixture's frames to convergence halve
(paste before and after).
**G-look:** shots `02`, `08`, `16`, `17`, `18`: pairs, `--quality` numbers (PSNR/SSIM not down by 0.5 dB/0.01, flicker, clipped and pop not up) and rubric scores, against the previous step's record.
Hill to climb: `probes again` p50, now (paste) → 70 % of it; frames to convergence after a sun
step, → half.


### B10. Light in the air

Nothing local exists: aerial perspective from the air's LUT (`light.wrela:74-86`), a marched
mist for the falls (`water.wrela:448-515`) and shafts that are off in the floor's view
(`stage.wrela:174, 616`). A froxel volume (Wronski 2014; Hillaire 2015; §9) gives fog, light
shafts through the canopy and the falls' mist in one place.

- `engine/fog.wrela` (new): `Froxels { scatter: Texture3d<Rgba16Float>, lit: Texture3d<
  Rgba16Float>, last: Texture3d<Rgba16Float> }`, 160×90×64 over the camera's frustum to `FOG_FAR
  = 200 m`, slice `i` at depth `near × (FOG_FAR / near)^(i / 64)`; jittered per frame along the
  view ray by the TAA's index and reprojected from `last` by the camera's motion with blend 0.9
  (a disocclusion falls back to the current).
- `Scene::fog(self) -> FogField` (new hook; default none): a field over world space the floor
  authors, `FogField { density: fn-like params }`: the engine supplies `HeightFog { base_y, d0,
  h, albedo }` (`d0 = 0.004 /m`, `h = 12 m`, **proposed**) and a list of `FogSphere { at, radius,
  density }` the falls' mists feed (`history::MISTS` replace `mist_over`); a floor may compose
  its own field in `fog_at(world) -> (density, albedo)` on the GPU.
- **Inject**, a `@compute(4, 4, 4)` per froxel: `density` from the field plus a canopy term:
  the probes' `density` volume (leaves at 1 m) sampled at the froxel × `Fog::canopy` (`0.02`,
  **proposed**); in-scattering `S = density × albedo × (sun × sunlit(pages) × cloud_shadow ×
  HG(g = 0.5, cosθ) + sky_light(probes) / π)`, extinction `σ = density`; written to `scatter` as
  `(S, σ)`.
- **Integrate**, one invocation per column: front to back, `T *= exp(−σ Δz)`, `L += T × S × Δz`;
  `lit` holds `(L, T)` per froxel.
- **Apply:** `lit::through_air` (`lit.wrela:160`) samples `lit` at the pixel's (uv, slice) and
  composes `colour × T + L` before the aerial LUT, whose term is scaled by `T` at the far slice;
  the sky pass applies the far slice. The shafts (`shafts.wrela`), their quarter-res march and
  `mist_over` retire: the pages' shadows sampled per froxel give the shafts their shape under the
  trees. `Quality::fog: bool = true` (D12); off, the volume isn't cooked and `through_air` is
  today's.
- **Cost, estimated:** inject 0.3 ms, integrate 0.1, apply 0.2–0.4 at 1080p on the M4; the
  retired shafts and mist give back up to 0.4.
- **Tests:** `engine/tests/render/fog` (new): a uniform density over a black plane gives `1 −
  exp(−σ d)` within 1/255 at three depths; a column in full shadow has no sun in-scatter (within
  1/255 of ambient only); the temporal blend converges within 16 frames after a step in density.
- **Gate:** G-profile: the fog's labels at most 1.0 ms p50 in the opening and the net rise at
  most 0.6 ms with shafts and mist gone (**estimated**); G-stills (deliberate; attach `02`, `08`,
  `09`, `11-the-windthrow`); G-both.

**Done when:** (1) the three fog tests pass; (2) `Scene::fog` exists with the height fog and
spheres, and the Last Green's falls' mists feed it; (3) `shafts.wrela`'s march and `mist_over`
are gone from the frame (no pipeline named `march_shafts`, `add_shafts` or `mist_over` in
`wrela pipelines games/last-green`); (4) the fog's labels are at most 1.0 ms p50 in the opening
and the frame's net rise at most 0.6 ms against B9 (paste three pairs); (5) `Quality::fog`
off restores B9's image within 1/255; (6) G-stills with `02`, `08`, `09`, `11`; (7) G-both.
**G-look:** shots `02`, `08`, `09`, `11`, `15-the-wildwood`: pairs, `--quality` numbers (PSNR/SSIM not down by 0.5 dB/0.01, flicker, clipped and pop not up) and rubric scores, against the previous step's record; flicker may rise by the fog's jitter only if the TSR settles it within 16 frames (the test).
Hill to climb: the fog's labels, → at most 1.0 ms; take the inject's froxel count down (160×90×48)
before taking its light samples down if the first measurement misses.


### B11. Geometry from the field

The cluster path's error is a heuristic, its levels are separate extractions with skirts, it
measured slower than the forward path at the banks, and it has no group DAG, skinning or
streaming (`docs/world-class-next.md` §2.6). A field offers what a mesh can't: extraction at any
resolution and exact projection of a vertex onto the surface. Each sub-step is a commit.

**B11a. Continuous LOD per instance, from the field.**

- **What the field gives, exactly.** Every level is an extraction of the **same raw field** at a
  coarser grid (`realize_rigid` at cells `c, 2c, 4c`); where the extraction's contract holds
  (smooth surfaces; `look.wrela:9` excludes corners and parts thinner than a cell), each level's
  vertices lie on the raw surface within its tolerance, so the distance from a fine vertex to the
  coarse mesh's surface is at most the coarse tolerance, which the level choice keeps under
  `max_cell_px` on screen (`look.wrela:58`). The plan does **not** rely on that implication: it is a
  hypothesis about production assets, not a theorem (crags have corners, posts are thin, and
  `realize.wrela:1807`'s error measure is one-sided), and the switch sweep below is the
  acceptance criterion; B11a.0 runs it before anything is rewired. If the sweep passes without a
  position morph, none is added;
  what pops is **shading** (the coarse mesh's normals are the raw gradient at its own vertices,
  so detail the coarse grid can't carry changes the normal field) and **topology** (a feature
  thinner than the coarse cell has no coarse counterpart and vanishes). The filtered field
  (`std::derive::filtered`, spec §14.1: a first-order box filter along each input, not an exact
  convolution) is the tool for both, and only for both.
- `engine/realize.wrela`: `realize_levels(through: Through, bounds, surface: look::Mesh, levels:
  u32 = 3) -> Levels` extracts the three meshes and computes, per vertex of levels 0 and 1, two
  extra attributes in a kernel `level_blend`: a **coarse normal**, `normalize(∇ filtered(d, p,
  width: vec3(2 × cell_{k+1})))` (the gradient of the filtered field at the vertex, the normal the
  next level's surface has on average there), packed as `Oct32`; and a **presence** from the next level's **occupancy**, not from the filtered field (whose
  value is 0 on any surface point, thick or thin): the coarse extraction already knows which of
  its blocks hold surface (`cull_blocks`' live list and `block_map`, `realize.wrela:190-197`);
  a fine vertex within one coarse cell of a **coarse vertex** (the coarse mesh's own vertices,
  found through the live block's vertex range and its neighbours') has presence 1, and otherwise
  presence falls with that distance over one more cell (0 at two cells or more); a coarse level
  with no live block at all gives presence 0 everywhere, so an instance the coarse grid loses
  entirely fades out instead of vanishing. A face the coarse level represents keeps full
  presence; a feature the coarse grid lost fades, whether it stands alone or shares a live block
  with retained geometry. `SkinVertex`
  (`realize.wrela:160-166`) grows by 8 bytes (48 → 56).
- The vertex shaders that draw realized meshes (`character.wrela`'s `skin_matrix` users,
  `stone.wrela`'s `stone_instance_vertex`, the built things' `draw_built`) take a per-instance
  `blend: f32` in `[0, 1]`: the shading normal is `normalize(mix(normal, coarse_normal, blend))`,
  and the vertex colour and material blend the same way toward the coarse level's (sampled per
  vertex at extraction from the coarse `Material` buffer by nearest coarse vertex, stored as the
  fine vertex's `coarse_material`); fragments whose interpolated `presence` is under `blend` are
  discarded through the plant's Bayer dither (`lod.wrela:168`), so a thin feature fades over the
  band instead of vanishing at the switch. The instance's level `k` and `blend` come from its
  distance `d`: `k` is the coarsest level whose cell projects to at most `max_cell_px` at `d`, and
  `blend = saturate((d − d_k) / (d_{k+1} − d_k))` across the band between the switch distances; at
  `blend = 1` the instance switches to `k + 1` with `blend = 0`. One instance is one level: no
  cracks.
- **The prototype first (B11a.0, its own commit).** Before any caller changes: `realize_levels`
  on the tower alone, drawn in a fixture with and without the blend, the switch sweep below run
  on it from four views, and the measured switch pop and silhouette displacement pasted, with
  the blend and without. The tolerance-bound argument is a claim until this number exists; if the
  pop without the blend is already under the threshold, the blend's cost is weighed against what
  it buys and the commit says so; if the pop with the blend is over the threshold, the step stops
  here with a Blocked note and the numbers, before anything else is rewired.
- **Who draws through it:** built things (`Thing { near, far }`, `stage.wrela:132-136`, becomes
  `Thing { levels: Levels }`), bank prototypes and their placements, `Rocks`, the crags (B11b),
  and creatures past 30 m (`Cast` keeps level 0 near; the `Realizer`'s dithered level fades
  (`realize.wrela:1146-1148`) become this blend for the herd).
- **Tests:** `engine/tests/virtual` becomes `engine/tests/lod`: (1) a **switch sweep** on
  production assets, not a sphere: the tower, a crag, a door post and a bank stone, each drawn
  from four views while a dolly crosses every switch distance; at each switch the silhouette's
  edge rows move by at most 0.5 px and the mean absolute frame-to-frame luma difference (the
  G-look `pop` measure) is at most the same dolly's pop with the level **pinned** (no switch: the same camera motion,
  the same units, so only the switch's error remains), plus 0.002, a starting value B11a.0
  calibrates; (2) a
  thin solid (a 3 cm rod) fades over its band and never pops (no frame-to-frame difference over
  the threshold), and a thick cuboid swept through the band keeps its face interiors fully
  covered at every blend (no discarded fragment inside a face); a 3 cm rod standing on a thick
  base, sharing the base's coarse blocks, fades while the base keeps full coverage; an instance
  whose coarse level is empty fades out whole; (3) the coarse normal at 1,000 fine vertices is within 10° of the coarse mesh's
  interpolated normal at the nearest point; (4) the herd at 4× instances draws a triangle count
  that falls with distance (read back the levels' draw counts).
- **Gate:** G-pixels within 2/255 on the clearing's fawn and the herd (the near level is
  unchanged); G-profile: the herd at 4× instances is under 1.3× the 1× GPU span (**estimated**);
  G-look on the built things' shots.

**B11b. Crags and cliffs as realized solids.** The crag's face is a heightfield's terraced edge
and a smooth sheet (`04-the-crags-top`, `09-the-waterfall`). `games/last-green/ground.wrela:416-
436` already walks the crags as solids inside `substrate_height`; make them `Built` things: each
crag's field (the same functions, as a `Surface`) is realized through B11a with the rock look
(B11d), and the heightfield's substrate leaves their volume out (`Terrain::substrate_height`,
`terrain.wrela:47-54`). Collision keeps reading `height` (the full ground), unchanged.
- **Tests:** the sim's `height_at` at 1,000 points on and around a crag is unchanged within
  2 cm (G-sim holds: the sim never read the mesh); the crag's realized surface is within
  `tolerance` of its field.
- **Gate:** G-sim; G-stills (deliberate: `04`, `09`; the terracing gone).

**B11c. The cluster path leaves the game (D14).** Delete `ClusterStones` (`engine/stone.wrela:
1191-1335`), the `CLUSTERED` constant and its branches (`stage.wrela:62-70, 1474-1487, 2023-2037`,
and the bank cluster draws in `choose`, `prepass`, `shade`); `engine/virtual.wrela` stays for
`examples/clusters`, `engine/tests/soft-raster` and `engine/tests/virtual`'s renamed successor's
cluster tests, with a module note that the engine's LOD is B11a's.
- **Gate:** G-pipelines: `wrela pipelines games/last-green` loses the path's pipelines (state
  the count); G-pixels identical on the Last Green (the path was off).

**B11d. Rock as a material.** `engine/rock.wrela` (new): a `Rock` look for `cook_materials`
(`character.wrela:215-224`, or its tape form), cooked per vertex at realization: **strata** (a
bedding direction and layered bands: albedo between two tones by a 1D noise along the bedding
normal, 0.3–1.2 m bands), **cracks** (Worley edge distance, Worley 1996 already cited: darkening
and a baked normal perturbation along the edge), **lichen** (slope under 30° and height above
ground over 0.5 m: a tint toward a grey-green, 20 % at most), and **wetness** (height above the
nearest water level under 0.6 m, from the water tiles' levels: albedo × 0.6, roughness 0.35).
`Material` (`character.wrela:210`) gains `rough: u8` and `wet: u8`; `stone_shade` reads them. The
falls' wall, the crags and the bank stones use it; the tower's masonry keeps its own look.
Parameters are **proposed** and live in `content/kit` for Ryan.
- **Tests:** `engine/tests/render/rock` (new): a cube cooked with `Rock` has at least three
  distinct albedo bands along its bedding normal and a wet band under the water level.
- **Gate:** G-stills (deliberate: `04`, `09`, `07-the-mere`'s bank); G-profile: `cook_materials`
  per built thing at most 2× today's (one-time, at load).

**Done when, B11a:** (0) the prototype's switch-pop and silhouette numbers for the tower, with
and without the blend, are in B11a.0's commit; (1) the four LOD tests pass (the production-asset switch sweep within
0.5 px and the pop threshold from four views, the thin rod's fade, the coarse normal within 10°,
triangles falling with distance); (2) built things, bank placements, rocks and distant creatures
draw through `Levels` with a `blend` (a test reads the instance's level and blend for three
distances); (3) G-pixels within 2/255 on the clearing's fawn and the herd;
(4) the herd at 4× instances is under 1.3× the 1× GPU span (paste); (5) G-stills.
**Done when, B11b:** (6) `height_at` at 1,000 points round a crag unchanged within 2 cm and
G-sim; (7) the crag's realized surface within `tolerance` of its field; (8) G-stills with `04`
and `09` (no terracing).
**Done when, B11c:** (9) `ClusterStones`, `CLUSTERED` and the bank cluster branches are gone;
(10) `wrela pipelines games/last-green` falls by the path's pipelines (paste); (11) G-pixels
identical on the Last Green.
**Done when, B11d:** (12) the rock cube test shows three albedo bands and a wet band; (13) the
falls' wall, the crags and the bank stones draw with `Rock`; (14) `cook_materials` per built
thing at most 2× before (paste); (15) G-stills with `04`, `09`, `07`.
**G-look:** shots `04`, `05-the-towers-foot`, `07`, `09`, `13`: pairs, `--quality` numbers (PSNR/SSIM not down by 0.5 dB/0.01, flicker, clipped and pop not up) and rubric scores, against the previous step's record.
Hill to climb: the herd at 4× against 1×, → under 1.3×; the switch sweep's largest pop on the
tower, → at most the pinned dolly's pop + 0.002; the crag shots, from terraced to solid (stills).


### B12. Water

One static mesh, two ripple noises, SSR, foam at banks and a flat quad for the falls
(`engine/water.wrela`; `09-the-waterfall`). Each sub-step is a commit; D25 keeps the looks Ryan's.

1. **Flow.** `land/water.wrela`'s `Course` points (`:29-33`: x, z, width, depth) gain a
   direction and a speed per point (from the course's order and its drop: `speed = clamp(0.4 +
   2 × slope, 0.3, 3)` m/s, **proposed**). The clipmap's `fields` texture (`terrain.wrela:347`)
   becomes a two-layer `TextureArray<Rgba8>`: layer 0 as today (light, trodden, wet, thorn),
   layer 1 flow (`dir.x`, `dir.y` as snorm in unorm, `speed / 4`, `foam`), cooked per tile when
   it lands by a kernel from the courses within the tile. `water_shade` (`:323-331`): the ripple
   normals are advected by the flow with two phases a half period apart (Vlachos 2010, §9),
   `flow_ripple` scales with speed; `WaterVert.flow` (`:35-41`) stays as the fallback where no
   field tile is loaded.
2. **Falls.** For each of `history::MISTS` (a fall's foot), the floor's `Scene::emit` pushes
   spray: `Particle { kind: SOFT, size: 0.3–0.8, life: 1.5 s, vel: the fall's direction × 2 m/s
   + spread, colour: the foam's at 0.6 alpha }` at 200 a second within 60 m of the eye, and a
   foam column at the base (`kind: FOAM`, new: additive, the water's `foam` colour, 0.8 s life);
   the fall's sheet keeps its mesh and gains a vertical streak term from the flow speed in
   `water_shade`'s `FALLING` branch (`:403-414`); the mist sphere goes into the froxel volume
   (B10) and `mist_over` retires. The frame's particles exist and are unused by the floor; this
   is their first use.
3. **Wet margins.** The `wet` field (layer 0's third channel) is set from the water's distance
   at bake (`land/bake.wrela:893-906`) within 1.5 m with a falloff (today `MARGIN = 0.45`,
   `water.wrela:60`); `ground_shade` darkens albedo × 0.7 and sets `rough 0.5` by it; the rock
   material's wetness (B11d) reads the same.
4. **A planar reflection of the mere (D15).** `engine/reflect.wrela` gains `Planar { colour:
   Texture<Rgba16Float>, depth: DepthTexture }` at half the scene's size and a new hook
   `Scene::draw_mirrored(self, pass, cam: Camera, plane_y: f32)`; when a `STILL` water tile
   (`:28`) is in the frustum, the frame mirrors the camera about `plane_y` and the Last Green
   draws the terrain (the clipmap with the mirrored camera and `discard` below the plane), the
   wood's impostor and volume ways only (no cards, no grass), the built things' coarsest level
   and the sky; `water_shade` blends the planar sample, distorted by the ripple normal, over the
   SSR result where the planar texel is in bounds (SSR keeps the near objects, the planar pass the
   far bank and the sky). `Quality::planar: bool = true`.
5. **Culled by tiles.** With B5.2: the water mesh laid in 64 m tiles, one `draw_multi`, the
   `below` copy scissored to the visible tiles.
- **Tests:** `engine/tests/render/water-flow` (new): a marker advected over a straight course
  travels at the course's speed (read the shade's advection phase back through a debug output);
  the planar pass is recorded only when a still tile is in view (`Commands`); foam appears where
  speed exceeds 1 m/s (pixels of the `foam` class); `water-steps` and `water-reflection` tests
  hold within their tolerances.
- **Gate:** G-stills (deliberate: `07`, `08`, `09`, `10-the-burn`); G-profile: the water's labels
  at most 1.0 ms p50 and `planar` at most 1.5 ms when the mere is in view (**estimated**; if
  planar exceeds it, it ships with D15 flipped to off and the number); G-memory: the flow layer
  within the `terrain` share's +4 MiB.

**Done when:** (1) the flow test advects a marker at the course's speed; (2) `Commands` records
the planar pass only when a still tile is in view; (3) foam pixels appear where speed exceeds
1 m/s; (4) `water-steps` and `water-reflection` hold within their tolerances; (5) the falls emit
spray and a foam column (a test counts particles after 60 frames near a fall) and `mist_over` is
gone (B10); (6) the `wet` field darkens the ground within 1.5 m (a pixel test on a bank);
(7) the water's labels at most 1.0 ms p50 and `planar` at most 1.5 ms when the mere is in view
(paste both; if planar misses, D15 off with the number); (8) the flow layer adds at most 4 MiB;
(9) G-stills with `07`, `08`, `09`, `10`.
**G-look:** shots `07`, `08`, `09`, `10`: pairs, `--quality` numbers (PSNR/SSIM not down by 0.5 dB/0.01, flicker, clipped and pop not up) and rubric scores, against the previous step's record.
Hill to climb: `planar` when in view, → at most 1.5 ms (levers in order: half the mirrored
terrain's levels, impostors only past 40 m, skip the sky's clouds in the mirror).


### B13. The image

Each item is a commit; the step's gate is the opening's frame with everything on.

1. **The resolver's three missing parts, and what moves.** `engine/temporal.wrela`:
   - **Dilated motion:** `resolved` (`:315-325`) reads the motion of the nearest-depth texel of
     the 3×3 round the pixel, not the nearest of its four samples (FSR 2's dilation, §9).
   - **A reactive mask without a new target:** the prepass writes class tags into `ids`
     (`frame.wrela:288-298`); the resolve reads the tag and treats cards, impostors, grass,
     water and particles as reactive: the clip box's γ widens to 2.5 and the blend floor rises
     to 0.25 for them (`:402-414, 436-447`).
   - **A lock for thin features:** `lock: Texture<R8>` at history size counts the frames a
     pixel's luma stayed inside its clip box while flagged thin (`neighbourhood`'s `.w` bit,
     `:115-138`); a locked pixel (count ≥ 8) blends at 0.04 and the count resets on
     disocclusion or a reactive tag; grass and far trunks stop shimmering.
   - **What moves:** the wood's cards write `deforming_of` motion from the wind as grass does
     (`:463`), the water writes its drift, built things and terrain stay camera-only; each kind
     carries a `moves: bool` the frame passes to the motion pass (the audit's static-or-moving
     flag, by hand).
   **Tests:** `engine/tests/render/temporal-*` extend: a thin vertical line moving 1 px a frame
   leaves no trail over 2 px; a swaying card against a 16-sample truth within 4/255; the
   `temporal-vacancy` and `-sway` tests hold.
2. **Cloth that reads.** `engine/light.wrela`: `Lit` gains `sheen: f32 = 0.0` and `sheen_tint:
   vec3 = vec3(1.0)`; `gloss` (`:149`) adds the Charlie sheen lobe (Estevez and Kulla 2017, §9:
   `D = (2 + 1/α) sin^(1/α)(θ_h) / 2π`, visibility `1 / (4 (N·L + N·V − N·L N·V))`), and
   `character` (`:198-206`) a rim from `through`'s `backlit` for thin cloth. The climber's
   garment shader (`content/climber/draw.wrela:502-521`) sets `sheen 0.6`, `sheen_tint (0.08,
   0.06, 0.12)`, `albedo 0.02` (**proposed**: near-black velvet whose folds show), and the hood's
   inside reads `ambient_at` so it stays dark.
   **Tests:** a sphere with `sheen 1` has an edge-to-centre luma ratio over 1.3 against under 1.1
   without (read back).
3. **HDR output (D11).** `engine/paint.wrela`: `Look::output: Output = Sdr | Hdr { peak: f32 =
   4.0 }`; with `Hdr` the frame records `std::gpu::screen(Screen { hdr: true, gamut: DisplayP3 })`
   (A12) at its first frame and `develop` outputs linear light with a roll-off above 1.0
   (`y / (1 + y / peak)` on the luminance above 1, chroma kept) instead of `tone` (`light.wrela:
   219`); `screen_is()` false (a display without HDR) falls back to `Sdr`. The capybara's and
   the console's 2D paths are untouched.
   **Tests:** the develop fragment with `Hdr` maps 3.0 white to over 1.0 and under `peak`;
   `Sdr` is within 1/255 of before.
4. **`f16` twins (G-precision).** Run `wrela precision` (A9) over: `ground.wrela`'s cover noises
   (`wood_floor`, `cover_at`), `grass::place`'s `sward` and `bloom_drift`, `sky::march`'s density
   and light loop, `probes::trace`'s radiance accumulation, `ambient`'s slices, `temporal::
   resolved`'s colour math, with boxes from the kernels' real ranges. Where every op on a path is
   `safe` at `tol 1e-3`, write the `f16` twin as a second entry point (`vec3h` math inside, `f32`
   in and out), selected at the dispatch by `features().f16` (the twin is under a condition, so
   tier 1). Keep a twin only if G-precision holds within 2/255 **and** its label's GPU time falls
   by at least 10 % on the M4 (record every twin's numbers, kept or not).
5. **Relaxed SIMD, measured (D16).** Build the Last Green with and without `--relaxed-simd`
   (A13); `wrela profile --chrome --compare`, 7 alternating pairs, CPU frame p50; `--compare-
   ticks` equal; pixels within 1/255 (`wrela same --within 1`). D16's rule decides the release
   default; the numbers go in `docs/bench.md`.
6. **Leaves against the sun.** `engine/plant.wrela` and `impostor.wrela`: the `through` term
   gains a sun-facing lobe (`backlit`, `light.wrela:114-116`) scaled by a thickness from the
   card's alpha gradient (thin edges glow more) and by the pages' sun visibility at the leaf,
   strength 0.35 (**proposed**).
- **Gate (whole step):** G-golden re-blessed where an item says the image changes, with
  difference images in the PR; G-profile: the opening's GPU frame p50 at most 12 ms with every
  switch of B5–B13 on (**the claim's number**; if it misses, the commit says by how much and
  which label); G-stills (`16`, `19`–`22` for the climber; `02`, `17` for the leaves); G-both.

**Done when, per item:** (1) the thin-line and swaying-card tests pass, `temporal-vacancy` and
`-sway` hold, and the resolve costs at most 0.15 ms more (paste); (2) the sheen test's edge
ratio exceeds 1.3 and the climber's garment sets the values, with G-stills `16`, `19`–`22`;
(3) the HDR develop test passes and `Sdr` is within 1/255 of before; (4) six `wrela precision`
reports are attached, every twin's G-precision and time delta pasted, and the kept twins each
save at least 10 % of their label; (5) seven pairs with and without `--relaxed-simd`, equal
tick logs, pixels within 1/255, and D16 applied to the release default; (6) G-stills `02`, `17`
for the leaves.
**Done when, the step:** (7) G-golden re-blessed with difference images where an item changed
the image; (8) the opening's GPU frame p50 at most 12.0 ms with every switch of B5–B13 on
(paste three pairs); (9) G-both.
**G-look:** shots all 26: pairs, `--quality` numbers (PSNR/SSIM not down by 0.5 dB/0.01, flicker, clipped and pop not up) and rubric scores, against the previous step's record; the resolver's flicker and pop numbers must fall, not merely hold.
Hill to climb: the kept `f16` twins' saving, summed, pasted per twin; the opening held at 12.0 ms
with everything on.


### B14. The frontier

Four experiments. Each is a **hypothesis** (§0.9): its code behind its switch, its measurement
and its record in `docs/bench.md` are the deliverable, and only the switch's default depends on
the result (D18). None may be skipped, and "too hard" is not an outcome: each is cut into
sub-steps whose first is a unit test and a number reachable in a day, each sub-step is its own
commit with its own **Done when**, and each has a **ladder** of rungs with pass marks so the
implementer climbs toward a target instead of judging the whole. Where the hypothesis fails, the
rung table in the commit is the result, and it is as much a deliverable as a win.

Why these four: they are the places where a compiler that sees the fields, a build that already
runs the program, and a renderer whose every artefact is cooked on the device give this engine a
step no asset pipeline can take (`docs/world-class-next.md` §5.11, §7): weights can be trained
at build time and shipped as numbers; the impostors, the resolver's rejection and the bounce light
are each a small function of inputs the engine already has. Start each experiment with its test,
not its shader: a day that ends in a passing test and a number is the sub-step's commit.

**B14.1 `studio/fit`: a trainer that writes weights as source.** Needed by 14.2–14.4 and by
G-look's moving measures. Reading: none beyond `std::tool::drive` and the keys tool
(`games/capybara/keys`, which writes `takes.wrela`) as the model for a tool that writes source.

- **1a. The model and a CPU reference.** `engine/neural.wrela`: `Model { inputs: u32, hidden:
  [u32; 2], outputs: u32 }`; `Net<I, H, O>: Copy + GpuData` holding `w0: [u32; …]` (f16 bits,
  two a word), `b0`, `w1`, `b1`, `w2`, `b2`; `eval(net, x: [f32; I]) -> [f32; O]` on the CPU in
  f32 (ReLU hidden, linear out; the reference for every test); `eval_h`, the GPU twin in `f16`
  (`vec4h` dot products, the loops over `H` unrolled by the compiler's inliner), used by 14.2
  and 14.3.
  **Done when:** a hand-set `2 → 2 → 1` net's `eval` equals a by-hand computation at four inputs;
  `eval_h` on the GPU matches `eval` within 1/255 after scaling to [0, 1] at 1,000 random inputs
  (readback); `wrela doc engine::neural::Net` answers.
- **1b. The kernels.** `forward_batch` (a batch of inputs through the net, activations kept),
  `backward_batch` (per-sample gradients summed per workgroup in `Shared`, then a `reduce`
  kernel), `adam_step` (β₁ 0.9, β₂ 0.999, ε 1e-8, lr 1e-3), a `Trainer { model, batch: 16384,
  steps, lr }` and the losses `Mse` and `L1`. The gradient is written by hand for this one
  architecture; the compiler's forward-mode `gradient` is the wrong tool for thousands of
  weights.
  **Done when:** the finite-difference test (central differences at 1e-3 on 100 random weights
  of a `4 → 8 → 8 → 2` net, every partial within 1e-3 relative) passes; a run on `y = sin(x)`
  over `[-π, π]` with a `1 → 16 → 16 → 1` net reaches a held-out MSE under 1e-3 within 2,000
  steps, the loss printed every 100 steps (the first hill: paste the curve).
- **1c. The tool.** `wrela tool fit -- --net <sine|impostor|rejector> [--kind <plant>] --out
  <file.wrela> [--steps n] [--report]` (`wrela.toml`: `read = ["ws=."]`, `write = ["ws=.",
  "out=build/fit"]`): builds the net's fixture program (`studio/fit/fixtures/<net>/`, which makes
  the dataset, below), runs it on the native host with the GPU (`drive::gpu()`), reads the
  weights back and writes `pub const <NAME>: Net<I, H, O> = Net { … }` as source with the f16
  bits, plus a doc comment naming the dataset, the steps, the held-out error and the date;
  `--report` prints the loss curve.
  **Done when:** `--net sine --out <tmp>/sine.wrela` writes a file `wrela check` accepts and a test
  package that imports it reproduces `sin` within 1e-2 at 100 points; the run takes under 60 s on
  the M4 (paste it); the tool refuses to overwrite a file outside `content/` or `build/`.
- **1d. The truth renderer.** `wrela tool fit -- --truth <pkg> --input <script> --frames 200
  --out build/fit/truth/`: for each frame writes the play-mode inputs the resolver sees (the
  jittered scene render before the resolve, its depth, motion and `ids`, the history it would
  read) and the **truth**, `Quality::reference()`'s 64-jitter accumulation of the same frame
  with the frame's time frozen at that frame (§B1.8: the sim, wind, drift and particles held while
  the 64 samples are taken, then advanced one frame), as `.rgba` with a `frames.json`; so the
  truth is an instantaneous supersample aligned with its play frame, not a motion blur. It is 14.3's dataset and G-look's moving reference.
  **Done when:** 200 frames of the walk write in under 20 minutes on the M4 (paste); the
  resolver's own output for a still frame scores at least 30 dB against the truth (a sanity
  number: the resolver already works; paste it).

**B14.2 Neural impostors.** Reading: Zeltner et al. 2024 (an MLP evaluated in the shader with
f16 FMAs, and how many materials it scales to); Belcour and Benyoub 2025 (latent grids decoded
by a small MLP; the shape of 2b's rung C). What exists: a kind's impostor is a 1024² atlas of 256
hemi-octahedral views at 64², colour with coverage and normal with depth (`engine/impostor.wrela:
7-11, 130-150`), 3 MiB compressed, 26 kinds, 4 views blended per pixel (`:457-470`).

- **2a. The dataset.** `studio/fit/fixtures/impostor`: bakes the named kind's atlas as the engine
  does and writes rows `(view_uv: 2, texel_uv: 2) → (rgb: 3, coverage: 1, oct_normal: 2, depth:
  1, through: 1)`, 1,048,576 of them; every 16th view is held out.
  **Done when:** the file writes and a test reads 100 rows back equal to the atlas's texels.
- **2b. The ladder.** Each rung is a `--net impostor-<rung>` model trained with a fixed budget
  (4,000 steps at batch 16,384, three seeds, the best kept) so a rung's result is a result about
  reconstruction under that budget, comparable across rungs; the measure is PSNR over colour and coverage at the
  held-out views, coverage-weighted. **All four rungs run and are recorded** (parameters, PSNR per
  seed, training time, the loss curves), whether or not an earlier rung passed 32 dB; a rung that
  fails its mark under its budget is a result about reconstruction under that budget (an
  unconverged curve is said to be one, not called a capacity limit), and the ladder goes on to
  the next. The trainer's correctness is established by 1b's
  finite-difference and sine tests, never by a rung: a rung's failure doesn't reopen 1b unless
  the loss curve doesn't fall at all (then 1b's tests are rerun on this dataset's shape first).
  - **A** `4 → 16 → 16 → 8`, no grid: pass mark **26 dB** (an **estimated** mark: the first run
    calibrates it, and the record keeps the measured value whatever it is).
  - **B** `4 → 32 → 32 → 8`: **29 dB**.
  - **C** A's inputs plus 8 features from a learned 16×16×8 grid over `texel_uv` (bilinear; the
    grid trains with the net; 12 inputs): **32 dB**.
  - **D** C with a 32×32×8 grid and hidden 48: **33 dB**.
  **Done when:** all four rungs' rows are in the commit with their seeds' numbers, whether or not
  one passed 32 dB; a ladder in which none passed is a valid result that ends with the switch off
  (2d), not a Blocked note. The step's minimum deliverable is the four rows.
- **2c. The shader.** `impostor.wrela`'s fragment takes the neural path under `Quality::neural`
  (default off): `Mlp::eval_h` with the kind's weights from a storage buffer filled at load from
  the `const` in `content/kit/nets/<kind>.wrela`, the grid as a small `Rgba16Float` texture;
  its colour, coverage, normal, depth and `through` feed the same shading as the atlas's.
  **Done when:** the shader's outputs for 1,000 (view, texel) samples are within 1/255 of the CPU
  `eval` (readback); the opening renders with the switch on without a validation error.
- **2d. Measurement and the rule.** On the opening: the impostor draws' GPU time with the switch
  on against off (`wrela profile --chrome --compare`, three pairs, the `impostors` label, added if
  missing); PSNR of the neural impostor against the atlas over all 256 views at 64² (the fixture
  renders both); bytes per kind (atlas, compressed, against the net and grid). **G-spike:**
  `Quality::neural` defaults on for kinds drawn past 160 m if the pass is at most 1.3× the
  atlas's time and PSNR is at least 32 dB; else off. G-look on `00`, `13`, `15` when it turns on.
  **Done when:** the three numbers, the rung table and the stills are in `docs/bench.md`
  "Experiments", the default follows the rule, and the trained beech net is committed under
  `content/kit/nets/`.
  Hill to climb: PSNR per rung, → 32 dB; the pass's time, → at most 1.3× the atlas's.

**B14.3 A learned history rejector.** Reading: Craig 2026 (where learning belongs in an upscaler:
closed-form everywhere it works, a network only for the recognition the closed form gets wrong).
What exists: `resolved` (`engine/temporal.wrela:240-448`) and the blend `a` it computes from
heuristics (`:436-447`), with B13.1's dilation, reactive mask and lock.

- **3a. The dataset.** From 1d's truth frames: per sampled pixel, 14 inputs (the current sample's
  3×3 luma, the reprojected history's luma, `|motion|` in pixels, the depth delta, the thin flag,
  the reactive class) and the target (the truth's luma, with the current's and the history's kept
  to form the loss); 2,000,000 samples from 200 frames, 10,000 a frame, half of them from pixels
  the heuristic clipped or rejected (stratified so the net sees the hard cases).
  **Done when:** the dataset writes and a test checks 100 rows against a direct read of the
  frames' `.rgba`.
- **3b. The baseline.** The heuristic resolver's PSNR against the truth over the 200 frames (per
  frame and the mean) at B13.1's state, now unmeasured.
  **Done when:** the number is in the commit and in `docs/bench.md`.
- **3c. The ladder.** The loss is `|mix(history, current, a) − truth|` on luma with `a` the net's
  sigmoid output; the measure is reconstruction PSNR over the held-out 20 % of frames against 3b;
  stop at the first rung that passes and record every rung tried:
  - **A** `14 → 16 → 8 → 1`: pass mark **+1.0 dB** over 3b.
  - **B** A plus the current sample's 3×3 CoCg (32 inputs): **+1.0 dB**.
  - **C** B plus the history's 3×3 luma (40 inputs): **+1.0 dB**.
  **Done when:** the rung table is in the commit, with a Blocked note if none passed.
- **3d. The shader and the rule.** The net's weights (at most 1,024 f16 values) in a uniform;
  `resolved` computes `a` from `eval_h` under `Quality::learned_resolve` (default off) and as
  before otherwise. The resolve's GPU time with and against without (three pairs); G-look's
  moving measures (flicker, pop) on all 26 shots. **G-spike:** on if the held-out PSNR is at least
  +1.0 dB over 3b, the resolve costs at most 0.2 ms more, and flicker and pop are not up on any
  shot; else off.
  **Done when:** 1,000 pixels' `a` from the shader are within 1/255 of the CPU `eval`; the time,
  the G-look numbers and the default are recorded.
  Hill to climb: PSNR against the truth, the baseline → +1.0 dB; the resolve's extra time,
  → at most 0.2 ms.

**B14.4 Radiance cascades over the distance volume.** Reading, in this order: Sannikov 2023 (the
method: radiance intervals, why angular resolution grows as spatial resolution shrinks, the
merge); the two Chalmers 2025 theses (screen-space probes with world-space intervals in a 3D
game, the depth-aware upsample); Freeman and Sannikov 2026 (world-space probes in a hashmap and
ray splitting: the step after this one, not this one). What exists: B9b's distance volume (solids)
and the probes' density volume (leaves), B9a's probes with visibility (the sky and sun-bounce
terms a hit gathers), the shadow pages' sun visibility, and `ids`' class tags.

- **4a. Layout and the trace.** `engine/cascades.wrela`: four cascades, each a `Texture<
  Rgba16Float>` laid out as probes × directions: c0 240×135 probes × 4×4 directions (960×540
  texels), c1 120×68 × 8×8, c2 60×34 × 16×16, c3 30×17 × 32×32 (each about 4 MB; the probe
  counts follow the scene's size); intervals c0 `[0, 0.5 m]`, c1 `[0.5, 2]`, c2 `[2, 8]`, c3
  `[8, 32]`. A probe's origin is its pixel's world position pushed 2 cm along the normal.
  `trace_cascade(i)`, one invocation per (probe, direction): sphere-trace the distance volume from
  `t0` to `t1` with a **conservative bounded advance**, `least = (t1 − t0) / steps` (16 steps
  unless the rung says otherwise: c0's least 3 cm, c3's 1.5 m; the far cascades' probes are
  coarse too): where `d ≥ least` the step is `d` (safe: no surface within it); where `d < least`,
  with `a = −dot(∇d, dir)` (the rate at which the ray approaches the surface), if `a > 0` and
  `d / a ≤ least` the ray **hits at `t + d / a`** (so a slab thinner than `least` is not jumped: a
  forced advance never crosses a surface the field says is ahead), else it advances by `least`
  and reads the field at the midpoint and the end, a negative or sub-threshold value at either
  being a hit at the first; the last advance clamps to `t1`. Every iteration advances at least
  `least` or ends, so **every ray reaches `t1` or a hit within the rung's step count**. The
  leaves' density integrates along the same ray for transmittance; a ray that leaves the distance
  volume's box (96 m wide round the eye) continues as empty space to `t1`, a known limitation of
  this spike that the commit records; at a solid hit (`d < 0.1`), radiance = `0.3 × (sun_colour
  × sunlit_pages(hit) × max(dot(n_hit, sun), 0) + probes_sky(hit))` with `n_hit` from the
  volume's gradient (an albedo of 0.3 for every surface is this spike's known limitation; a
  surface cache is the next step); stored `rgb`, and `.a` the transmittance to `t1` (0 at a hit). There is no exhausted state to
  hand on. Tests, run for every rung's step count: a wall nearly parallel to the ray at the
  volume's least distance (every ray ends at `t1` or a hit within the step count, no false hit);
  a solid slab spanning ray distances 8.2–9.2 m and a thinner one spanning 8.6–8.7 m, both in
  c3's interval (hits, never sky); a slab entered at 30°; and a **leak measure**: over 10,000
  random rays in the test scene, the share that pass a solid the 256-step reference trace
  catches is under 1 % (recorded per rung, so the approximation is measured, not assumed). The **sky boundary**: a c3 ray that reaches 32 m without a hit
  adds `.a × clouded_sky(dir)` (the dome, `sky.wrela:692`) to its radiance, so an open sky is
  seen through every cascade's transparency; the composition is Sannikov's `(radiance,
  transmittance)` pair per interval.
  **Done when:** an empty scene under a uniform sky: every c3 texel holds the sky's radiance and
  `.a = 1` for c0–c2 (readback); a ray that starts inside a closed box returns `.a = 0` and zero
  radiance at every cascade; a single wall 1 m from a probe is seen by c1 (its interval holds the
  hit) and by no other cascade's own trace.
- **4b. Merge and resolve.** `merge(i)` from c3 down to c0: for each (probe, direction) of
  cascade i, `R_i ← R_i + T_i × R_{i+1}` and `T_i ← T_i × T_{i+1}`, where `R_{i+1}, T_{i+1}` are
  the **depth-aware** bilinear-over-probes, mean-over-its-four-child-directions pair of cascade
  i+1: an upper probe whose depth differs from this probe's by over 10 % gets weight 0 and the
  weights renormalize, falling back to the nearest valid probe when all four are rejected (the
  method's screen-space variant interpolates with depth awareness at every query, not only at the
  final resolve, or light mixes across unrelated surfaces); the radiance and transparency compose
  together, as the method defines. `resolve`: per pixel, the
  four nearest c0 probes weighted bilinearly and by depth similarity (a probe whose depth differs
  by over 5 % is dropped), the cosine-weighted sum over the 16 merged directions → irradiance `E`.
  **Done when (after the merge):** a sphere 1 m from a white wall, the sun on the wall's side: the
  sphere's wall-facing hemisphere's merged c0 radiance is positive (at least 5 % of the wall's
  own radiance, so an all-zero result can't pass) and at least 3× its far side's (readback); a
  foreground post 0.5 m before a lit background wall shows no wall light on the post's
  wall-facing side beyond what the post's own c0 interval sees (the depth-aware merge test); a closed box's inside resolves to zero; an open plane under a uniform sky resolves
  to within 10 % of the sky's irradiance (`π × sky radiance` for a uniform sky) at 100 points;
  two occluders at 1 m and 5 m in one direction give a merged c0 radiance equal to the near
  occluder's alone (the far one is behind it) within 1 %.
- **4c. Into the light.** `lit::sun_and_sky` adds `E × albedo` in place of the probes' diffuse
  term under `Quality::cascades` (default off); the directions rotate per frame by the TAA's
  index in a 4-frame cycle so the TSR gathers them; the probes' term stays for the open sky and
  the specular path.
  **Done when:** with the switch on, the opening renders without a validation error and the
  trunk-shade luma measure of 4d can be taken.
- **4d. The ladder and the rule.** Measures on the opening: the cascades' labels' GPU ms at 1080p;
  the mean luma of the beech trunks' shaded sides in `02-beech-high-forest` (the trunk class's
  pixels whose normal faces away from the sun, before and after); a leak check under the
  climber's hem in `16` (no pixel under the hem brighter than the hem's shaded side). Rungs, each
  with pass mark ≤ 2.0 ms **and** the quality mark (trunk shade luma +15 %, no leak):
  - **R1** c0 at 8 px spacing as above;
  - **R2** c0 at 16 px (120×68) with c1–c3 halved alike;
  - **R3** R2 with 8 steps per ray (`least` doubles; the termination and leak tests rerun at 8).
  **G-spike:** `Quality::cascades` defaults on at the finest rung that meets both marks; off if
  none does. G-look on `02`, `08`, `16`, `17` when it turns on.
  **Done when:** the rung table (ms, luma gain, leak) is in the commit and in `docs/bench.md`
  "Experiments" with the stills, and the default follows the rule.
  Hill to climb: ms at the finest rung that holds the quality mark, → at most 2.0; the trunk
  shade's luma, → +15 %.

**Done when (whole step):** every sub-step's checklist above; the "Experiments" table in
`docs/bench.md` with the four decisions and their numbers; G-profile: the opening's GPU frame p50
at most 12.0 ms with every item that turned on; G-look on the shots each turned-on item names.
Hill to climb: the four ladders', one rung at a time.

### B15. Records

- `docs/toolchain-gaps.md`: "A branch under a false constant still makes its pipelines" is
  Fixed (0.8.0, A6); new gaps found in Part B are added open (per-pass captures for `wrela tool
  passes` if `drive` lacked them; conservative depth if A11 found naga can't; anything else).
- `docs/citations.md`: a row for each method in §9 whose code landed, in the "Methods the code
  follows" table.
- `docs/bench.md`: the sections this plan added (hitch tables, memory shares, warm opens,
  devices, the others, experiments) and "Whether D10 holds".
- `docs/toolchain.md`: `wrela tool bench --lan`, `--row`, `wrela tool passes`, `wrela tool fit`,
  `wrela precision`, `--worst`, `--relaxed-simd`, `--report data`.
- `games/last-green/design.md` (allowed by `wrela.toml` as `games/*/design.md`): the floor's
  switches and their defaults (fog, planar, visibility, neural), the fog field, the water looks,
  the rock parameters, as contracts Ryan can edit.
- `wrela.toml`'s `allow`: `docs/plan-world-class-2.md`, `docs/world-class-next.md` (already).
- `AGENTS.md`: unchanged (it points at `docs/bench.md` already).
- `docs/vision.md`'s constraints table names the budgets; propose (don't edit) D10's wording to
  Ryan in the PR's "For Ryan".

**Done when:** each document named above has the section (grep for each in the commit:
`docs/toolchain-gaps.md` "Fixed in 0.8.0", `docs/citations.md` rows for §9's methods that
landed, `docs/bench.md`'s six sections, `docs/toolchain.md`'s eight commands,
`games/last-green/design.md`'s switches, `wrela.toml`'s allow lines).


### B16. The final gates and the PR

1. `wrela suite` on 0.8.0 passes, every member.
2. G-pixels: the capybara at 1280×720 within 2/255 of `build/base-0.7`; the herd and the climber
   lab within 4/255; the clearing and the Last Green are deliberate changes with lookbook stills
   before and after in the PR (`wrela tool lookbook --package examples/clearing --stills`, and
   the floor's 23 shots).
3. G-pipelines: `wrela pipelines games/last-green --json`'s counts per tier in the PR with the
   per-step changes summed.
4. G-profile (Chrome and native) for the clearing's path, the Last Green's opening and walk,
   the capybara's run and the herd: the table in the PR against `build/base-0.7`.
5. G-size: `wrela tool release` into `build/release`; sizes in the PR against the budgets
   (runtime ≤ 1 MB, time-to-play ≤ 6 MB, cold start ≤ 8 MB as served over the edge's
   compression, `docs/vision.md`); and `find build/release -name '*.br' -o -name '*.gz'` prints
   nothing, for the standard and the relaxed builds (D26).
6. G-bench: `docs/bench.md`'s new entry beside the B13 baseline, with the hitch tables, the
   memory shares, the warm opens, the device rows (B8) and the others' rows (Part C); and
   "Whether D10 holds", scene by scene and device by device, in a sentence each; a scene or a
   device that misses says by how much and what the next lever is.
7. G-hitch, G-cold, G-warm, G-memory as §0.1 states them, on the M4 Air; the M1 Air and the Iris
   Xe rows as measured, whatever they show.
8. The PR description follows the first plan's shape (`docs/plan-world-class.md` §B13.7): what
   changed, deviations from this plan with reasons, the checks with their numbers, the review's
   findings, and "For Ryan" (D10–D26 as built, every hypothesis's outcome, and anything that
   needs his call).

---

**Done when:** the eight items are in the PR with their numbers, `docs/bench.md`'s "Whether D10
holds" has a sentence per scene per device, and every step's **Done when** checklist above is
satisfied or carries a **Blocked** note Ryan has seen (link each in the PR).


## Part C: the others' scenes (`rywible/wrela-others`)

A sibling repository, MIT, for what this workspace may not hold (JavaScript and TypeScript). Its
one PR makes the claim's comparison a measurement. It is independent of Parts A and B and must
be done before §B8.4 and §B16 need its rows.

- **The brief, the same for every engine:** a 1 km² terrain from a heightmap the floor exports
  (`wrela tool export-height -- games/last-green --out height.png`, a new studio sub-command
  writing the floor's `height_at` over 1 km² as a 2048² 16-bit PNG, and `export-path` writing
  `LG_WALK`'s camera positions per frame as JSON, so the land and the walk are the same); 20,000
  trees of three kinds as glTF models with three LODs and a billboard impostor (CC0 models,
  Kenney or Quaternius, licence file beside them); grass as instanced blades within 60 m
  (200,000 blades); a water plane with the engine's reflections; one directional light with
  four 2048² shadow cascades; TAA, bloom, tonemapping; a sky with clouds (the engine's own, or a
  texture); 1080p; the camera on the exported path for 1,200 frames after 60 warm-up frames.
  Each engine at its defaults for these features ("same brief, each engine's defaults"; the
  content's fidelity differs and the README says so).
- **The engines (D19):** `three/` (three.js, its WebGPU renderer and TSL materials, current
  release), `babylon/` (Babylon.js, current major, WebGPU engine), `playcanvas/` (the PlayCanvas
  engine on WebGPU); `unity/` only if a licence is to hand.
- **The harness** (`harness/`, TypeScript, run by `puppeteer-core` against the same headless
  Chrome the bench uses): frame intervals from `requestAnimationFrame` over the 1,200 frames
  (p50/p95/p99, counts over 16.7 and 33 ms); GPU spans from `timestamp-query` where the engine
  exposes a hook to write timestamps round its frame (three.js and Babylon.js expose their GPU
  timing when `timestamp-query` is enabled; note which does); a cold start with the devtools
  cache disabled (`Network.setCacheDisabled`) as the time from navigation to the first presented
  frame after assets load, and a warm start without; JS heap from `performance.measureUserAgent
  SpecificMemory` (GPU memory isn't readable from a page: say so); output JSON in the shape of
  `studio/bench/report.wrela`'s `Measured`, so `wrela tool bench --row` can import it.
- **Acceptance:** each scene runs in Chrome 155 on the M4 Air; `npm run bench -- --engine
  three` writes `out/<engine>-<machine>-<date>.json`; a README names the commands, the engine
  versions and the content's licences; the numbers are transcribed into `docs/bench.md` by hand
  with the command and date (§B8.4).

---

**Done when:** (1) `npm run bench -- --engine three|babylon|playcanvas` each writes its JSON on
the M4 Air in Chrome 155; (2) the three scenes carry the exported heightmap and camera path and
the README says which features each engine supplied and at what setting; (3) the models'
licences sit beside them; (4) the JSONs import with `wrela tool bench --row` and the rows are in
`docs/bench.md` with commands and dates (§B8.4).
Hill to climb: engines with a row, 0 → 3.


## 8. Worked examples

Sketches of the shapes the steps expect, for the implementing agent. None is compiled; names
follow the code they extend, and a sketch that disagrees with the code yields to the code.

**A5, A7: a manifest entry with a tier and constants** (`manifest.json`, one pipeline):

```json
{ "name": "shade", "shader": "pipeline_41.wgsl", "kind": "compute", "entry": "shade",
  "workgroup_size": [64, 1, 1], "conditional": true,
  "constants": { "ambient": true, "taps": 4 },
  "uniform": { "binding": 0, "size": 192, "space": "uniform" }, "bindings": [ ... ] }
```

**A6: the program whose dead pipeline must leave the build:**

```wrela
const ON: bool = false
fn on(s: State) -> bool { ON && s.n == 3 }          // folds to false once inlined into its caller
pub fn frame(mut s: State) {
    if on(s) { dispatch(cull.bind(s.items()), over: s.items()) }   // no pipeline after A6
}
```

**A7: a `const` parameter and what it becomes:**

```wrela
@compute(64)
pub fn blur(src: [f32], out: mut Slots<f32>, taps: const u32, id: GlobalId) { ... }
dispatch(blur.bind(src: a.items(), taps: 4), over: out)   // one pipeline per distinct `taps`
```

```wgsl
override taps: u32;            // WGSL; the host passes { constants: { taps: 4 } }
```

**A12 and B13.3: HDR output:**

```wrela
std::gpu::screen(Screen { hdr: true, gamut: Gamut::DisplayP3 })   // once, at the first frame
// develop, in Hdr: linear light out, a roll-off above 1.0
let y = luma(c)
let rolled = if y > 1.0 { c * ((y / (1.0 + y / look.peak)) / y) } else { c }
```

**B1.2: a hitch row** (the bench's Markdown, under a scene):

```
| Frame | Interval ms | CPU ms | Phases | Heaviest labels | Draws | Written KB |
| 618 | 41.2 | 4.1 | cook 1.9, passes 1.7 | terrain cook 23.4, vegetation 4.0, shadows near 2.5 | 912 | 118 |
```

**B3: the split texel and a rectangle cook:**

```wrela
pub struct Relief: Copy + GpuData { pub height: f32, pub normal: Oct32 }               // 8 B
pub struct Cover: Copy + GpuData { cover, meadow, flowers, under, flora, materials: Unorm8x4 } // 24 B
// follow(): per level, the cover's dirty rectangle only
if let Some(r) = self.dirty[l] {
    dispatch(cook_cover.bind(terrain, fields:, fs:, fields_at:, tiles:, at:, origin:, rect: r,
             out: mut self.covers.span(l * STRIDE, STRIDE)),
             groups: ((r.z - r.x + 7) / 8, (r.w - r.y + 7) / 8, 1))
    self.dirty[l] = None
}
```

**B6a: the page table's word, and the lookup's fallback:**

```
bit 31 valid | bit 30 requested | bits 16–23 age | bits 0–15 pool page (y * 32 + x in a 4096² pool)
lookup(p): for level k from the finest the pixel's footprint allows, upward:
    page = table[k][page_of(p, k)]; if valid(page): return compare(pool, page, p)   // 4 taps
return 1.0   // lit: no page covers p at any level this frame
```

**B9a: the Chebyshev weight of one probe sample:**

```wrela
let d = length(world - probe_at)                 // distance to the probe
let v = visibility.sample_level(s, tile_uv(probe, dir), 0.0)   // .r mean, .g mean²
let var = max(v.g - v.r * v.r, 1e-4)
let cheb = if d > v.r { let t = var / (var + (d - v.r) * (d - v.r)); t * t * t } else { 1.0 }
let back = { let b = (dot(n, normalize(probe_at - world)) + 1.0) * 0.5; b * b + 0.2 }
let w = cheb * back * trilinear
```

**B10: the froxel integrate, one invocation per column:**

```wrela
var t = 1.0; var l = vec3(0.0)
for i in 0..SLICES {
    let s = scatter.load(x, y, i)                 // .rgb in-scatter S, .a extinction σ
    let dz = depth_of(i + 1) - depth_of(i)
    let step_t = exp(-s.a * dz)
    l += t * s.rgb * (1.0 - step_t) / max(s.a, 1e-4)   // analytic within the slice
    t *= step_t
    lit.store(vec3u(x, y, i), vec4(l, t))
}
```

**B11a: the coarse normal and the presence of a fine vertex:**

```wrela
let w = vec3(2.0 * cell_next)
let g_w = gradient(|x| filtered(|y| field.distance(y), x, w), p)
vertex.coarse_normal = Oct32::encode(normalize(g_w))      // what the next level's surface shows here
let near = coarse.nearest_vertex(p)                        // the coarse mesh's own vertices, by block
vertex.presence = saturate(2.0 - distance(p, near) / cell_next)   // 1 within a cell of the coarse surface
// in the vertex shader: n = normalize(mix(n, coarse_normal, blend)); discard where presence < blend
```

**B14.1: what `studio/fit` writes:**

```wrela
// content/kit/nets/beech.wrela, written by `wrela tool fit -- --net impostor --kind beech`
pub const BEECH: Net<12, 32, 8> = Net { w0: [ /* 12×32 f16 bits */ ], b0: [...], w1: [...], b1: [...], w2: [...], b2: [...] }
```

## 9. Sources to cite when their code lands

Each gets a row in `docs/citations.md` (method, where it's used, licence or publication) when its
step merges. Use them as references, not as code to copy. The first plan's §7 rows that still
apply (Haar and Aaltonen; Karis 2014, 2021; Greene; Burns and Hunt; Jimenez; Majercik; Olsson;
McGuire and Bavoil; Satish; van Waveren; Chlumský; Hillis and Steele) are already in
`docs/citations.md`.

| Method | Reference | Used in |
|---|---|---|
| Virtual shadow maps: page tables, clipmaps, caching and invalidation | Epic Games, *Virtual Shadow Maps* (UE5 documentation) and *Virtual Shadow Maps in Fortnite Battle Royale Chapter 4* (tech blog); octoon, *UnityVSM* (GitHub, after the GPU Zen 3 article); M. Sakmary, *Virtual Shadow Maps*, master's thesis, CTU Prague 2025; *Resolution Matched Virtual Shadow Maps*, CESCG 2025 | §B6 |
| Percentage-closer soft shadows | R. Fernando, *Percentage-Closer Soft Shadows*, SIGGRAPH 2005 sketch | §B6c |
| Probe visibility (depth and Chebyshev weights) | Majercik, Guertin, Nowrouzezahrai, McGuire, *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields*, JCGT 2019 (already cited; the visibility half) | §B9a |
| Global signed-distance fields for tracing | Epic Games, *Lumen Technical Details* (software ray tracing against mesh and global distance fields) | §B9b |
| Variable-rate ray budgets | M. Olejnik, *Variable Rate Ray Tracing in Call of Duty: Modern Warfare 4*, SIGGRAPH 2026 Advances in Real-Time Rendering | §B9c |
| Froxel volumetric lighting | B. Wronski, *Volumetric Fog: Unified Compute Shader-Based Solution to Atmospheric Scattering*, SIGGRAPH 2014 Advances; S. Hillaire, *Towards Unified and Physically-Based Volumetric Lighting in Frostbite*, SIGGRAPH 2015 Advances; A. Mueller, *Smolder*, SIGGRAPH 2026 Advances (layering) | §B10 |
| Cloud light-march economy | A. Schneider, *The Real-time Volumetric Cloudscapes of Horizon: Zero Dawn*, SIGGRAPH 2015 Advances (already cited as Nubis 2017) | §B5.4 |
| Continuous LOD from the field: coarse normals and presence from the filtered field | The engine's own method; the filtered derivation is `spec/language.md` §14.1; J. Hable, *Adaptive Tessellation and Subdivision*, SIGGRAPH 2026 Advances, read for the screen-adaptive half | §B11a |
| Interval pruning of field expressions | M. Keeter, *Massively Parallel Rendering of Complex Closed-Form Implicit Surfaces*, SIGGRAPH 2020 (already in the vision's prior art) | §A8, §B2.5, §B9b |
| Flow-mapped water | A. Vlachos, *Water Flow in Portal 2*, SIGGRAPH 2010 Advances | §B12.1 |
| Sheen for cloth | A. C. Estevez and C. Kulla, *Production Friendly Microfacet Sheen BRDF*, SIGGRAPH 2017 talk | §B13.2 |
| Temporal upscaling: dilation, reactive mask, locks | AMD, *FidelityFX Super Resolution 2* (MIT), its documentation's structure; D. Craig, *Upgrading PSSR on PlayStation 5 Pro*, SIGGRAPH 2026 Advances (where learning belongs) | §B13.1, §B14.3 |
| Neural appearance in shaders | T. Zeltner et al., *Real-Time Neural Appearance Models*, ACM TOG 43(3), 2024 (arXiv 2305.02678); L. Belcour and A. Benyoub, *Hardware Accelerated Neural Block Texture Compression with Cooperative Vectors*, arXiv 2506.06040, 2025 | §B14.2 |
| Radiance cascades | A. Sannikov, *Radiance Cascades: A Novel Approach to Calculating Global Illumination* (2023, github.com/Raikiri/RadianceCascadesPaper); the Chalmers 2025 theses on screen-space RC in 3D games (Gavras; *Screen Space Global Illumination Using Radiance Cascades in 3D Video Games*); R. Freeman and A. Sannikov, *Split Radiance Cascades: Real-Time Global Illumination via Sparse Radiance Probes*, arXiv 2607.20384, 2026 | §B14.4 |
| Pipeline-overridable constants, `textureGather`, conservative depth | W3C, *WGSL* specification (§ `override` declarations; `textureGather`; the `fragment_depth` language extension); Chrome for Developers, *What's New in WebGPU 155–156* | §A7, §A10, §A11 |
| HDR and wide-gamut canvases | W3C, *WebGPU* specification (`GPUCanvasConfiguration`: `colorSpace`, `toneMapping`); Chrome for Developers, *What's New in WebGPU 129* and *155–156* | §A12 |
| WebAssembly code caching and streaming compilation | V8, *Code caching for WebAssembly developers* (v8.dev blog) | §A2 |
| Relaxed SIMD | WebAssembly *Relaxed SIMD* proposal (standardized; Chrome 114+, Firefox 146+, not Safari per caniuse) | §A13 |
| Edge compression | Cloudflare, *Content compression* (the compressible content types, `application/wasm` among them) | §A3, §B2.1 |
| Nanite-class renderers in the browser, as context | Scthe, *nanite-webgpu* (MIT); *three-virtual-geometry* (npm); *CuRast*, arXiv 2604.21749, 2026 | §0.6 (why software raster stays off) |
| Structural similarity for the lookdev instrument | Z. Wang, A. C. Bovik, H. R. Sheikh, E. P. Simoncelli, *Image Quality Assessment: From Error Visibility to Structural Similarity*, IEEE TIP 2004 | §B1.8, G-look |
| Models for Part C | Kenney (CC0) or Quaternius (CC0) tree packs; the licence file beside them | Part C |

## 10. The council's findings

A council of two Codex reviewers (`gpt-6.1-sol`, correctness and verification; `gpt-6-astra`,
architecture and risk; both at high reasoning effort, read-only, through `codex exec` on the
branch's checkout) reviewed the plan on 2026-10-25 with a Claude leader (this thread; the council
is not single-vendor). Three rounds, six reviewer tasks, the protocol of `~/.claude/skills/council`.
The findings of each round, the decision on each and what changed.

### Round 1

Both reviewers returned six findings; four were raised by both.

| ID | Finding | Decision | What changed |
|---|---|---|---|
| R1-A-F1 | B5.6 declared `WithDepthGreater` blanket: the soft raster's merge draws at z 0.5 and writes arbitrary depths, and impostors had no bound; WGSL may return an indeterminate depth when the promise is broken. | Accepted. | §B5.6 establishes a bound per draw (the merge triangle at z 0.0, the far plane; impostor cards at their instance's far bound so depth only moves nearer) and adds `engine/tests/render/conservative-depth`, which asserts `written ≥ rasterized` per fragment and identical results against plain `WithDepth`. |
| R1-A-F2, R1-B-F2 | B11a claimed that projecting fine vertices onto the filtered field's zero set gives a crack-free, pop-free correspondence to the coarse mesh. `filtered` is a first-order box filter, not a convolution; the coarse mesh's connectivity and thin features differ; a sphere test proves nothing general. | Accepted; the claim withdrawn. | §B11a rewritten: both levels extract the same raw field; position continuity is measured by a switch sweep on production assets (the tower, a crag, a post, a bank stone, four views, every switch distance) against the same dolly with the level pinned; the filtered field supplies only the coarse shading normal; a presence term fades features the coarse grid lost; a prototype rung B11a.0 on the tower runs the sweep before any caller is rewired. |
| R1-A-F3, R1-B-F4 | B14.4's cascades had no sky at the outer boundary, a wall at 1 m couldn't be seen by c0's own interval, the merge mixed probes across depth, and all-zero radiance could pass the ratio test. | Accepted. | §B14.4: a c3 miss adds the dome's sky times transmittance; `(R, T)` compose per interval; the merge is depth-aware at every query; tests moved after the merge and require positive radiance; new tests for a uniform sky, a wall seen only by c1, two occluders, a post before a lit wall. |
| R1-A-F4 | A7 gave overrides numeric ids while the manifest keyed constants by name (WGSL §7.2.2 forbids mixing), and tested dead-code folding by the WGSL text, which the driver folds later. | Accepted. | §A7: no `@id`, name-keyed `constants` on both hosts; behavioural tests (distinct readbacks, no validation errors) and a recorded microbench replace the text test. |
| R1-A-F5, R1-B-F5 | G-look's reference accumulated 64 frames while time advanced (a motion blur, not a truth); flicker and pop reward smoothing; the implementer could name its own exceptions and score its own rubric; a reference that tracks the approximation can't catch detail removal. | Accepted. | §0.5 G-look and §B1.8: time frozen for the reference and every measure; negative controls (blur on a named detail shot, a ghost trail, a sharp moving edge, a detail-removal control whose standing is "degraded" whatever PSNR says); measures read together; exceptions need Ryan's written acceptance; judging is blind, by a separate agent in a fresh context with the pair in random order, calibrated on the degradation control, every attempt kept; `docs/lookdev.md` states per subsystem whether the reference bypasses the approximation or only raises its settings. |
| R1-A-F6, R1-B-F6 | A13 told the build to write `game.relaxed.wasm` with "its own `.br`", against D26. | Accepted. | §A13 writes no compressed file; §A3's test covers builds with and without `--relaxed-simd`; §B16's G-size asserts `wrela tool release`'s output holds no `.br` or `.gz`. |
| R1-B-F1 | B14.2b made a 26 dB mark for a 488-parameter net the proof of the trainer, and "return to 1b until A passes" could trap the mandatory experiments forever. | Accepted. | §B14.1b's finite-difference and sine tests are the trainer's correctness; every rung runs under a fixed budget (4,000 steps, batch 16,384, three seeds) and is recorded with its loss curves; a failed mark is a result under that budget and the ladder goes on; the minimum deliverable is the four rows; the 26 dB mark is labelled estimated. |
| R1-B-F3 | B6b promised a coarse fallback without saying how coarse pages become resident; after a cold start or a teleport, receivers and fog samples would find no page and read "lit". | Accepted. | §B6b reserves pages that are always requested and never released, draws them first after a cold start, teleport, recentre or sun change, and gates `Frame::playable` on them; the coverage test runs from an empty pool, after a teleport and after a sun step, for receivers and fog samples. (Round 2 corrected the reservation's extent; see below.) |

Reviewer A also confirmed the conservative-depth contract and the override rules against the WGSL
specification, and Reviewer B the coarse-page requirement against Epic's documentation.

### Round 2

Both reviewers read the revised plan. They marked R1-A-F1, R1-A-F4, R1-A-F6/R1-B-F6, R1-B-F1 and
U4 resolved, and sustained four objections with evidence the leader verified; two new findings
were raised, both about the leader's Round-1 fixes.

| ID | Finding | Decision | What changed |
|---|---|---|---|
| R2 on F2 (both) | The revised B11a still asserted an unconditional fine-to-coarse distance bound; `look.wrela:9` excludes corners and thin parts from the extraction's contract and `realize.wrela:1807`'s error measure is one-sided. | Accepted. | §B11a says the plan does not rely on the implication; the switch sweep is the acceptance criterion and B11a.0 runs it first. |
| R2-A-F1, R2-B-F1 | The new presence term, `saturate(−filtered(d, p, w) / cell)`, is 0 on every surface point (the filter preserves an affine field's zero), so broad faces would be discarded during the blend. | Accepted. | Presence comes from the coarse extraction's own occupancy (the live-block list and `block_map`), 1 inside a live coarse block and falling to 0 over one coarse cell; a thick cuboid swept through the band must keep its face interiors covered (new test); the §8 example corrected. |
| R2 on F3/F4 (both) | A ray that spends its 16 steps short of `t1` carries no reached distance into an `(R, T)` pair, the next cascade starts at its own interval, and c3 has no successor: an occluder at 6 m could be skipped. | Accepted. | §B14.4 4a: `step = max(d, (t1 − t0) / 16)` per cascade, so every ray reaches `t1` or a hit within 16 steps by construction; no exhausted state exists; a ray that leaves the distance volume continues as empty space to `t1` (a recorded limitation); the worst-case grazing wall is a unit test. |
| R2 on R1-B-F3 (both) | Reserving each level's central 4×4 covers a quarter of its width: level 9's reservation was a 1,024 m square, not 4 km, and the "k+2" argument runs out of levels at k = 8, 9. | Accepted. | §B6b reserves all 256 pages of level 9 (the whole 4 km domain at 2 m texels) plus the central 4×4 of levels 0–8 (400 of 1,024; 624 serve requests; a growth rule to 1,536 pages if the requested p95 exceeds them); the guarantee is restated; the checklist adds the eviction rate and a coverage test that projects the whole frustum into shadow space at every level after snapping and filter footprint. |
| R2-A-F2, R2-B-F2 | The ghost-trail control required `pop` to rise, but a history blend lowers frame-to-frame differences while it trails: a correct instrument would fail the control. | Accepted. | §B1.8 adds `trail` (the ghosting length behind the sharpest moving edge against the reference at four dolly positions) and `still_pop` (pop's comparator in pop's units); the ghost control must lengthen `trail` and lose PSNR while `pop` may stay or fall; the blur control runs on the named detail shot; G-look (3) lists `trail`. |
| R2 on U3 (both) | The switch-pop threshold added a variance (flicker) to a mean absolute difference (pop). | Accepted. | The comparator is the same dolly with the level pinned (no switch), the same units, plus 0.002 as a starting value B11a.0 calibrates. |
| R2 on U1 (both) | Fixed per-rung budgets are a fair bounded experiment, but a failure under them is not a capacity claim; "until one passes" conflicted with "all four rungs". | Accepted. | All four rungs run regardless; loss curves recorded; failures described as results under the budget. |
| R2 on U4 (both) | A fresh, blinded judge is a reasonable floor; one calibration pair doesn't establish sensitivity to lighting or temporal errors. | Accepted. | Every calibration attempt is kept; temporal changes are judged on three-frame strips at a moving edge as well as stills. |

### Round 3

Both reviewers read the four rewritten passages. They marked R1-A-F2/R1-B-F2 and R1-A-F5/R1-B-F5
(with R2's two new findings) resolved, R1-A-F3/R1-B-F4 resolved by Reviewer B and disputed by
Reviewer A on one rung, R1-B-F3 still disputed by both on one sentence, and raised two new
findings, both about the leader's Round-2 fixes. The leader verified each and revised the plan;
the council was at its round limit, so these last revisions were not re-reviewed.

| ID | Finding | Decision | What changed |
|---|---|---|---|
| R3-A-F1, R3-B-F1 | The forced minimum advance `max(d, (t1 − t0) / 16)` can jump over a solid thinner than the advance (a slab at 8.2–9.2 m from c3's 8 m start, distance 0.2, lands at 9.5 m and admits sky); the termination test proves completion, not visibility. | Accepted. | §B14.4 4a: a conservative bounded advance: a step of `d` where `d ≥ least`; where `d < least`, a hit at `t + d / a` when the ray approaches the surface within `least` (`a = −dot(∇d, dir)`), else an advance of `least` with the field read at its midpoint and end; the last advance clamps to `t1`; tests for the 8.2–9.2 m and 8.6–8.7 m slabs, a 30° slab, the grazing wall, and a leak measure under 1 % against a 256-step reference, per rung. |
| R3-A on F3/F4 | R3's rung allowed 8 steps while the construction assumed 16. | Accepted. | The floor is the rung's step budget (`least = (t1 − t0) / steps`); the termination and leak tests rerun at 8. |
| R3-A-F2, R3-B-F2 | The pool-growth branch (1,536 pages, 96 MiB) contradicted the 1,024-page stack assertion and the 68 MiB shadow gate. | Accepted. | `POOL_PAGES` is one constant every assertion reads; the `shadows` share is at most 68 MiB at 1,024 pages and 101 MiB at 1,536, whichever the commit chose. |
| R3 on R1-B-F3 (both) | Level k+2's central 4×4 spans exactly level k's width, so differently snapped centres or the filter's footprint break containment; (9b) could pass by finding level 9. | Accepted. | The near-eye guarantee is k+3 (twice the width: half a width of margin each side) for k ≤ 6, levels 7–8 fall back to level 9; (9b) asserts the returned level bound, not mere residency. |
| R3-B on presence | A thin appendage sharing a live coarse block with retained geometry got presence 1 and couldn't fade; presence was undefined for an empty coarse level. | Accepted. | Presence is distance to the coarse mesh's nearest vertex (1 within a cell, 0 at two), 0 everywhere when the coarse level has no live block; tests for the rod on a thick base and the empty coarse level. |
| R3-B on 14.2b | "a fact about the model's capacity" contradicted the budget-qualified reading. | Accepted. | "a result about reconstruction under that budget, comparable across rungs". |

Reviewer A noted that acceptance of B11a remains conditional on the sweep's measurements, and
Reviewer B that the resolutions settle the planning objections, not whether the gates will pass:
both are what the gates are for.

**Remaining uncertainty, and the check that settles each:** the 26 dB rung mark and the 4,000-step
budgets (the first ladder calibrates them; §B14.2b records every curve); the pool pressure under
400 reserved pages (§B6b measures requested pages, pages drawn and the eviction rate, with the
growth rule); the switch-pop allowance of +0.002 over the pinned dolly (§B11a.0 calibrates it);
whether a blinded fresh-context judge is independent enough (the calibration record says; Ryan's
verdict is final either way). None changes the plan's shape; each has a gate.
