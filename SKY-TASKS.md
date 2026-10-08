# Sky, clouds, GI, specular (deleted when the work is done)

The ask: all six, each measured (cost; flicker and the clearing's tests) and shown on sheets.
The sun stays where it is.

- [x] 2 3D textures: `std::gpu` type, sampling, loads, kernel writes; both hosts; tests
- [x] 1 Atmosphere (Hillaire 2020): transmittance, multiple scattering, sky-view and (as the
      sun stays put) aerial perspective tables, all cooked at load; the sun's light, `sky()`,
      `hazed()` and the probes' sky and open sky from them; a metered exposure (a hazy sky
      toward a low sun is 10x the rest). Frame 3.42 -> 3.44 ms (noise). Haze: 0.35/km Mie at
      the ground, 0.5 km layer, g 0.76; sun above the air 5.21.
- [ ] 3 Clouds as a field marched each frame: weather map scrolled by the wind, cooked noise
      (3D), quarter resolution, 1/16 of its pixels a frame; moving cloud shadows
- [ ] 4 Probes: sky and sun-bounce parts apart, the bounce scaled by the cloud shadow; a strip
      re-baked each frame
- [ ] 5 Bounces: traces read the probes where they land
- [ ] 6 Specular: a GGX sun lobe, the sky's reflection, occluded by the probes; per material
- [ ] Numbers before and after; vision.md and #26; this file deleted
