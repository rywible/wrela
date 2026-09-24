# Faster agent authoring: implementation and evidence

The gate pilot exposed avoidable discovery, capture and handoff overhead. This change gives agents a shorter supported path and makes its costs observable. It does not establish a productivity win over Unreal or Blender, or demonstrate AAA art quality.

## Delivered

- Task-specific context includes brief, work/source keys, semantic parts, dependencies, constraints, operation templates and matched cameras. Assemblies include a member-end detail view.
- `work evaluate` retains a candidate, executes constraints and captures baseline/candidate views, image metadata and a contact sheet in one hardware browser invocation. Render hosts are reused across cameras. Failed evaluations remain recorded without publishing source.
- `work finish` preflights portable evidence, uses the existing recoverable publication authority, and exports final source, images and handoff bundle. Benchmark mode creates the submission atomically only after artifacts exist and refuses replacement.
- Studio exposes the same workflow through `work.fast` and the Work sessions panel. Browser validation exercises real hardware captures, publication, portable artifacts and persistence after reload.
- Typed opening edits coordinate both posts, span length and reserved clearance while protecting profiles and socket definitions. Timber edits isolate material definitions and align grain with member sweeps, including horizontal and diagonal beams. Growth rings cross end faces; ground staining follows authored height rather than erroneously staining one end of a horizontal beam.
- The runtime's assembly splitting now preserves material coordinates. Close-up visual inspection found this omission after compiler-only checks had passed. The split also retains the lighting work's sky-visibility channel, transformed to each part's local frame, with memory accounting.
- CLI telemetry records successful and failed commands, discovery duration, overlapping tool wall spans and first rendered candidate time. Unmeasured model/inspection/idle time remains explicitly unattributed. Benchmark requests include task context and the fast workflow, and benchmark records retain the measured telemetry.

## Scope and limits

Opening inference supports static orthogonal frames; ambiguous and articulated relationships require explicit operations. Timber weathering assumes object-local +Y ground at zero. It does not infer joint cavities or produce realistic damage chips. The grain correction improves physical meaning; these materials still need art direction and independent acceptance.

Default cameras are suggestions. Benchmark-prescribed cameras and task dimensions remain authoritative. Per-packet first-image time starts at packet execution; the CLI summary can measure it from the work or benchmark start. A rendered image timestamp does not imply that the agent has inspected it.

The old fresh-agent pilot remains unchanged: approximately nine minutes for constrained revision and a timeout for continuation. No fresh-agent rerun or native-engine comparison was performed for this change. Scripted integration excludes model reasoning, discovery decisions and visual judgement and must not be presented as a two-second replacement for a nine-minute agent task.

## Validation provenance

Final validation uses source fingerprint `415704301745e998f20cdde0c9bfcf6b6533a04eb0d4935ef478ddeceb5906a3` in `output/fast-authoring-verified/source`. This is the coherent earlier source snapshot plus five final authoring/runtime/test files. The shared checkout was simultaneously receiving a separate lighting contract; one intermediate snapshot caught its import before its new file existed. That unrelated typecheck failure is retained in the intermediate logs rather than patched from this task.

The [source manifest](../../output/fast-authoring-verified/source-manifest.json) and [overlay provenance](../../output/fast-authoring-verified/provenance.json) identify the exact tested bytes. Earlier snapshots and failed diagnostic runs remain in `output/fast-authoring-final` and `output/fast-authoring-runtime-fix`.

Useful commands:

```sh
bun test ./packages ./apps ./games ./tools ./verification
bunx tsc --noEmit
bunx tsc -p tsconfig.browser.json
bun tools/boundaries.ts
bun tools/build.ts
bun tools/fast-authoring-check.ts output/fast-workflow
bun tools/authoring-work-browser-check.ts
```

The scripted hardware cases are a widened gate, its timber treatment, and a shorter braced trestle with different member IDs and a diagonal. Each exercises public context/evaluate/finish APIs and portable export. The trestle is a transfer smoke test, not a held-out agent study.

Final results: **1,013 tests passed, zero failures**, across 221 files (81.01 seconds). Both TypeScript configurations, dependency boundaries and the 17-asset build passed. Scoped formatting checks passed with 16 non-null-assertion warnings and one template-string suggestion. The real browser test retained 16 artifacts and a 1,078,129-byte contact sheet, published reviewed source, and verified both the adopted work record and saved material value after reload. See [validation logs](../../output/fast-authoring-verified/logs/alltests.log) and [browser result](../../output/fast-authoring-verified/logs/browser.log).

| Scripted case | Complete workflow | Review packet | First candidate within packet | Constraints passed |
| --- | ---: | ---: | ---: | ---: |
| Widen opening | 1.87 s | 0.77 s | 0.69 s | 11 |
| Apply timber | 1.68 s | 0.71 s | 0.64 s | 15 |
| Braced trestle | 1.72 s | 0.72 s | 0.64 s | 20 |

These are individual local integration observations, after the CPU suite completed, not statistically established latency guarantees. An earlier run measured 9.43 seconds for opening and approximately 1.8–1.9 seconds for the other cases; all observations remain retained. The final work-relative first-image observations were 1.45, 1.28 and 1.33 seconds. Handoff timing is a snapshot taken inside finish, so the current finish command is absent from that exported trace; the runner's later trace read includes the completed command.

[Raw scripted results](../../output/fast-authoring-verified/result.json), [timber contact sheet](../../output/fast-authoring-verified/timber-contact-sheet.png), [timber close-up](../../output/fast-authoring-verified/timber-detail.png), [trestle close-up](../../output/fast-authoring-verified/trestle-detail.png), and [Studio work panel](../../output/fast-authoring-verified/work-panel.png) preserve the review. Images demonstrate directional grain and crosscut rings; independent visual acceptance remains outstanding.

See the [workflow guide](agent-authoring.md#fast-review-and-handoff) for requests and recovery behavior. The next evidence needed for the agent-native thesis is repeated fresh-agent attempts with matched budgets and independent visual review; the plumbing to record and submit those attempts is now included.
