# Isolated coat A/B interface

The reference is existing material9. Candidate material15 is opt-in through Sunhare's proposed saved `coatStudy` control (default0). No current species defaults to15. The project material implementation and optional hooks are in `Games/Sanctuary/Authoring/Materials.metal`; root owns applying the shared call sites. No new texture/vertex/uniform ABI is needed.

1. Preserve current `projectSurface` call. Candidate15 returns its nap/roughness and exits before other project recipes. Actual normal correction is a prerequisite.
2. After material overrides/scene-look roughness but **before bumpNormal**, obtain `wet=u.environment.y`. Under `#ifdef PROJECT_COAT_RESPONSE`, call `projectWetFinish(kind,wet,rough,bump)`. If it returns false (or the macro is absent), retain the exact existing generic roughness wet blend. All kinds retain existing wet albedo darkening. Compute bumpNormal after this so candidate wet nap flattens. Do not also halve candidate roughness.
3. After direct BRDF/groom selection, form `ambientResponse=base*(1-metal)*ambient` and `environmentScale=1`. Under the same macro call:

```metal
projectLightFinish(kind,in.local,n,v,l,base,ambient,wet,
                   directBRDF,ambientResponse,environmentScale);
```

4. Compose `ambientResponse*ao + directBRDF*sunlight*visibility`. Preserve the original kind9 ambient grazing term for reference. Multiply the later existing reflected-environment contribution by `environmentScale`. No exposure, sky irradiance or visibility compensation.

All project functions return without changes when kind!=15. The macro is defined only by Sanctuary's material snippet; Cave/default snippets remain compatible without new stubs. Negative material.w clay view sets kind7 and consequently bypasses coat response.

Sunhare's author owns the study selector/cache key and safe part classification. Only body/head/outer-ear/front+hind limb descriptors change9→15 when the candidate mode is selected; shapes, colors, rig and normals remain identical. Muzzle/nose/eyes/lining/tail keep reference materials. The project recipe fades candidate nap and sheen to zero at bindY≤.04m (full coverage at.075m). The first nap is a generic three-dimensional bind-space field, **not an anatomical comb field**. Do not label it a groom. All-zero/default control must recreate existing kind9 descriptors exactly.

No candidate shader compilation or GPU capture has been run by this author. Retake matched original/candidate views only after the shared normal fix. A working source build is not an appearance/performance approval. Exact visual/performance gates and the observed headroom are in `docs/studies/SANCTUARY_FUR_RESEARCH.md`.
