# Agent authoring and comparative evaluation

Wrela's agent advantage is a hypothesis. Measure accepted output, total iteration cost and retained constraints. Successful source transactions and rendered images do not establish artistic acceptance or a win against another engine.

## Progressive inspection

`AuthoringSession.discover()` returns a bounded catalog. The reference project returns about 4.3 KB, compared with 366.5 KB for the complete schema. Use `discover({target,search,offset,limit})`, then `discover({operation:"field.update"})` for one schema. `discover({contract:"intent"|"constraint"|"experiment"|"editRecipe"})` supplies one workflow contract. `discoverFull()` retains the explicit legacy schema endpoint. `inspectFields({target,path,offset,limit,depth})` returns bounded source selections with their identity. `impact(batch)` validates an edit and returns its dependency closure. Declared dependencies are not measured visual influence.

The Studio exposes these through `window.wrela`, plus `explain(target,node?)` and `explainPixel({x,y,width?,height?})`. Picking requires the current rendered revision. Creature body repairs keep the existing rest-space grounding and protection contract. `experimentAuthoring` measures one numeric source perturbation at a time against explicit review metrics, retains invalid trials and suggests only measured improvements whose declared constraints pass. It never adopts automatically.

CLI equivalents:

```sh
bun run author /path/to/workspace discover query.json
bun run author /path/to/workspace fields selection.json
bun run author /path/to/workspace impact proposal.json
bun run author /path/to/workspace explain polar-bunny body
```

## Durable work

The Studio's **Agent operations → Work sessions** panel retains briefs, alternatives, review results and feedback. It can preserve operations as an alternative, run constraints, record design acceptance, retain recipes and export evidence. `window.wrela.work` supplies create/list/open/export/import, backup/restore, propose/plan/review/capture/experiment, feedback/decide/adopt and recipe/recipes/instantiate. Browser records use a dedicated IndexedDB database with compare-and-swap writes. This is local storage; export important work.

The CLI stores portable JSON under `.wrela/work`, separately from source generations. File writes use atomic replacement, cross-process locking and a work-content precondition. Candidates survive process restarts as operations against a retained baseline. Evidence lives beside the record. Browser image evidence has `work-artifact:` references in IndexedDB. `backup` packages the work, images and nested measurement reports together with SHA-256 checksums; `restore` verifies and relocates every included reference. The Studio's export button uses this portable bundle. Both environments read the same bundle format, capped at 64 MiB. The lower-level `export` command returns only the record.

```sh
bun run author /path/to/workspace work init brief.json
bun run author /path/to/workspace work inspect my-work
bun run author /path/to/workspace work propose my-work proposal.json
bun run author /path/to/workspace work review my-work review.json
bun run author /path/to/workspace work adopt my-work adoption.json
```

CLI responses are under `work`. `init` accepts `{id,brief,constraints:[]}`. Mutations require the `expectedKey` returned by `inspect`. A proposal request is `{expectedKey,proposal,operations,actor?}`. A review request is `{expectedKey,proposal,hardware:true}`. CPU-only review leaves required image/GPU measurements unmeasured. Adoption requires every declared constraint to pass against the exact baseline, candidate and constraint identities. Unknown or incomplete results cannot pass. `requireArtReview:true` additionally requires an explicit recorded decision; automatic constraints do not award art approval.

`feedback` accepts `{expectedKey,event:{id,at,kind,actor,summary,evidence?,tokens?,humanInterventions?}}`. Counts default to unknown, not zero. `decide` accepts `{expectedKey,proposal,decision:{reviewer,verdict,reason}}`. `export` emits the complete source/proposal/event record. `import` validates the record and replays each proposal's ordinary operations to verify its contents. `capture` accepts `{expectedKey,proposal?,settings:{target,camera,width?,height?,tick?,mode?}}`. Omit proposal to capture the baseline. Captures use the production runtime and renderer in an isolated hardware browser.

Each proposal starts from the work's immutable baseline. To revise an alternative, submit its complete operation sequence under a new proposal identity. After accepted source changes, start a new work session with the accepted project as baseline; earlier records remain evidence. A fresh author can begin with `inspect` and only retrieve source paths relevant to the next edit.

`work backup my-work` emits a bundle that can be saved directly from stdout and passed to `work restore bundle.json` or imported in Studio. Publication retains a durable intent before changing source; retrying an interrupted adoption can recover the same committed receipt.

CLI feedback copies explicitly attached local files into managed evidence, including nested reports and images. Files must be inside the feedback request's directory or the work's existing evidence directory. Web links remain links. Backup rejects dangling local evidence. Older records can use `work retain-evidence my-work request.json` with `{expectedKey}` to retain their referenced files before exporting; place this request alongside the attachments. Browser callers store blobs through `work.store.saveArtifact(name,value)` and use the returned reference in feedback.

## Intent, constraints and learning

`planAuthoringIntent` provides five typed plans: `forest.shelter` coordinates maturity, canopy, wind stiffness, populations, clearing exclusions and route/sightline constraints; `world.route` coordinates graded path geometry; `bundle` composes the existing domain recipes; `assembly.opening` coordinates two posts, a span and the reserved opening; `assembly.timber` supplies isolated member materials with longitudinal grain, crosscut rings and ground-height staining. These are explicit bounded plans, not a natural-language interpreter. Returned operations use the existing transaction authority. Scope limitations and affected definitions accompany the plan. Use `work plan request.json` to inspect a plan before proposing it.

## Fast review and handoff

```sh
bun run author /path/to/workspace work context my-work timber-frame
bun run author /path/to/workspace work evaluate my-work evaluation.json
bun run author /path/to/workspace work finish my-work finish.json
bun run author /path/to/workspace timing
```

`context` returns the current work key, brief, declared constraints, bounded semantic parts and dependencies, applicable operations, typed request templates and matched review cameras. Assembly defaults include front, angle, reverse and member-end detail views. Use dimensions and prescribed cameras from the brief; example values are templates. The source envelope used for framing is conservative, not a measured geometric certificate.

An evaluation request is `{expectedKey,proposal,target,intent?,operations?,views?,width?,height?}`. Supply either an intent or operations for a new proposal, or neither to review an existing proposal. For example, `intent:{kind:"assembly.opening",target:"timber-frame",width:3.2}` preserves member cross-sections and socket definitions while widening the connected frame. Timber requests use a new material ID: `{kind:"assembly.timber",target:"timber-frame",material:"aged-oak",age:0.45,grainScale:1.4,bevel:0.005}`. Intent preservation constraints are added before alternatives exist; start a new session to introduce a different constraint set.

Evaluation retains the candidate before GPU work, runs production constraints, reuses render hosts across cameras, and saves baseline/candidate images, image metadata and a contact sheet in one browser invocation. Failures remain work events and do not publish source. Newly created targets receive explicitly labeled absent-baseline images. Current source and constraint identities bind every review to its candidate.

Finish accepts `{expectedKey,proposal,requireArtReview?}`. It preflights portable evidence, uses the shared publication/recovery protocol, then exports final source, the portable work bundle, handoff, images and timing under `.wrela/exports`. A packaging failure after publication reports `published:true,packagingPending:true` with a retry key. It never implies that committed source was rolled back. For benchmark attempts, add `benchmarkRequest` with the runner's request path: finish writes all required artifacts and atomically creates the submission without replacing an existing one. The benchmark still independently evaluates source and enforces its deadline. Art approval is never inferred.

Studio exposes the same path through `window.wrela.work.fast.context/evaluate/finish`. The Work sessions panel has **Review views** and **Finish and export** buttons. Review images and portable evidence survive IndexedDB reloads.

The CLI records bounded command traces, including read-only discovery and failed calls. `timing` reports call counts, discovery duration, union of tool wall spans, and first rendered candidate latency when available. The packet's `firstCandidateImageMs` begins at packet execution; the trace summary measures from the work/attempt start. Neither is the time the agent first looked at the image. Unattributed wall time includes model latency, inspection, manual shell work and idle time; `modelMs` remains unknown. Browser evaluation records packet timings and failure events, but does not measure every browser API call or model turn.

Opening inference deliberately supports only an unambiguous orthogonal static frame; attached/articulated structural edits require explicit operations. Timber grain follows member sweep coordinates through rigid transforms and repeats. Ground staining assumes object-local +Y with zero ground height; curved members omit that stain. Physical joint cavities, damage chips, broad construction solvers and artistic acceptance remain outside this bounded recipe.

Result constraints cover exact source preservation, compiled mesh/part dimensions, reserved-volume clearance, sampled route clearance, declared sightlines, actual runtime planted-foot slip/contact residuals, matched silhouette regions, and CPU/GPU/allocation budgets. Review is shared between Studio and CLI. Volume checks use triangle intersection and closed-surface winding in rest coordinates. Route checks use compiled assembly bounds and sampled graded terrain; they do not certify free-space navigation or scattered vegetation. Motion reviews cover the declared flat-ground runtime. Silhouette checks describe only the supplied camera/time/region. Resource checks include simulation/extraction/submission and uniquely attributed GPU samples after warmup; they do not certify all devices or long thermal sessions.

`work experiment` takes `{expectedKey,proposal?,hardware?,experiment:{controls:[{target,path,delta,units}],objective:{constraint,metric,direction}}}`. It retains reports and finite differences for baseline and trials. The same work can collect failed hypotheses rather than losing the investigation on restart.

`work recipe` promotes an explicitly accepted proposal with passing constraints into a reusable edit recipe with provenance and optional numeric parameter bindings. It persists in the searchable `work recipes [search]` library and the work evidence. The request is `{expectedKey,proposal,recipe:{id,description,parameters:[]}}`. Parameter entries declare name, units, description, min/max/default and `{operation,path}` bindings. Default bindings must match the accepted edit; overlapping bindings and out-of-range values are rejected. `work instantiate request.json` accepts `{recipe,parameters?,targets?}` and returns ordinary operations plus fresh review requirements. Target substitution applies to edit targets; creation/dependency remapping requires explicit source operations. Declared ranges are not automatically certified artistic ranges. Every reuse starts unreviewed.

Novel requests are accounted for as ordinary content, reusable recipe work or engine development. The content benchmark forbids engine changes; engine-development attempts belong in a separate track and include their cost.

## Comparative benchmark

```sh
bun run bench:agents availability
bun run bench:agents prepare output/agent-study config.json
bun run bench:agents run output/agent-study runner.json
bun run bench:agents blind output/agent-study
bun run bench:agents report output/agent-study
```

Preparation config: `{model:"the actual model identity",seed:7919,repetitions:3,seconds:600,tokens:16000}`. The suite predeclares creation, constrained revision, defect repair, scene composition and fresh-agent continuation briefs. It freezes result constraints in the suite rather than accepting an author's modified constraints. Revision and repair test compiled part bounds, headroom and doorway clearance. Every task includes an engine-neutral starter with named editable meshes, semantic structure, materials and coordinate conventions. Numeric dimensions vary with seed. Use held-out seeds for evaluation; fixture parameter variation alone is not proof of generalization to unrelated content. Assets, track, model, camera, runtime budget and acceptance requirements must be held fixed within a comparison. Scene tasks compare Wrela with Unreal; asset tasks also admit Blender. Browser delivery remains a separate result. Run against a frozen source snapshot; the runner rejects a different engine fingerprint. Do not run timed attempts concurrently on shared hardware.

Runner config: `{engine:"wrela",agent:"runner identity",model:"matching model",command:["path/to/fresh-agent-runner"],task:"revise",repetition:0}`. The runner receives an absolute request JSON path as its final argument and in `WRELA_BENCHMARK_REQUEST`. It must start a fresh model context and write the indicated `submission.json`. Use native scripting APIs, not a handicapped click-only baseline. Blender/Unreal paths are configured with `BLENDER_PATH` and `UNREAL_EDITOR_PATH`. Missing executables produce unavailable attempts; no fallback silently substitutes Wrela output.

Submission format:

```json
{
  "tokens": null,
  "humanInterventions": null,
  "constraintsPassed": null,
  "artifacts": [
    {"path":"final.json","kind":"source"},
    {"path":"front.png","kind":"image"},
    {"path":"handoff.json","kind":"handoff"}
  ],
  "stages": [{"name":"discovery","durationMs":1200}],
  "reason":"Describe incomplete measurements explicitly"
}
```

Paths must stay inside the attempt directory. Artifacts are hashed; failures, timeouts and engine changes are retained. Wall time is runner-measured; submitted stage times are explicitly agent-reported. Token/intervention counts remain unknown unless the runner actually instruments them. Wrela constraints are reevaluated by the benchmark after submission. External engines require an independent native evaluator before constraint qualification; an agent's claimed pass is not accepted. Handoff attempts require a completed predecessor and receive its retained work without its conversation history.

Native integrations can supply `nativeEvaluator:["executable","script"]` in the operator's runner configuration. The evaluator runs after the author exits and receives a request path containing the frozen task, submitted source paths and hashes, attempt directory and expected report path. It must reopen native source and write `{version:1,suiteKey,task,evaluator,sources:[{path,sha256}],checks:[{id,status,scope,measurements,evidence}]}`. Every registered constraint needs exactly one result; missing measurements are `unmeasured`. Native evaluation cannot alter submitted source. This is an adapter interface, not a claim that an Unreal or Blender evaluator has been installed or validated here. `tools/agent-benchmark/wait-for-submission.ts` supports hosts that launch fresh agents separately; atomically write `submission.json` only after all artifacts are complete.

`blind` copies final images and briefs to anonymous sample cards and keeps engine mapping outside the gallery. Reviewers assess visual quality before inspecting editable source in a separate phase, then record `{version:1,blindId,reviewer,verdict,reason,scores:{brief,quality,editability}}` under `judgements/`. Scores range from 1 to 5. Reported acceptance requires complete matching measurements, constraint qualification and independent blind acceptance. Duplicate attempts/votes and modified evidence are rejected. Scripted smoke tests must declare `execution:"scripted-smoke"` and are excluded from agent economics.

The benchmark intentionally makes incomplete evidence visible. It does not claim a comparative advantage when engines, measurements or independent reviews are unavailable.

## Shared domain studies

`work study <work-id> request.json` generates one to four alternatives, reviews them in one hardware browser, and retains a comparison gallery. Supported domain adapters are `timber` (swept assemblies) and `vegetation` (botanical architecture, excluding event-based developmental growth). Both use the same session, preservation constraints, search, review, publication, portable evidence and recipe infrastructure.

Get a ready request from `work context <work-id> <target>`, under `study.input`. Set `expectedKey` to the current work key. The request contains `id`, `brief: {domain,target,conditions,quality}`, `candidates` and optional `spread`/`views`. Conditions are normalized authoring controls in [0,1]: exposure, moisture, maturity and variation. Their meaning is adapter-specific; they are not calibrated physical measurements. `quality` contains an explicit direction, visual criteria, reference notes and up to four retained reference paths. Text guides the reviewer; it does not secretly drive numeric generation.

Default review uses a whole-subject neutral view, a neutral detail view and a grazing detail view. These lighting rigs are temporary fixtures with identical lighting for baseline and candidate. They never modify the authored project. An explicit missing stage is an error; it cannot silently fall back to different lighting. Fixed daylight captures reject blank, dark or constant images. This verifies capture usability, not artistic quality. Every candidate and failed evaluation remains in the work history.

`work finish` selects a reviewed passing proposal and exports its editable source and evidence. This does not grant artistic acceptance. After an explicit accepted review decision, `work remember <work-id> request.json` (with `expectedKey`, `proposal`, `recipe`) promotes the study's conditions into a reusable semantic recipe. Its retained conditions must reproduce the approved edit. Instantiation on another target replans through the adapter, establishes source preservation constraints and returns an **unreviewed** edit; acceptance does not transfer. Existing recipe discovery and instantiation work for both operation recipes and semantic recipes.

A generic `geometry` result constraint checks a target's compiled triangle count and bounding extents against declared budgets. Instanced foliage counts expanded triangles. This is a regression budget, not an estimate of actual GPU cost or an aesthetic score.

### Native agent tool endpoint

Start `bun tools/authoring-agent.ts --mcp` as a stdio MCP server. It exposes `wrela_context`, `wrela_study` and `wrela_finish`. Study returns structured metadata and a PNG gallery as native image content, without another image-loading round trip. Images are bounded and restricted to retained workspace evidence. Hosts must connect this server; implementing the endpoint does not install it into a running agent's tool list.

The CLI-compatible bridge accepts a JSON file:

```json
{
  "workspace": "/absolute/workspace",
  "work": "wood",
  "action": "study",
  "input": {
    "id": "weathering-1",
    "expectedKey": "CURRENT_WORK_KEY",
    "brief": {
      "domain": "timber",
      "target": "gate",
      "conditions": {"exposure": 0.8, "moisture": 0.35, "maturity": 0.85, "variation": 0.55},
      "quality": {"direction": "Weathered trail timber", "criteria": ["Directional grain and readable end grain", "Restrained silvering and ground staining"]}
    },
    "candidates": 3
  }
}
```

Run `bun tools/authoring-agent.ts invocation.json`. It returns `{result,images}`. A host can execute this command and display its returned images in one tool invocation. The fresh-agent transfer benchmark uses this bridge because the current host's live tool list cannot install the new MCP endpoint mid-run. `finish` accepts `benchmarkRequest` for atomic benchmark submission as before.

Studio exposes the same study controls in the work-session panel under **Explore reviewed alternatives**, and **Remember accepted study** retains an approved semantic recipe.
