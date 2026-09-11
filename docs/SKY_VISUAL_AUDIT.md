# Sky visual audit — photographic comparison

Reference board: [eight real sky photographs](sky-reference-board.html). These
are remotely displayed references with source links, not game assets. Inspected
NOAA's cumulus, cirrus, stratocumulus, stratus and altocumulus examples, Jonathan
Saleh's sunset cloud photograph, Penny Edwards's crepuscular-ray photograph,
and a Belt of Venus photograph. Exposure and white balance vary between photos;
compare morphology and lighting relationships, not exact RGB values.

| Observed mismatch in our captured sky | Cause in the implementation | Correction / verification |
| --- | --- | --- |
| Cloud bodies resemble a perforated sheet; weak rounded volume | Thresholded noise throughout a single slab; no coherent convective masses | Replaced with a continuous moisture/convection field, shared condensation level and separate fine erosion; seeded cloud templates were tried and removed because their repetition remained visible |
| Repeated cloud sizes and evenly scattered blobs | Independent placements with variations of one ellipsoid template | Removed placement loops; continuous fields now form clustered banks, large gaps and unequal tops; inspected seeds 4, 17 and 83 |
| Uniformly soft shapes, then excessive fine holes when sharpened | Broad value noise determines both interior and boundary | Separate macro shape from billow displacement and fine edge erosion; inspect full-resolution close lobes |
| Blue-black cloud undersides | Ambient was derived from the camera's viewing direction through the atmosphere | Use clear hemispherical sky illumination, atmospheric multiple scattering and approximate cloud multiple scattering, independent of camera direction |
| Near and far clouds have similar contrast, and sunset turns uniformly orange | Ad-hoc aerial blend unrelated to the integrated atmospheric foreground | Composite clouds behind the actual accumulated foreground scattering and transmittance |
| Missing delicate high cloud scale and elevated twilight light | Only one low-altitude cloud layer | Add a separate thin, advected high ice-cloud layer; inspect zenith and twilight |
| Overcast/rain still have bright blue holes | Coverage parameter only changed the probability of cumulus | Add a continuous stratus deck, with diffuse sky illumination; inspect all directions |
| Twilight loses readable atmosphere too abruptly | Fixed daytime exposure, missing elevated clouds and incorrect cloud ambient | Correct cloud illumination, add the higher layer and meter clear-sky luminance for bounded twilight exposure; inspected −1.2° sun elevation |
| Sun seems fixed / misplaced | Aiming-dot overlay in studio; sky rays also inherited an unnecessarily tiny near plane | Hide the aiming dot in studio; reconstruct translation-free sky rays; rendered sunset disk verified within one pixel of expected projection |
| Sun looks like a flat white dot | Tone mapping before light could spread into adjacent pixels | Preserve HDR radiance and apply controlled bloom before one display transform |
| False luminous curtain under horizon | Sampling a clamped horizon direction for the whole lower hemisphere | Render a neutral virtual ground hemisphere with a narrow horizon transition |
| Aligned horizontal bands | Shadow samples rounded just outside the cloud base; mismatched coarse/fine shadow density also added contours | Project shadow queries 1 m inside the layer; use shared density for view and shadow integration, and account for residual radiance at early termination |
| Shafts fail to match cloud openings | Secondary extinction did not follow enough of the actual cloud structure | Propagate optical depth through the same reconstructed density, with extra local samples at low sun; the cache is still a filtered lighting approximation |

The first contact sheet is `.soundstage/sky-contact-sheet.jpg`; full-resolution
captures and sidecars are in `.soundstage/captures`. Corrections listed here are
the work list, not a claim that every comparison has already passed. Record
the resulting checks below after the new captures are inspected.

## Result of the reference pass — September 11

Inspected the resulting 18-view sky matrix (six lighting conditions × sunward,
away and zenith), three weather seeds, the 28-view stone matrix and wet/dry seed
captures. Native controls were exercised in Soundstage. The corrected sky matrix
is `.soundstage/final-sky-matrix.jpg`; capture paths and exact shader fingerprints
are in `.soundstage/sky-study.json`. Earlier captures remain diagnostic evidence,
not the final appearance. The stone matrix revealed a constant daytime ground
bounce, which was subsequently fixed; `twilight-ground-bounce-fix` records the
correction. The rain boundary correction was checked in a full-resolution frame,
then the entire sky matrix was repeated.

Additional causes corrected during comparison:

- 2048×1024 angular caching stretched clouds over several screen pixels. The
  current cache is 4096×2048; signed density cached at 62.5 m horizontal spacing preserves more edge
  detail than interpolating the earlier coarse, pre-clamped density.
- A truncated broad bloom kernel made the sun halo square. Its tail now falls
  close to zero at the kernel boundary.
- High ice clouds were too opaque and too regular. Their flow is warped at
  multiple scales, with much lower optical depth. This remains a simplified
  cirrus model, rather than a reproduction of the reference's fine fall streaks.
- Dense cloud integration left enough transmission to expose the extremely
  bright sun. Opaque early exits now consume the remaining radiance consistently.
- The virtual ground repeated bright cloud patterns below the horizon. Its
  atmospheric transition now uses the clear atmosphere; ground bounce follows
  available illumination instead of staying bright all night.

The sky is visibly less repetitive and the rain/golden sunlight bands are gone.
It is not a finished atmospheric simulation: thick undersides can still look too
smooth, cirrus remains an approximation, and close inspection can reveal cloud
sampling detail. Cumulus has a finite cache footprint, updates have several
seconds of latency, and spatially varying ground cloud shadows and full scene GI are not implemented.
Performance measurements are separate from the visual claims and include live
cache updates; see `VALIDATION.md` for the latest measured limits.

The garden integration check also replaced fixed overcast sunlight attenuation
with actual solar transmission from the completed sky. This is currently one
uniform attenuation over the small garden, not a terrain shadow map. Agent
status now groups whole-frame GPU durations by density, lighting, sky and idle
phases, making expensive cache updates visible rather than hiding them in a
single median. The lighting cache now propagates optical depth slice by slice.
