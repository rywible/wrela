# M4 tasks (deleted when the milestone closes)

Scope #43, criteria #42. Status: `[ ]` open, `[x]` done.

## 0. First measurement
- [x] Spike 01's fixtures in `compiler/tests/fixtures/spike01` (WGSL, grazer.js, main.js, cpu.wasm)
- [x] Measure sketch 02 (extraction, frame) and sketch 03 (tick), physique; post on #42

## 1. Platform and compiler foundations
- [ ] Memory: thread slots and blocks (stack, panic message, par descriptor, allocations), tick region; DATA_BASE moves
- [ ] `@thread_entry`, `@effects(...)`; only the unsafe core; worker and audio through them; special cases go
- [ ] `std::mem::task` (a task from a function): audio, tick and jobs use it
- [ ] `std::par`: a descriptor per starting thread, nested jobs inline, job slots, `job`, `Job::done`, `Job::take`, traps kept
- [ ] `allocations()` per thread
- [ ] `std::tick`: `start`, `Ticked`, `origin`, hash reporting; `__tick`
- [ ] `std::handoff`: triple buffer, test-build checks
- [ ] Native host: tick instance, lockstep, records, tick log, `--no-gpu`, `--replay`, no batches on long runs, cached CpuBuild
- [ ] `@test(frames: n)` ticks in lockstep; `@test(ticks: n, input: ...)`
- [ ] Stream v6 `DrawIndexedIndirect`; manifest cull and depth bias; `draw(indices:, cull:, depth_bias:)`, build-time constants
- [ ] Browser: sim worker, clock, catch-up, visibility and shutdown words, input ring's second reader, stamping, lockstep and paced test modes, helpers = min(cores − 3, 8), job traps, load timing
- [ ] x86-64 native host under Rosetta

## 2. Engine
- [ ] std `Surface::filtered` (a footprint), combinators forward it
- [ ] `run` (driver, `Asks`, `TickInput`), `Timeline` input `Clone`, `DT` and `TICKS_PER_SECOND` go
- [ ] `present`: snapshots, interpolation
- [ ] `realize`: grid, 64-bit part masks, shared-corner masks, corners in workgroup memory, QEF, skin weights, quads, indexed indirect
- [ ] Mesh pools, slabs, calibration and overflow readbacks, queue, LOD
- [ ] `render`: posing pass (`Poser`), skinning, shadow pass, shading, views
- [ ] `physique`: the spike's method, as a job
- [ ] `creature`: skin bindings (bone, chain, split)

## 3. The herd
- [ ] `examples/herd`: grazer, walk, terrains, placement, tick, snapshot, look, cameras

## 4. Checks
- [ ] Harness (TS): port of `encodeExtract`, `update`, `encodeFrame`; held to the recorded run
- [ ] AC1–AC7 tests; AC9 `check.sh --gpu`; speed and size budgets

## 5. Record
- [ ] language.md, vision.md, #26, retrospective on #42, AC8
