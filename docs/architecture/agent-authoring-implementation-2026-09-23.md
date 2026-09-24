# Agent authoring implementation and first pilot

The authoring workflow and comparative benchmark are implemented. The pilot demonstrates editable, reviewed source and a portable handoff, but does **not** establish a productivity advantage over Unreal or Blender or AAA visual quality.

## Delivered

| Gap | Working implementation |
| --- | --- |
| Excessive discovery context | Paginated capabilities, one-operation schemas, bounded source inspection and dependency impact. The reference catalog is 4,324 bytes versus 366,503 bytes for the full schema. |
| Low-level edits without intent | Typed forest shelter, graded route and composed recipe plans; ordinary validated operations remain the publication authority. |
| Weak visual diagnosis | Current-revision pixel/source attribution, source-control explanations and measured single-control experiments that retain failed trials. |
| Preservation and outcome checks | Shared source, part dimensions, clearance, routes, sightlines, runtime motion, silhouette regions and hardware resource reviews. Missing measurements block adoption. |
| Lost alternatives and context | Durable work sessions in Studio/IndexedDB and CLI/file storage, immutable candidates, decisions, feedback, review evidence and recoverable publication. |
| Fragile handoffs | Checksummed source/evidence bundles with validated imports, relocated references, local attachment ingestion and rejection of dangling evidence. |
| Repeated successful edits | Searchable, persisted recipes promoted from explicitly accepted, constraint-passing edits; parameter ranges, bindings and provenance; fresh review on reuse. |
| Unsupported productivity claims | Seeded five-category benchmark, fixed budgets and constraints, fresh-context runner interface, engine-source fingerprints, immutable attempts, failure accounting and blind review gallery. |

The shared checkout also received a concurrent architecture refactor. These features use its public review package, indexed authoring authority, compact work records and recoverable publication protocol. This report describes the resulting integrated workflow rather than attributing that separate refactor to this task.

## Use

Open **Agent operations → Work sessions** in Studio. Create a brief and constraints, retain alternatives, review, adopt, record feedback and export a portable bundle. The same workflow is available through `window.wrela.work` and `bun run author <workspace> work …`.

The [authoring guide](agent-authoring.md) documents requests, constraints, experiments, recipes and the benchmark protocol. Start a study with `bun run bench:agents prepare <directory> <config.json>`, run each declared cell with an instrumented fresh-agent command, then generate `blind` and `report` artifacts.

## Actual fresh-agent pilot

The pilot used seed `104729`, one repetition, a 600-second wall-clock budget and a declared 16,000-token budget. Both agents started with fresh context and only the supplied task/source/evidence. The frozen engine fingerprint was `bfe109e4e0aab5cc1a67cf86a34f11578c224d22fbd76ae70731ece5caa46f11`.

| Attempt | Official outcome | Evidence |
| --- | --- | --- |
| Wrela constrained revision | Completed in 553,704 ms (9m 14s); all six registered geometric/route constraints passed | Editable source, three prescribed final views, retained decisions and handoff bundle; no engine edits. |
| Wrela fresh-agent continuation | Timeout at 600,120 ms | Later atomic submission retained as diagnostics. Independent diagnostic evaluation passed all six constraints. The compiled geometry matched the predecessor, but this does not change the timeout. |
| Blender, four eligible cells | Unavailable | Executable absent. |
| Unreal, five eligible cells | Unavailable | Executable absent. |

The other three Wrela categories were not run in this pilot. The inherited model was not exposed as an exact version, token usage was unavailable, and independent blind artistic review was not performed. The completed revision records zero human interventions; the timed-out official result conservatively retains unknown costs. The report correctly sets `comparisonEstablished: false` and awards no accepted productivity results.

The [machine report](../../output/agent-authoring-implementation/pilot/source/output/study/report.json) and [blind review gallery](../../output/agent-authoring-implementation/pilot/source/output/study/blind/index.html) preserve the registered results. [Post-timeout handoff review](../../output/agent-authoring-implementation/pilot/source/output/study/diagnostics/handoff-review.json) is separate diagnostic evidence. The submission was prepared before the deadline but atomically published after it; file modification time alone is not a valid completion timestamp.

## What the pilot changed

Local feedback attachments could originally remain outside the managed evidence store. A bundle could then miss part of an agent's investigation. Feedback now ingests those files, nested reports are retained, and backup rejects unresolved local evidence. Rehoming the actual revision record preserved **20 artifacts, including 10 image files**, with identical source identities and image bytes. The [portability result](../../output/agent-authoring-implementation/portable-revision/result.json) and portable bundle were generated from a copy, preserving the original attempt.

The final implementation also strengthens suite checks for material ownership, beam profiles, rotation and sockets, requires source artifacts to match the published project, records a running attempt before execution and reserves each cell exclusively. These changes were made after the pilot snapshot; its older six-check results are not retroactively presented as results of the strengthened suite.

Observed authoring friction included guessed source paths, a rejected timestamp and an overly dark first grazing-light setup. These failures remain in the work history. The inspected weathering images still show soft grain, repetitive broad variation and a shared grain direction across posts and lintel. Geometry correctness and successful publication are insufficient measures of artistic success.

## Verification and limits

Verification uses a frozen copy because other tasks are actively editing the shared checkout. Its fingerprint is `9d1d96213febdd7d245fb6207304c36862b3186e312fec969430cdf49a9248c7` across 825 source/configuration files. Both TypeScript configurations, the production Studio/player build, dependency boundaries and the scoped authoring lint checks pass. The full suite passes **1,003 tests across 219 files**, with zero failures (90.98 seconds). The focused integration suite separately passes 41 tests.

The real browser check verifies IndexedDB reload, stale-write rejection, evidence relocation, persisted recipes and switching retained sessions. This is a storage/UI integration check; its small image fixture is not a rendering-quality test.

The production WebGPU smoke also passes exact source preservation, a matched silhouette check (zero changed pixels) and a 30-frame resource review after 30 warmup frames on the Apple Metal adapter: CPU p95 0.80 ms, GPU p95 2.10 ms, 48,811,573 owned GPU bytes and zero incomplete frames. Its gates are deliberately permissive (100 ms CPU/GPU and 512 MiB); these are integration measurements, not a device-wide performance or agent-productivity claim. [Validation artifacts](../../output/agent-authoring-implementation/verified/validation.json) retain logs, source identity and the browser/hardware reports.

External-engine support currently supplies neutral editable starters, process orchestration and an independent native-evaluator contract. It does **not** include an installed, validated Unreal/Blender evaluator. A complete comparison still requires those native integrations, all matched cells on held-out tasks, exact model/token instrumentation and independent blind visual/editability review.

Semantic plans are bounded typed tools, not a universal intent interpreter. Spatial review samples declared terrain/assembly geometry; it is not general navigation certification. Image checks cover specified views and times. Resource checks cover one measured device and scene. Recipe parameter ranges do not certify art quality throughout the range.
