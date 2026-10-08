# Sky, clouds, GI, specular (deleted when the work is done)

The ask: all six, each measured (cost; flicker and the clearing's tests) and shown on sheets.
The sun stays where it is.

- [x] 2 3D textures: `std::gpu` type, sampling, loads, kernel writes; both hosts; tests
- [ ] 1 Atmosphere (Hillaire 2020): transmittance, multiple scattering and sky-view tables
      cooked at load; aerial perspective per frame; `afternoon()`'s constants, `sky()` and
      `hazed()` from it; the probes bake from it
- [ ] 3 Clouds as a field marched each frame: weather map scrolled by the wind, cooked noise
      (3D), quarter resolution, 1/16 of its pixels a frame; moving cloud shadows
- [ ] 4 Probes: sky and sun-bounce parts apart, the bounce scaled by the cloud shadow; a strip
      re-baked each frame
- [ ] 5 Bounces: traces read the probes where they land
- [ ] 6 Specular: a GGX sun lobe, the sky's reflection, occluded by the probes; per material
- [ ] Numbers before and after; vision.md and #26; this file deleted
