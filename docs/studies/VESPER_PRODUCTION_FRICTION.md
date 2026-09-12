# Vesper production friction

Working record, 12 September 2026. Earlier review baselines remain intact.

| Artistic change | Obstacle to a direct edit | Reusable capability | Rendered result |
|---|---|---|---|
| Expose a strong ribcage, scapulae, neck tendons and four articulated toes | Existing surface strokes could move the prototype but could not create or cut anatomical structures | Named positive/negative anatomical volumes with source coordinates, skin regions and explicit rebinding diagnostics | Larger paws and anatomical masses read better in first clay frames; separate shoulder seams failed. Connected source removes seams but revealed extraction/binding defects below. |
| Compress and coil a massive body without collapsing shoulders | Linear skin blends lose radius under opposing rotations | Rigid dual-quaternion skinning with exact affine fallback for garment guides | Exact posed renders still show folded triangles; dual quaternions alone do not solve discontinuous weights or poor extraction. |
| Deepen the crouch while preserving paws and delaying the head | Old generator keys were overridden by an active source phrase | Native bounded interval edits operating on actual phrases and planted trajectories | Actual 12-second clay movie has readable compression and turn. Low initial/final stance weakens acting contrast; a taller authored alternative is under review. |
| Replace wire-like mane with broad locks and finer tips | Every guide emitted equally exposed tubes | Authored groom envelopes with fibre detail and existing guide dynamics | First envelopes resembled a rake. Unequal falling collar improves direction, but opaque masses still read as carved tufts; strand coverage is needed. |
| Give the mantle believable construction | Repeated medallions and high-frequency folds dominated its silhouette | Procedural turned hem, seams and a restrained original sigil | Quieter ornament improves hierarchy. The blanket-like mass and arm intersections remain visibly inadequate. |
| Judge anatomy without costume or material camouflage | Isolation selected only one part/subtree at a time | Study-owned clay review and named part visibility, same renderer/deformation/shadows | Native Parts/clay control exercised, actual stripped frames and movie expose shoulder seams and weight discontinuities. Useful and directly improves diagnosis. |
| Keep live authoring within the frame budget | Immutable rig and source work repeated across frames and substeps | Compiled runtime bindings and secondary coefficients with deterministic invalidation | Awaiting live game measurement |
| Join torso and limbs without losing sculpt detail | Repeated conservative smooth-CSG bound expansion turned a five-metre body into a roughly 24-metre extraction domain | Certified tight field enclosure before meshing | CPU audit: enclosure 21.65×22.45×24.09 m → 2.368×3.683×4.516 m; same source128 mesh 8,368 → 376,016 triangles, rest reversed faces6 →0. Actual render and heavier live cost still require review. |
| Keep connected skin continuous under extreme poses | Local primitive bindings plus obsolete spine overrides exceed four influences and change abruptly | Separate source skin blending and explicit influence-loss diagnostics; retire incompatible old fields | CPU candidates reduce reversals but do not yet resolve them. Final finished mesh review remains mandatory. |
| Prevent cloth cutting through moving forelegs | Four old body capsules excluded the limbs; edge-only cloth contact misses interiors | Authored limb/body proxies plus reusable cloth patch interior constraints | Expanded 17-capsule audit now detects 185.6 mm mantle overlap at 4 s. This is a detected defect, not physical success; solver/refinement pending. |
| Read form without diagonal false shadows | Finite-area shadow sampling spread the paired kernel beyond its intended footprint | Continuous finite-source PCF with receiver-plane correction | Diagnostic disabling direct shadow removed stripes; normal shadow restored. Matched fixed-kernel review still needs clean geometry. |
| Preserve and replay the complete character study | Rich source exceeded old 1 MiB pretty-JSON study guard | Bounded 8 MiB codec and compact atomic study writes, with legacy compatibility | Native session restore and save now succeed; first movie's restore failure remains recorded. Exact replay regression pending. |

No row is considered artistically resolved from a successful build or diagnostic alone.

## Resumed native review, 12 September

Preserved incoming work in
`.soundstage/studies/vesper-session-start-20260912.json`. The running app was
built at 22:30 on September 11, before the mesh/skin/face fixes already present
in source. Rebuilt with `scripts/build`, quit/reopened Soundstage, then applied
`author_vesper_anatomy.py` through its public transaction. The incoming study
also retained the obsolete `spine-root` and `spine-chest` weight fields.

| Artistic change | Obstacle | Reusable capability / response | Actual review |
| --- | --- | --- | --- |
| Inspect smooth connected anatomy before adding finishing detail | Running binary predated fixes; incoming study retained superseded weight overrides | Build/source fingerprints, preserved study, existing revision-checked content transaction | Matched bind/crouch PNGs `vesper-current-bind-0-a5998dff.png` and `vesper-current-procession-34-62f56053.png` remove the triangular surface artifacts seen in `vesper-audit-clay-34-8269c352.png`. Neck/shoulder continuity is substantially better. Body masses remain too rounded; motion review follows. |
| Replace the flat cheek disks with an integrated facial structure | The latest face source had not been loaded in the running app | Recompile project-owned field recipe | Actual clay now has a connected nasal bridge, recessed cheek and orbital structure. The eye aperture and muzzle still need close-up review. |

Focused verification: 15 anatomy-continuity, rigid-skinning, production-motion
and study-codec tests passed. These do not establish the visual target. Reviewed
the official Bandai Namco Europe trailer at 1:19 and 1:24, with playback through
the sweep: irregular hair hierarchy and the separate cloth/body arcs remain
useful reference criteria; no reference geometry is imported.

| Artistic change | Obstacle | Reusable response | Actual review |
| --- | --- | --- | --- |
| Shape a less segmented trunk and clear shoulder planes | Round source masses lacked medium anatomical landmarks | `refine_vesper_form.py` uses named anatomy and public sculpt strokes | Clay `vesper-form-0-a87ac147.png` has a more connected abdomen and flatter scapula. Catch-pose folds remain. |
| Inspect one bad instant without recompiling the entire asset | Full-phrase reports rebuilt all geometry and obscured the selected interval | `craftReport` accepts `start`/`end`, including a single pose, and reads the actual compiled preview surface | Native interval/rejection/replay suite passed; current catch can be inspected directly. |
| Push a visible posed shoulder region outward | A bind-space displacement had to be guessed through the rig | `posedCorrective` projects onto the posed triangles, inverts the actual blend, and stores an ordinary corrective field; misses and fold-prone brushes reject atomically | Native selection error 8 micrometres and fit residual 0.12 micrometres. The first displacement increased reversed area from .137 to .149 square metres, so it was removed. Accurate control is established; that artistic edit failed. |
| Smooth the catch-pose shoulder | Increasing global influence overlap spread weights into unrelated regions | Existing local weight fields, placed at the shoulder with explicit neck/upper-arm weights | Global `.35` blend rejected: reversed area .301 versus .137 square metres. Local fields reduced it to .099 square metres, but rear clay still shows a deep fold. These figures belong to the restored older score, before loading the current performance recipe. |
| Give the mane a connected interior and finer outer strands | A single translucent shell appeared as a stippled wedge | Guide-bound nested layers with independent phases and a shorter opaque core | First three-layer render still stipples and is inadequate. Longer coverage and a larger core are under review; no finish claim. |

The current `author_vesper_performance.py` score was reapplied. The apparent
older paw targets were actually authored targets plus persistent `contactOffsets`
(fore ±7 cm/−4 cm, hind ±5 cm); the catch did not change. Preserve `.soundstage/studies/vesper-score-current-20260912.json`
for the next comparisons. None of these studies has been published over the
canonical Vesper source during this resumed review.

| Artistic change | Obstacle | Reusable response | Actual review |
| --- | --- | --- | --- |
| Break broad hair wedges into smaller uneven locks | Each guide owned only one silhouette mass | Envelope `clumps` 1…8, deterministic local widths/lengths, existing guide correspondence | Smaller tips read better, but the first simulated review exposed twisted knots. More subdivisions alone did not solve the hair. |
| Keep the smooth authored mane under simulation | Independent node orientation reconstructed roll from bend constraints | Explicit `curve` frame mode: structural chain neighbours and parallel transport; shared runtime/editor | Matched `vesper-groom-dynamic-03226e27.png` versus `vesper-transported-ruff-feb216e4.png` removes the conspicuous crossed knots at idle. Moving review and remaining collar density still need work. |
| Retain face detail within the runtime budget | Mask/jaw together used roughly 415k vertices; early reduction prevented later sculpting | Recipe-owned rigid reduction runs after source edits with a 1 mm normal-aware error limit; skin bindings reject this rigid policy | Approximately 101k vertices after reduction; close-up preserves the form. Face sculpt initially rejected on the reduced mesh, then succeeded with the correct ordering. Full compile/pose integration and boundary checks pass. |
| Read facial planes rather than noise contours | Uniform contour texture dominated the ivory surface | Existing local finish fields and quieter project-owned material relief | `vesper-quiet-ivory-dd17c8b2.png` removes the scribbled surface appearance. The cleaner image also exposes the still-soft facial design; it is not finished. |

Live studio baseline before the face reduction: native 1920×1080 on Apple M4,
12.60 measured seconds, submitted 60.02 FPS, GPU median 13.82 ms / p95 15.32 ms /
max 16.03 ms. Frame interval p95 16.69 ms / max 25.40 ms. CPU simulation median
9.97 ms / p95 10.75 ms / max 21.80 ms. Normal live secondary updates included;
the studio sky was disabled. `scripts/profile` warms up for three seconds, so
this nonlooping clip measurement includes its later motion and settled hold.
Report: `.soundstage/profiles/vesper-current-live-20260912-071225.json`.
This does not establish the representative game-scene target.

## Agent sculpting workflow, 12 September

The user requested workflow development before further character finishing.

| Artistic change | Obstacle | Reusable response | Actual review |
| --- | --- | --- | --- |
| Reduce the cheek while preserving facial structures | Bind coordinates had to be guessed from an image; nearest-vertex queries could select the wrong surface | Actual-frame pixel picking with barycentric source correspondence and stale-frame rejection | Six visible facial locations selected in 0.44 s. This removes coordinate guesswork, not the need to understand lion anatomy. |
| Move two cheek locations and retain four facial landmarks | Independent brushes required trial-and-error compensation | Bounded compact-field fitting with per-target tolerances, conflict rejection and full-surface displacement limits | 45 mm cheek request fitted; held landmarks within 0.007 mm. The overall face remains wrong. |
| Iterate quickly enough to compare alternatives | Sculpt-only edits repeatedly extracted unchanged anatomy; the native response dropped residuals | Reuse extracted anatomy/actual preview buffers and forward the solver report | Same fit fell from 17.9 to 0.96 s; native residuals now available. |
| Compare 0 / half / full strength | Changing geometry can shift framing and lighting scale; group intensity was absent | Named layers, saved comparison frame, matched renders/replay studies and revision-checked restoration | `.soundstage/studies/sculpt-cheek-alternatives-20260912/` preserves three actual images; source restored exactly. Update times 0.14–0.74 s. PNGs inspected directly; local-file browser viewer was blocked. |
| Protect an area of the eye rather than four isolated points | Point constraints say nothing about the surface between them | Field protection volumes and full protected-vertex displacement report | 9,276 vertices held exactly, but a conspicuous ridge appeared at the mask transition. Trial rejected and removed. Explicit feather width and normal-change reporting are being exercised next. |
| See what is actually protected before committing a sculpt | Numerical mask coordinates concealed the overly broad protected orbital bulge | Native orange influence / blue protection overlay, with exact restoration | `.soundstage/captures/sculpt-influence-diagnosis-00e16765.png` reveals the broad held region. Wider feather softens the ridge but retains the wrong mound; that trial was also removed. |
| Bound distortion while exploring edit requests | Accurate target fits could still create visible creases elsewhere | Explicit maximum normal-change limit, plus `explore_sculpt` for up to eight independently fitted alternatives from one baseline | The 45 mm local-mask request rejects at 52.07° against a 30° limit; 20 mm accepts at 24.78°. Inputs, residuals and images are archived in `sculpt-intent-exploration-20260912`; no trial retained. |
| Frame the clay without hidden mane/costume or old motion-envelope padding | Normal fit used the full recipe bounds | Frame visible deformed vertices and hold the resulting study frame; native Parts button and agent API | Native button exercised. Isolated fixture verifies its 1 m shell is framed while a hidden 8 m form is excluded. |
| Edit the pivot and material shown in the native inspector | Craft overrides were authoritative, but the inspector read/wrote recipe defaults | Route existing authored joints/materials to their actual source records; include all authored joints in parent choices | Native suite confirms actual joint/material changes, no shadowed recipe override and exact undo. |
| Put deformation capacity in a small facial patch | Only whole-part uniform subdivision was available | Bounded conforming local edge refinement with skin/attribute interpolation | CPU/native tests cover continuity, skin interpolation, budget rejection and exact undo. On Vesper, 5 mm refinement took 1.39 s and reduced the selected edge from 8.51 to 4.26 mm. The subsequent scrape took 0.69 s; the face remained wrong. Trial removed; no field detail is recovered by tessellation alone. |

| Change the fields actually responsible for a visible bulge | Selection named one winner, concealing blended contributions and counterintuitive radius response | Optional actual-pixel source explanation: composed coefficients, active/inactive controls and local normal response | Four contributors at the cheek; zygomatic .471 versus masseter .272. Two source alternatives compiled in about 3 s each, but exposed a bad separate brow plate. Both restored without retention. |
| Compare structural source alternatives with sculpt alternatives | Exploration was specific to newly fitted sculpt layers | Shared bounded `explore_edits` for ordinary craft transactions, complete revision-checked craft restoration | Actual Vesper field alternatives archived and source restored exactly. Generic native rejection/replay validation is being extended. |

## Lion reference correction

| Artistic change | Obstacle | Reusable response | Actual review |
| --- | --- | --- | --- |
| Replace the disorienting overall clay form with coherent feline anatomy | Prior work refined local planes while preserving a faulty skeleton and mask/neck relationship | Existing semantic anatomy/joint interfaces; replace the source proportions together, preserve the rejected study | User rejects the current form. Directly viewed Jun Huang flesh/muscle studies and Maria Panfilova front/skeleton studies; replacement review pending. |
| Plant a digitigrade paw below its hock | Two-bone contact treated the terminal joint as the contact point | Optional bind-space `contactOffset`, rotated with the authored foot heading and surface normal before solving the ankle target | Test and moving review pending; this is a kinematic constraint, not simulated physical balance. |
