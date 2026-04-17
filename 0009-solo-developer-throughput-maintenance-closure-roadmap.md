# RFC 0009: Solo-Developer Throughput, Architectural Boundaries, And Maintenance Closure Roadmap

Status: Proposed post-Phase-48 maintenance-closure roadmap after repo read, RFC read, and workspace/tooling review

Author: GPT-5.4 Pro

Created: 2026-04-16

Target: post-Phase-48 `wrela` workspace, compiler, runtime, CLI, tests, benchmarks, and local human-plus-agent workflows

## Summary

Wrela now has enough semantic structure, performance machinery, and benchmark discipline that the next bottleneck is no longer “can the engine do interesting things?”

The bottleneck is whether the repo itself is fast, obvious, and trustworthy to work in every day.

The current codebase already has the hard ingredients that many ambitious compiler/engine repos never reach:

- explicit query contracts, plans, and execution backends
- typed presentation and collision pipelines
- a CPU oracle and a serious WGSL path
- closure-oriented performance profiles
- broad regression coverage across compiler, runtime, CLI, presentation, and collision
- a strong semantic bias in the architecture and the repo vision

That is excellent news.

It also means the maintenance problems are now first-class product problems.
If the local loop is slow, if file ownership is unclear, if command surfaces are fragmented, if invariants live only in the author’s head, or if the hot paths keep paying obvious allocation and lookup costs, feature velocity will decay no matter how good the architecture is.

This RFC proposes a **maintenance closure program** optimized for the actual use case of this repo:

**one developer shipping quickly, locally, with agent collaborators, without CI, while preserving semantic rigor.**

This repo is also largely written by agentic AI.
That is not an implementation detail.
It means the maintenance plan must make Wrela legible, sliceable, and verifiable for a human-plus-agent workflow where contributors orient from local context, follow explicit seams, run canonical commands, and leave behind trustworthy evidence.

The target is simple:

**Wrela should feel like a gold-standard shipping repo: fast to change, obvious to navigate, hard to misuse, and pleasant to work in for long stretches.**

This is not a generic cleanup document.
It is a concrete roadmap with budgets, module surgery, command-surface rules, readability standards, low-risk performance work, and explicit human/agent collaboration constraints.

The work is organized around five closures:

1. **Developer-loop closure** — build, check, test, perf-smoke, and ship flows become measurable and fast.
2. **Architecture closure** — bounded contexts, public seams, and dependency rules become explicit.
3. **Readability closure** — dense code explains itself through naming, module shape, types, and targeted invariant comments.
4. **Hot-path hygiene closure** — low-risk performance wins are taken systematically instead of incidentally.
5. **Workflow closure** — `just`, `cargo`, and `wrela` stop competing and start forming one coherent local workflow.

## Relationship To Earlier RFCs And Repo Vision

This roadmap builds directly on:

- `language/spec/rfcs/0001-field-game-language.md`
- `language/spec/rfcs/0002-field-engine-implementation-roadmap.md`
- `language/spec/rfcs/0003-phase-9-5-semantic-convergence-plan.md`
- `0004-question-families-query-contracts-roadmap.md`
- `0005-realtime-presentation-view-plans-frame-contracts-roadmap.md`
- `0006-certified-world-snapshots-temporal-semantics-artifact-runtime-query-program-spine-roadmap.md`
- `0007-shared-acceleration-spine-1080p120-rendering-collision-roadmap.md`
- `0008-gpu-resident-framegraph-1080p120-rendering-collision-roadmap.md`

RFCs 0004 through 0008 made the engine and compiler more semantically explicit and more performance-serious.
They gave the repo real contracts, real closure targets, and real runtime architecture.

RFC 0009 does not replace those documents.

It makes the **repo itself** worthy of those designs.

In other words:

- RFCs 0004–0008 made Wrela structurally correct and increasingly fast.
- RFC 0009 makes Wrela structurally delightful to ship from.

That means this RFC is intentionally repo-centric:

- it optimizes for solo local development, not CI theater
- it treats the developer feedback loop as a first-class budget
- it treats file boundaries and command boundaries as architecture
- it treats human-and-agent collaboration as a first-class workflow constraint
- it treats readability and invariants as real engineering assets
- it prefers measurable throughput improvements over aesthetic cleanup churn

## Current Repo Read

The repo is strong in exactly the places that justify a maintenance RFC instead of a rewrite.

### What is already strong

1. **The domain model is real.**  
   The workspace is not a loose pile of engine experiments. It has distinct modules for `scene_ir`, `semantic_evidence`, `query_contract`, `query_plan`, `query_solver`, `presentation_contract`, `presentation_plan`, `collision_contract`, `collision_plan`, `artifact_contract`, and `artifact_store`.

2. **Performance work already has real instrumentation surfaces.**  
   `compiler/perf_target/mod.rs`, the benchmark suites under `benchmarks/`, and the phase-48 playbooks in `docs/perf/` mean the repo already knows how to talk about closure, budgets, and bottlenecks.

3. **The tests are broad.**  
   The compiler test surface is large and touches many real seams: CLI behavior, query execution, presentation execution, collision execution, semantic evidence, contracts, project end-to-end flows, and more.

4. **The repo vision is coherent.**  
   `AGENTS.md` is unusually clear about the long-term substrate: authored meaning survives lowering, query families are first-class, the CPU path remains an oracle, and semantics matter more than opportunistic tricks.

5. **The runtime and CLI already solve real workflows.**  
   The repo is not only a library; it has a working `wrela` tool, benchmark protocols, perf comparison flows, preview/frame/presentation commands, and certification/test flows.

That means the missing work is not “find a vision.”

The missing work is to make the repo’s daily working experience match the quality of the underlying architecture.

### Where the repo is currently hostile to fast local shipping

The maintenance pain is visible in concrete repo facts.

1. **Developer throughput is not yet a first-class measured surface.**

The repo has serious runtime/perf instrumentation, but it does not yet have a canonical local developer-loop scorecard for:

- warm check time
- representative incremental rebuild time
- fast verification lane time
- full verification lane time
- perf-smoke lane time
- one-command pre-ship time

That means local slowness is felt, but not budgeted.

2. **The workspace currently has only two crates, which makes blast radius larger than it needs to be.**

The root workspace currently contains:

- `compiler`
- `runtime`

That is simple, but it also means the huge command/tooling surface and the huge compiler surface are still packed into too few build units.

3. **Incremental local builds are explicitly disabled in the dev and test profiles.**

`Cargo.toml` currently contains:

```toml
[profile.dev]
debug = 0
split-debuginfo = "off"
incremental = false

[profile.test]
debug = 0
split-debuginfo = "off"
incremental = false
```

That may have been a useful truth-first setting earlier.
For a solo local workflow, it is now too expensive by default.

4. **There are several severe godfiles.**

The repo currently contains multiple files that are well past “dense but okay” and firmly into “too much ownership in one place” territory, including:

- `compiler/tests/cli.rs` — 13,239 lines
- `compiler/query_exec/cpu.rs` — 10,409 lines
- `compiler/mir/lower.rs` — 9,716 lines
- `compiler/bin/wrela/perf_engine.rs` — 7,852 lines
- `compiler/tests/query_exec.rs` — 7,771 lines
- `compiler/bin/wrela/commands/shared.rs` — 7,273 lines
- `compiler/query_exec/mir_helpers.rs` — 6,915 lines
- `compiler/bin/wrela/commands/test_eval_perf.rs` — 6,517 lines
- `compiler/backend/cranelift.rs` — 6,160 lines
- `compiler/query_exec/mir_scene_semantics.rs` — 6,018 lines
- `compiler/query_exec/wgsl/codegen.rs` — 5,783 lines
- `compiler/presentation_exec/wgsl.rs` — 4,610 lines

These are not isolated outliers.
They are systemic pressure signals.

5. **Several huge areas are flattened with `include!`, which hides module seams and enlarges human blast radius.**

Today the repo uses `include!` to flatten large source partitions in places such as:

- `compiler/hir/typeck.rs`
- `compiler/query_exec/mir.rs`
- `compiler/bin/wrela/command_handlers.rs`

That approach can be expedient, but it pushes the repo toward “one giant file split for convenience” instead of true modules with explicit ownership.

6. **The CLI internals are currently monolithic and stringly typed.**

`compiler/bin/wrela/cli_args.rs` currently parses many commands into one `ParsedArgs` struct with many optional fields that are irrelevant to most commands.
`execute(...)` in `compiler/bin/wrela/commands/shared.rs` then destructures that struct, performs a long sequence of command-specific validity checks, and finally dispatches on a command string.

That is workable, but it is not delightful.
It is hard to change safely and hard for a junior engineer to reason about locally.

7. **The repo has broad test coverage, but not a clear fast-by-default local verification story.**

The top-level `justfile` currently exposes only:

- `test`
- `test-runtime`
- `test-compiler`
- `test-cli`
- `build`
- `build-release`
- `lint`
- `fmt`
- `fmt-check`

And `test` is just:

```just
test:
    cargo test --workspace
```

That is honest, but it does not distinguish:

- fast local smoke
- focused domain verification
- full local closure
- perf smoke
- pre-ship gate

8. **The current command surface is fragmented.**

The repo has at least three active surfaces:

- raw `cargo` commands
- `just` recipes
- the `wrela` CLI, which currently recognizes 25 commands:
  `init`, `update`, `check`, `analyze`, `fix`, `fmt`, `build`, `compile`,
  `query-contracts`, `collision-contracts`, `collision-plan`, `collision-run`,
  `preview`, `frame`, `frame-contracts`, `presentation-plan`,
  `presentation-debug`, `verify-cert`, `run`, `dev`, `test`, `eval`, `perf`,
  `perfcmp`, and `matrix`

Those surfaces are all useful.
They are not yet unified by one obvious mental model.

9. **Readability debt is real in dense areas.**

Many large files have little or no explanatory commentary at all.
Examples include:

- `compiler/bin/wrela/perf_engine.rs`
- `compiler/bin/wrela/commands/shared.rs`
- `compiler/query_exec/mir_helpers.rs`
- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/collision_exec/cpu.rs`
- `compiler/bin/wrela/cli_args.rs`

That does not mean the code is bad.
It means too many critical invariants still live only in structure and author memory.

10. **Generic module names still hide ownership.**

Files such as:

- `compiler/bin/wrela/commands/shared.rs`
- `compiler/query_exec/mir_helpers.rs`

already tell us that some semantic responsibilities are still grouped as “stuff we didn’t name precisely enough yet.”

That is exactly the kind of debt this RFC should remove.

## Goals

1. Make the local developer loop a measured, first-class engineering surface.
2. Make `just` the canonical repo workflow interface.
3. Keep `wrela` as the authored-world/product interface, not the primary repo-orchestration interface.
4. Reduce compile blast radius through better module boundaries and selected crate splits.
5. Stratify verification into fast, focused, full, and perf-smoke lanes without losing semantic coverage.
6. Define bounded contexts, dependency rules, and public seams so names and imports stop drifting.
7. Use a temporary migration glossary only while boundaries and names are being normalized, then shrink or delete it once the code speaks clearly.
8. Break up godfiles into modules a junior engineer or an agent can understand and edit safely.
9. Surface important invariants in types, tests, root module docs, and targeted comments instead of oral tradition.
10. Make the repo legible to both humans and agent contributors from local context.
11. End with one obvious local “ship it” command and a final before/after scorecard.

## Non-Goals

1. This RFC does not replace the semantic or performance roadmaps from RFCs 0004–0008.
2. This RFC does not weaken CPU-oracle behavior or remove correctness-heavy validation lanes.
3. This RFC does not require CI, remote orchestration, or a cloud-first workflow.
4. This RFC does not document every line of code. It documents ownership and invariants where they matter.
5. This RFC does not force crate splitting for ideology. Crate boundaries are a tool, not a religion.
6. This RFC does not justify “cleanup” that cannot point to a measurable improvement in throughput, readability, or hot-path behavior.
7. This RFC does not treat comments as a substitute for naming, types, or good module boundaries.
8. This RFC does not pause feature work forever; it creates a codebase that makes future feature work cheaper.
9. This RFC does not create a permanent heavyweight glossary or a sprawling AI-only companion manual.
10. “DDD” in this RFC does not mean ceremony for its own sake; it means explicit ownership, dependency direction, and public seams.

## Design Rules

1. **The fast local loop is a product surface.**  
   If the developer loop is slow or confusing, that is a real product bug in the repo.

2. **`just` is the canonical repo workflow surface.**  
   `cargo` remains the substrate. `wrela` remains the authored-world tool. The README and primary docs should lead with `just`.

3. **Fast by default, full by availability.**  
   The default local lane should be fast and trustworthy. The full lane must remain available and documented.

4. **The repo must be legible from local context for both humans and agents.**  
   Common workflows, entrypoints, proving lanes, and module ownership should be discoverable without private folklore.

5. **Operational DDD, not ceremony.**  
   This RFC uses bounded contexts to define ownership, dependency direction, and public seams, not to add process theater.

6. **No generic ownership names in touched domains.**  
   New or refactored code must not hide behind names like `shared`, `helpers`, `misc`, or `util` unless the module truly owns only generic infrastructure and that ownership is documented.

7. **Module seams are real architecture.**  
   A file or module must answer “what do I own?” in its first screenful.

8. **Comments explain invariants and why, not mechanics.**  
   If the code is obvious, comment less. If the code is dense, comment the rules and failure modes.

9. **Contexts need allowed and forbidden dependency directions.**  
   A boundary is not real until the repo states what may cross it and what must not.

10. **Boundary crossings should happen through small named contracts.**  
    Public nouns and anti-corruption seams matter more than convenience imports.

11. **Every phase must produce measurable evidence.**  
   Work does not close on vibes. Each phase leaves behind timings, file-size changes, command-surface changes, or perf diffs.

12. **Junior-executable and agent-executable mean explicit file targets and explicit acceptance criteria.**  
   No task should require folklore to begin.

13. **Canonical workflows should be machine-friendly where useful.**  
    Stable lane names, stable scenario ids, and machine-readable reports are part of the repo surface.

14. **No maintenance churn without payoff.**  
   A task should improve at least one of:
   - local build time
   - local test time
   - command discoverability
   - module readability
   - hot-path performance
   - change blast radius

15. **The repo should optimize for long stretches of focused shipping.**  
    The right repo shape is one that helps the author keep momentum instead of bleeding it away.

## Operational DDD For This Repo

This RFC uses “DDD” in an intentionally operational sense.
For Wrela, it means explicit ownership, dependency direction, public seams, and evidence-backed boundaries.
It does **not** mean adding a large layer of ceremony on top of the code.

### 1. Explicit bounded contexts

The candidate contexts for this roadmap are:

- **authoring frontend** — source files, parsing, surface syntax, authored-code diagnostics
- **semantic compilation pipeline** — HIR, MIR, lowering, semantic transforms, compiler contracts
- **query execution** — query contracts, plans, program spine, CPU/WGSL query execution, backend selection
- **presentation execution** — presentation contracts, frame plans, bindings, execution
- **collision execution** — collision contracts, plans, execution
- **runtime and artifact substrate** — runtime ABI, artifact stores, world identity, time semantics, acceleration, `gpu_runtime`
- **tooling and orchestration** — `wrela` CLI, `just`, benchmarks, perf harnesses, repo workflows

If some of these remain siblings under a broader execution umbrella early in the RFC, they should still be documented separately for ownership and dependency purposes.

### 2. Dependency direction rules

The repo should state and enforce rules such as:

- the authoring frontend may depend on shared compiler vocabulary, but not on backend execution details
- the semantic compilation pipeline may lower toward query/presentation/collision contracts, but may not depend on CLI/perf orchestration
- execution contexts may consume semantic or planned artifacts, but may not reach back into parse/front-end concerns
- tooling may orchestrate contexts, but domain logic must not migrate into CLI/perf convenience layers
- `shared` and `helpers` modules must stay tiny and boring; if they accumulate behavior, that is a boundary violation

### 3. Public nouns and anti-corruption seams

Each context should expose a small set of public entrypoints and public nouns.
Everything else stays internal.
Boundary crossings should happen through small named contracts or adapters, not through convenience imports and incidental shared structs.

### 4. Module split vs crate split criteria

A crate split is justified only when it improves the repo in measurable ways:

- compile isolation materially improves a measured local loop
- the context has a stable public API and hidden internals
- dependency direction becomes clearer after the split
- tests and benchmarks can target the split unit more precisely
- the split reduces navigation overhead instead of increasing it

If those conditions are not true yet, prefer modules and document why.

### 5. Temporary glossary, permanent code clarity

The glossary in this RFC is a migration aid, not a forever artifact.
It exists to help normalize overloaded or unstable names while the refactors are in flight.
Once the main renames land and root module docs clearly describe ownership, the glossary should be reduced to public nouns only or removed entirely.

## Project-Level Acceptance Criteria

RFC 0009 is not complete until all of the following are true:

1. **Canonical workflow closure**
   - The primary repo docs lead with `just`, not raw `cargo`.
   - A documented `just ship` command exists.
   - The command boundary between `just`, `cargo`, and `wrela` is explicit and stable.
   - The docs explicitly say that `just` may compose both Rust-native `cargo` verification and authored-world `wrela` verification under one repo workflow.

2. **Developer-loop measurement closure**
   - A baseline developer-loop report exists from Phase 49.
   - A final developer-loop report exists from Phase 56.
   - Both reports use the same machine protocol and the same named scenarios.

3. **Build-speed closure**
   - Warm representative `just check` is at least **2× faster** than the Phase-49 baseline **or** is **<= 10 seconds** on the measured machine.
   - Warm representative `cargo test --workspace --no-run` is at least **30% faster** than the Phase-49 baseline.

4. **Fast-test closure**
   - The default `just test` lane is at least **3× faster** than the Phase-49 full-workspace default lane **or** is **<= 30 seconds** on the measured machine.
   - `just test` still exercises every top-level bounded context at least once.

5. **Full-verification closure**
   - A documented `just test-all` lane exists.
   - A documented `just perf-smoke` lane exists.
   - `just ship` runs a full local pre-ship gate and is at least **30% faster** than the Phase-49 equivalent full local gate **or** is **<= 15 minutes** on the measured machine.

6. **Architecture closure**
   - A checked-in bounded-context map exists.
   - The context map includes allowed and forbidden dependency directions for the primary contexts.
   - A temporary migration glossary exists only while names are being normalized, and is reduced to public nouns only or removed by final closure.
   - At least one crate-split candidate is evaluated with compile/readability evidence, and any executed split is justified by that evidence.

7. **Godfile closure**
   - The following files are no longer single-file god-objects:
     - `compiler/query_exec/cpu.rs`
     - `compiler/mir/lower.rs`
     - `compiler/bin/wrela/perf_engine.rs`
     - `compiler/bin/wrela/commands/shared.rs`
     - `compiler/query_exec/mir_helpers.rs`
     - `compiler/query_exec/wgsl/codegen.rs`
     - `compiler/presentation_exec/wgsl.rs`
     - `compiler/tests/cli.rs`
   - Their replacement module roots are explicit and small.
   - Any unusually large replacement leaf file is explicitly justified instead of silently accepted.

8. **Readability closure**
   - No `include!` remains in the refactored CLI, `hir/typeck`, or `query_exec/mir` module trees.
   - No `shared.rs` / `helpers.rs` buckets remain in touched domains.
   - Every touched dense module root has a module header documenting ownership and invariants.

9. **Tooling closure**
   - The CLI parser and dispatcher are typed by command variant, not driven by a free-form command string plus a giant bag of unrelated option fields.
   - Lane names are aligned across repo docs, `just`, and user-facing `wrela` commands.

10. **Agent-legibility closure**
   - Touched contexts document typical entrypoints and proving lanes that a contributor or agent can use to orient quickly.
   - Common repo workflows have one canonical command and machine-readable output where useful.
   - The future-facing maintenance playbook explains how to carve changes into reviewable slices for human-plus-agent work.

11. **Behavior closure**
    - `cargo test --workspace`
    - `wrela test --lane full`
    - `just test-all`
    - `just ship`
    - representative perf-smoke commands

    all remain green at the end of the project.

If a metric target above is missed, the RFC cannot close silently.
The final scorecard must explain the remaining bottleneck and the next concrete maintenance task.

## Phase Overview

- **Phase 49** — truthful developer-loop baselines and canonical command surface
- **Phase 50** — context map, dependency rules, and agent workflow contracts
- **Phase 51** — test pyramid closure and faster default verification
- **Phase 52** — compile throughput closure and workspace boundary cleanup
- **Phase 53** — explicit ownership, public seams, and godfile breakup
- **Phase 54** — readability, invariants, and typed tooling closure
- **Phase 55** — low-risk performance sweep and hot-path hygiene
- **Phase 56** — closure gates, scorecard, and legacy cleanup

---

# Phase 49: Truthful Developer-Loop Baselines And Canonical Command Surface

## Goal

Make the local loop measurable and make the repo have one obvious workflow surface.

## Why this phase exists

The repo already measures runtime and rendering closure in real detail.
It does not yet measure the development loop with the same honesty.

If the team can answer “why not 120?”, it should also be able to answer:

- why is a warm check slow?
- why is the fast test lane slow?
- what is the one command I should run before I ship?
- which workflow should go through `just`, which through `wrela`, and which through raw `cargo`?

This phase creates that foundation.

### Workstream A: Developer-loop measurement

#### Task 49A1 — Add a developer-loop measurement harness and baseline report

**Description**

Add a small local harness that measures the canonical development lanes and writes a machine-readable report under `.artifacts/devloop/`.

This is not a benchmark suite for engine output.
It is a benchmark suite for developer throughput.

**Files**

- new `docs/dev/devloop_playbook.md`
- new `docs/dev/lanes.md`
- new `scripts/devloop_measure.py` or `scripts/devloop_measure.sh`
- `justfile`
- `README.md`

**Implementation notes**

Measure at least these scenarios:

- `check_warm` — warm `cargo check` / `just check`
- `build_no_run_warm` — warm `cargo test --workspace --no-run`
- `rust_fast_lane` — the default repo fast test lane once it exists
- `rust_full_lane` — full workspace verification
- `perf_smoke` — a minimal representative perf lane
- `ship` — the pre-ship local gate once it exists

The harness should record:

- command text
- wall-clock duration
- success/failure
- machine tag
- git SHA
- whether the lane was cold or warm
- notes about what was intentionally excluded

Do not hide cache state.
The playbook must state exactly how “warm” is defined.

**Code sketch**

```python
{
  "schema_version": 1,
  "machine_tag": "ryan-devbox",
  "git_sha": "abc1234",
  "scenarios": [
    {
      "id": "check_warm",
      "command": "just check",
      "warm": true,
      "elapsed_ms": 8123,
      "success": true
    }
  ]
}
```

**Acceptance criteria**

- Running the measurement command writes a JSON report to `.artifacts/devloop/`.
- The playbook explains cold vs warm protocol.
- The README points to the playbook.
- A Phase-49 baseline report is captured and used by later phases.

#### Task 49A2 — Add representative per-context dev-loop scenarios

**Description**

The measurement harness should not only time one generic “build.”
It should capture representative edit scopes so later refactors can prove blast-radius improvement.

**Files**

- `docs/dev/devloop_playbook.md`
- `scripts/devloop_measure.py` or `scripts/devloop_measure.sh`
- `justfile`

**Implementation notes**

Add named scenarios such as:

- `frontend_edit_check`
- `query_exec_edit_check`
- `cli_edit_check`
- `full_workspace_no_run`
- `fast_verify`
- `full_verify`

The harness does not need to mutate files automatically.
A simple, documented protocol is acceptable if it is repeatable.
For example, the playbook may define a representative “touch this file and rerun” workflow.

**Acceptance criteria**

- The dev-loop report includes named scenarios for at least frontend, query execution, and CLI/tooling edits.
- The playbook explains how to reproduce each scenario.
- Later phases can compare before/after blast radius using the same scenario ids.

### Workstream B: Canonical command surface

#### Task 49B1 — Make `just` the canonical repo workflow surface

**Description**

The repo should expose one obvious entry point for daily work.
That entry point should be `just`.

**Files**

- `justfile`
- `README.md`
- `docs/dev/lanes.md`

**Implementation notes**

Add canonical recipes for at least:

- `check`
- `build`
- `test`
- `test-all`
- `test-cli`
- `test-query`
- `perf-smoke`
- `perf-closure`
- `ship`
- `baseline-devloop`
- `fmt`
- `fmt-check`
- `lint`
- `fix`

`just test` should become the fast default lane.
`just test-all` should remain the full semantic lane.

Those repo lanes are allowed to compose multiple proving surfaces.
They do not need to be Rust-only wrappers if the truthful proof for part of the repo is a native `wrela` command.

The primary docs should stop leading with raw `cargo` examples for common repo workflows.
Raw `cargo` stays valid, but it becomes the substrate, not the front door.

**Code sketch**

```just
check:
    cargo check --workspace

test:
    cargo test -p wrela --test repo_smoke
    cargo run -p wrela --bin wrela -- test --lane=fast language/spec

test-all:
    cargo test --workspace
    cargo run -p wrela --bin wrela -- test --lane=full language/spec

perf-smoke:
    cargo run --bin wrela -- perf benchmarks/micro --profile=smoke --runs=1

ship:
    just fmt-check
    just lint
    just test
    just test-all
    just perf-smoke
```

**Acceptance criteria**

- The `justfile` contains the canonical repo workflows.
- The README leads with `just`, not raw `cargo`.
- A documented `just ship` recipe exists.
- A junior engineer can find the right command without reading source.

#### Task 49B2 — Define the boundary between `just`, `cargo`, and `wrela`

**Description**

The three command surfaces should stop overlapping ambiguously.

**Files**

- `docs/dev/lanes.md`
- `README.md`
- `benchmarks/README.md`

**Implementation notes**

The boundary should be:

- **`just`** — repo workflows
- **`cargo`** — low-level escape hatch / substrate
- **`wrela`** — authored-world and product-facing workflows (`preview`, `frame`, `perf`, `test`, etc.)

Document this explicitly.
In particular:

- `cargo test` proves Rust units, Rust integration crates, and implementation-internal harnesses.
- `wrela test` proves authored `.wr` projects and the native Wrela test-runner semantics.
- `just` is the repo front door that composes the right `cargo` and `wrela` commands for a named workflow such as `test`, `test-all`, or `ship`.

Also standardize the lane vocabulary:

- `fast`
- `full`
- `perf-smoke`
- `perf-closure`
- `ship`

**Acceptance criteria**

- The repo docs define the boundary explicitly.
- The same lane names are used consistently in docs and commands.
- No primary doc gives contradictory advice about which surface to use.
- A junior engineer can tell when a lane is satisfied by `cargo test`, when it requires `wrela test`, and when `just` runs both.

## Phase 49 exit criteria

- A Phase-49 developer-loop baseline report exists.
- The repo has a canonical `just` surface.
- The workflow boundary between `just`, `cargo`, and `wrela` is documented.
- A documented `just ship` placeholder exists, even if later phases still improve its implementation.

---

# Phase 50: Context Map, Dependency Rules, And Agent Workflow Contracts

## Goal

Make the repo’s ownership boundaries explicit before structural surgery begins, and make the workflow legible to both human and agent contributors.

## Why this phase exists

Without concrete context ownership and dependency rules, later refactors risk turning into naming churn.
That is especially dangerous in this repo because the codebase is largely written by agentic AI.

Humans can sometimes survive fuzzy boundaries with tacit knowledge.
Agents are much worse at that.
If the repo expects human-plus-agent collaboration, it must let contributors orient from local context instead of author memory.

This phase is intentionally light.
It is not a giant doc-writing phase.
It produces one small context map, explicit dependency arrows, temporary migration scaffolding, and clear rules for how later phases should carve work.

### Workstream A: Operational context map

#### Task 50A1 — Publish a one-page context map with ownership, entrypoints, and proving surfaces

**Description**

Check in a concise context map that says what the primary contexts own and how contributors should orient inside them.

**Files**

- new `docs/architecture/contexts.md`
- `README.md`

**Implementation notes**

Start from the candidate contexts in the operational DDD section of this RFC.
For each context, define:

- what it owns
- its typical entrypoints
- its primary public nouns
- the tests, benchmarks, or lanes that prove it still works

Keep this document short and operational.
It should be a map, not an essay.

**Acceptance criteria**

- The context map is checked in.
- Every context lists ownership, entrypoints, and proving surfaces.
- Later phases map touched modules to one or more named contexts instead of inventing boundaries on the fly.

#### Task 50A2 — Define allowed and forbidden dependency directions and anti-corruption seams

**Description**

Make the context map enforceable by stating which dependencies are allowed, which are forbidden, and which seams are the approved crossing points.

**Files**

- `docs/architecture/contexts.md`
- `README.md`

**Implementation notes**

Document rules such as:

- front-end concerns may not depend on execution backend details
- execution contexts may consume semantic/planned artifacts, but may not reach back into parse/front-end concerns
- tooling may orchestrate contexts, but must not become the place where domain logic lives
- boundary crossings should happen through small named contracts or adapters, not convenience imports

Include at least a few concrete boundary-violation examples so the rules are not purely abstract.

**Acceptance criteria**

- The checked-in context map includes allowed and forbidden dependency directions.
- The primary public seams for touched contexts are named.
- The doc includes concrete examples of what counts as a boundary violation.

### Workstream B: Temporary glossary and human/agent workflow contracts

#### Task 50B1 — Use a temporary migration glossary with explicit shrink/delete criteria

**Description**

Use a glossary only as a migration aid while names are being normalized.
It should not survive as a large permanent artifact if the code becomes clearer.

**Files**

- new `docs/architecture/glossary.md`
- `docs/architecture/contexts.md`

**Implementation notes**

The glossary should be intentionally small.
It should cover only overloaded, unstable, or newly normalized terms that contributors are likely to confuse during the refactor window.

The document must also state its own exit criteria:

- once the main renames land
- once root module docs explain ownership clearly
- once the surviving public nouns are stable

At that point the glossary should be reduced to public nouns only or removed.

**Acceptance criteria**

- The glossary is explicitly documented as temporary migration scaffolding.
- The glossary has shrink/delete criteria.
- The final phase is required to shrink it to public nouns only or remove it entirely.

#### Task 50B2 — Make workflow expectations explicit for human-plus-agent contributors

**Description**

Document the small set of repo rules that make the codebase easy to navigate and change for both humans and agents.

**Files**

- `docs/dev/lanes.md`
- `README.md`
- `docs/architecture/contexts.md`

**Implementation notes**

Document expectations such as:

- common repo workflows have one canonical command
- machine-readable output is used where it helps automation and verification
- touched module roots should state owns / does not own / primary entrypoints / invariants
- tasks should be sliceable into explicit file ownership and proving lanes
- independent final review remains a closure requirement

Keep this lightweight.
The goal is not an AI handbook.
The goal is to make the repo itself crisp.

**Acceptance criteria**

- The docs explain how a contributor or agent can orient quickly inside a context.
- Canonical commands and proving lanes are stated for common repo workflows.
- The later maintenance playbook can build on these rules instead of inventing them from scratch.

## Phase 50 exit criteria

- A checked-in context map exists with ownership, entrypoints, and proving surfaces.
- Allowed and forbidden dependency directions are explicit.
- The temporary glossary exists with explicit shrink/delete criteria.
- Human-plus-agent workflow expectations are documented well enough to guide later structural refactors.

---

# Phase 51: Test Pyramid Closure And Faster Default Verification

## Goal

Make the default local verification path fast without deleting semantic confidence.

## Why this phase exists

The repo already has a lot of tests, which is good.
The problem is that broad coverage and fast daily iteration are not yet shaped into an intentional pyramid.

The local question should not be “do I run everything or skip testing?”
It should be “which named lane is right for this edit?”

### Workstream A: Rust repo test lanes

#### Task 51A1 — Add explicit `fast` and `full` repo test lanes

**Description**

Create an explicit fast lane for daily work and keep a documented full lane for full local verification.

**Files**

- `justfile`
- new `compiler/tests/repo_smoke.rs` (or the equivalent crate-local smoke harness after any accepted CLI split)
- `README.md`
- `docs/dev/lanes.md`

**Implementation notes**

The fast lane must exercise every top-level bounded context at least once, even if only via smoke coverage.
At minimum it should touch:

- parsing / frontend
- type checking / lowering
- query execution
- presentation planning or execution
- collision planning or execution
- CLI smoke
- benchmark/perf manifest loading

Keep the smoke tests small, deterministic, and cheap.
The fast repo lane may combine a cheap Rust smoke harness with a cheap native `wrela test --lane fast` invocation if that is the truthful way to exercise authored-world semantics.
The full repo lane should likewise compose the complete Rust lane and the required native `wrela test --lane full` lane rather than pretending one replaces the other.

**Code sketch**

```rust
#[test]
fn smoke_query_exec_cpu_roundtrip() {
    let ctx = minimal_query_exec_context();
    let result = execute_minimal_cpu_query(&ctx).expect("query executes");
    assert_eq!(result.items.len(), 1);
}
```

**Acceptance criteria**

- `just test` runs a documented fast lane.
- `just test-all` runs the full local lane.
- The fast lane exercises every top-level bounded context at least once.
- The fast lane is measured by the developer-loop harness.
- The fast lane has an explicit time budget and the scorecard calls out misses instead of normalizing them.
- The documented repo lanes make clear which proof comes from Rust integration tests and which proof comes from the native `wrela` test runner.

#### Task 51A2 — Refactor giant integration harness roots into modular test crates without increasing crate count

**Description**

Several large integration-test roots are too big to navigate sanely.
They should be split into submodules while preserving crate count so compile cost does not get worse accidentally.

**Files**

- `compiler/tests/cli.rs`
- `compiler/tests/query_exec.rs`
- `compiler/tests/presentation_exec.rs`
- `compiler/tests/collision_exec.rs`
- new `compiler/tests/cli/*.rs`
- new `compiler/tests/query_exec/*.rs`
- new `compiler/tests/presentation_exec/*.rs`
- new `compiler/tests/collision_exec/*.rs`

**Implementation notes**

Keep one integration-test crate per domain harness, but make the root file small:

```rust
// compiler/tests/cli.rs
mod support;
mod help;
mod build;
mod preview;
mod perf;
mod slow_cert;
```

Move common helpers into `support.rs`.
Move fixtures into narrow, domain-specific helpers instead of copy-pasting setup code.

Do not split one giant integration file into many new integration crates unless a measurement shows that is better.

**Acceptance criteria**

- The named integration roots above are reduced to thin module roots.
- Shared helpers are factored out of the root files.
- The integration crate count does not increase by accident.
- A junior engineer can navigate the CLI or query integration tests by topic.

### Workstream B: User-facing lane alignment

#### Task 51B1 — Add `fast` and `full` aliases to `wrela test`

**Description**

The repo-level vocabulary and the product/tool vocabulary should use the same lane names.

**Files**

- `compiler/bin/wrela/cli_args.rs` or the equivalent extracted CLI crate files
- `compiler/bin/wrela/commands/test_eval_perf.rs`
- `README.md`
- `docs/dev/lanes.md`

**Implementation notes**

Today `wrela test --lane` uses lanes such as:

- `spec`
- `integration`
- `sim`
- `model`
- `default`

That is useful, but it should also accept higher-level aliases:

- `fast`
- `full`

Recommended mapping:

- `fast` => `spec` + `default`
- `full` => all lanes

This preserves existing semantics while making the CLI easier to learn.
It also makes it possible for `just test` and `just test-all` to call native Wrela test lanes without inventing a second vocabulary.

**Code sketch**

```rust
pub enum TestLaneSelection {
    Single(TestLane),
    Preset(TestLanePreset),
}

pub enum TestLanePreset {
    Fast,
    Full,
}
```

**Acceptance criteria**

- `wrela test --lane fast` works.
- `wrela test --lane full` works.
- Existing lane names still work unless deliberately deprecated and documented.
- Docs use the same lane vocabulary across `just` and `wrela`.
- The docs explicitly say that `wrela test` is the native authored-world test runner, not a synonym for `cargo test`.

#### Task 51B2 — Align benchmark docs and perf-smoke naming with the repo lanes

**Description**

The benchmark docs already have `smoke`, `standard`, `deep`, and `1080p120` profiles.
The repo docs should connect those to the new workflow vocabulary instead of making users translate mentally.

**Files**

- `benchmarks/README.md`
- `docs/dev/lanes.md`
- `README.md`

**Implementation notes**

Document that:

- `perf-smoke` => benchmark `--profile=smoke`
- `perf-closure` => benchmark `--profile=1080p120`
- `just perf-smoke` and `just perf-closure` are the canonical repo wrappers

**Acceptance criteria**

- The benchmark docs and repo docs use a consistent vocabulary.
- A junior engineer can tell which perf lane is cheap and which one is closure-grade.
- `just perf-smoke` and `just perf-closure` are documented in one place.

## Phase 51 exit criteria

- The repo has explicit `fast` and `full` verification lanes.
- Giant integration harnesses are modularized.
- `wrela test` accepts user-friendly lane aliases.
- The repo and benchmark docs use aligned lane vocabulary.

---

# Phase 52: Compile Throughput Closure And Workspace Boundary Cleanup

## Goal

Reduce compile blast radius and make the workspace structure serve local iteration.

## Why this phase exists

The codebase is now large enough that compile shape matters as much as runtime shape.
Today the workspace is too coarse and the local profiles are too pessimistic for daily solo work.

This phase takes the obvious wins first:

- re-enable incremental local builds
- evaluate whether the CLI deserves its own crate yet
- replace `include!` flattening with real submodules
- start measuring compile cost by edit scope

### Workstream A: Cargo profile and crate boundaries

#### Task 52A1 — Re-enable incremental dev/test builds and add cleanroom escape hatches

**Description**

Turn incremental local builds back on for the default developer profiles.

**Files**

- `Cargo.toml`
- `justfile`
- `docs/dev/devloop_playbook.md`

**Implementation notes**

Update the workspace profiles to:

```toml
[profile.dev]
debug = 0
split-debuginfo = "off"
incremental = true

[profile.test]
debug = 0
split-debuginfo = "off"
incremental = true
```

Then add explicit cleanroom recipes for the rare cases where a full non-incremental pass is desired:

- `just check-clean`
- `just test-clean`

Do not make the truth-first cleanroom path the everyday default.

**Acceptance criteria**

- `profile.dev` and `profile.test` use incremental compilation by default.
- Cleanroom recipes exist and are documented.
- The Phase-52 dev-loop report compares warm vs cleanroom behavior.

#### Task 52A2 — Evaluate the CLI as the first workspace crate split and execute it only if the evidence supports it

**Description**

The CLI is an obvious crate-split candidate, but the split should be evidence-led rather than assumed.
If compile-burst and navigation evidence show that isolating the CLI materially improves the local loop, execute the split.
If not, keep the CLI in place for now and record why module boundaries are the better move.

**Files**

- root `Cargo.toml`
- new `wrela_cli/Cargo.toml` if the split is approved
- move `compiler/bin/wrela.rs` to `wrela_cli/src/main.rs` if the split is approved
- move `compiler/bin/wrela/*` to `wrela_cli/src/*` if the split is approved
- move CLI integration tests from `compiler/tests/cli.rs` to `wrela_cli/tests/` if the split is approved
- `README.md`
- `justfile`
- new `docs/architecture/crate_split_decision.md`

**Implementation notes**

If the split is approved, the recommended structure is:

```toml
[workspace]
members = [
    "compiler",
    "runtime",
    "wrela_cli",
]
```

The extracted `wrela_cli` crate should depend on the library crate and own:

- CLI argument parsing
- command dispatch
- perf engine
- matrix/perfcmp flows
- CLI integration tests

Keep the binary name `wrela` so the user-facing tool stays stable.

Use `just` recipes so users do not have to care about the package boundary.

**Acceptance criteria**

- A checked-in decision record explains whether the CLI split is justified now.
- If the split is approved, the library crate can be checked/tested without always building the CLI.
- If the split is approved, the `wrela` binary still works as before from the user’s perspective and CLI tests move with the CLI crate.
- If the split is deferred, the blockers and next required dependency inversions are explicit.

### Workstream B: Real modules instead of flattened pseudo-modules

#### Task 52B1 — Replace `include!` trees with real Rust modules

**Description**

Convert flattened `include!` partitions into explicit modules with narrow ownership.

**Files**

- `compiler/hir/typeck.rs` -> `compiler/hir/typeck/mod.rs`
- `compiler/query_exec/mir.rs` -> `compiler/query_exec/mir/mod.rs`
- CLI command handler module tree in the extracted `wrela_cli` crate if the split lands, or the existing CLI tree otherwise

**Implementation notes**

Before:

```rust
include!("typeck/types.rs");
include!("typeck/context.rs");
include!("typeck/stmt.rs");
include!("typeck/expr.rs");
```

After:

```rust
mod types;
mod context;
mod stmt;
mod expr;
mod calls;
mod conformance;
mod async_effects;

#[cfg(test)]
mod tests;
```

Use explicit `pub(crate)` re-exports only where necessary.
Do not recreate one giant pseudo-file with a thin wrapper.

**Acceptance criteria**

- The touched module trees above no longer use `include!`.
- Their module roots are small and explicit.
- Internal visibility is narrower and easier to reason about.
- A junior engineer can tell which submodule owns which responsibility.

#### Task 52B2 — Add compile-burst measurements by edit scope

**Description**

Once the crate and module boundaries move, the developer-loop harness should measure the result in a way that reflects real editing patterns.

**Files**

- `scripts/devloop_measure.py` or `scripts/devloop_measure.sh`
- `docs/dev/devloop_playbook.md`
- `docs/dev/compile_budgets.md`

**Implementation notes**

Measure at least:

- frontend-only edit
- query-exec-only edit
- CLI-only edit

The report should say how long the representative warm check took after each edit scope.

**Acceptance criteria**

- The developer-loop report includes per-context compile timings.
- The report shows whether the chosen CLI boundary reduced unrelated rebuilds or why it did not yet.
- The compile-burst measurements are reproducible from the playbook.

## Phase 52 exit criteria

- Incremental dev/test builds are on by default.
- The first crate-split decision is backed by compile/readability evidence.
- The major `include!` trees are replaced with real modules.
- Compile-burst measurements exist for representative edit scopes.

---

# Phase 53: Explicit Ownership, Public Seams, And Godfile Breakup

## Goal

Make the codebase follow the checked-in context map instead of merely gesturing at it.

## Why this phase exists

The earlier context-map phase makes the boundaries explicit on paper.
This phase makes those boundaries visible in code structure.

This is where operational DDD stops being naming guidance and starts constraining implementation:

- touched contexts expose small public seams
- boundary crossings happen through named contracts or adapters
- giant files are broken up by responsibility
- generic buckets disappear
- public nouns stop leaking across incidental helper imports

### Workstream A: Ownership and seam closure

#### Task 53A1 — Apply the checked-in context map to touched domains and define their public seams

**Description**

The context map from Phase 50 should start constraining real code.
Each touched context must publish a small set of public nouns and entrypoints, and boundary crossings should happen through named seams instead of convenience imports.

**Files**

- `docs/architecture/contexts.md`
- touched `mod.rs` roots across the refactored contexts
- `README.md`

**Implementation notes**

For each touched context:

- define the small set of public entrypoints and public nouns
- mark incidental bridge structs and helper types as internal
- replace convenience imports across contexts with named contracts or adapters where needed
- update the context map if ownership changes during the refactor

If query execution, presentation execution, and collision execution still share infrastructure inside one crate, keep their seams explicit anyway.
The goal is not a crate count.
The goal is that a file path and module root tell a contributor what belongs there and what does not.

**Acceptance criteria**

- Touched contexts expose a small, named public surface.
- Boundary crossings in touched areas happen through explicit seams instead of incidental convenience imports.
- The context map stays aligned with the refactor instead of drifting behind it.

#### Task 53A2 — Eliminate generic ownership buckets in touched domains

**Description**

Refactor files like `shared.rs` and `mir_helpers.rs` into role-based module trees.

**Files**

- `compiler/bin/wrela/commands/shared.rs` or its extracted CLI-crate equivalent
- `compiler/query_exec/mir_helpers.rs`
- any newly created replacement modules

**Implementation notes**

Examples of acceptable replacements:

Instead of:

- `shared.rs`

use names such as:

- `contracts_command.rs`
- `collision_command.rs`
- `presentation_command.rs`
- `preview_eval.rs`
- `frame_export.rs`
- `command_parsing.rs`

Instead of:

- `mir_helpers.rs`

use names such as:

- `support_summary_lowering.rs`
- `world_domain_lowering.rs`
- `wgsl_bridge_lowering.rs`
- `backend_guard_lowering.rs`
- `capture_index_lowering.rs`

The exact names may change once the split is underway, but they must describe owned behavior.

**Acceptance criteria**

- `shared.rs` and `mir_helpers.rs` are gone from the touched domains.
- Replacement modules are named by responsibility.
- A file’s path tells a junior engineer or agent what it owns.

### Workstream B: Break up the major godfiles

#### Task 53B1 — Break up the named godfiles into role-based submodules

**Description**

Break up the largest files into smaller modules that each own one coherent slice of behavior.

**Files**

- `compiler/mir/lower.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/presentation_exec/wgsl.rs`
- `wrela_cli/src/perf_engine.rs` if the CLI split lands, or the equivalent existing CLI tree otherwise
- `compiler/tests/cli.rs` (already modularized in Phase 51 but included here for final structure)

**Implementation notes**

Recommended target layouts:

**MIR lowering**

```text
compiler/mir/lower/
  mod.rs
  function.rs
  kernel_lower.rs
  semantic_field.rs
  region_lower.rs
  domain_lower.rs
  render_helpers.rs
  abi.rs
  interface_dispatch.rs
```

**CPU query execution**

```text
compiler/query_exec/cpu/
  mod.rs
  context.rs
  world_trace.rs
  solver.rs
  artifacts.rs
  support_bounds.rs
  portable_eval.rs
  value_eval.rs
```

**WGSL codegen**

```text
compiler/query_exec/wgsl/
  mod.rs
  codegen/
    mod.rs
    world_distance.rs
    world_trace.rs
    radiance.rs
    media.rs
    normals.rs
    prelude.rs
```

**CLI perf engine or equivalent existing CLI tree**

```text
wrela_cli/src/perf_engine/
  mod.rs
  collection.rs
  presentation.rs
  collision.rs
  whole_frame.rs
  closure.rs
  perfcmp.rs
  matrix.rs
  render.rs
```

Do not stop at “move chunks around.”
Each module should have a narrow API and documented ownership.

**Acceptance criteria**

- The named godfiles above are replaced by role-based module trees.
- Replacement leaf files should target less than 2,500 lines, and any larger exception is justified explicitly.
- Module roots are small and readable.
- Behavior stays green under the relevant test lanes.

#### Task 53B2 — Produce the next crate-split decision record after the first evidence-led split decision

**Description**

After the first formal crate-split decision in Phase 52, decide which bounded context should be the next extraction candidate and either execute it or record why it is blocked.

**Files**

- `docs/architecture/crate_split_decision.md`
- touched crate files if an extraction is approved

**Implementation notes**

Evaluate at least these candidates:

- presentation
- collision
- artifact/runtime substrate

Score each candidate on:

- compile isolation gain
- API stability
- dependency inversion cost
- test migration complexity
- readability gain

If one candidate is clearly ready, extract it.
If none is ready, write down the blockers and the required dependency inversions.

This task is not allowed to end with “we should think about it later.”
It must leave behind either an extraction or a real decision document.

**Acceptance criteria**

- A checked-in decision record exists.
- It uses real compile/readability evidence.
- Either one context is extracted, or the blockers are explicit and actionable.

## Phase 53 exit criteria

- Touched contexts expose small named public seams.
- Generic ownership buckets are removed from the touched domains.
- The named godfiles are broken into real modules.
- The repo has a concrete decision record for the next stable crate split.

---

# Phase 54: Readability, Invariants, And Typed Tooling Closure

## Goal

Make dense code understandable without private folklore.

## Why this phase exists

Good naming and file structure do most of the readability work.
But some code will remain inherently dense:

- query execution
- lowering
- GPU/runtime coordination
- CLI orchestration and perf reporting

In those places, the codebase needs explicit invariant surfacing and more typed internal models.

### Workstream A: Invariant surfacing

#### Task 54A1 — Add module header docs to every dense touched module root

**Description**

Every dense module root created or refactored in earlier phases should explain itself in the first screenful.

**Files**

- all new `mod.rs` roots introduced in Phases 52–53
- any dense touched module root over 300 lines

**Implementation notes**

Use a standard header shape:

- owns
- does not own
- key invariants
- primary entrypoints
- failure modes / common pitfalls

**Code sketch**

```rust
//! Owns CPU-side query execution for world, shape, and batch queries.
//! Does not own query planning or public contract selection.
//!
//! Key invariants:
//! - CPU execution remains the semantic oracle.
//! - backend guards are evaluated before bridge calls.
//! - observability counters must reflect the executed fallback path.
```

**Acceptance criteria**

- Every touched dense module root over 300 lines has a header doc.
- A new contributor can identify ownership and invariants from the first 15 lines.
- The header docs are kept close to the code, not only in external docs.

#### Task 54A2 — Add targeted invariant and “why” comments to ambiguous algorithms

**Description**

For dense algorithms, add comments where the logic is not self-evident from names and types alone.

**Files**

- `compiler/query_exec/cpu/*`
- `compiler/mir/lower/*`
- `compiler/presentation_exec/*`
- `compiler/collision_exec/*`
- `wrela_cli/src/perf_engine/*` if the CLI split lands, or the equivalent existing CLI tree otherwise

**Implementation notes**

Comment:

- ordering assumptions
- fallback rules
- cache validity assumptions
- witness reuse rules
- performance-sensitive invariants
- semantic guardrails

Do **not** add narration like “increment the loop counter.”

**Code sketch**

```rust
// Invariant: this fallback counter must reflect the method that actually ran,
// not the method that was originally proposed, otherwise closure reports lie.
```

**Acceptance criteria**

- Touched ambiguous algorithms have targeted invariant/why comments where needed.
- No line-by-line narration is introduced.
- Comments are reviewed for staleness risk before phase close.

### Workstream B: Typed tooling models

#### Task 54B1 — Replace the monolithic CLI command bag with typed command variants

**Description**

Refactor the CLI parser so command structure is encoded in types instead of “a command string plus many unrelated `Option<T>` fields.”

**Files**

- `wrela_cli/src/cli_args.rs` if the CLI split lands, or the equivalent existing CLI tree otherwise
- `wrela_cli/src/command_handlers.rs` if the CLI split lands, or the equivalent existing CLI tree otherwise
- `wrela_cli/src/commands/*` if the CLI split lands, or the equivalent existing CLI tree otherwise

**Implementation notes**

Today the CLI parse result looks roughly like:

```rust
pub struct ParsedArgs {
    pub command: String,
    pub path_arg: Option<String>,
    pub test_jobs: Option<usize>,
    pub perf_profile_name: Option<String>,
    // many more fields...
}
```

Refactor it toward:

```rust
pub enum ParsedCommand {
    Check(CheckArgs),
    Build(BuildArgs),
    Preview(PreviewArgs),
    Frame(FrameArgs),
    Test(TestArgs),
    Perf(PerfArgs),
    PerfCmp(PerfCmpArgs),
    Matrix(MatrixArgs),
}
```

Each command struct should own only the fields it can legally use.

Validation should happen at parse time.
The dispatcher should match on typed variants, not strings.

**Acceptance criteria**

- The CLI no longer stores the command kind as a free-form string.
- The dispatcher does not validate command legality via string comparisons.
- Each command handler receives a typed argument struct.

#### Task 54B2 — Replace stringly internal protocols in touched tooling paths with enums/newtypes

**Description**

Where the repo currently passes lane names, scenario ids, or loosely structured values as free-form strings in touched tooling paths, convert them to typed models.

**Files**

- `wrela_cli/src/cli_args.rs` if the CLI split lands, or the equivalent existing CLI tree otherwise
- `wrela_cli/src/perf_engine/*` if the CLI split lands, or the equivalent existing CLI tree otherwise
- dev-loop reporting code
- any touched tooling structs that still use free-form lane or status strings internally

**Implementation notes**

Examples:

- `TestLaneSelection`
- `PerfLaneKind`
- `DevLoopScenarioId`
- `ShipGateStatus`

Keep string parsing only at the CLI boundary.
After parsing, use types.

**Acceptance criteria**

- Internal touched tooling paths use enums/newtypes for lane and scenario identity.
- Parsing and rendering are separated cleanly.
- The typed model reduces hand-written string comparisons.

## Phase 54 exit criteria

- Dense module roots have header docs.
- Ambiguous algorithms have targeted invariant comments.
- The CLI parser/dispatcher model is typed by command.
- Touched tooling paths avoid unnecessary stringly-typed protocols.

---

# Phase 55: Low-Risk Performance Sweep And Hot-Path Hygiene

## Goal

Take the obvious wins that improve runtime and tooling throughput without changing semantics.

## Why this phase exists

Not every performance problem requires a new architecture.
By this point in the roadmap, the repo will have better boundaries and better measurements.
That makes it the right time to take the low-hanging fruit systematically.

The rule for this phase is simple:

**no speculative micro-optimization; every change must point to a measured hotspot or repeated allocation/lookup pattern.**

### Workstream A: Allocation hygiene

#### Task 55A1 — Add reusable scratch buffers for hot query, presentation, and collision paths

**Description**

Replace repeated small-to-medium hot-path allocations with reusable scratch storage where profiling shows churn.

**Files**

- `compiler/query_exec/cpu/*`
- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/collision_exec/cpu.rs`
- any touched metrics/reporting files that expose the improvement

**Implementation notes**

Look for repeated hot-path patterns such as:

- `Vec::new()` inside tight loops
- repeated `to_vec()`
- repeated `collect::<Vec<_>>()`
- repeatedly rebuilt pending/candidate/bounds buffers

Introduce context-owned or function-owned scratch structures where appropriate:

```rust
#[derive(Default)]
struct QueryScratch {
    pending: Vec<u32>,
    candidate_ids: Vec<u32>,
    bounds: Vec<SupportBound>,
}
```

Then reuse with `.clear()` and `with_capacity(...)`.

**Acceptance criteria**

- The top measured allocation hotspots are reduced or eliminated.
- Before/after evidence is recorded in the maintenance scorecard.
- No semantic behavior changes.

#### Task 55A2 — Precompute indexed lookups for stable or semi-stable tables

**Description**

Replace repeated linear scans over static or semi-static collections with maps or dense index tables where measurements justify it.

**Files**

- `compiler/query_exec/cpu/*`
- `compiler/query_exec/wgsl.rs`
- `wrela_cli/src/perf_engine/*` if the CLI split lands, or the equivalent existing CLI tree otherwise
- any touched scenario/test lookup helpers

**Implementation notes**

Examples of appropriate candidates:

- stable capture lookup tables
- stable shape lookup tables
- benchmark scenario lookup by name/id
- command lookup tables where appropriate

Do not blindly replace every `Vec` with a `HashMap`.
Only precompute where the lookup is repeated enough to matter.

**Code sketch**

```rust
pub struct ScenarioIndex {
    by_id: HashMap<String, usize>,
    scenarios: Vec<Scenario>,
}
```

**Acceptance criteria**

- Named repeated O(n) lookup paths are converted where justified by measurement.
- The change is documented in the scorecard.
- The new structure is owned at a clear boundary instead of leaking everywhere.

### Workstream B: Tooling-side throughput hygiene

#### Task 55B1 — Reduce temp-file, process, and manifest churn in CLI and perf tooling

**Description**

Some repo-side slowness comes from repetitive tooling work rather than runtime execution.
Tighten the obvious tooling paths.

**Files**

- `wrela_cli/src/perf_engine/*` if the CLI split lands, or the equivalent existing CLI tree otherwise
- `wrela_cli/tests/*` if the CLI split lands, or the equivalent existing CLI tests otherwise
- any shared subprocess/fixture helpers

**Implementation notes**

Examples of acceptable work:

- reuse parsed benchmark manifests when running multiple scenarios
- centralize subprocess helpers instead of ad hoc copies
- reduce redundant temp-dir creation in repeated helper paths
- avoid re-reading the same fixture data in tight local loops

This is still measurement-led.
Do not guess.

**Acceptance criteria**

- At least one tooling-side throughput hotspot is improved with evidence.
- CLI/perf smoke timing improves or the hotspot counter clearly drops.
- The new helpers are better named and easier to reuse.

## Phase 55 exit criteria

- Hot-path allocation churn has been reduced in measured places.
- Stable repeated lookups have been indexed where justified.
- At least one tooling-side throughput hotspot is improved with evidence.
- All changes are documented in the maintenance scorecard.

---

# Phase 56: Closure Gates, Scorecard, And Legacy Cleanup

## Goal

Make the maintenance work durable and leave the repo with one obvious way to ship.

## Why this phase exists

Maintenance work tends to evaporate if it ends only as a pile of refactors.
This phase turns the earlier work into durable repo behavior:

- one obvious ship command
- one scorecard
- one playbook
- removal of stale, duplicate, or misleading docs and helpers

### Workstream A: Hard local gates

#### Task 56A1 — Finalize `just ship` as the one-command local pre-ship gate

**Description**

Make `just ship` the canonical local closure gate.

**Files**

- `justfile`
- `docs/dev/lanes.md`
- `README.md`

**Implementation notes**

`just ship` should include the full set of gates chosen by the repo, for example:

- formatting check
- lint
- fast lane
- full lane
- perf smoke

The exact contents can change, but the key rule is:
**one documented command, no guesswork.**
If the truthful closure gate requires both Rust integration coverage and native authored-world coverage, `just ship` should run both instead of hiding one of them behind vague wording.

**Code sketch**

```just
ship:
    just fmt-check
    just lint
    just test
    just test-all
    just perf-smoke
```

**Acceptance criteria**

- `just ship` exists and is documented as the canonical pre-ship command.
- The command is measured by the dev-loop harness.
- The final scorecard includes its timing before and after the RFC work.
- The docs state whether `just ship` shells out to `cargo`, `wrela`, or both for each verification obligation.

#### Task 56A2 — Remove stale workflow aliases, docs, and legacy guidance

**Description**

Delete the repo guidance that no longer reflects the canonical workflow.

**Files**

- `README.md`
- `benchmarks/README.md`
- `docs/dev/*`
- `justfile`
- any stale comments or legacy docs in touched areas

**Implementation notes**

Examples:

- raw `cargo` examples that are no longer the recommended default
- obsolete notes about old command expectations
- stale references to removed or renamed module buckets
- outdated lane names

Do not leave both the old and new systems in primary docs.
That creates a bilingual repo and loses the benefit of the cleanup.

**Acceptance criteria**

- Primary docs point only to the canonical workflows.
- Stale lane names and stale command guidance are removed.
- The README and dev docs agree.

### Workstream B: Final evidence and future-proofing

#### Task 56B1 — Publish the final maintenance scorecard

**Description**

Write the before/after scorecard that proves what the RFC achieved.

**Files**

- new `.artifacts/devloop/final-scorecard.json`
- new `docs/dev/maintenance_scorecard.md`

**Implementation notes**

The scorecard should report at least:

- Phase-49 baseline timings
- final timings
- percentage improvement
- remaining bottlenecks
- file-count / file-size improvements for named godfiles
- command-surface simplification wins
- any missed closure target and the next concrete task

**Acceptance criteria**

- The scorecard exists in both machine-readable and human-readable form.
- It compares against the Phase-49 baseline.
- Any missed target is called out honestly with a next step.

#### Task 56B2 — Add a maintenance playbook and module template for future work

**Description**

Document how future code should be added so the repo does not drift back into the same maintenance debt.

**Files**

- new `docs/dev/maintenance_playbook.md`
- new `docs/dev/module_template.md`
- new `docs/architecture/change_checklist.md`

**Implementation notes**

The playbook should cover:

- how to add a new module
- how to add a new CLI command
- how to add a new test lane or smoke test
- how to decide whether a comment belongs in code
- when a crate split is justified
- how to record developer-loop measurements after large maintenance changes
- how to carve a task into a reviewable slice with explicit file ownership
- how to name entrypoints and proving lanes so a contributor or agent can orient quickly
- how to hand work off for an independent read-only review before closure

**Acceptance criteria**

- The playbook exists.
- The module template exists.
- The change checklist exists.
- A junior engineer or agent can extend the repo without reintroducing the same maintenance debt immediately.

## Phase 56 exit criteria

- `just ship` is real and measured.
- Stale workflow guidance is removed.
- The temporary glossary has been reduced to public nouns only or removed entirely.
- The repo has a final before/after maintenance scorecard.
- A future-facing maintenance playbook exists.

---

## Final Note

The spirit of this RFC is intentionally simple:

**make the codebase feel as disciplined as the architecture already is.**

Wrela already thinks seriously about semantics, contracts, closure, and runtime truth.
The repo now needs to think with the same seriousness about human-plus-agent shipping:

- time-to-feedback
- file ownership
- command discoverability
- invariants
- legibility from local context
- reviewable slices and trustworthy proof paths
- and the basic joy of working in the code every day

That is not secondary work.

For a solo shipper, it is the work that compounds everything else.
