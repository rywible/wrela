# Bounded conformal undercoat experiment

## Actual0212 native rejection

All30 matching PNGs inspected: rectangular raised tiles/basket pattern remains
over smooth skin, with stronger wet plastic highlights. Face/lining and sampled
parent motion remain readable, but fur identity fails. Default stays off; no cost
profile or baseline promotion. Hash archive:
`.build/sanctuary-native-20260912/undercoat-0212-review/review.json`.
Next diagnostic and exact scalar replay are in
`Tools/Experiments/SanctuaryFur/COVERAGE_DIAGNOSIS.md`; no geometry revision follows
from the CPU pass.


## Actual0212 compiled qualification

The current adaptive source passed all four production coat tests in32.455s;
the focused run passed21 tests with zero failures. Actual default retained-vertex
exposure is0.79492533, maximum sampled reduction error0.0009913478m, raw mesh
bytes484,448. All three owned source/test hashes exactly match immutable0212
and the checkpoint. Receipt, copied source and log SHA are retained in
`.build/sanctuary-native-20260912/undercoat-0212-qualification/compiled-receipt.json`.
This supersedes earlier uncompiled/estimated-count status below. Exact adaptive
vertex/triangle counts were not printed; do not infer them from raw bytes. Native
images, material identity and live cost remain unqualified; default remains0.


Authoritative source is `Games/Sanctuary/Project/SanctuarySunhareCoatDesign.swift`,
with saved opt-in/cache integration in `SanctuarySunhareDesign.swift` and the
existing registered Sunhare generator. No precompiled groom asset is required.

Root owns execution. After building the matching source and selecting an isolated
Soundstage workspace, establish the reference:

```sh
./scripts/stagectl project sanctuary
./scripts/stagectl catalog
./scripts/stagectl subject sunhare
./scripts/stagectl pause true
./scripts/stagectl rig softbox
./scripts/stagectl view quarter
./scripts/stagectl shape --set coatStudy 0 --set furStudy 0
./scripts/stagectl saveStudy sunhare-groom-reference.json
./scripts/stagectl capture --label sunhare-groom-reference-quarter
./scripts/stagectl shape --set furStudy 1
./scripts/stagectl saveStudy sunhare-groom-candidate.json
./scripts/stagectl capture --label sunhare-groom-candidate-quarter
```

Repeat front/left under matched noon and indoor light, dry/wet, then the existing
idle/blink/hop clips and saved-study reload. Inspect actual PNGs and native live
motion at0.5–4m. Reject a quilled contour, pasted tufts, exposed roots, blocked
face, ear-lining fibres, temporal sparkle or visible pop. Compare against both
study controls0; localized nap remains an independent optional variable.

CPU gate: `SunhareCoatGeometryTests` in `SanctuaryProjectTests` exercises actual
registered compilation and rig. No tests or builds were run by this author.
The current uncompiled undercoat retains352 patches,352 guides and4 batches.
Adaptive retained rows/columns determine actual counts within16,896 triangles
and1MiB;
root must retain the new compiler receipt. It explicitly throws on failed roots,
invalid controls, >1mm sampled approximation error or budget changes. Cache≤4
recipes/4MiB coat data; steady posing accesses metadata and cached arrays.

Geometry inherits one exact existing parent per batch. Existing kind13 and
`GroomCoverage` use per-vertex analytic coverage through the existing4×MSAA
alpha-to-coverage pipeline. These are overlapping conformal surface patches,
not the historical opaque tubes. They cast no additional shadows. Wet clumping
and volume multiple scattering remain absent. See `UNDERCOAT_HANDOFF.md` for
current source boundary, exact four-test gate and shader assumptions.

Native cost is unmeasured. Retain the provisional incremental GPU p95≤0.20ms,
CPU update p95≤0.05ms gate and perform a matched populated-game follow-up with
normal clouds/wind/cache work. Single short noisy pairs cannot certify sustained
60Hz. The full research report retains the observed2337 surface failure and its
actual studio costs; neither is approval for this geometry experiment.

## Exact bounded matched run

The registered CLI uses `shape --set furStudy 1`; arbitrary semantic controls
are not dedicated command-line flags. The authored JSON uses the equivalent
native `shape` command keys. Nothing in this handoff has been executed against
an app. Root should set `WRELA_CONTROL_ROOT` to the isolated matching app's
absolute `.soundstage` directory, then run:

```sh
./scripts/stagectl replay Tools/Experiments/SanctuaryFur/groom-acceptance.json > "$WRELA_CONTROL_ROOT/groom-acceptance-receipt.json"
```

This produces30 actual captures:12 bind comparisons (front/quarter/left,
dry/wet, fur0/1);12 side hop comparisons at0,.187,.3995,.612,.731,.85s;
four front idle samples at0/2.68s; and candidate-study before/reload. The second
idle time is predicted near the closed lid from the current production blink
function and `sunhare-001` phase; verify the actual blink metadata rather than
accepting the label. Seed17, existing softbox16/fill.18/size.18/warmth.25,
camera2.2761991m/FOV60.1605682°, `coatStudy=0`, default anatomy, no override
roughness/metallic, and real contact solving are retained. The existing shared
look/exposure is not overridden. Source/shader fingerprints and contact
diagnostics arrive in each capture's metadata. The JSON does not save/publish
an asset or accept a baseline.

Before accepting a pair, verify metadata says1920×1080, same MSAA, source/shader
fingerprints, time, physical camera and resolved light position. A differing
resolved framing/light transform invalidates the pair; it is not an art result.
The physical camera is explicitly reapplied after each shape/view change.
The candidate starts from the existing subject's source, so inspect its saved
anatomy/overrides first if the isolated workspace is not fresh.

Classify failures separately:

- Geometry/contact: disconnected roots, broken parent motion, lining/face/sole
  contamination, new exposed seams, base shape changed, or unchanged paws now
  appearing displaced. Check actual mesh/rig CPU gate and paused side strip;
  the revised groom should add16,896 source triangles and four batches only.
- Material/identity: attached geometry is present but reads as quills, sparse
  whiskers, plastic wires or a halo; wet shimmer; absent fur cue at interaction
  distance. This fails appearance even if all geometry tests pass.
- Rendering cost: correct geometry/appearance still must fit the live allowance.
  No new shadow geometry is expected because strands have `castsShadow=false`.

Only after the paused candidate is worth measuring, prepare matching short idle
runs. The profile tool itself resumes, warms3s, clears metrics and retains normal
live cache frames; no manual frozen-sky shortcut is needed:

```sh
./scripts/stagectl view quarter
./scripts/stagectl camera --metres 2.2761991
./scripts/stagectl parameters --wetness 0 --wind 1
./scripts/stagectl creature --mode idle --seconds 0 --seed 17
./scripts/stagectl shape --set coatStudy 0 --set furStudy 0
./scripts/profile --stage --seconds 8 --label groom-reference-live
./scripts/stagectl creature --mode idle --seconds 0 --seed 17
./scripts/stagectl shape --set furStudy 1
./scripts/stagectl camera --metres 2.2761991
./scripts/profile --stage --seconds 8 --label groom-candidate-live
```

Require1920×1080/M4/same MSAA/model state, normal cloud/wind/cache, and record
GPU median/p95/max, CPU simulation/encode p95/max, presented intervals, RSS and
Metal allocation deltas. Accept at most+0.20ms GPU p95 and+0.05ms CPU update p95
provisionally; retain4MiB source-cache cap and measure GPU allocation rather than
equating it with991,232 raw bytes. Repeat in reversed order only if near the
noise/budget boundary. This studio pair cannot replace populated native opening
validation: the latest0017 populated eye-level workload measured GPU
median/p95/max25.576/32.937/37.668ms, RSS max788.563MiB, thermal fair. The old
15.199ms p95 was a downward pitch−0.64 view versus−0.08, with6.46× fewer visible
submitted triangles; it is not representative eye-level headroom. Different
source/shaders and thermal state prevent attributing that delta to groom.
See `.build/sanctuary-native-20260912/groom-parity-0017-populated-profile-analysis.md`.
The incremental ceiling does not qualify the already over-budget populated
scene for60Hz or authorize promotion on studio timing alone.

## Existing2337 attachment check

Reinspected actual front/quarter/left reference PNGs plus left hop.187/.3995/.731
from `local-nap-2337-stage-review.json`. Eyes are seated and dark; ears meet the
skull, head meets neck, and tail meets haunch. Shoulder/hip joins retain visible
creases and the dark floor contact can look detached in these views, but the
sampled images show no new open attachment gap requiring a source repair.
The old hop apex intentionally lifts the paws. No base anatomy edit was made
from this review. These pre-groom images establish what already existed and
cannot validate the unrendered strands, their live motion, or contact shadows.

Compile-ready source archive with per-file SHA256:
`.build/sanctuary-native-20260912/groom-source-20260913-061358/manifest.json`.
It includes the root-owned throwing generator adapter as read-only provenance.
This replay receipt is later than that source archive; model/test source hashes
are unchanged. Next dependent gates are root's matching compiler/tests and
native app. No independent further model change is warranted by this evidence.

The latest source revision follows the actual0017 appearance failure: see
`groom-source-checkpoint.json` for current source hashes and revised topology.
The0017 source/archive and all30 reviewed image hashes remain in
`.build/sanctuary-native-20260912/groom-0017-review/review.json`. The same replay
commands apply. Historical0130 tube geometry passed four compiler tests but failed actual
30-image fur identity review. Current conformal undercoat source has not been
compiled or rendered. The prior pass does not qualify this representation; see
`groom-source-checkpoint.json` and `UNDERCOAT_HANDOFF.md`.

0208 compiled but rejected body patch0 at1.4372791mm. Current correction retains
source rows/columns at the worst-error sample until the same1mm triangular
approximation gate passes. It starts5×3 and may refine to the finite25×9 source
grid, without changing352 source patches or weakening global mesh/cache caps.
Actual total budget and exposure still require the next compiler run; no passed
receipt is implied. Per-guide ranges replace fixed35-vertex indexing in tests.

## Actual0237 calibrated coverage and next source

All23 actual calibration PNGs reviewed. Native unresolved density .25/.5/.75/.94
yields estimated flat coverage .256/.505/.744/.937 and lifted .265/.500/.738/.937.
These are qualified display inversions, not HDR/sample-mask measurements.
Coverage works at interaction distance; raised hard side boundaries remain a
representation defect. The separate4m front planar disappearance occurs even
opaque and is unqualified. Exact evidence, limits and one isolated per-vertex
transverse duty correction are in `Tools/Experiments/SanctuaryFur/COVERAGE_0237_DECISION.md`.
Current source changes only groom.z and actual edge/center test probes; unchanged
geometry/exclusions/parents and defaultoff. New compilation and same30native
images remain required; previous0212 metrics are not a new-source test pass.

## Actual0306 gate

All30 images inspected; all14pairs match, study reload pixels identical.
Four0300coat tests passed32.167s; exact owned source retained0306.
Appearance remains rejected: softer raised spots/dashes over smooth skin, wet
plastic highlights; face and sampled poses retained. No default promotion.
Archive `.build/sanctuary-native-20260912/groom-0306-review/review.json`.
No further geometry change follows this review.
