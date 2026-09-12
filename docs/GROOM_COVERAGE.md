# Procedural groom coverage

Guide curves and `GroomDesign` remain authoritative. The optional envelope
compiles the unresolved interior of each guide clump into a guide-aligned mesh.
Its explicit vertex `groom` channel contains strand phase, longitudinal fraction,
root coverage and strand length variation. A zero root coverage means opaque
geometry. AO remains in `color.w`; wind remains in `normal.w`.

The vertex layout is now four aligned float4 values (64 bytes) in Swift and
Metal. CGeometry receives the actual stride rather than assuming 48 bytes.
Mesh reordering preserves all attributes; midpoint refinement interpolates groom
coordinates. Skinning and correctives retain the source coordinates while world
positions and normals deform. Existing per-fibre geometry carries guide
coordinates with coverage disabled, so it can share directional lighting.

An integer-seeded procedural field supplies unequal strand endings. A periodic
strand pulse is integrated over the fragment's projected footprint. At close
range it exposes fine longitudinal gaps; at distance it converges toward the
mean coverage instead of flickering between unfiltered stripes. Fibre count,
thickness, groom width, envelope width and source seed determine its compiled
parameters. Root fill includes an explicit unresolved-interior approximation;
it is not a measured volumetric extinction coefficient.

Covered indexed and mesh pipelines enable Metal alpha-to-coverage on the existing
four-sample HDR surface pass. They retain per-sample depth writes and require no
transparency sorting or full-screen transparency resolve. Other scene pipelines
retain their existing states. The single-sample shadow map uses a dedicated
covered-groom pipeline with the same filtered strand field and a fixed 4×4
ordered coverage threshold. Existing PCF filters the binary shadow samples.
Ordinary objects keep the fragment-free shadow pipeline. This shadow treatment
approximates fractional surface occlusion; it is not deep opacity or multiple
hair-layer transmittance and requires live-motion review for residual patterning.

Material 13 uses a guide direction reconstructed from source strand-coordinate
and deformed-position derivatives. An anisotropic GGX distribution with correlated
Smith masking replaces its ordinary direct specular response. Diffuse and scene
lighting remain shared. This is a directional **surface** approximation, not a
Marschner hair scattering model: refraction, internal reflection and multiple
scattering through a groom are not modeled. Hair direction falls back safely to
the ordinary surface response if the derivative basis degenerates.

Primary references are Apple's [alpha-to-coverage
contract](https://developer.apple.com/documentation/metal/mtlrenderpipelinedescriptor/isalphatocoverageenabled)
and [MSAA sample
resolve](https://developer.apple.com/documentation/metal/improving-edge-rendering-quality-with-multisample-antialiasing-msaa),
and Eric Heitz's [masking-shadowing analysis for microfacet
BRDFs](https://www.jcgt.org/published/0003/02/03/paper.pdf).
The strand field, source-coordinate packing and ordered shadow representation
are Wrela implementation choices, not claims of reproducing a cited renderer.

`GroomCoverageTests` checks minification mean density, exposed strand gaps,
bounded endpoints, independent AO and coordinate preservation through refinement,
mesh reordering and meshlets. `--check-renderer` compares 8192 evaluations of the
production GPU coverage function against FieldCore and checks that the expanded
vertex channel survives production GPU deformation. Those numerical checks do
not replace actual image, motion, shadow and runtime-budget review.
