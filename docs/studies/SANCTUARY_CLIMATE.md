# Sanctuary climate presentation

Sanctuary samples its climate from saved expedition play time and seed. It never
uses wall-clock time, so a loaded slot and an uninterrupted session choose the
same named sky, weather, wetness, and outdoor ambient floor.

The shared atmosphere provides daylight and cloud-filtered irradiance. The
`outdoorAmbientFloor` is a separate, bounded authoring control for nighttime
readability. The initial capture sweep used `(0.018, 0.028, 0.040)`; its selected
provisional game value is recorded below. Sanctuary ramps its cool floor across
late sunset and dawn, holds it through the night, and leaves it zero in daylight.
It is carried in the existing outdoor uniform lanes and does not rebuild the sky
cache.

This is not a lunar lighting simulation: there is no moon disk, moon shadow, or
claimed physical moon irradiance. It only prevents a moonless procedural sky
from making ordinary terrain and landmarks unreadable. Native captures must tune
the initial values before treating them as approved art direction.

The 2026-09-12 Soundstage comparison recorded in
`.build/sanctuary-native-20260912/global-ambient-experiment.json` found that
raising the shared daylight `ambientStrength` from 1.08 to 1.60 only slightly
lifted the flower and cabin; golden and overcast conditions remained
unreadable. That experiment is not a reason to publish a brighter global scene
look. It motivated a separate, night-only floor for a native capture study.

Soundstage records the matching `outdoorAmbientFloor` in every study. Use:

```sh
./scripts/stagectl outdoorAmbientFloor --red 0.0063 --green 0.0098 --blue 0.014
```

The command accepts each RGB channel in `0...0.25`; old studies omit the field
and restore a zero floor. Test the same fixed subject, camera, sky, seed, and
paused time before comparing captures.

## 2026-09-12 night-floor study

The native Soundstage study swept zero, 0.05, 0.15, and 1.0 times the original
`(0.018, 0.028, 0.040)` candidate on both Moonhart and the construction cabin.
The raw record is
`.build/sanctuary-native-20260912/night-floor-stage-study.json`; the added 0.35
captures are listed in
`.build/sanctuary-native-20260912/night-floor-035-captures.json`. Root inspected
the actual PNGs. Zero was fully black. At 0.15 Moonhart was readable but the
cabin remained very dark. At 1.0 the subjects read as flat, daylit blue against
a black sky. The 0.35 captures made both subjects readable, so the provisional
game candidate is RGB `(0.0063, 0.0098, 0.014)`.

Those captures used the Sanctuary project in a paused Soundstage studio at
1920x1080 with 4x MSAA, `afterglow` preset, time zero, and an authored sky of
altitude `-12`, azimuth `-35`, coverage `0.42`, density `0.8`, haze `0.55`, and
cloud seed `4`. Both 0.35 metadata records report shader digest
`7f55536414b10c04dcf7a28ba822768356f68327da4bec5fa21b3d9c0b6d08d7`, source
digest `3fb0d95ba1d2e836e8ff52c6cc6d019afbf1d3e1ff450e86a94c65e5b01e0664`, and
source revision `e41b252-working-tree`. The renderer check passed on an Apple
M4; its report at `.build/sanctuary-native-20260912/night-renderer-check/report.json`
records source digest `cf9166a615899f5fb0d0c46a63e63bbfa828e643528a9a921343e48ecde1a8eb`,
diffuse irradiance maximum error `0.006604582`, creature deformation maximum
error `3.591767e-07`, and groom coverage maximum error `4.3570995e-05`.

The study confirmed invalid floor input was rejected atomically and that saved
documents replayed exactly (`invalidRejected`, `invalidAtomic`, and
`exactDocumentReplay` are all true). This is a provisional readability setting,
not an approved visual baseline. The fill remains flat and casts no shadows;
there is still no moon disk, stars, lunar transport, or lunar shadows.
