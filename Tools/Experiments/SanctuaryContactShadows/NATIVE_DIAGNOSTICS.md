# Root-operated contact-shadow A/B packet

This packet is ready for root's serial native diagnostics. The CPU experiment is
archived in `docs/studies/SANCTUARY_CONTACT_SHADOWS.md`; do not rerun it or expand
the renderer while the actual comparison remains pending. Sunhare source is
owned separately by the creature author.

## Two distinct patches

`patches/A.patch`: add a **directional outdoor-only 3×3 filter**, using nine
receiver-relative bilinear comparison fetches. The receiver gradient, receiver
bias, raster bias, projection and map stay unchanged. All finite-light paths
retain their original code.

`patches/B.patch`: **outdoor raster slopeScale 1 → 0** only. Constant depth bias
1 and positive clamp .001 remain. Frames without `outdoorShadow` retain slope 1.
No shader source changes in B; this Swift call-site change requires build/reopen.

Both patches are against immutable source
`sanctuary-local-nap-20260912-2337`. Full hashes and readable replacement chunks
are in `patches/manifest.json`. A is not to be combined with B. The runner checks
both affected files before every operation, validates the patch itself, checks
its exact replacement, and uses an atomic file replacement. Any source drift,
including unrelated edits to either file, causes rejection before writes.

| File/state | SHA-256 |
| --- | --- |
| Surface reference/reverted | `f86a24ffadf5309636653828fe01bcfa57ee7932748d922b5d0bc78aff2f9cc4` |
| Surface variant A | `32a86c28ac7b1a690f015d5d2ae57200ea1bd9f2046c9d71a67d29498f4ccc7b` |
| MetalRenderer reference/reverted | `e8216f1147f79d84659ed98a3d525831fe2ebd21c27a466b61401996e91428ff` |
| MetalRenderer variant B | `40b7e440ce1c9292bec652cb18e0bd77477733f68e7d4362d042a4f04ad6fa07` |

Use root-created **disposable copies** of the 2337 source. Do not edit the
preserved 2337 candidate, a player's working source, or a candidate already
sealed for another review. Applying the patch after a candidate is sealed
invalidates its provenance; prepare/build/seal a fresh diagnostic candidate using
the existing harness. These commands deliberately do not clone, build or launch
anything automatically:

```sh
PACK=/Users/ryanwible/projects/wrela/Tools/Experiments/SanctuaryContactShadows
DIAG=/absolute/path/to/root-owned-disposable-2337-copy
python3 "$PACK/variant.py" check A --root "$DIAG"
python3 "$PACK/variant.py" apply A --root "$DIAG"
python3 "$PACK/variant.py" check A --root "$DIAG"
```

For an app explicitly configured to load shaders from that diagnostic copy,
root may hot reload A using the existing `reloadShaders` command, and verify
that the capture shader digest changed. Otherwise build inside the copy. Run the
existing numerical renderer check as root before trusting an edited shader. A
successful reload/check is not the contact/acne acceptance result.

With every other Wrela renderer window closed, the compiled diagnostic
Soundstage executable exposes the existing check directly:

```sh
"$DIAG/.build/Soundstage.app/Contents/MacOS/Soundstage" --project sanctuary --check-renderer
```

For A hot reload on its already-running diagnostic app, use the explicitly
selected isolated control root:

```sh
WRELA_CONTROL_ROOT="$CONTROL" "$DIAG/scripts/gardenctl" reloadShaders
```

```sh
python3 "$PACK/variant.py" revert A --root "$DIAG"
python3 "$PACK/variant.py" check A --root "$DIAG"
python3 "$PACK/variant.py" apply B --root "$DIAG"
# Root builds/seals and opens this separate diagnostic app; shader reload cannot apply B.
python3 "$PACK/variant.py" check B --root "$DIAG"
# After the B run, restore source exactly and rebuild/reopen if reusing that app:
python3 "$PACK/variant.py" revert B --root "$DIAG"
```

Source reversion does not revert an already loaded shader or executable. Reload
the A reference pipeline or reopen the reference executable, then capture its
reported source/shader fingerprints. The original binary is never overwritten
by this packet. `variant.py` is idempotent but always checks the complete known
reference pair; it will not silently rebase patches.

## Exact fixture and paused repeat captures

The latest actual image is
`local-nap-2337-water-construction-rejected-6d09f947.png`, referenced by
`.build/sanctuary-native-20260912/local-nap-2337-water-construction-rejected.json`.
It still shows bench feet visually separated from a broad soft shadow after the
material correction. It uses a different view from 4478337b, so these two images
do not measure improvement/regression magnitude. The Sunhare is visible nearby.

Root launches one isolated Sanctuary testing app for each diagnostic candidate,
with all other Wrela renderer windows closed. The helper below restores the
receipt's actual production snapshot and exact camera, then sets its captured
light/sky/exposure/wetness and complete shared scene look. It requires publication to settle and refuses a camera
mismatch. It does not teleport or use alternate collision/brain functions.

Transient nature/construction previews must be inactive in every comparison.
Start a fresh isolated app, or cancel any active preview using its native control.
The helper checks this instead of silently changing gameplay tools. These are
new matched reference/variant images; the original receipt has a nature preview.
`simulationRestore` intentionally resets the renderer clock/wind, while restoring
the saved production brain. Thus it provides identical new inputs across runs,
not pixel identity with the historical screenshot. No default save is touched;
the production bridge rejects simulationRestore in a non-testing process.

```sh
CONTROL=/absolute/path/to/isolated-session/runtime
RECEIPT=/Users/ryanwible/projects/wrela/.build/sanctuary-native-20260912/local-nap-2337-water-construction-rejected.json
OUT=/absolute/path/to/contact-shadow-native-results
python3 "$PACK/native_capture.py" --control-root "$CONTROL" --receipt "$RECEIPT" \
  --label reference-morning --output "$OUT/reference-morning.json"
# Repeat on the A/B app, changing label/output only:
python3 "$PACK/native_capture.py" --control-root "$CONTROL" --receipt "$RECEIPT" \
  --label A-morning --output "$OUT/A-morning.json"
python3 "$PACK/native_capture.py" --control-root "$CONTROL" --receipt "$RECEIPT" \
  --label A-low-sun --sun-altitude 12 --output "$OUT/A-low-sun.json"
```

Capture reference/B at the same two sun altitudes. The helper captures twice,
records exact PNG-byte and metadata hashes and all capture paths/fingerprints,
and reports the input receipt/snapshot hashes. It validates camera, light, sky,
wetness, exposure, shared look and the saved ambient floor against the actual
capture metadata. The sun/cache flags must be known and coherent. Source/shader
digests must exist and remain identical between the two captures. Failure writes
the report with `comparisonEligible: false` and exits nonzero. It does not discard
the failed images. Two matching PNGs establish paused repeatability only.

The helper does not assert that a digest belongs to variant A/B: root must match
each app's sealed source manifest and loaded shader provenance to the intended
patch. Identical inputs with the wrong executable are not an A/B experiment.
The packet is deliberately pinned to 2337. Later production source (including
0017/0053) may be read for diagnosis but is not automatically patch-compatible.
Use the known disposable 2337 source or prepare and review a separate rebased
packet; never weaken the existing full-file hash checks.

For the Alpine acne regression, root passes its **same saved isolated Alpine
receipt** through the helper for reference/A/B; it must contain `status.state`
and `simulation.snapshot` like the bench receipt. Use both ordinary and low sun
and keep this fixture hash fixed. Do not replace the slope with a flat lab plane.
In Soundstage, load the already reviewed Sunhare hop study unchanged and capture
apex .3995 s, contact .612 s and settle .731 s from the same left view. Verify
flight clearance survives the stronger contact, then inspect actual native
Motion/Behavior playback. No new animation is supplied by this experiment.

## Short live cost and shimmer gate

After restoring the same fixture for each candidate, root may use the existing
profile command. It resumes production simulation, warms up three seconds and
records 18 seconds, then restores the original pause flag:

```sh
WRELA_CONTROL_ROOT="$CONTROL" "$DIAG/scripts/profile" --seconds 18 --counters \
  --label contact-shadow-A
```

Use `reference`/`B` labels on their matching apps. Keep native 1920×1080, device,
temperature and other running Wrela windows comparable. All ordinary live cache
updates belong to the measurement; manual capture/rebuild costs are separately
labelled. Record GPU p95/max, presented interval max, and shadow/scene counter
intervals without summing overlapping stages. Targets are GPU p95 ≤16.7 ms and
presented max ≤33.4 ms. Keep every outlier and compare against the matched
reference, including if neither run meets the target.

After the timed run, use ordinary native mouse look and a short walk/stop/turn
near the bench, then repeat at the Alpine slope. Inspect actual frames for moving
shadow edges, contact crawling and grass acne. This is a qualitative native gate,
not a claim that the timing workload reproduces an identical player route.

Reject a variant if it adds diagonal acne, dark face/ear striping, lost thin
casters, crawling contact, a shadow under the Sunhare during flight, projection
clipping or unexplained regressions. A must improve visible grounding without
unacceptable hard-edge shimmer. B must improve separation without new caster
self-shadowing. Restore the source/binary when either fails. Neither an analytic
gain nor a successful compiler result authorizes baseline acceptance or a new
shadow stack.
