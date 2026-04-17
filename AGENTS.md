## Vision

This repository is growing into a field-native game engine.

The long-term goal is not just to render beautiful images, but to define an entire world as a semantic, queryable substrate built from fields. Geometry, materials, media, lighting, interaction, traversal, perception, and eventually gameplay itself should emerge from one coherent world model instead of being split across disconnected systems.

We are building toward an engine where the world is authored once and then interrogated many ways. Rendering is one question. Collision is another. Visibility, audibility, navigation, affordance, combat, AI perception, cinematic composition, and tooling are others. The engine should answer these questions through disciplined query families and explicit contracts, with clear guarantees about truth, approximation, conservatism, and cost.

The compiler is central to this vision. It should understand the meaning of the world well enough to optimize it across CPU and GPU, preserve semantic structure through lowering, and generate specialized execution paths without losing correctness. The CPU path should remain a trusted oracle. The GPU path should be portable, testable, and increasingly equivalent in meaning. The system should be designed so that better math, better planners, and better backends can make the engine smarter over time without forcing authors to rewrite their worlds.

This codebase is moving toward a fully fledged engine, not a shader playground. That means the field language must eventually support not only rendering, but the broader building blocks of games: world structure, interaction, gameplay queries, simulation-friendly semantics, tooling, and deterministic testing. We want an engine where authored meaning survives compilation, where world queries are first-class, and where agents and humans alike can build ambitious games on top of a rigorous substrate.

The destination is an incredible game engine based on fields: expressive, portable, testable, semantically rich, and powerful enough to support both cutting-edge rendering and real gameplay.

When there is tension between short-term feature delivery and preserving the semantic substrate, prefer the substrate unless the plan explicitly says otherwise.

## Engineering Principles

- Preserve authored meaning through every lowering stage. If semantic structure is lost, make that loss explicit, justified, and tested.

- Treat query families and contracts as first-class architecture. Helper names, runtime wiring, kernels, and adapters are implementation details; public contracts should describe identity, schemas, guarantees, backend support, and policy requirements.

- Keep the CPU path trustworthy. CPU execution is the semantic oracle for GPU, WGSL, virtual GPU, and future backends. Backend optimizations should be checked against CPU meaning whenever feasible.

- Prefer correctness and semantic clarity before backend cleverness. Specialized execution paths are valuable only when they preserve the contract they claim to implement.

- Separate public authored surface from internal refactors. Avoid unnecessary syntax churn. When migration is necessary, make it deliberate, documented, and covered by compatibility tests.

- Keep public contracts distinct from internal adapters. Internal bridge records and helper shapes may exist, but they should not leak through public APIs, docs, or builtin catalogs.

- Test behavior, not just shapes. Add regression tests that prove semantic guarantees, backend equivalence, compatibility behavior, and public/private boundary enforcement.

- Optimize for long-term engine architecture. Local shortcuts that make one query or backend pass while weakening the world model are not acceptable.

- When completing a planned phase, verify the full acceptance criteria, run appropriate end-to-end gates, then perform an independent review before calling the phase complete.

## Workflow Surface

- `just` is the canonical repo front door. Prefer named repo lanes over raw `cargo` for routine work.

- `cargo` is the Rust substrate and low-level escape hatch for narrow debugging or implementation-local checks.

- `wrela` is the authored-world and product-facing workflow surface (`test`, `perf`, `preview`, and similar commands). `just` is allowed to compose both `cargo` and `wrela` when the truthful proof spans both surfaces.

- The current canonical `just` lanes are: `check`, `check-clean`, `build`, `build-release`, `test`, `test-clean`, `test-all`, `test-runtime`, `test-compiler`, `test-cli`, `test-query`, `perf-smoke`, `perf-closure`, `lint`, `fmt`, `fmt-check`, `fix`, and `ship`.

## Intended Dev Loop

- Start with the cheapest truthful lane for the change. Prefer focused lanes like `just test-runtime`, `just test-compiler`, `just test-cli`, or `just test-query` before broader repo lanes.

- Use `just check` for fast compile feedback while iterating.

- Use `just test` as the fast default repo lane. It is allowed to combine Rust-native and authored-world proof when both are part of the real contract.

- Use `just test-all` for the full local semantic lane.

- Use `just perf-smoke` for cheap perf sanity when touching perf-sensitive code, and `just perf-closure` only when working the representative 1080p120 closure lane.

- Use `just check-clean` and `just test-clean` when you need cleanroom validation with isolated artifacts and incremental compilation disabled.

- Run `just ship` before handoff unless the task explicitly scopes a smaller proof surface.

## Completion Gate

After completing acceptance criteria for a given project, that project is not complete until you launch a new subagent to review your work for correctness, architecture, maintainability, and performance. It should also verify that the project has been fully completed based on the tasks in the plan and expected outcomes. Take that feedback into account and fix whatever comes up from this independent review. This should always be your last task. When you launch the subagent, in your message, tell it that it is a review subagent. Provide the subagent with any test findings that you already ran.

If you are reading this and you have been told you are a review subagent, YOU ARE THE SUBAGENT. Do not launch your own subagent. Just do the code review and return with your findings. Don't run any tests, just do a code review.
