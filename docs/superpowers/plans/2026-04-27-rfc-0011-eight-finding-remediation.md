# RFC 0011 Eight-Finding Remediation Plan

> **Completion gate:** this remediation is not complete until an independent subagent reviewer audits the live repository against the original RFC 0011 implementation plan, using the same kind of code-review pass that produced these eight findings, and gives the result a pass.

**Goal:** Close the eight audit gaps in the in-progress RFC 0011 interactive runtime so the repo is not merely report-shaped, but actually executes authored live runtime behavior with honest latency, input, presentation, systems, physics, and perf-latency gates.

**Strategy:** Fix the live spine first, because authored project execution depends on input and subsystem ordering being trustworthy. Then replace compatibility shells with real project-derived subsystem plans. Finally make the perf and latency gates prove the acceptance criteria instead of proving that smoke reports exist.

## Findings Covered

- F1: Authored projects are only counted, not executed.
- F2: Late input drops the bindable detail.
- F3: Presentation can run before gameplay subsystems finish.
- F4: Project systems remain unsupported.
- F5: Collision-backed physics does not execute collision batches.
- F6: Interactive live exits after one frame by default.
- F7: perf-latency is not measuring the latency acceptance criteria.
- F8: Display latency stage is always zero.

## Phase 1: Repair Live Input and Loop Semantics

**Fixes:** F2, F6.

- [ ] Preserve `TimestampedRawEvent.detail` when converting to `TickInputEvent`; do not stringify `RawInputKind` as the semantic detail.
- [ ] Keep the raw kind available through a separate telemetry/debug note only if needed; it must not replace the bindable source/detail pair.
- [ ] Add a regression test that pushes a real reference-host keyboard event (`source=keyboard`, `detail=key.KeyW`) through `RawInputRingLateSampler` and verifies `InputMapPlan` produces `MoveForward`.
- [ ] Change interactive `wrela live` semantics so a missing `--frames` means continuous interactive mode. Keep `--frames=N` as an explicit bounded smoke/test mode.
- [ ] Ensure `wrela live --headless` still requires or defaults a finite frame count suitable for scripts.

**Verification:**

```bash
cargo test -p wrela --test input_subsystem
cargo test -p wrela --test live_host
cargo run -p wrela -- live examples/surface_and_input/src/main.wr --frames=2 --json
```

## Phase 2: Enforce Correct Live Subsystem Ordering

**Fixes:** F3.

- [ ] Update the reference presentation adapter so Presentation runs after every gameplay producer it consumes: at minimum `StateAdvance`, `Input`, `System`, `Residency`, `Physics`, and `Audio` when those adapters are registered.
- [ ] Prefer deriving this dependency list from the host's registered subsystem set rather than hard-coding a stale order in multiple places.
- [ ] Keep Save after Presentation for one-shot saves, matching the existing smoke expectation.
- [ ] Add an ordering test that proves `presentation.swapchain_acquire` starts after the terminal jobs of System, Residency, Physics, and Audio.

**Verification:**

```bash
cargo test -p wrela_reference_host --test smoke
cargo test -p wrela --test engine_frame
```

## Phase 3: Build Project-Derived Runtime Plans

**Fixes:** F1, F4.

- [ ] Replace `ReferenceProjectExecutor`'s count-only behavior with a `ReferenceProjectRuntime` built from `LoadedProject`.
- [ ] Build `InputMapPlan` from authored `RuntimeFunctionMetadata::InputMap` instead of the hard-coded `reference_input_map`.
- [ ] Build `SystemProgram` from the loaded module and remove the `UnsupportedBackend` rejection for projects that contain systems.
- [ ] Provide a real compiled-system invoker boundary. If full MIR execution is still incomplete, make unsupported system constructs fail loudly at project load instead of silently substituting the no-op reference invoker.
- [ ] Add an integration fixture whose authored input_map and system mutate observable runtime state, then assert the reference host report/inspector reflects that authored state.
- [ ] Ensure a project with no authored input_map/system still gets a tiny generic fallback so smoke tests remain useful, but label it explicitly as fallback in report notes.

**Verification:**

```bash
cargo test -p wrela --test system_access_summary
cargo test -p wrela --test system_plan_validation
cargo test -p wrela --test system_adapter
cargo test -p wrela_reference_host --test smoke
cargo run -p wrela -- check examples/systems_basic/src/main.wr
```

## Phase 4: Make Collision-Backed Physics Actually Collision-Backed

**Fixes:** F5.

- [ ] Split `PhysicsSolver::step` into CPU-oracle and collision-backed execution paths.
- [ ] In the collision-backed path, submit each generated `CollisionWorkloadBatch` through the existing collision execution contract and derive contacts from returned overlap/sweep/TOI witnesses.
- [ ] Keep the analytic ground/sphere solver as CPU oracle and fallback, not as the implementation used after declaring the backend collision-backed.
- [ ] Record `physics.detect_contacts`, `physics.contact_readback`, and CPU-oracle-divergence findings from real execution results.
- [ ] Add a test that uses a fake or noop collision executor and fails if generated batches are only recorded and never submitted.
- [ ] Add CPU-oracle parity coverage for a small deterministic scene.

**Verification:**

```bash
cargo test -p wrela --test physics_adapter
cargo test -p wrela --test physics_xpbd_determinism
cargo test -p wrela --test collision_exec
```

## Phase 5: Report Honest Display and Motion-to-Photon Latency

**Fixes:** F8.

- [ ] Extend the live host or presentation adapter to provide display timing metadata: refresh rate, present mode, and whether VRR is known to be active.
- [ ] Populate `estimated_present_to_photons_nanos` as one refresh interval for non-VRR FIFO-style presentation, and zero only when VRR-in-range or another explicit low-latency present path is known.
- [ ] Set `MeasurementQuality` honestly: synthetic for benchmark synthetic stamps, exact only when GPU timestamps/present callbacks are actually used, estimated otherwise.
- [ ] Add tests for 60 Hz FIFO, 120 Hz mailbox/VRR, and unknown-refresh fallbacks.

**Verification:**

```bash
cargo test -p wrela --test live_host
cargo test -p wrela --test present_mode_policy
cargo test -p wrela_reference_host --test smoke
```

## Phase 6: Replace perf-latency Smoke With a Real Latency Gate

**Fixes:** F7.

- [ ] Implement or wire `wrela perf-latency <project>` so it injects synthetic timestamped input through the same late-input path used by the reference host.
- [ ] Collect per-frame motion-to-photon samples and report p50/p95/p99.
- [ ] Fail the lane when p99 exceeds the policy target, using closure findings such as `presentation.motion_to_photon_perf_lane_over_budget`.
- [ ] Update `just perf-latency` to call the real perf-latency command and pass the latency budget gate, not just bounded `wrela live`.
- [ ] Keep the lane deterministic/offscreen-friendly for CI; hardware-specific full-window measurements can be a separate optional human lane.

**Verification:**

```bash
just perf-latency
just ship
```

## Phase 7: Full Acceptance Sweep

- [ ] Run the focused test set from Phases 1-6.
- [ ] Run the canonical project gates:

```bash
just check-clean
just test
just test-all
just perf-engine-closure
just perf-latency
just lint-layering
just ship-interactive
```

- [ ] Manually smoke `wrela live examples/surface_and_input/src/main.wr` and confirm it stays open interactively unless `--frames` is provided.
- [ ] Confirm reports contain honest subsystem spans and no fallback notes for authored features that should be project-derived.

## Independent Review Gate

- [ ] After all remediation work and verification commands pass, spawn an independent reviewer subagent.
- [ ] Give the reviewer the original RFC 0011 plan and the eight review findings.
- [ ] Ask the reviewer to audit the current live repo state against the original plan, looking for bugs, AC gaps, performance risks, and maintenance smells.
- [ ] Remediation is complete only if that independent subagent reviewer gives the implementation a pass.
- [ ] If the reviewer finds issues, convert them into a follow-up checklist and repeat this gate after fixes.

