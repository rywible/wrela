# Sky, clouds, GI, specular (deleted when the work is done)

The ask: all six, each measured (cost; flicker and the clearing's tests) and shown on sheets.
The sun stays where it is.

- [x] 2 3D textures: `std::gpu` type, sampling, loads, kernel writes; both hosts; tests
- [x] 1 Atmosphere (Hillaire 2020): transmittance, multiple scattering, sky-view and (as the
      sun stays put) aerial perspective tables, all cooked at load; the sun's light, `sky()`,
      `hazed()` and the probes' sky and open sky from them; a metered exposure (a hazy sky
      toward a low sun is 10x the rest). Frame 3.42 -> 3.44 ms (noise). Haze: 0.35/km Mie at
      the ground, 0.5 km layer, g 0.76; sun above the air 5.21.
- [x] 3 Clouds as a field marched each frame: a weather map from the cloudscape's cumulus (and
      a gap toward the sun over the meadow), Perlin-Worley and Worley noise in 3D textures,
      the whole field drifting with the wind; a 3072 x 960 dome, a 32 x 32 tile in 64 marched a
      frame; the shadow map cooked once and moved with the field. 0.31 ms + 0.04 scatter.
- [x] 4 Probes: sky and sun-bounce parts apart, the bounce scaled by the cloud shadow; the sky
      and its clouds in the rays; 16 columns baked again a frame (a ray an invocation, 64 rays,
      half kept), paused when the wind is. 0.25 ms.
- [x] 5 Bounces: the rebake's rays read the probes where they land (ground) and light the
      crowns' leaves by the sun and the probes. +0.09 ms.
- [x] 6 Specular: a GGX sun glint, the probes' light toward the reflection (Karis's fit),
      Lagarde's occlusion from each surface's AO; leaves 0.6, far crowns 0.7, grass and
      flowers 0.75, stone 0.8, bark 0.85, ground 0.9. +0.09 ms.
- [ ] Numbers before and after; vision.md and #26; this file deleted
