# Wrela architecture and agent authoring audit — September 23, 2026

The findings below describe the original snapshot. See the [remediation report](audit-remediation-2026-09-23.md) for implemented changes and subsequent verification.

**Assessment:** Keep the semantic source → compiler → simulation → renderer architecture. It has strong foundations and useful executable contracts. The next architectural investment should make the authoring lifecycle durable, make alternative proposals inexpensive, and give games an explicit integration boundary. Those changes will contribute more to scalable agent development than a broad engine rewrite or cosmetic file splitting.

The code supports substantial procedural asset and scene authoring. Creating another complete game still requires knowledge of engine and Player internals. The distinction matters: a good agent game engine must make game rules, interaction, testing, persistence, and delivery as approachable as geometry and materials.

**Scope and evidence.** This audit covers the current checkout, including uncommitted work. Execution used a frozen source snapshot taken at `2026-09-23T22:43:32.936Z`, fingerprint `19569aeb2e242cef726f28df0b3e44a35b173dbcd1f6d58ef06d7cd5f4411fa3`, based on Git HEAD `13e39e2ca8915f6287a062fb9e6a81e0cc007b2b`. The checkout continued changing during the audit. Findings refer to that snapshot unless explicitly stated otherwise.

The snapshot contains 742 TypeScript/TSX/WGSL files and 132,089 lines, including 211 test files and 28,072 test lines. Apps and engine packages account for 527 files and 104,651 lines. The repository's snapshot helper excludes the independent `tools/field-research` and `tools/transport-research` directories; their tests are outside these execution totals. Source inspection, dependency analysis, CPU checks, a production build, and focused authoring probes were performed. This audit did not run GPU/browser playback or assess visual quality and game feel.

Raw results and the exact source are retained under [output/architecture-audit-2026-09-23](../../output/architecture-audit-2026-09-23). That directory is ignored by Git; this report is the durable summary. No engine implementation files were changed by this audit.

**What is already worth preserving**

- The dependency policy is clear and passed its check: authoring and rendering depend on the shared model; compiler depends on model; world builds on compiler; runtime builds on world. The inspected package graph is acyclic. Rendering consumes evaluated scene data instead of owning authored source.
- `AuthoringSession` provides immutable source, atomic batches, document preconditions, idempotent in-session transactions, targeted revert, and bounded history. Its existing 200-definition benchmark accepted edits at **24.04 ms p95** in this run and retained the identity of 199 unchanged documents. This measures synchronous source acceptance, not time to a complete rendered frame.
- Compiler product keys separate geometry, binding, motion, and material changes. Bounded caches, worker settlement, source fingerprints, cooked artifact validation, and honest fidelity diagnostics are valuable foundations.
- Runtime separates fixed simulation ticks from scene extraction. World scheduling distinguishes collision-critical work, bounds queues and abandoned jobs, and maintains dormant entities. These are appropriate foundations for hardware and world-size scaling.
- Review captures retain source identities and completeness. Persistent work records, constraints, progressive discovery, provenance, and explicit unmeasured outcomes already exist. These should become the common authoring workflow.

Evidence: [dependency policy](../../tools/boundaries.ts), [transaction tests](../../packages/authoring/src/transactions.test.ts), [product keys](../../packages/compiler/src/products.ts), [bounded scheduler](../../packages/world/src/scheduler.ts), [runtime contract](../../packages/runtime/README.md), [review contract](../../packages/authoring/src/review-contract.ts).

**Priorities**

| Priority | Finding | Consequence |
| --- | --- | --- |
| P1 | 1. Source adoption and durable receipts have different commit points | An agent can receive failure after its edit has taken effect |
| P1 | 2. Retained work duplicates and reparses whole projects | A few trivial alternatives exhaust storage and make operations take seconds |
| P1 | 3. Independent publication and retry semantics stop at the process boundary | Agents working on separate documents still invalidate each other's proposals |
| P1 | 4. Games lack a reusable module and delivery contract | New games require modifications across engine-owned code |
| P1 | 5. Agent discovery and validation do not form one consistent contract | Agents receive invalid suggestions or successful no-op responses to malformed edits |
| P2 | 6. The source project is also the entire asset library and delivery unit | The hard 256-definition ceiling blocks larger projects before streaming helps |
| P2 | 7. Package interiors and public APIs do not sufficiently contain feature growth | New features spread across shared coordinators and manual product plumbing |
| P2 | 8. Verification needs a reliable integration baseline and authoring outcome gates | Strong subsystem tests do not establish a reliable complete agent workflow |

P1 means address before substantially expanding autonomous game authoring. P2 means address through the next integration milestones. The ordering within a priority reflects different concerns, not a claim that every change must happen serially.

**1. Source adoption must have one observable outcome**

In [AuthoringWorkbench.adopt](../../apps/studio/src/authoring-workbench.ts#L44), `adoptWork` changes the live `AuthoringSession` before `store.put` persists the work record. A storage failure or a competing work-record update therefore rejects the operation after source has changed. The CLI similarly publishes source before writing its work receipt in [authoring-work.ts](../../tools/authoring-work.ts#L49), with a recovery path for a matching published candidate. Browser adoption does not have equivalent retry recovery.

**Reproduced:** an injected receipt-store failure caused `adopt()` to reject while the source revision advanced from 0 to 1 and the project key changed. This was an in-memory fault-injection probe, not a browser storage failure observed in normal use. See [probe-results.json](../../output/architecture-audit-2026-09-23/probe-results.json).

Introduce a durable operation identity and explicit publication state that covers source and adoption receipt. A local journal or combined source/receipt commit is sufficient; a distributed transaction system is unnecessary. If receipt persistence can remain pending, return that state explicitly and make recovery/query/retry deterministic. Avoid attempting a blind rollback after an asynchronous failure because another edit may already have followed it.

Acceptance: inject failures before and after source publication, restart the process, and retry the same operation. It must have one effect and a recoverable outcome. Browser and CLI should satisfy the same contract.

**2. Retained alternatives undo the transaction engine's scaling gains**

[work-session.ts](../../packages/authoring/src/work-session.ts#L15) embeds the complete project in every proposal as well as retaining a full baseline. `parseWorkSession` parses those projects, constructs a new `AuthoringSession` for every proposal, reapplies each batch, and hashes the results. Proposing, reading, feedback, and handoff repeatedly call this validation path. The ordinary transaction engine already uses a more economical changed-document representation.

For a valid 200-definition project with 128-node fields and a 3.35 MB compact source, the audit generated alternatives that only renamed one object:

| Retained alternatives | Work record, pretty JSON | Add alternative | Parse/read |
| --- | ---: | ---: | ---: |
| 1 | 27.0 MB | 616 ms | 296 ms |
| 3 | 56.2 MB | 1,481 ms | 744 ms |
| 4 | 70.8 MB | 1,960 ms | 972 ms |
| 6 | 100.1 MB | 4,349 ms | 1,446 ms |

These timings are individual local observations on an Apple M4 with 16 GiB RAM, not statistical performance claims. The storage failure is deterministic: [WorkFileStore.put](../../tools/authoring-work-store.ts#L37) rejects above 67,108,864 bytes. An actual attempt to persist four alternatives returned `Work record exceeds 64 MiB`; see [work-limit-probe.json](../../output/architecture-audit-2026-09-23/work-limit-probe.json).

Store immutable documents once by content key. Retain a baseline manifest, operation batch or changed-document set, review references, and decisions per proposal. Validate imported records fully at the boundary; cache validation of immutable local records. Materialize only the candidate being inspected or rendered. Reuse the transaction engine's existing diff and identity machinery.

Acceptance: a 200-definition baseline with 32 small alternatives fits comfortably within the current storage budget. Listing work and reading a handoff should not replay all alternatives. Measure these operations alongside accepted-edit latency and edit-to-complete-frame latency.

**3. Multi-agent publication needs persistent document identities and receipts**

The in-process session correctly accepts disjoint document edits. [tools/author.ts](../../tools/author.ts#L30) creates a new session on every invocation, resetting its revision/receipt history, and rejects publication when the entire `workspaceKey` differs. [WorkspaceBridge](../../tools/bridge.ts#L118) correctly prevents lost updates, but its comparison is also over the whole project. [adoptWork](../../packages/authoring/src/work-session.ts#L88) requires the exact whole-project baseline.

**Reproduced:** two proposals started from the same workspace and renamed different materials. The first published; the second was rejected as stale. Retrying the first proposal with the same transaction ID was also rejected. This is safe rejection, but it prevents the advertised independent editing behavior from extending across CLI processes.

Persist document content/revision preconditions and transaction receipts at the publication authority. Reuse `AuthoringSession` to validate a batch against the latest source under the publication lock. Preserve the whole-project key for provenance. Revalidate or rerun reviews whose dependency closure changed; source-disjoint edits can still interact through lighting, materials, routes, and composition.

Acceptance: independent CLI writers can both publish disjoint changes; a shared dependency change produces a structured conflict; a process restart followed by an identical retry returns the original receipt. Visual constraints remain tied to the source they actually reviewed.

**4. Make a game a first-class integration unit**

[Winter Valley rules](../../packages/runtime/src/game/winter-valley.ts) and the creature encounter live inside the runtime package. [Player](../../apps/player/src/main.ts#L558) directly instantiates `WinterPlayer` and `CreatureStudyPlayer`; game selection, input, camera, presentation, and controls branch around those implementations. [WinterPlayer.supported](../../apps/player/src/winter-game.ts#L324) requires named IDs including `polar-bunny` and `winter-valley`. [buildStudio](../../tools/build.ts#L19) builds the reference project and includes special handling for an optional named game.

The model package also contains 21 content/look-development files selected by filename, totaling 5,403 lines: fixtures, alpine compositions, winter brook, creature studies, and similar reference content. These are useful assets, but their ownership should be outside the engine's universal source vocabulary.

Introduce a small trusted `GameModule` interface around initialization, semantic input, fixed-step update, inspection, save/load, and disposal. Bind it to a project through a build manifest. Game code owns objectives, interactions, camera policy, HUD, and audio; the host supplies simulation, world queries, rendering, and persistence services. Imported project JSON can retain its existing data-only contract; trusted TypeScript modules are selected and compiled by the local build.

Move the existing games into `games/<id>` and reusable review content into `examples` or `fixtures`. Let Player and build/export consume the same game manifest. This should extract working behavior before inventing a general scripting language or gameplay framework.

Acceptance: an agent builds a second small game—such as activating a switch to open a gate—with input, an objective, saved progress, an inspectable play trace, and an export, using only game-owned files and public engine APIs.

**5. Make discovery, execution, and errors agree**

Progressive discovery is a good start: the default session response measured only 3,315 bytes. However, applicability, schemas, implementation, CLI dispatch, browser exposure, and descriptions are maintained separately.

Two concrete inconsistencies were reproduced:

- [discovery.ts](../../packages/authoring/src/discovery.ts#L66) advertises `material.assign` when the target is a material document. Execution rejects it with `This definition has no surface material`.
- `field.update` with `changes: { raduis: 2 }` is accepted as an empty update. Zod strips the unknown key before execution. The response reports no changed documents and a transaction ID instead of a useful validation error. Project parsing also silently removes unknown source properties. See [schema-probes.json](../../output/architecture-audit-2026-09-23/schema-probes.json).

There is further transport drift: the CLI prints only error messages, losing `RevisionConflict.code` and structured conflict details; rich assembly/world/performance helpers often require TypeScript calls or knowledge of `document.set` paths; and a shared CPU/GPU review implementation is owned by [apps/studio/src/authoring-review.ts](../../apps/studio/src/authoring-review.ts), which headless tools import directly.

Define one typed capability catalog containing operation schema, applicability, handler, semantic target description, effects/read-write requirements, and error contract. Derive discovery and transport bindings from it. Preserve existing domain helpers and expose the useful ones through this surface. Move shared review orchestration below the app layer, with browser rendering and artifact storage supplied as adapters.

Reject unknown source/operation fields with document ID and property path, or preserve explicitly supported extension namespaces. Publish structured errors with stable codes, conflicts, and actionable next steps. Extend the creature tools' useful field metadata—units, spaces, bounds, provenance—to other authoring domains.

Acceptance: every discovered operation has a valid example for its target type; malformed optional fields fail visibly; CLI and browser return equivalent results/errors; a fresh agent can perform the supported workflow from discovery without reading UI implementation files.

**6. Separate the project catalog from a loaded authoring/rendering set**

[projectSchema](../../packages/model/src/documents.ts#L402), session validation, bridge manifests, and cooked manifests all impose a 256-definition limit. Materials consume entries alongside characters, objects, vegetation, worlds, and stages. Project import also has an 8 MB text limit. Source is eagerly loaded and validated, and [cooking](../../packages/compiler/src/cooked.ts#L463) visits every definition rather than an entry's dependency closure.

Runtime world streaming therefore does not establish scalability of the source library or authoring workflow. Raising these constants alone would enlarge whole-project parsing, hashing, retained-work costs, and exports.

Introduce an indexed document repository and explicit manifests for scenes, game entries, and reusable libraries. Resolve references by stable ID; load and validate the dependency closure needed for a task. Keep bounded working-set and runtime budgets, with library capacity treated separately. Cook an explicitly declared release closure, including dynamic content referenced by the game manifest.

Acceptance: a catalog larger than 256 definitions can be searched and inspected without loading it all. Adding an unused library asset should not change the game's cooked payload. Measure 1,000-definition catalog operations before choosing larger capacity targets.

**7. Contain feature growth inside domain and lifecycle boundaries**

The package-level dependency graph is healthy, but ownership inside packages is less clear. Important coordinators have accumulated substantial policy:

| File | Snapshot lines | Responsibilities observed |
| --- | ---: | --- |
| `render-webgpu/src/index.ts` | 2,933 | Pipeline creation, resource residency, pass submission, temporal state, capture, recovery |
| `apps/studio/src/main.tsx` | 2,673 | App shell, selection, authoring controls, viewport interaction |
| `runtime/src/session.ts` | 2,021 | Actor control, creature dynamics, water, assembly state, events, replay, persistence |
| `apps/studio/src/controller.ts` | 1,638 | Source refresh, storage, compiler, preview, encounters, capture, agent API, export |
| `runtime/src/scene-host.ts` | 1,552 | Scene preparation, compilation, runtime replacement, materials, extraction, delivery state |
| `authoring/src/session.ts` | 1,337 | Transaction engine, domain dispatch, candidates, solver jobs, discovery, history |

Line count alone is not a defect. The issue is that changes in separate domains frequently require editing the same integration owners. For example, the Studio controller contains both workspace recovery and creature encounter policy. The renderer's `render` method occupies roughly 1,000 lines even though many pass helpers already exist.

Retain the current packages and organize their internals by domain and ownership. Extract preview, persistence, review, and game integration services from Studio; keep its controller responsible for composition and lifecycle. Separate simulation/replay coordination from actor-specific systems. Make rendering passes own their resources and disposal under a small explicit pass sequence.

Tighten public APIs at the same time. Nine cross-package relative source imports remain among apps/packages; each package exports a broad barrel, and package manifests do not declare their workspace dependencies. The AST analysis found one value import cycle, `compiler/index.ts ↔ cooked.ts`. The boundary checker enforces allowed package directions but does not enforce public entry points or inspect side-effect imports.

Use intentional subpath exports for source contracts, runtime products, compiler clients, and review services; declare workspace dependencies; enforce entry points with AST-aware import checks. Give each package a short purpose/API/ownership/verification map. Only runtime currently has a package README.

Finally, mesh stream lifecycle handling is repeated across [artifact transfer](../../packages/compiler/src/index.ts#L73), [cooking](../../packages/compiler/src/cooked.ts#L63), [CPU accounting](../../packages/runtime/src/artifact-memory.ts), and renderer packing. Consolidate buffer enumeration and serialization descriptors where their semantics match, while keeping specialized GPU packing explicit. A new product stream should force compiler errors or contract-test updates for transfer, round trip, and ownership accounting.

Acceptance: adding a domain capability or render product has a small, documented integration surface. Existing preview/runtime replacement, disposal, replay, and capture tests continue to pass after each extraction.

**8. Use integration and agent outcomes to control architectural growth**

The frozen source produced these results:

| Check | Result |
| --- | --- |
| Browser TypeScript | Passed |
| Package boundaries | Passed |
| CPU tests in snapshot | 974 passed, 1 failed; 975 tests across 211 files, 78.26 seconds |
| Existing accepted-edit benchmark | Passed; 24.04 ms p95 versus 50 ms gate |
| Production build | Passed |
| Full TypeScript | Failed on two errors in in-progress `tools/agent-benchmark/suite.ts` |
| Biome over frozen snapshot | 86 errors, 178 warnings; mostly formatting/import organization |

The CPU failure is the review-mode ABI expectation in [creature-inspection.test.ts](../../packages/runtime/src/creature-inspection.test.ts#L66): the expected tail omitted the appended `indirect-cache` mode. This is a contract-test mismatch, not evidence that the renderer is broken. The benchmark source changed during the audit; its frozen type failures must not be treated as a permanent current-tree diagnosis.

A follow-up `tsc --noEmit` against the live checkout passed after those benchmark edits changed. The snapshot results above remain fixed for reproducibility. The full CPU suite was not repeated against the evolving checkout. The adoption commit order, proposal duplication, workspace-wide publication check, and discovery/unknown-field behaviors were still present in the final inspected source.

CI already includes source checks, CPU tests, an authoring benchmark, a build, and an optional hardware job. Restore a green integration baseline and preserve these gates. Make source/test scope explicit: README suggests bare `bun test`, whereas authoring documentation correctly recommends explicit directories to avoid discovering historical source snapshots in `output`.

Extend verification around the actual agent loop: discover → inspect → propose → review → adopt → save/reopen → play → export. Include malformed requests, disjoint writers, persistence faults, restart/retry, multi-proposal storage, stale reviews, and source-to-render identity. Run matched GPU review for renderer-affecting milestones. New agent benchmark scaffolding is useful, but this audit did not establish autonomous completion rates, token costs, artistic acceptance, or a comparative ranking.

**A practical target structure**

```text
games/<game>/          rules, input vocabulary, HUD/audio, scenarios, build manifest
examples/             reference worlds, specimens, look-development source
packages/model/       semantic schemas, identities, product and scene contracts
packages/authoring/   capabilities, transactions, proposals, work/review contracts
packages/compiler/    domain lowering, products, caches, cooking, worker client
packages/world/       spatial planning, residency, persistent occurrences
packages/runtime/     simulation, actors, queries, replay/save, game host
packages/render-webgpu/  resources, explicit passes, presentation, capture
apps/studio/          human UI and composition of shared services
apps/player/          generic host for a selected game/project manifest
tools/                CLI adapters and maintained verification entry points
```

Shared review execution should be a small module outside the Studio app; whether it earns a separate package depends on its dependency needs. Authoring should continue to emit validated source edits, compiler products should remain replaceable, and the renderer should continue consuming evaluated data.

**Recommended delivery sequence**

| Milestone | Deliverable | Completion criterion |
| --- | --- | --- |
| 1. Trustworthy authoring | Structured errors, strict request fields, discovery parity, durable adoption/retry, compact work records, green checks | The reproduced failures above are regression tests; 32 alternatives on the 200-definition workload remain usable |
| 2. A reusable game boundary | Extract Winter Valley behind a small GameModule and manifest; build the second game through it | Input, objective, save/reopen, semantic play trace, and standalone export work without edits to engine internals |
| 3. Agent collaboration and larger libraries | Persistent per-document preconditions/receipts, review dependency invalidation, indexed catalog, release closure | Disjoint writers publish safely and a 1,000-definition catalog supports bounded inspection and cooking |
| 4. Extract shared responsibilities as they are touched | Domain directories, explicit public APIs, review service, render resource ownership | Features require fewer shared-file edits while behavior and measured performance are preserved |

The decisive benchmark is a fresh agent taking a game brief through playable, inspectable, saved, reviewed, and exported output. Record failed attempts, time, tokens, human interventions, visual acceptance, and hardware cost separately. That exercise will reveal which abstractions improve game creation and which merely add architecture.
