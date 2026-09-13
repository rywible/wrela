# Reproducible calibration interpretation

Use actual0237 calibration captures and `measure_coverage.py`; do not substitute
the earlier Sunhare image for the isolated patch test. Root reports the three
registered calibration CPU tests passed. All23 actual PNGs are now inspected; measured results and limitations are in
`COVERAGE_0237_DECISION.md`. The retained ROI config supplies real coordinates.

## Match and inspect first

For each analytic mode1 capture select mode2 at the same distance, orbit,
density and wetness. The opaque geometry does not depend on density, so its
.94 frame is also valid for .25/.5/.75 analytic frames. Require identical
camera, scene look, source/shader digest, render size and MSAA. Preserve label,
metadata, image hashes and pixel coordinates for every region. The script
rejects mismatched camera/metadata. Mode0 is ordinary shading and must **never**
be numerically interpreted as coverage.

Inspect the three reference chips and both patches at native resolution. Select
integer `[x,y,width,height]` rectangles inside black,gray(.18),white(1) chips,
then within each patch's root/interior/tip and along its side boundary. Exclude
polygon edges from interior statistics. Interior progress.2–.5 is useful because
the source tip taper has not started. Root is the lower end of the vertical
patch; the tapered tip is the upper end. Right patch is lifted. At grazing angles
keep rectangles away from overlap and do not infer source progress from a
front-view bounding box. If a far chip/ROI lacks uncontaminated interior pixels,
mark that measurement unavailable rather than guessing its value.

Retain the reviewed regions in a JSON file:

```json
{
  "pairs": [{
    "name": "native-front-density094",
    "opaque": "/absolute/opaque-capture.png",
    "analytic": "/absolute/analytic-capture.png",
    "justifiedNoBloomInput": false,
    "regions": {
      "black": [0, 0, 1, 1], "gray": [0, 0, 1, 1], "white": [0, 0, 1, 1],
      "flatInterior": [0, 0, 1, 1], "liftedInterior": [0, 0, 1, 1],
      "flatTip": [0, 0, 1, 1], "liftedTip": [0, 0, 1, 1]
    }
  }]
}
```

The coordinates above are schema placeholders, **not valid capture ROIs**. Fill
them only after reviewing the actual image; save the resulting file under the
matching `.build` evidence directory. Add separate boundary rectangles as needed.

```sh
/Users/ryanwible/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3 Tools/Experiments/SanctuaryFur/measure_coverage.py --config .build/sanctuary-native-20260912/coverage-rois.json --out .build/sanctuary-native-20260912/coverage-measurement.json
```

The script uses bundled Pillow/numpy. Its synthetic transfer inversion check
(`--self-check`) returned maximum error4.064e−11. That validates arithmetic
only; it is not a GPU or visual result.

## Display transfer and linear limits

The actual renderer captures the postprocessed `.bgra8Unorm_srgb` drawable into
an8-bit PNG with alpha forced255. Consequently PNG alpha cannot reveal sample
coverage, and RGB cannot be treated as linear alpha. The source postprocess
adds bloom, applies exposure/warmth/contrast, the rational tone curve, display
saturation and hardware sRGB encoding.

The script decodes sRGB and inverts the known tone/contrast curve. Independent
per-channel white calibration absorbs exposure and warmth; the .18 chip tests
that fit rather than being another free parameter. It rejects a clipped white,
black above2 encoded levels, nonunit display saturation, and gray mismatch above
3 encoded levels. It also requires explicit zero-bloom-input justification.
Diagnostic colors never exceed1, and bloom extraction starts above1; however,
the surrounding scene can contribute bloom. Check that background/materials do
not contain HDR sources, inspect black chips and retain that reasoning before
setting the flag true. Chip consistency alone is not a proof against all spatial
postprocess contamination. If any check fails, output is display-only.

Even successful inversion is a **calibrated display estimate**, not a direct HDR
resolve/sample-mask measurement. Linear readback remains preferable. Do not
change the display grade/exposure to make a calibration pass.

## Decision order

1. Opaque mode must show both exact polygon boundaries and reference chips.
   Broken bounds or unequal vertices across modes are input/geometry failures.
2. Analytic close views should resolve intervals; tips expose black. Far
   interiors should preserve monotone source-density ordering. Compare the
   scalar sweep at the actual projected footprint; the approximate native
   front footprint is3.052 periods/pixel. Density.94 is expected near opaque.
3. Retain mean and p05/p50/p95 across interior ROIs and density ordering. Four
   MSAA samples quantize coverage count into0,.25,.5,.75,1, but hardware mapping,
   spatial dithering and correlated overlap are unknown. Allowing one sample
   (0.25) is only a coarse sanity bound, not precision certification. Require
   large density differences to remain distinguishable. No threshold is changed
   after seeing results; out-of-bound cases become diagnostic findings.
4. Compare flat/lifted analytic boundaries. Both share coverage UVs, so differences
   can arise from depth, projection and derivatives. In ordinary kind13, new
   contrast that is absent in analytic mode implicates shading/source normals.
   Only this controlled comparison can distinguish the raised normal response
   from a missing coverage mask. Boundary contrast alone is not a normal test.
5. If A2C/filter behavior is wrong, repair that measured issue first. If correct,
   retain the0212 coat rejection: centimetre-scale lifted tiles with no transverse
   coverage envelope remain a representation failure. The smallest subsequent
   coat change must target the demonstrated cause, then repeat the same30-image
   anatomy/face/motion gate. No density increase or default promotion follows
   automatically from calibration success.

No renderer, material, geometry or anatomy was changed for this measurement
procedure. Production fur remains off.
