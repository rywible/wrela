# Atmosphere and cloud rendering notes

Reviewed September 11, 2026. These are implementation decisions, not a claim to
have reproduced a published renderer exactly.

## Primary reading

**Sébastien Hillaire, A Scalable and Production Ready Sky and Atmosphere
Rendering Technique, EGSR 2020.**
[Paper](https://sebh.github.io/publications/egsr2020.pdf),
[author's reference implementation](https://github.com/sebh/UnrealEngineSkyAtmosphere).
Read the paper, including the multiple-scattering derivation and limitations.
Earth-radius geometry, exponential Rayleigh/Mie densities and the ozone profile
produce wavelength-dependent transmittance. The transmittance lookup uses
Bruneton's distance-to-top mapping to resolve the horizon. A separate 32×32 LUT
estimates isotropic higher-order scattering using L2/(1−fms), integrated over
64 directions with 20 steps. Sky view directions concentrate samples near the
horizon. The solar disk is evaluated separately from that lookup.

The current code uses kilometres internally, including reciprocal-kilometre
scattering coefficients. The scene uses metres. Mie scattering is .003996/km;
extinction is .004440/km, including absorption. Rayleigh scale height is 8 km,
Mie 1.2 km, ozone peaks at 25 km with a 15 km half-width. Haze scales Mie only.
Direct light and irradiance use the same atmosphere. Radiance is scaled for our
camera pipeline; this is not lux-calibrated. Display exposure meters unobstructed
hemispherical sky luminance with a bounded 1–24× gain at twilight. The user
exposure control multiplies that result. The meter does not depend on camera
orientation or passing clouds, avoiding exposure pumping.

**Andrew Schneider, Nubis³, SIGGRAPH 2023.**
[Presentation and slides](https://www.guerrilla-games.com/read/nubis-cubed).
Reviewed cloud modeling, density profiles, detail synthesis, traversal, lighting
and temporal stability sections. Relevant lessons: build readable cloud masses
before erosion; preserve different billow/wisp scales; avoid evaluating expensive
detail in empty regions; approximate multiple scattering for inner glow; use
sampling strategies appropriate to distance. Nubis³ uses fluid-simulation-based
authored voxel clouds, compressed SDF traversal and procedural detail. It does
not establish that live thermodynamic cloud simulation is cheap. Our ground-view
cloud layer does not need Nubis's flight-through-cloud voxel machinery yet.

**Condor et al., Don't Splat your Gaussians, TOG 2024/2025.**
[Paper and project](https://arcanous98.github.io/projectPages/gaussianVolumes.html).
Reviewed the representation and analytical transmittance approach. Compact
Gaussian/Epanechnikov scattering primitives are interesting future field
representations. Their path-traced examples are not evidence of 60 Hz on M4;
this is not the current implementation.

**Alexander Mueller, Smolder, SIGGRAPH 2026.**
[Course and slides](https://advances.realtimerendering.com/s2026/index.html).
Reviewed speaker notes on failed particle approaches, ray integration, moment
shadow maps, coherent traversal, scene integration and performance. Key lesson:
decouple expensive shadow integration from primary rays and make volumetrics
usable through the normal authoring loop. The shipped system's real-time fluid
simulation remained future work in the presentation. We use a cheaper coarse
secondary extinction estimate; moment shadow maps remain a possible next step.

## Current implementation and deliberate approximations

- A continuous moisture field controls local coverage, with a separate convection
  field controlling cloud tops. A shared 1.5 km condensation base and 2.15–4.65 km
  tops produce connected banks, gaps and unequal cloud groups. There are no
  randomly placed cloud ellipsoids. Broad C1 value noise and Worley billows supply
  the mass; two smaller billow scales erode the boundary.
- The 1024×96×1024 R16F volume stores signed density potential before clamping,
  including erosion. Its horizontal spacing is 62.5 m. Sampling and then clamping
  preserves boundaries. A further 30 m billow scale erodes existing edges; it does
  not add isolated sub-step wisps. Cloud mass uses broad value noise and two
  stronger Worley billow scales, rather than mostly smooth value-noise lumps.
- Three max-reduction levels and a neighboring-cell halo produce conservative
  128×12×128 empty-space certificates. Ray traversal bounds the Jacobian of
  spherical altitude, wind shear, and the bilinear updraft warp. Maximum liquid
  and neighboring updraft gradients come from the actual weather snapshot.
  Skips are capped at 500 m; ordinary integration remains a numerical
  approximation. `verifySky` probes certified segments on the GPU against the
  actual reconstructed density. This is a stress check, not a formal proof.
- A 256×64×256 R16F volume propagates solar optical depth in sunlight order. Each slice integrates the same density,
  with additional substeps for low sun. Interpolating optical depth avoids the
  light leaks of interpolating transmission, but upstream bilinear propagation
  diffuses shadows. A short local extinction correction restores some billow
  detail beyond the coarse lighting cache. Cumulus fades between 24 and 32 km
  from the center along either horizontal axis. Beyond that footprint an analytic
  stratus approximation remains in overcast/rain. This is not a world clipmap.
- Beer–Lambert integration, a two-lobe phase approximation, approximate cloud
  multiple scattering, and integrated atmospheric foreground scattering.
  Cloud ambient uses the unobstructed sky hemisphere. Shadow samples projected
  onto the condensation level use a 1 m inward offset: rounding a sample below
  the level previously caused spurious sunlight bands.
- A thin high ice-cloud layer at 8.5 km uses warped anisotropic noise. It is
  deliberately subtle; detailed ice microphysics and cirrus fall streaks are
  not simulated.
- Hillaire's complete frustum aerial-perspective LUT and volumetric terrain
  shadowing are not implemented. Garden haze uses a short-path approximation.
- `MoistWeather` evolves a periodic 64×64 grid at a fixed two-second step over
  64 km. Conservative upwind transport carries vapour, suspended liquid,
  temperature and updraft. A discrete stream-function curl supplies divergence-
  free horizontal face velocities. Saturation adjustment at a representative
  800 hPa exchanges vapour/liquid and latent heat; buoyancy, adiabatic cooling,
  prescribed surface heating/evaporation and a precipitation sink evolve columns.
  Water mixing ratios are g/kg. Tests account for water sources/sinks and verify
  cadence-independent replay. The sky uses liquid to modulate density and
  updraft to stretch cloud crowns, replacing the former sinusoidal strain.
  This is a simplified moist-column model: no 3D pressure solve, ice microphysics,
  rain particles, terrain-coupled hydrology or conservation of rendered voxel
  water mass. The procedural 3D shape remains an artistic reconstruction of
  those columns. Preset changes replay the column model with that preset's solar
  forcing; this is an editing convention, not a time-of-day history simulation.
- Density and its bounds rebuild only when coverage, density or seed changes.
  Wind advects/shears the cached field; immutable weather textures are uploaded
  for each sky cycle. The 64 km tile wraps rather than streaming indefinitely.
  Ground vegetation still has its separate 2D wind simulation.
- Molecular scattering and cloud-shadowed atmospheric shafts use three
  512×256 caches: radiance, foreground radiance and foreground transmission.
  High-resolution cloud rays sample these instead of reintegrating 40 atmospheric
  steps each. The 128×64 unobstructed sky still drives ambient light and exposure.
- Finished sky/irradiance pairs blend together; partial updates remain hidden.
  The sky cache is **8192×1024, upper hemisphere only**: twice the horizontal
  resolution at the same allocation as the former 4096×2048 full sphere.
  At sun elevations of at least 15°, a cycle uses 32 cloud-lighting frames,
  eight air-scattering frames and 256 sky frames (about 4.9 seconds at 60 Hz).
  Below that, one lighting slice and two sky rows per frame bound longer ray
  paths: a full shape/lighting refresh takes about 9.7 seconds. Rays use 64×1
  work groups to preserve neighboring-ray coherence. A continuous spherical
  pullback at a representative 2.2 km cloud height reprojects wind motion every
  displayed frame; sunlight visibility uses the same reprojected lookup.
  Per-pixel depth refinement was removed: transparent depth discontinuities tore
  silhouettes in aged snapshots. The continuous mapping removes that artifact
  and three RG16F depth allocations (96 MiB), but approximates layer parallax.
  Completed snapshots blend for at most 1.5 seconds. This separates continuous
  motion from slower shape/light updates. It remains an approximation for
  transparent overlapping layers, disocclusion and changing cloud shapes; the
  atmospheric component in cloud-covered directions shares the warp. Paused
  captures rebuild the exact current-time sky without reprojection. Parameter
  rebuilds are synchronous and measured separately from ordinary live updates.
- Sky rays use a rotation-only camera transform, independent of the near plane
  used to inspect millimetre-sized objects. The aiming dot is hidden in studios.
- HDR rasterization preserves radiance through a 16-bit-float target. A bounded
  bloom pass supplies glare before a single display transform. The solar disk
  uses the same sun vector as scattering and shadows, with irradiance divided
  by its solid angle. Low-altitude atmospheric direct scattering is shadowed by
  approximate cloud extinction, enabling crepuscular shafts in suitable haze.
- Surface lighting uses GGX direct light and integrated sky irradiance, plus
  approximate ground bounce and sky reflections. It is not full scene GI or a
  properly prefiltered specular environment. Workshop indoor rigs use an editable
  finite key light with inverse-square attenuation, an explicit ambient fill,
  and a perspective shadow map. Source-size penumbrae and highlights are
  approximate; indoor reflected geometry and full room GI remain unimplemented.
- Clouds do not yet cast spatially varying shadows onto the landscape. Direct
  sunlight follows the completed sky's transmission toward the sun, uniformly
  across the current garden; it is no longer an unrelated preset attenuation. Rain is a sky and
  material study, not precipitation or hydrology simulation.

## Acceptance views

Judge noon for blue depth, neutral cloud highlights and dimensional undersides;
golden hour for warm rims; sunset for warm-to-cool separation; afterglow for a
credible transition after direct sunlight disappears. Inspect away from the sun
and overhead as carefully as the sunward view. Avoid compensating for a broken
medium with exposure. Check horizon bands, texel enlargement, solar disk clipping,
cloud edge noise, repeated patterns, motion ghosting and object overexposure.

Next improvements should be driven by captures and GPU counters: layered
reprojection, more resolved detail at high elevations, better diffuse transport
inside thick clouds, and coupling the column model to terrain and precipitation.

## Surface and display costs

Material function constants specialize the common PBR shader per draw batch.
Sun irradiance, solar-disk atmospheric attenuation, cloud visibility and exposure
are evaluated once per frame into
an immutable-in-flight light buffer, rather than repeated per fragment. The
25-tap bilinearly filtered shadow box is paired into nine hardware comparisons;
receiver-plane correction is retained at the new sample positions. Shadow LODs
use a sub-texel error budget in the shadow projection and do not change the eye's
LOD hysteresis. The bloom blur pairs its Gaussian weights into 13 filtered
lookups per axis. Four-sample HDR color uses tile memory on this Apple GPU.
Mesh shaders remain the default: indexed drawing was slower in the tested garden.


## Project art-direction layer

`Authoring/ArtDirection.json` supplies a shared `SceneLook` to the game and workshop.
It leaves physical atmospheric integration intact. Outdoor light-energy scaling and
sky saturation are applied consistently to visible sky, sky irradiance/reflections,
solar disk and the authoring plane's horizon term. Surface detail/roughness are
controlled before lighting; the final HDR/display grade covers the whole image.
These are deliberate art controls, not new physical atmospheric parameters.
The Look inspector previews them, and Publish project look updates the shared file.
See `SOUNDSTAGE.md` for publication, saved-study and frozen-baseline semantics.


## September 11 performance follow-up

Cloud attenuation now carries optical depth through its scattering-weight family,
using two exponentials instead of transmission followed by fractional powers.
The density representation, traversal bounds, panorama resolution, snapshot
publication and spherical pullback remain unchanged. Weather uses a fused analytic
saturation derivative, persistent transport buffers and a bounded replay checkpoint
cache keyed by seed, forcing and tick. The first uncached replay is still synchronous.
See `PERFORMANCE_IMPLEMENTATION.md` for visual comparisons, measured CPU gains,
mixed GPU results and the architectural work still outstanding.
