# M4 tasks (deleted when the milestone closes)

Scope #43, criteria #42. Status: `[ ]` open, `[x]` done.

## 0. First measurement
- [x] Spike 01's fixtures in `compiler/tests/fixtures/spike01` (WGSL, grazer.js, main.js, cpu.wasm)
- [x] Measure sketch 02 (extraction, frame) and sketch 03 (tick), physique; post on #42

## 1. Platform and compiler foundations
- [x] Memory, `@thread_entry`, `@effects`, `std::mem::task`, `std::par`, `std::tick`, `std::handoff`
- [x] Native host: tick instance, lockstep, records, tick log, `--no-gpu`, `--replay`
- [x] `@test(frames: n)`, `@test(ticks: n)`; stream v6 indexed indirect draws; cull mode and depth bias
- [x] Browser: sim worker, clock, lockstep and paced test modes, helpers, job traps, load timing
- [x] x86-64 native host under Rosetta
- [x] Both hosts: a buffer write is a copy in the encoder (upload ring), not a submission
- [x] `std::derive::within`; `Lipschitz::interval_near`

## 2. Engine
- [x] `run`, `present`, `realize` (grid, masks, shared corners, workgroup corners, QEF, skin pass, quads, indexed indirect)
- [x] Mesh pools, calibration and overflow readbacks, queue, LOD
- [x] `render` (posing, skinning, shadow, shading), `physique` (spike's method, a job), skin bindings

## 3. The herd
- [x] `examples/herd`: grazer, walk, terrain, placement, tick, snapshot, look, cameras

## 4. Checks
- [x] Spike harness (Rust/wgpu) held to the recorded run
- [x] AC2 test passes (live, holes, Hausdorff, triangles, GPU ≤ 1.25×)
- [ ] AC2: shared-corners test build; readback count test
- [ ] AC1: parity frames (native vs spike, Chrome vs native); 600 paced frames; pipelines ≤ 64; load numbers
- [ ] AC3: frame GPU times vs the spike harness
- [ ] AC4: LOD image and cost, spawn and camera load, latency, memory, 10-minute tour, cold pipelines
- [ ] AC5: 10,000-tick hashes in every configuration; tick p99; misuse diagnostics
- [ ] AC6: check every item against earlier work (physique job, replays, torn reads, jobs)
- [ ] AC7: field 1e-6 near the surface, channels 1e-5, skin weights vs fixture mesh, layering, units, std examples
- [ ] AC9: `check.sh --gpu`, speed and WGSL budgets, runtime size

## 5. Record
- [ ] language.md, vision.md, #26, retrospective on #42, AC8
