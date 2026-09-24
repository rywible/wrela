# Alpine material look development

The shared target is a late-summer alpine creek, a young lodgepole-pine grove, and a weathered limestone ruin with an oak-and-iron gate. These source presets are an art-direction starting point, not proof that the scene meets a finished-game bar.

`createLookdevMaterials()` in `packages/model/src/material-lookdev.ts` owns the common palette. Terrain and architecture reference these IDs rather than defining competing material documents. `alpineConiferMaterials()` owns the needle/bark pair and is reused by the common palette.

## Direction

- Needles: low-saturation forest green, small attached detail, restrained thin-surface transmission, and rough highlights. Needle geometry and age-dependent vertex tint supply most visual variation. Reject glossy plastic strips, cyan cloud patches, and a uniformly luminous canopy.
- Bark: muted warm brown, matte and low contrast. Geometry carries the large ridges and scars. Reject horizontal stripe bands painted around the trunk.
- Limestone and masonry: darker warm gray/taupe with low-contrast world-aligned bedding, a sparse lichen layer and high dry roughness. Geometry must carry broken planes, blocks and fractures. Reject marble striping, broad plastic highlights and noisy speckle that overwhelms the silhouette.
- Creek rock: same parent stone palette with reduced roughness and darker diffuse response. It should read as damp stone, not a different blue substance.
- Ground: restrained olive/earth variation, exposed steep stone, and a damp bank mask below 0.05 m fading by 0.35 m. These bands match the shared creek source, whose intended water elevation is -0.2 m and bed approximately -0.85 m. Moving the creek vertically requires updating this authored bank mask.
- Timber and metals: muted warm oak, dark wrought iron with irregular oxidation, and a small patinated-bronze accent. Wear layers are dielectric and can cover a conducting substrate.
- Snow: a reserved pale neutral/cool material for high sheltered areas, not a uniform coating over the entire summer clearing.
- Understory: two shared sources, `alpine-lookdev-understory` and `alpine-lookdev-understory-dry`, give world composition muted live sedge and warm dry litter. Their plant geometry and distribution determine whether they read as habitat rather than colored blobs.

RGB values are linear, selected for this scene. They are not measured spectral coefficients.

## Reference constraints

The [US Forest Service Sierra lodgepole species review](https://research.fs.usda.gov/feis/species-reviews/pinconm) describes 3–6 cm needles in pairs and thin bark. The [National Park Service fire ecology article](https://www.nps.gov/articles/wildland-fire-lodgepole-pine.htm) provides branch/tree photographs and also identifies paired needles and thin bark. Those references constrain botanical scale and structure; they do not supply the shader's optical coefficients.

## Review protocol

Run `bun tools/material-lookdev.ts` for a real-GPU chart under neutral, grazing and backlit sunlight and at twice the camera distance. The twelve spheres are ordered left to right, top to bottom: needles, bark, ground, rock, damp rock, snow, oak, iron, bronze, masonry, live understory and dry understory. The tool saves image captures, ordered source material metadata, GPU completeness, diagnostics and the normal source/evidence manifests.

A sphere chart checks diffuse values, highlight widths, history masking and distance stability. It cannot approve the plant, terrain or assembled scene. Judge the same material sources on actual branch/needle geometry, fractured rocks, worn gate assemblies and the main playable camera. Compare silhouette, interior shadow mass, direct/indirect balance and whether each object's substance remains legible without adding noisy contrast.

The first whole-scene capture failed this bar: near-black needle pixels, a uniform olive floor and pale, smooth rock masses. A later material chart on Apple Metal verifies the new palette and filtered procedural detail run in the renderer under all four review conditions. The chart still has nearly black backlit opaque spheres; environmental fill is being tuned separately. The cliff study also remains too smooth in silhouette despite restrained bedding, so the geology geometry and final combined scene require another visual pass before claiming acceptance.
