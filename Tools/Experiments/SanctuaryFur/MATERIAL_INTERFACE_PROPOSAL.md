# Bounded coat and membrane material proposal

Status: design only. All subsequent native strand and raised-patch variants through 0306 fail fur identity; this historical semantic interface was not promoted. Default `furStudy=0` and `coatStudy=0` remain unchanged. See [the distinct continuous-volume decision](CONTINUOUS_VOLUME_DECISION.md) for the next proposed representation gate. This document adds no source interface, renderer ABI or save field.

## Visible purpose and present consumers

A short coat needs both a soft surface response and some resolved edge fibres.
The actual surface-only 2337 candidate removed textile repetition but retained a
clay silhouette. The actual 0017 groom added sparse flecks without changing that
reading. Neither result licenses a larger generic material framework.

Current project recipes already have stable part names, bind-space positions,
normals, vertex color and groom coordinates. Kind9 is the original coat;
kind15 is the opt-in localized nap/sheen experiment; kind13 is explicit groom
with derivative-derived strand tangent and anisotropic GGX. `projectSurface`
and `projectLightFinish` can express the existing surface experiment. No new
engine species switch or per-frame mesh compilation is needed.

The smallest useful extraction, only after a groom passes native review, is a
project-owned immutable recipe keyed by stable part ID and a recipe version.
It would replace the current four regions' constants without changing output.
The source, not a generated texture or a mesh cache, remains authoritative.

| Semantic value | Units and validation | Existing consumer / limit |
| --- | --- | --- |
| Parent and region | Exact registered part ID; missing parent rejects compilation | Four current body/head/outer-ear parents; no new rig |
| Comb direction | Finite nonzero vector in parent bind coordinates, normalized | Project onto the bind normal, then pose the resulting geometry with that parent |
| Guide length | Metres; current body26–32mm, head20–26mm, ears18–24mm | Authored guide control points; not an unimplemented shader slider |
| Strand width/radius | Metres, positive, within the reviewed recipe bounds | Existing `CraftGroom` compilation and strict mesh caps |
| Clumping | Dimensionless0…1, current0.25 | Existing compile-time groom clump; not simulated wet clustering |
| Roughness | Dimensionless0…1, current explicit groom0.86 | Existing project material/part override |
| Nap relief | Metres, current candidate bound0.24mm | Existing filtered kind15 surface only; does not extend silhouette |
| Exclusion | Source field or actual outer-mesh region plus margin in metres | Reject face/lining/sole contamination before caching |

Do not project a bind comb against a posed/world normal. A degenerate tangent
projection requires a deterministic orthogonal fallback in the same bind frame.
Unknown fields, nonfinite values, invalid ranges and unavailable material
families must reject explicitly; successful metadata generation must not conceal
failed mesh compilation. Keep the existing throwing generator and four-recipe
cache instead of introducing a second compilation path.

## Fur is not a continuous wing membrane

[Lengyel et al.](https://www.hhoppe.com/fur.pdf) supply volume/silhouette fibre
representations; [KHR_materials_sheen](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_sheen/README.md)
models the aggregate response of a fibre layer. These support separating groom
geometry and sheen. They do not establish a physical leathery wing model.

For a future visibly needed membrane study, keep continuous wing geometry and
its attachment/tension folds primary. Proposed semantic inputs are thickness in
metres, roughness, bind-space crease direction and optional absorption per metre.
Thickness must come from an authored bounded field or geometry, not fur length.
These are **unsupported proposal fields**, not controls to expose now. The
current renderer's selected-material additive backlight is a heuristic; it does
not integrate thickness or absorption. A double-sided surface alone does not
implement transmission. Existing kind11 grain is likewise not a validated
membrane recipe. Do not assign kind13 fibres or kind15 sheen to a wing merely to
reuse a material.

A later membrane proposal would first need one folded, attached patch viewed
front/back under matched reflected and transmitted light, and a stated transport
approximation. It would need a separate root-owned shader handshake and budget.
No bat model, membrane implementation or extra literature tranche is proposed now.

## Next decision and ownership

Astra reviews the actual 352-strand images first and fixes only an evidenced
owned-coat defect. Root owns matching build, native review and live cost. If the
352 recipe still reads as wires/flecks/clay, hold adoption and report which cue
failed; do not extract a reusable API merely because its numerical inputs exist.
If it passes, the next extraction is only the four existing semantic regions and
their exact compile-time values. Preserve both study selectors, unchanged base
mesh/brain/pose and the current1MiB-per-recipe / four-recipe cap.
