# Creature authoring: an implementation plan for an agent-native studio

Date: 21 September 2026  
Status: implementation underway; this document preserves the intended scope and acceptance gates. See the [development runbook](../creature-development-runbook.md), [installed operations](../../packages/authoring/CREATURES.md), and [experiment results](../research/creature-authoring-experiments.md) for current behavior and evidence. Types, tests, and UI availability do not by themselves close the visual or gameplay gates.  
Quality reference: Elden Ring's finished creature design and in-game presence, without assuming or reproducing FromSoftware's internal pipeline.

## 1. The outcome

Give a human directing agents the tools to author extraordinary, original creatures: distinctive anatomy, convincing materials, expressive faces, convincing hair and fur, purposeful movement, responsive physical behavior, and strong performance on ordinary hardware.

The deliverable is a reusable authoring system demonstrated by finished creatures. Each visual milestone must be achievable through public, inspectable authoring operations. A spectacular creature assembled through inaccessible one-off renderer code is insufficient; an elegant API that produces mediocre creatures is also insufficient.

Fields and the compiler are our central advantage. A creature should retain enough authored meaning that the compiler can derive its shape, surface appearance, deformation, attachments, contacts, and runtime representations together. Agents should author relationships and deliberate exceptions rather than manually synchronize unrelated assets.

Our strongest hypothesis is that **one coherent description of anatomy and its fields can remove a large fraction of conventional cross-discipline authoring work**. We will test that hypothesis on edits that cross systems: enlarge a shoulder without breaking its coat, scar, rig, armor, or attack; shorten a leg while repairing stance and stride; change a mane while preserving its silhouette at distance.

## 2. Product criteria and working principles

The quality bar has five independent dimensions:

| Dimension | Product question |
| --- | --- |
| Identity | Is the creature recognizable, original, and intentional in silhouette and proportion? |
| Physical credibility | Do anatomy, tissue, material boundaries, attachments, contacts, and weight agree? |
| Performance | Do poses, expression, timing, attention, anticipation, and recovery communicate character? |
| Coherence | Does the design survive motion, edits, different lighting, and changes in runtime detail? |
| Playability and cost | Does it work in a real encounter within measured hardware budgets? |

There is no single automated AAA score. Numerical checks expose failures; comparative visual review and playing the encounter establish product quality.

Implementation principles:

1. Keep authored intent separate from realization. Fields can compile to meshes, patches, curves, textures, response programs, collision proxies, or combinations of them.
2. Make procedural source expressive enough for deliberate art direction. Noise and random variation cannot substitute for anatomy, composition, or history.
3. Support both semantic edits and exact local intervention. Agents need a shoulder control and a precisely bounded displacement stroke.
4. Preserve work through change. Stable anatomical identities, surface correspondence, provenance, and explicit invalidation are foundational.
5. Make the editor and agent API operate on the same commands and evidence.
6. Compile expensive reasoning and preparation ahead of time where possible. Avoid reevaluating the entire authored description every frame.
7. Every optimization has a stated domain, assumptions, fallback, quality evidence, and measured cost. Unknown is a valid result; silently losing a feature is not.
8. Build coherent creatures while building tools. Avoid completing an enormous general framework before testing whether an agent can make anything compelling with it.

## 3. Current foundation and limits

This baseline comes from source inspection and existing verification captures, not a new hardware verification run. The shared checkout contains concurrent rendering work; recheck its contracts when implementation starts.

| Area | Present | Missing or materially limited |
| --- | --- | --- |
| Shape | Primitive field composition, Boolean operations, smooth unions, extracted surfaces, source identities | Anatomical structure, rich local shapes, thin surfaces, local refinement, sculpt layers |
| Appearance | Multiple material assignments, metallic/roughness, procedural patterns and bump, two optional layers | Anatomical masks, material families for tissue/eyes/fibers, groom source, directed surface detail |
| Rig and deformation | Joint hierarchy, bounds-fitted templates, automatic envelope weights, four-influence linear blend skinning | Anatomical fitting, influence exclusions/overrides, control rig, IK, correctives, facial rig |
| Motion | Keyframes, quaternion sampling, clip blending, root-motion policies, event markers | Contact tracks, layered graph, trajectory tools, retargeting, performance-oriented authoring |
| Physics | Fixed-step Rapier adapter, character controller, body-level compounds, queries, contact events | Skeletal articulation, secondary dynamics, cloth/fiber solvers, controlled ragdoll transitions |
| Authoring | Typed edits, validation, transactions, recipes, history, source revisions, captures and diagnostic channels | Creature-domain commands, spatial grounding across systems, scenario review and candidate comparison |
| Compiler | Separable product keys, caching, field IR, fidelity diagnostics, cooked products, new render-product metadata | Shared anatomical field IR, character realization alternatives, groom/pose products, coherent character LOD |

Specific limitations must remain visible during planning:

- `snow-fur` is procedural surface appearance, not actual fur.
- Character compounds are attached to one rigid body; they are not a ragdoll.
- Current joint limits clamp authored angles, not anatomical constraint solves.
- Template rigs are fitted to bounds, not recognized landmarks.
- Character extraction remains a uniform grid, capped by source/quality settings. Across a three-metre axis, 80 cells means 3.75 cm sample spacing, not guaranteed surface accuracy.
- Static analytic primitive rendering does not establish a method for articulated skin, blended anatomy, or deforming hair.
- Existing local rendering measurements do not establish a budget for the proposed creature or support for untested hardware.

Implementation anchors: [documents](../../packages/model/src/documents.ts), [contracts](../../packages/model/src/contracts.ts), [surface compilation and binding](../../packages/compiler/src/surface.ts), [field IR](../../packages/compiler/src/ir.ts), [product keys](../../packages/compiler/src/products.ts), [render products](../../packages/model/src/render-products.ts), [animation](../../packages/runtime/src/animation.ts), [physics](../../packages/runtime/src/physics.ts), [commands](../../packages/authoring/src/commands.ts), and [session](../../packages/authoring/src/session.ts).

## 4. What an agent needs to do excellent work

An agent needs to reason about the creature, act precisely, see what happened, and retain what it learned. More exposed parameters alone do not provide that loop.

| Need | Required tool behavior |
| --- | --- |
| Understand the design | Inspect body plan, landmarks, relationships, intended proportions, expressive goals, constraints, and reference views |
| Find the relevant control | Resolve a visible patch or motion failure to anatomical regions and the source operations affecting them |
| Make deliberate changes | Edit an interpretable parameter, curve, field, pose, or local stroke with explicit scope and preservation constraints |
| Explore alternatives | Produce reproducible candidate variants without overwriting accepted work; compare matched views and motion |
| Delegate safely | Exchange bounded tasks with read/write dependencies, allowed changes, acceptance scenes, and evidence |
| Solve tedious coordination | Propagate a proportion edit through bindings, attachments, and motion constraints; report unsatisfied relationships |
| Judge outcomes | Obtain beauty captures, motion sequences, isolated channels, localized diagnostics, and hardware costs |
| Diagnose failure | Explain which source dependencies contributed to a pixel, contact error, bad deformation, or expensive pass |
| Accumulate expertise | Save parameterized recipes, corrective examples, rejected candidates, and reusable review scenarios |

A natural-language brief may guide the agent, but should not be the executable authoring format. The executable result is a typed, editable proposal with explicit parameters, constraints, source dependencies, and evidence. Avoid hiding artistic decisions inside an opaque prompt-to-creature service.

## 5. A coherent creature description

### 5.1 The source domains

Represent a creature as linked source domains with stable identities. Start with the subset needed by the first fixture; do not immediately turn every concept into a new document type.

| Domain | Authored meaning |
| --- | --- |
| Design | Intended silhouette, scale, proportions, mood, important features, reference views, protected design decisions |
| Anatomy | Body regions, landmarks, symmetry, bones, articulation, mass relationships, tissues, surface frames |
| Shape | Primary volumes, swept cross-sections, patches, creases, displacement layers, explicit local corrections |
| Appearance | Region masks, orientation fields, material families, scales/pores/wrinkles, damage, dirt, wetness |
| Groom | Growth regions, guides, layers, density, length, clumping, curl, taper, coloration, physical response |
| Rig | Controls, chains, limits, influence fields, exclusions, correctives, attachments, facial controls |
| Performance | Poses, curves, trajectories, contacts, expressions, clips, layers, transitions, semantic events |
| Physical behavior | Mass distribution, proxies, constraints, damping, collisions, simulation ownership and blending |
| Review | Cameras, lighting, scenarios, protected features, tolerances, budgets, accepted comparisons |

Support nonhuman topology from the beginning: named limbs and chains rather than a fixed biped schema. Body-plan templates are reusable recipes. Missing limbs, extra jaws, wings, tentacles, and intentional asymmetry must not require fictional human joints.

### 5.2 Fields beyond surface distance

Use typed scalar, vector, and selected tensor-valued fields over well-defined domains. A field has units, coordinate space, support, dependencies, evaluation stage, and a declared meaning.

Examples include tissue thickness, fur density, preferred fiber direction, deformation stiffness, muscle activation, material coverage, contact suitability, semantic importance, and pose-dependent wrinkle amplitude. An implicit surface value is not automatically a signed distance; a direction is not a displacement; a rest-space field is not a world-space field.

For a material point `X` in the rest creature and pose/state parameters `q, s`, separate:

- rest geometry and attributes evaluated at `X`;
- the deformation mapping `x = D(X, q, s)`;
- pose/state modifiers such as compression wrinkles or wetness;
- runtime products compiled from those definitions.

Start with bounded, typed operators and trusted compiler implementations. Versioned source remains data rather than arbitrary executable scripts.

### 5.3 Persistent surface correspondence

An anchor references an anatomical region, a local chart or feature coordinate, and an attachment rule. It must not depend solely on generated vertex or triangle indices. Generated geometry stores enough reverse mapping to recover the source anchor and region.

For the first implementation, support robust charts on swept forms and explicit patches, with bounded projection onto neighboring compatible surfaces. Record residual, confidence, and ambiguity. General implicit blends may not have globally unique coordinates; use local overlapping charts and explicit seams instead of pretending they do.

Changes have distinct policies:

- A proportion change can transport an anchor through its authored chart.
- A local sculpt can reproject within a bounded anatomical neighborhood.
- A topology change can invalidate an anchor and request repair.
- A deleted region cannot silently transfer its scars, fur, or hitboxes to a nearby unrelated surface.

Material regions may overlap and blend. Physics and control ownership require their own explicit resolution rules. One convenient hierarchy must not impose false semantics on every system.

## 6. Shape authoring and local precision

Implement a compact, composable vocabulary:

1. Swept volumes with editable cross-section profiles, taper, curvature, and twist for limbs, necks, horns, tails, fingers, and tendons.
2. Anatomical masses related to landmarks and bones; blend width and attachment shape are explicit controls.
3. Thin surface patches for ears, eyelids, lips, membranes, cloth, and wings, including orientation and optional thickness.
4. Feature curves for creases, scars, mouth rims, tendons, and material boundaries.
5. Local sculpt/displacement layers with surface selection, falloff, signed magnitude, direction, symmetry policy, and preserved constraints.
6. Independent attached components for eyes, teeth, claws, armor, straps, and accessories.

Keep low-frequency form, medium-scale anatomy, and high-frequency detail separately editable. Changes to pores should not rebuild the skeleton. Enlarging a chest should not erase a deliberately authored scar.

The compiler should allocate representation effort by feature requirements and spatial support. Evaluate adaptive extraction, patch tessellation, and mixed realizations against the current extraction baseline. Thin lips and claws must have explicit feature-preservation requirements; raising a global resolution cap is not a complete solution.

For animation, preserve a stable runtime surface/correspondence through the supported pose domain where practical. Remeshing every frame risks identity changes, unstable normals, and flicker. Pose-specialized products need explicit transition and correspondence rules.

## 7. Appearance as structured, shared fields

Build material families for hard opaque surfaces, skin/soft tissue, eyes/wet surfaces, cloth, and fibers. Give each a small, legible parameter set with meaningful units and supported domains.

Skin needs thickness-aware transmission/subsurface approximations, spatial roughness, and region-dependent detail. Eyes need coherent cornea/iris appearance, eyelid contact, gaze, wet boundaries, and stable highlights. Cloth needs direction and suitable sheen; horns and claws need growth-aligned structure; metals need intentional wear and roughness variation.

Provide field operators for anatomical region masks, distance to feature curves, growth direction, rest-space curvature, thickness, exposure, contact history, and authored localized marks. Curvature or exposure generated from an approximate surface must carry that dependency and uncertainty.

Preserve correlations: a scar can change color, roughness, local shape, and fur growth together; a scale field can drive geometry, orientation, and material variation; compressed skin can modulate wrinkles without unrelated color noise. These should share a source recipe rather than separately seeded approximations.

The compiler chooses whether a region's appearance is evaluated directly, partially evaluated, baked into a local texture/atlas, or approximated through a filtered response. Derived textures remain reproducible products with source coordinates, invalidation, filtering, and versioned recipes. Support external reference/import paths when useful without making opaque imported data the only way to achieve detail.

For subpixel pores, scales, and fibers, investigate compiling distributions and lighting responses instead of retaining every geometric element. Averaging normals or albedo alone cannot preserve correlated nonlinear shading. Extend the rendering research only where the assumptions hold; do not assume water's phase-response derivations automatically apply to hair or tissue.

## 8. Groom authoring and realization

Source controls: growth regions, guides, direction fields, length, density, width/taper, clumping, curl, frizz, partings, root/tip color, sparse patches, underfur, guard hairs, manes, brows, and whiskers. Include intentional asymmetry and manually placed hero tufts.

Generate guides deterministically from anchored regions and authored flow; permit guide editing and local overrides. The visible coat and simulated guide set are separate products derived from the same source. Bind them through persistent surface coordinates, with explicit collision and attachment policies.

First evaluate generated cards/tufts and simplified guide dynamics for the reference creature. Cards require alpha coverage, fiber-aware shading, stable tangent frames, shadows, and filtering; they are not free merely because they contain fewer polygons. Measure overdraw and shadow cost. Evaluate shells/fins for appropriate short coats and strands for regions where they visibly earn their cost.

LOD must preserve silhouette, apparent coat volume, directionality, color variation, and shadow/transmission behavior. Derive transitions from a shared density/orientation description and test them in movement and backlighting. Reduced guide simulation must preserve large-scale motion; teleport and resume behavior must be explicit.

The compiler opportunity is a single groom source with several affordable realizations, not a requirement to draw every authored hair.

## 9. Rigging, deformation, and face

Fit rigs from authored landmarks and chain relationships. Add per-axis or swing/twist limits, pole preferences, anatomical influence restrictions, rigid regions, twist distribution, and editable local weight fields. Distance-to-bone remains a baseline, not an anatomical truth.

Compare improved linear blending, dual-quaternion options, and local corrective fields in representative poses. Dual quaternions are not a universal cure for tissue behavior. Pose-dependent correctives must remain authorable, inspectable, and transferable through source correspondence.

Initially represent muscles as authored masses and activation/pose-driven deformation. Support breathing, tension, jaw opening, and local volume correction without requiring a full biomechanics simulator. Restrict correction support to the intended anatomical region.

Define facial controls around meaningful actions: gaze, blink, squint, brow compression, lip curl, jaw opening, nostril motion, and snarling. Couple lips and eyelids to their contact boundaries; include teeth, tongue, and oral surfaces where the creature exposes them. Preserve asymmetry and combinable expressions.

Review sweeps should expose shoulder collapse, twisting artifacts, jaw/cheek failure, eyelid penetration, volume loss, and attachment separation. Geometry alone does not determine whether a deformation is artistically good; show the pose, silhouette, and localized measurements together.

## 10. Performance authoring, IK, and contacts

Make motion source describe intent at several editable levels: key poses, control trajectories, timing curves, contact intervals, expression/attention tracks, semantic events, and exact joint overrides. Preserve the ability to author anticipation, asymmetry, menace, hesitation, effort, impact, and recovery.

Start with two-bone limb IK, chain/aim controls, foot orientation, pelvis compensation, and explicit contact planting/release. Add multi-effector/full-body solves when actual poses expose a need. Report target residuals, joint-limit violations, reachability, and conflicting constraints.

Locomotion joins stride, phase, root trajectory, pelvis/spine motion, foot support, and terrain adaptation. IK alone cannot repair an incoherent gait. Contact constraints must distinguish stance, intentional sliding, impact, airborne motion, and release. High-speed motion needs collision/sweep treatment, not just endpoint tests.

Compile an explicit animation/control graph for clip layers, additive motion, masking, transitions, root authority, IK, and events. Contact synchronization and inertial transition techniques should be introduced where they improve the tested starts/stops/turns. Preserve event timing and collision authority through blends and interruption.

Support reusable pose and motion recipes, clip import with provenance, retargeting through anatomical correspondences, and local correction after retargeting. Generative motion can later supply candidates; it does not bypass readable timing, editable source, contact checks, or gameplay review.

For an attack, author anticipation, commitment, trajectory, hit interval, follow-through, recovery, target-adaptation limits, and interruption rules. The creature tools expose the tracks and attachment transforms; the game owns damage rules and decisions. Validate a real encounter rather than declaring success from a looping animation.

## 11. Physical behavior and ownership

Represent explicit movement authority and scheduling. Gameplay/root integration, authored pose, constrained adaptation, articulated response, secondary simulation, corrective deformation, and render extraction must have a defined order, spaces, and feedback boundaries.

Separate:

- authored performance and gameplay movement;
- foot/hand/gaze constraints;
- skeletal rigid-body articulation and controlled ragdolls;
- secondary chains for tail, ears, mane guides, straps, and accessories;
- deformable cloth and membranes where simpler chains are inadequate;
- pose-dependent tissue effects, with more expensive soft-body behavior deferred until justified.

Provide mass distribution, stiffness, damping, limit, attachment, collision exclusion, and animation/physics blend fields or parameters. Build simple secondary chains before general cloth. Ragdoll entry and recovery require pose/velocity transfer and explicit root/controller ownership. Define pause, replay, save, teleport, dormancy, and reactivation semantics for every simulated subsystem.

Derive simplified collision proxies from anatomical regions with explicit approximation status. Render, contact, and gameplay hit volumes have different purposes and may differ, but their mappings and expected discrepancy must remain inspectable.

Cross-platform bitwise determinism is not assumed. Record seeds, ticks, solver versions, and initial state; use declared tolerances for repeatability. Store necessary checkpoints when simulation cannot be reconstructed from source and time alone.

## 12. Make the compiler a creative instrument

### 12.1 Shared analysis, multiple products

Extend the existing product model incrementally. The dependency graph should distinguish anatomy/charts, geometry, region masks, appearance, groom, binding, correctives, motion, contact plans, collision, simulation, and review evidence.

Compile a creature into reusable products rather than a monolithic opaque asset:

| Product | What the compiler can exploit |
| --- | --- |
| Geometry and charts | Local support, minimum features, curvature, seams, rigidity, deformation domain |
| Appearance evaluator or bake | Rest-space stability, frequency, material correlations, pose/state dependencies |
| Binding and correctives | Region exclusions, sparse support, repeated rig structures, bounded pose domains |
| Groom representations | Shared guide structure, density/direction fields, projected coverage, important silhouette tufts |
| Contact/control program | Named chains, contact phases, sparse constraints, fixed relationships |
| Collision and attachments | Anatomical occupancy, allowed overlap, rigid components, required clearance |
| Runtime detail variants | Protected features, projected size, pose relevance, cost and memory observations |

Carry source identity, algorithm version, units/domain, dependencies, assumptions, measured/bounded/unknown errors, resource ownership, fallback, and invalidation rules. Reuse the existing render-product concepts where they fit; keep solver convergence evidence distinct from a rendering error bound.

Partial evaluation should separate rest-static, per-creature, per-pose, per-state, per-frame, and per-pixel work. Compute shared fields once when profitable. Region support and dependency analysis should let a mane edit avoid recompiling teeth or a gait change avoid rebuilding skin appearance.

Dynamic products require conservative or explicitly unknown deformation bounds for culling and shadows. A bound valid at rest must not remove an extended limb. Alternative representations must retain picking, depth, shadows, identity, and source correspondence as well as beauty.

### 12.2 Constrained inverse authoring

Provide a bounded edit solver over selected source parameters. The agent specifies objectives, hard preservation constraints, permitted controls, parameter ranges, and a work budget. Begin with finite-difference sensitivities and small systems; add analytic derivatives or automatic differentiation only for operators where it improves the measured loop.

Examples:

- Widen the chest in front view while preserving head size, forelimb reach, and armor clearance.
- Move the shoulder silhouette toward an authored outline across three views.
- Reduce planted-foot slip while retaining attack duration and the accepted anticipation pose.
- Reduce groom rendering cost while preserving selected mane silhouette samples and coat coverage.

The solver returns candidates, residuals, active constraints, unidentifiable parameters, and unsatisfied objectives. It never quietly relaxes a hard constraint. Multi-view constraints limit but do not eliminate 3D ambiguity. Discontinuous visibility, topology changes, and arbitrary artistic quality are not assumed differentiable.

An agent can direct a low-dimensional search far more effectively than guessing hundreds of unrelated coordinates. The durable output remains ordinary editable source.

### 12.3 Compile explanations and sensitivities

Add `explain` queries that connect observations back to source:

- Which regions, material layers, groom coverage, and lighting inputs affect this pixel?
- Which bones, weights, and correctives move this surface patch?
- Which motion/contact controls contribute to this slip interval?
- Which representation and pass dominate this creature's measured cost?
- Which authoring changes would invalidate the accepted evidence?

Dependency-based explanations come first. Sensitivity estimates are a later bounded experiment and must distinguish correlation from causal effects. Counterfactual captures—disable one layer or vary one control—provide stronger evidence than plausible text generated without measurements.

### 12.4 Continuous authored detail, discrete runtime choices

Author feature importance and scale continuously, then compile geometry/appearance/simulation products for actual use. A horn tip, eyelid, or distinctive mane can receive protected status; a dense unseen undercoat can receive lower priority.

Test pose-aware detail for faces and joints, field-derived normal/roughness distributions for tiny detail, and shared regional deformation products for repeated structures. Any pose envelope or appearance approximation must specify where it is valid and fall back outside that domain.

This is an opportunity to exceed conventional workflows: one source can coordinate geometric detail, material filtering, coat density, simulation frequency, and shadow complexity. A visually coherent reduction is more valuable than independently decimating each asset.

Do not assume a universal field renderer or a general inverse-deformation ray query is required. Deformed-field intersection can be difficult and ill-conditioned. Test restricted rigid or well-conditioned regions; retain conventional compiled surfaces as valid products.

## 13. Agent and human interface contracts

### 13.1 Tool families

The following names are proposed API families, not installed operations:

| Family | Example capabilities |
| --- | --- |
| `creature.inspect` | Body plan, regions, controls, dependencies, budgets, unresolved diagnostics |
| `creature.select` | Semantic selector or capture-space selection resolved to anchored source regions |
| `creature.edit` | Proportion, sweep, patch, local stroke, mask, attachment, and preservation constraints |
| `creature.solve` | Rig fitting, binding repair, contact motion, constrained parameter fitting |
| `creature.groom` | Growth/flow fields, guides, layers, local exceptions, realization settings |
| `creature.perform` | Poses, expressions, trajectories, contacts, clip layers, transitions, events |
| `creature.evaluate` | Review scenarios, metrics, captures, pose sweeps, performance workloads |
| `creature.compare` | Matched evidence for source revisions/candidates, deltas, protected-feature regressions |
| `creature.explain` | Source attribution, invalidation, constraint conflicts, measured cost explanations |

Implement these through the current typed session, headless tools, and `window.wrela` surface. Extend discovery with schemas, units, supported operator versions, limits, examples, and known approximation domains. The human UI should expose the same concepts with handles, surface strokes, curves, contact timelines, and comparison views.

### 13.2 Proposal and job lifecycle

An edit or solve request includes base revision/dependency preconditions, target regions, intent, allowed changes, protected constraints, seed where applicable, budget, and review scenario IDs. Large outputs such as generated guide arrays use content-addressed artifacts rather than enormous conversational responses.

Long operations return a job identity with progress, cancellation, deadline, and terminal status. A job produces an immutable candidate tied to its input revision; completion does not silently replace newer source. Adoption is an ordinary validated transaction. Retrying the same logical request is idempotent.

Candidate artifacts include editable source changes, dependency changes, solver diagnostics, previews, cost observations, and provenance. Independent candidates share unchanged products. Explicitly bound retained variants, caches, review output, and temporary artifacts; expose lifecycle and cleanup.

### 13.3 A concrete authoring exchange

Example intent: "Make the shoulders more imposing while preserving the face and the accepted lunge."

1. Inspect the shoulder region and its dependent skin, groom, armor, and motion controls.
2. Select the accepted face/silhouette views and lunge scenario as protected review evidence.
3. Propose bounded changes to shoulder width, scapular mass, and mane volume; preserve limb length and attachment clearance.
4. Compile affected products and solve only explicitly permitted adaptation, such as armor offset or local binding corrections.
5. Run the pose sweep and lunge, compare the same cameras/ticks, and report any violated constraints.
6. Refine or adopt the candidate; retain the reason and evidence with the transaction.

This exchange is the desired unit of creative work. The agent should not need to manually rediscover every downstream artifact that a proportion change can break.

### 13.4 Collaboration and reusable expertise

Define a task packet containing source/dependency identities, allowed regions and controls, desired result, preserved constraints, scenario IDs, and evidence requirements. A material specialist and a motion specialist can work independently when their read/write sets allow it. Shared anatomy edits require explicit reconciliation; do not infer safety from different filenames.

Successful tasks can become parameterized recipes with examples and domains. A scar recipe may include shape, material, and growth suppression; a gait recipe may include contacts, trajectories, and constraints. Retain local overrides through recipe refresh and expose conflicts.

Design notes and rejected alternatives are retrievable by region or goal. Keep machine-measured facts, human preferences, and agent hypotheses distinguishable. Accepted taste decisions are constraints or references, not fabricated physical truths.

## 14. The creature review stage

Build the review stage early and grow it with each capability. Reuse current revision-aware preview, capture completeness, camera presets, source identities, fixed ticks, and hardware manifests.

Required scenario families:

| Scenario | Evidence |
| --- | --- |
| Static identity | Front/side/back/three-quarter silhouettes; neutral clay; approved proportion overlays |
| Appearance | Neutral, raking, backlit, bright and dark environments; isolated material channels; face/paw/mane close-ups |
| Deformation | Joint and expression sweeps; shoulder/hip/jaw/eyelid views; stretch and influence diagnostics |
| Locomotion | Start/stop/turn, speed changes, slope and step traversal; contact overlays and root trajectories |
| Combat | Anticipation/strike/recovery, target offsets, interruption, hit response, and actual gameplay interaction |
| Secondary dynamics | Wind, acceleration, abrupt turning, impact, pause/resume, teleport, and reactivation |
| Detail transitions | Camera travel and changing pose across geometry, material, groom, shadow, and simulation transitions |
| Delivery | Cooked/exported runtime, replay/save where supported, bounded resource use, representative actor counts |

Capture sequences or contact sheets as well as single images. Support synchronized candidate playback, fixed lighting/exposure, exact tick selection, region crops, difference views, and beauty/diagnostic toggles. A changed camera or exposure must not masquerade as an improved creature.

Metrics include planted-contact velocity/error, unintended penetration, joint-limit residual, attachment separation, local deformation stretch, silhouette change, coverage change, temporal instability, compilation latency, edit-to-visible latency, CPU/GPU frame distributions, owned memory, and groom overdraw/pass cost where attributable.

Each diagnostic identifies source revision, region/control IDs, scenario, tick/time interval, coordinate frame, value/units, severity, and evidence links. Comparisons identify expected intentional changes separately from regressions. Some overlaps are intentional; contact and penetration rules must be authored rather than guessed globally.

Beauty approval remains a recorded human/agent judgment, with reasons and reference views. Automated acceptance must not claim to prove originality, emotional expression, or fun.

## 15. Experiments that can change the design

Run the cheapest useful experiment before expanding a system that depends on an uncertain assumption. Preserve source, controls, outputs, failures, and conclusions under `docs/research/` with bulky reproducible evidence under `output/`.

| Experiment | Question and controls | Promote when / change direction when |
| --- | --- | --- |
| E1: Surface anchors through edits | Attach scar, groom roots, and armor anchors to a shoulder; change proportions, sculpt locally, then change topology. Compare semantic charts to naïve nearest-surface projection. | Promote charts if local edits preserve intended regions with bounded error and topology ambiguity is reported. Add explicit repair tools where correspondence cannot be established. |
| E2: Shape vocabulary and realization | Author the same jaw/eyelid/claw/shoulder using current primitives and proposed sweeps/patches; compare effort, silhouette, thin-feature preservation, deformation, and compile cost. | Promote operators that demonstrably increase control or quality. If pure implicit composition is cumbersome, use authored patches/explicit surfaces within the same semantic source. |
| E3: Deformation | Compare envelope binding, region-constrained binding, and corrective fields across the same pose suite; include nearby unrelated limbs and asymmetric anatomy. | Promote a technique for visible pose quality and edit robustness at acceptable cost, not a lower numerical energy alone. |
| E4: Groom representations | Compile one coat to cards/tufts and a second plausible realization; compare silhouette, backlight, shadows, motion, transitions, and full-frame cost. | Choose per-region products from evidence. Retain a simpler implementation if more advanced fibers do not improve the actual creature enough. |
| E5: Inverse editing | Solve shoulder-width and foot-slip tasks from bounded controls; compare to direct agent edits using the same acceptance scenes. | Keep solving if it reduces correction cycles and preserves constraints. Expose ambiguity and fall back to direct edits when objectives are unstable or underdetermined. |
| E6: Compiled microdetail | Compare direct dense scale/fiber detail, conventional filtered products, and a field-derived response approximation under moving light/camera/pose. | Promote only within measured error/cost domains; preserve orientation and material correlations. Reject temporally unstable or appearance-changing approximations. |
| E7: Whole authoring loop | Perform a fixed set of cross-system design changes on the first creature, then repeat on a second body plan. | Count edits completed, downstream repairs, active agent/human time, latency, and retained quality. Redesign abstractions that merely move manual labor elsewhere. |

Do not block the first complete creature on E5 or E6. They are promising compiler advantages with conventional editable fallbacks. E1–E3 directly constrain the first architecture and need early answers.

## 16. Implementation sequence and acceptance gates

No calendar estimates are assigned before the uncertain parts are measured. Each phase is a coherent integration milestone, not permission to build every possible feature in its category. All phases start unimplemented.

| Phase | Deliverables | Dependencies | Exit evidence |
| --- | --- | --- | --- |
| P0: Baseline and review | Creature design brief; fixed review cameras/lights; current bunny baseline; first shoulder/head fixture; scenario/report schema | Existing capture/runtime | A source revision can reproduce static and moving evidence; baseline limitations are recorded without changing the art to hide them |
| P1: Anatomy and correspondence | Region/landmark/frame/anchor contracts; sweep and patch prototype; spatial selection; transactional region edits; E1/E2 | P0 | Scar, material region, and attachment remain meaningfully located through declared edits; ambiguous changes fail explicitly |
| P2: Form and rig | Local sculpt layers; adaptive/local surface realization; region-aware binding; limb IK; pose-corrective prototype; E3 | P1 | Head, jaw, shoulder, and limb meet static and extreme-pose gates; one edit updates geometry and deformation through public tools |
| P3: Surface and coat | Appearance fields/material families; eyes; groom authoring and first runtime products; E4 | P1, stable P2 contracts | First creature is visually compelling at rest and in pose sweeps under several lights; coat and material anchors survive proportion edits |
| P4: Performance | Motion/control graph; contact tracks; gait/trajectory tools; expression; attack authoring; initial retarget path | P2, review expansion from P0 | Walk/run/start/stop/turn and lunge/recovery work on slopes and steps with readable timing and localized contact diagnostics |
| P5: Physical presence | Secondary chains; guide/body collision; articulated reaction/ragdoll where needed; lifecycle handling | P4; P3 groom bindings | Mane, ears, tail, and equipment behave plausibly during acceleration, impact, pause/resume, and reactivation; authority is inspectable |
| P6: Compilation and iteration | Product invalidation refinement; candidate comparison; explanations; bounded inverse edits E5; detail transitions; optional E6 | Working P3–P5 reference | Cross-system changes are faster and preserve accepted quality; complete creature fits declared measured hardware budgets |
| P7: Generalization and delivery | Second contrasting creature; cloth/membrane capability as needed; reusable recipes; migration/cook/export; E7 | First playable creature | An agent authors a second body plan without hidden source hacks; actual gameplay and exported products pass product and technical review |

P3 and P4 may progress independently after their shared anatomy/binding contracts stabilize. Shared-source changes must retain transaction preconditions. Do not compensate for an unresolved foundation by adding more specialized features downstream.

### First creature: a complete tool exercise

Use an original large wolf-like guardian as the initial candidate: an intentional silhouette, sparse mane and short coat, exposed scarred skin, expressive eyes/jaw, claws, a tail, and a small amount of worn armor. The design brief should identify two or three distinctive traits; avoid making a generic animal merely to demonstrate features. This is a proposed test subject, not an immutable product decision.

It must support idle/attention, stalking walk, run, start/stop/turn, lunge, hit reaction, recovery, and an appropriate death state. Review face and shoulder close-ups, and play a short encounter with terrain variation. Add sound/event hooks and feedback sufficient to judge weight and timing in that encounter.

### Second creature: challenge the abstractions

Use a contrasting gaunt clothed biped or another deliberately incompatible body plan. It should expose joint/cloth layering, facial expression, asymmetry, and different gait requirements. Add a small extra-limb or membrane fixture if the second creature does not challenge body-plan assumptions. A second creature made by scaling the first does not establish generality.

## 17. Concrete first implementation backlog

Complete this initial backlog before committing to broad schema expansion:

| Work item | Likely repository area | Required behavior and focused verification |
| --- | --- | --- |
| C01: Capture the baseline | `tools/`, review fixtures, `apps/studio/src/controller.ts` | Reproduce a named camera/light/pose sequence at an exact source revision; include completeness and manifests |
| C02: Region contracts | `packages/model/src/` | Typed stable region/landmark/frame IDs, units, dependency validation; cycle/missing-reference errors; round-trip legacy projects |
| C03: Sweep/patch authoring spike | `packages/model/src/`, `packages/compiler/src/` | One limb sweep and one eyelid/ear patch with source correspondence; exercise thin-feature and orientation cases |
| C04: Anchor transport | Compiler and authoring | Move a scar marker and attachment through a proportion edit; measure residual; explicitly reject ambiguous topology reassignment |
| C05: Region selection and edits | `packages/authoring/src/`, Studio capture/picking | Resolve a capture selection to source; apply a bounded edit with preconditions; undo/retry without duplicated changes |
| C06: Minimal review runner | `tools/creature-verification.ts` and typed fixtures, proposed | Run static views and pose sweeps; emit structured region/tick diagnostics and image references |
| C07: First cross-system demonstration | Shared fixture and recipe | Widen shoulder while preserving a scar anchor, material region, and attachment clearance; adopt/revert candidate through session |
| C08: Record E1/E2 decisions | `docs/research/` | Preserve controls and failures; choose the next operator/realization work from evidence |

These are implementation slices, not instructions to create empty placeholder modules. Introduce code where a slice needs it and keep the existing package boundary rules.

## 18. Repository integration and compatibility

| Package | Responsibility |
| --- | --- |
| `model` | Source schemas, units/spaces, identities, diagnostic/evidence contracts, migrations; no runtime renderer dependency |
| `authoring` | Semantic edits, local strokes, proposals, constraint requests, candidate adoption, recipes, dependency preconditions |
| `compiler` | Field lowering/analysis, charts, products, binding, correctives, groom realization, motion preparation, approximation metadata |
| `runtime` | Authoritative time, control graph execution, contact/IK evaluation, simulation, lifecycle, save/replay, extraction |
| `render-webgpu` | Product realization, deformation, material/fiber shading, transparency/shadows, identity/depth passes, timing |
| `world` | Terrain/contact queries and residency for creatures; no embedded artistic authoring decisions |
| `studio` | Anatomical handles, surface editing, contact timelines, comparison/review, agent API integration |
| `tools` | Headless authoring, cooking, fixtures, evaluation jobs, hardware evidence and reproducible experiments |

Keep old characters loadable and visibly equivalent through explicit source migrations or a legacy adapter. Do not replace the current character schema wholesale in the first slice. Separate source format, compiled product format, solver/operator versions, and runtime-save compatibility.

Cooked releases include every required derived product or an explicit supported rebuild path. Generation must work in the export environment or complete before delivery; never assume a development-only external service is available at runtime. Imported rigs/clips/meshes or reference assets retain provenance and deliberate correspondence mappings.

Source changes invalidate only affected products and associated evidence. Material-only changes should not invalidate motion; topology changes must invalidate dependent bindings/charts unless the compiler establishes a valid transfer. Preserve known-good products while a candidate prepares, and label which revision is visible.

Keep the current bunny game working throughout. Migration, player export, capture completeness, picking, identity passes, origin handling, and device recovery are regression obligations when affected by a slice.

## 19. Budgets and acceptance policy

Set measured budgets in P0 for named device/browser targets and explicit scenes: a close hero, a gameplay encounter, and several simultaneous creatures. The repository's 1080p/60 Hz desktop ambition is a starting target, not a measured promise for this new content. Choose a genuinely lower-power target before claiming broad ordinary-hardware support.

Track full-frame CPU/GPU distributions, owned memory, preparation time, edit acceptance time, edit-to-visible latency, and review completion time. GPU pass timings may overlap; use end-to-end measurements for total cost and matched controls for attribution.

Provisional authoring UX targets to measure and revise: immediate low-cost handle feedback, interactive local previews where possible, and bounded asynchronous higher-quality compilation. Record actual latency distributions before assigning universal sub-second requirements to every solver or groom bake.

Set geometric/contact tolerances relative to physical size and gameplay purpose, and appearance tolerances relative to camera/lighting scenarios. Record concrete numerical thresholds with each fixture before evaluating a change. Do not invent one foot-slip or penetration threshold that works for mice, giants, and intentional sliding attacks.

Required test layers:

1. Numerical/contract tests for spaces, chart transport, influence normalization, constraint solving, and product invalidation where these can fail meaningfully.
2. Scenario tests for contact, pose, simulation lifecycle, source edits, and explicit failure cases.
3. Hardware visual evidence for appearance, shadows, transparency, identity/depth, temporal behavior, and completeness.
4. Comparative art review at fixed views and timings.
5. Actual encounter play and exported-runtime checks.

An optimization compares against a credible implementation at matched visual quality. Adding a capability is judged by the quality it enables and acceptable whole-game cost; it need not beat a renderer that does not render the capability. Compilation cost and storage remain part of the tradeoff.

## 20. Failure modes and decisions we must keep open

- **Semantic labels without useful controls:** demonstrate cross-system edits early; a vocabulary alone is not an authoring tool.
- **Too much shared coupling:** share meaning, but preserve independent local exceptions and explicit dependency boundaries.
- **Correspondence ambiguity:** report and repair; never silently move details across neighboring limbs or topology changes.
- **Procedural sameness:** support deliberately authored shapes, asymmetry, custom strokes, and examples outside the template's distribution.
- **Optimization that changes the design:** protect distinctive features and compare moving, lit results rather than triangle counts.
- **Solvers that consume the iteration budget:** bound work, expose residuals, cache setup, and retain direct editing.
- **Physics that destroys performance timing:** give authored intent and gameplay explicit authority; constrain adaptation.
- **Research that never becomes a tool:** require reproducible agent edits and a finished creature at each major milestone.
- **A tool that succeeds only for its first creature:** test the second body plan before declaring stable general interfaces.
- **A visual illusion of progress:** keep matched lighting/cameras, neutral material views, motion sequences, and an actual gameplay camera.

Keep full muscle simulation, universal differentiable rendering, general deformed implicit ray tracing, photoreal strand rendering everywhere, full-body learned control, and a universal material language outside the critical path. They remain legitimate research directions when a measured limitation warrants them.

## 21. Completion definition and implementation handoff

The program is complete when agents can create and substantially revise two distinct, convincing creatures through documented public operations; their appearance and movement survive the declared review suite; one participates in a satisfying playable encounter; source, derived products, and revisions remain inspectable and portable; and declared hardware budgets are met with retained evidence.

Demonstrate at least these authoring tasks end to end:

- Change proportions while retaining scars, materials, groom placement, and attachment intent.
- Repair a deformation locally without rebuilding the creature or breaking other accepted poses.
- Create and revise a layered coat, including intentional bald/damaged regions.
- Author a readable attack with grounded contacts, expression, secondary motion, and editable timing.
- Explain a visual/motion failure and connect it to a useful source edit.
- Generate, compare, and adopt design variants without losing accepted work.
- Produce cheaper runtime realizations that preserve the creature's defining appearance in motion.
- Reuse a learned recipe on a different creature and expose where adaptation is required.

The toolchain implementation and two study fixtures are now installed; see the [implementation map](creature-authoring-implementation.md) and [retained verification](../research/creature-authoring-verification.md). Continue from the runbook's open clay, movement, play, and hardware gates. Record actual results and update this plan when evidence contradicts a hypothesis. Do not mark later phases complete because their types or UI panels exist.

## References

- [Wrela architecture implementation](implementation.md)
- [Field realization research](../research/field-realization.md)
- [Rendering compiler research and proposed mathematics](../research/rendering-compiler-final.md)
- [Rendering compiler implementation and acceptance status](../research/rendering-compiler-implementation.md)
- [Existing agent operations and runtime conventions](../../README.md)
- [Runtime motion/event/save contracts](../../packages/runtime/README.md)
- [Epic: groom scalability and representations](https://dev.epicgames.com/documentation/unreal-engine/groom-scalability-and-performance-with-unreal-engine)
- [SideFX: generating hair cards from grooms](https://www.sidefx.com/docs/houdini/fur/haircards.html)
- [Epic: goals and solver stacks in IK Rig](https://dev.epicgames.com/documentation/unreal-engine/ik-rig-in-unreal-engine)

The external references establish useful existing techniques, not a ceiling on our design or evidence about Elden Ring's implementation. The novel compiler and inverse-authoring directions in this plan are hypotheses with explicit experiments.
