# Vesper reference and initial art critique

Reviewed 2026-09-11. This is an initial critique and authoring brief, not evidence that
new work has reached the visual target. No generated illustration, extracted game
asset, or external authoring application was used. References remain the owners'
work; they inform craft and observation, not a copied character design.

## Evidence inspected

- Wrela's actual [delivery PNG](../../.soundstage/captures/vesper-craft-delivery-5c38f787.png),
  1920×1080, viewed directly. Its corresponding
  [metadata](../../.soundstage/captures/vesper-craft-delivery-5c38f787.json) and prior
  [craft assessment](VESPER_CRAFT.md) retain the existing provenance.
- `Games/Sanctuary/Project/ProcessionalRecipe.swift`, including the skull's blended
  ellipsoids, socket cuts, limb tube profiles, paw primitives, garment grid and guide groom.
- Sanctuary's `Authoring/ArtDirection.json`: readable large/medium/small form,
  organic asymmetry, muted natural masses, selective accents and shared lighting.
  The user's new close-up and performance target raises the character requirement;
  it does not justify compensating Vesper with unrelated exposure.

## Primary references and limits

| Reference | What was actually inspected | Use |
| --- | --- | --- |
| [Bandai Namco America: official Shadow of the Erdtree gameplay reveal](https://www.youtube.com/watch?v=a8k8R0Q2ubY) | Original publisher video in the browser; paused shots at 1:19, 1:20 and 1:24, with playback between them. At 1:19–1:20 the hanging locks and fingers are clear; 1:24 is motion blurred. | Surface hierarchy, silhouette, contrast of material masses and overlap. This brief does not reconstruct its rig or infer exact physical parameters. |
| [Bandai Namco announcement](https://en.bandainamcoent.eu/elden-ring/news/fall-grace-elden-ring-shadow-of-the-erdtree-the-expansion-elden-ring-arriving-june) | Publisher provenance and trailer announcement dated 2024-02-21. | Establishes an official source, rather than fan recreation. |
| [San Diego Zoo: lion](https://animals.sandiegozoo.org/animals/lion), especially its [lion/lioness photograph](https://animals.sandiegozoo.org/sites/default/files/inline-images/lion_lioness.jpg) | Direct browser image: lioness in three-quarter profile and male face behind. | Nasal wedge, cheek plane, paired muzzle pads, chin, neck-to-shoulder transition and mane mass. |
| [Smithsonian National Zoo: lion](https://www.nationalzoo.si.edu/animals/lion) | Institution's physical description. | Compact body, powerful forelegs and jaws; mane variation. This is anatomical context, not a solved fantasy skeleton. |
| [Smithsonian National Zoo video page](https://nationalzoo.si.edu/animals/news/lion-video-update-sept-15-2014), linking to [Wild Inside the National Zoo: Lion Pride](https://www.youtube.com/watch?v=KgGPv1YrDUI) | Original Smithsonian Channel video opened; play and paused inspection at 0:05, 0:30 and 1:01. The first views show playing/resting animals and compressed body poses; the latter is partly obscured by fencing. | Real body compression and natural asymmetry. It is a useful motion reference, but this limited sampling does not establish an adult stride's timing or ground forces. |

The publisher's PDF press release was accessible as text, but its browser image view
was blank; no visual conclusions here rely on that PDF. The US product page presented
an age gate and was left untouched. The RVC movement article returned 403, so it is
not used as substantive evidence. No login or purchase was required for the references
actually inspected.

Observed in the official boss shots: thick grouped locks sit among much thinner
strands; the face and limbs contain intermediate ridges, creases and recesses; cloth,
hair and hard ornament occupy distinguishable masses. The turn shot shows separated
cloth and hair silhouettes. Inference for Wrela: coherent forms at several scales and
different overlap rhythms will matter more than simply increasing strand or triangle
counts. These observations do not establish how FromSoftware implemented the effects.

Observed in the zoo photograph: the nose belongs to a tapering bridge beneath the
inner brows; the muzzle separates into two whisker pads, with a smaller chin beneath;
the side cheek recedes; neck mass flows into the shoulder. Inference for Vesper:
carved mask stylization can retain those structural relationships without replicating
a real lion's proportions or covering a sphere with details.

## Current-render critique, in priority order

1. **Face structure.** The broad cheek remains inflated and spherical. Raised brow
   tubes look attached to the surface; the button nose and shallow eye region weaken
   intent. The open mouth has a regular picket of teeth. The mask needs a continuous
   nasal bridge, carved orbital rim, a broad cheek ridge with a hollow below it,
   distinct paired muzzle pads, and a substantial jaw hinge. Keep a deliberately
   carved mask identity; do not replace the face with generic facial noise.
2. **Weight-bearing anatomy.** The visible legs read as narrow rods with tiny mitten
   feet. Uniform lower-leg taper and weak wrist transitions give no sense of carried
   mass. The nearly black chest reads as an oval opening. Strip mantle and mane, then
   establish pectoral, scapular and neck masses before finish work.
3. **Groom hierarchy.** Pale parallel tubes form see-through fans. Large gaps expose
   cloth through what should be thick mane. A uniform fan of whiskers is decorative
   wire. Establish a dark undercoat and a handful of coherent swept locks, then
   secondary clumps and occasional flyaways; vary root-to-tip color, thickness and
   spacing. Whiskers should grow from varied muzzle roots and occupy different arcs.
4. **Costume construction.** Repeated rings and flower shapes are equally emphatic.
   The long edge and many equally spaced tassels resemble a decorative blanket.
   Give the mantle identifiable shoulder anchoring, overlapping panels, thicker edge
   binding and seam-led folds. Let one restrained motif family carry the design.
5. **Material hierarchy.** Mask, paws and hair share an undifferentiated pale family;
   the body falls almost to black. Establish separate broad roughness/color responses
   under the common softbox and outdoor look. Horn rings are too regular and glossy.

One quarter still cannot assess contact drift, neck collapse, hind-leg articulation,
costume back construction or acting. Those remain mandatory moving and clay reviews.

## Original design direction and compatible edits

Vesper is an aged processional guardian: a quiet, deliberate presence that gathers
its mass before a decisive sweep. The focal point is an ivory mask with recessed
amber eyes. Soot-brown flesh, dark-rooted flax mane and desaturated green textile
support that focal point. Bronze belongs at fastenings and selected edges. Preserve
the existing large horn gesture, but introduce modest asymmetry and irregular wear;
do not add more competing horns or decorative symbols.

| Source region | Concrete next edit within the existing rig | Review that decides whether it helped |
| --- | --- | --- |
| `mask` / `jaw` | Flatten lateral cheek volume; blend a wedge from inner eye to nose; cut a shallow infraorbital hollow; form two muzzle pads and a philtrum; thicken the orbital rim into the skull rather than adding a tube. Retain mask/jaw pivots and deliberate lower-jaw clearance. | Front and side clay close-ups; closed and open jaw; eye region still reads at normal distance. |
| `body`, `neck`, limb upper regions | Add a forward chest keel, paired scapular planes and neck-column volume that transitions into the skull base. Around each existing `restLimb` chain, use broad upper mass, a pronounced elbow/hock region, tapered tendons and a wider wrist. | Mantle/groom hidden; idle and deepest crouch; quarter/back views; continuous shoulder/limb junction without collapse. |
| `*-paw` | Keep each contact pivot and sole height; broaden the metacarpal mass, add four unequal toe knuckles, carve narrow toe valleys, shorten exposed claw length and add pad volume on the underside. Toe roots must join the paw instead of floating. | Near-ground front/side, loaded crouch and diagonal step; feet visibly support the creature and remain planted. |
| `mane` / `beard` | Choose 5–7 principal locks with clear direction and negative spaces; darken roots; layer finer clumps and irregular tip breakup. Beard hangs in fewer broad masses. Avoid filling all gaps with equal-density fibres. | Backlit and neutral views; root attachment and body clearance; motion after chest/head reversal. |
| `mantle` / `trim` | Build an obvious shoulder yoke and overlapping hanging panels; concentrate bronze into fastening/edge zones; reduce identical rings and flowers; use seam-oriented tension folds and heavier hems. | Side/back close-ups; crouch compression; swept turn; no floating ornament or skirt/body cuts. |

These are authored proposals, not measured improvements. Existing joint IDs and
contact anchors provide continuity, but any changed surface must still undergo the
system's correspondence/rebind checks and posed collision/deformation review.

## Art-to-tool friction entries for the next iteration

| Desired artistic change | Current obstacle | Reusable capability needed | Improvement established? |
| --- | --- | --- | --- |
| Hollow the cheek while retaining eye/whisker attachment | Post-mesh offsets alone cannot create a clean new opening or certify attachment transfer | Source-space addition/cut layers, persistent correspondence and explicit rebind diagnostics | No; proposed review above |
| Deepen crouch with loaded shoulders and broad paw contact | Contact points alone do not show flesh compression or contact area | Regional pose correctives, visible posed sculpting, pad/contact-area controls and residual reporting | No; clay motion review required |
| Direct a mane lock rather than a sheet of wires | Density control has no useful art hierarchy by itself | Rooted clump hierarchy, per-lock shape/color/taper, stable detail levels | No; compare silhouettes and moving overlap |
| Make cloth read as a constructed mantle | A continuous decorated surface lacks explicit panel/attachment hierarchy | Reusable panel, seam, binding and attachment source with guide mapping | No; use back and compressed-pose review |

Do not count a new control as resolving friction until its rendered before/after
comparison improves the intended region or interval.
