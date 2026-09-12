# Runtime skin deformation

`SkinningPalette` in FieldCore is the CPU definition used by deformation and
dense contact audits. `Surface.metal` evaluates the same operation in ordinary
vertices, meshlets and shadow vertices. Skin weights remain four normalized
nonnegative influences with stable joint names in authored content.

Palettes containing only rigid bind-to-pose transforms use normalized
dual-quaternion blending. Quaternions are converted once per palette, aligned
to the strongest active vertex influence's hemisphere and normalized after
blending. Position and normal use the resulting rotation; translation comes
from its paired dual quaternion. This avoids the collapsing cross-section of
linear matrix blending during large twists. Pre-skin compact pose correctives
remain active, including their normal Jacobians.

Scale, shear, reflection or non-affine matrix rows select the existing linear
matrix blend for the complete palette. This preserves the authored surface
differentials used by garment guide frames. The rigid classification tolerance
is 0.00002 in the orthonormality test, intended only for floating-point matrix
composition error. `rigidBlending: false` forces an affine palette for CPU uses
that require a stable affine convention. Authored animation that crosses between
rigid and nonrigid palettes can change the blend convention; use consistently
rigid bone animation and corrective fields for tissue compression. A future
source-level skinning-mode control should pin the convention for intentionally
animated scale. The current Vesper bone animation uses rigid transforms.

The GPU retains the existing 64-byte palette stride and 64-joint draw limit.
The high bit of the skin count distinguishes rigid quaternion entries from
affine matrix entries; rigid columns 0/1 store real/dual `xyzw` components.
This is a transient renderer encoding. Authored matrices, skin weights, source
schema and joint identities do not change. A zero count retains rigid parts;
nonzero encoded counts still disable rest-pose meshlet culling.

This is geometric deformation. Each vertex's interpolated transform is rigid,
but a varying weight field does not globally preserve mesh volume. Dual
quaternions can produce joint bulges and use the shorter rotational arc;
180-degree relative rotation is ambiguous. Existing pose correctives, weights
and the anatomy source must resolve visible defects. Normals use the blended
transform, without spatial derivatives of the weight field. This does not
implement tissue mechanics, self-intersection prevention or collision response.

The algorithm follows [Kavan, Collins, Žára and O'Sullivan, *Geometric Skinning
with Approximate Dual Quaternion Blending*, TOG 2008](https://users.cs.utah.edu/~ladislav/kavan08geometric/kavan08geometric.pdf).
The authors' [overview and limitations](https://users.cs.utah.edu/~ladislav/dq/index.html)
also distinguish scale/shear support and shortest-path flips. The implementation
here is written from the published equations, without copying their source.

## Verification

`swift test --filter RigidSkinningTests` checks large-twist cross-section
preservation, rigid transforms/translations, antipodality and sparse influences,
change-of-frame invariance, four influences, dense audit agreement, unchanged
weight/palette byte budgets and exact affine fallback under scale/shear/reflection.

`Soundstage --check-renderer` compiles the production deformation function into
its GPU numerical check. Five cases cover a rigid bend, affine garment frames,
antipodal rotations, a zero-weight first slot and four influences, with two
overlapping pose correctives active. Positions and normals must agree with the
CPU within 0.00002. This is numerical verification; rendered extreme-pose clay
review remains necessary, as does live frame measurement with normal updates.

## Finite-source shadow filtering

Posed clay review exposed diagonal shadow striping. A matched capture with
visibility held at one isolated this from material detail and normal lighting.
The finite-source branch had stretched the locations of an optimized adjacent-
texel filter without recomputing its weights. That changes its texel pairing
and can jump as the receiver crosses a texel boundary.

The compact outdoor filter still uses nine hardware comparisons for its exact
bilinear 5×5 box. Wider finite sources use 25 fixed receiver-relative samples;
these locations move continuously. Each comparison still applies the local
geometric receiver-plane depth correction. Its one-texel bias covers the
individual bilinear comparison footprint; increasing it by the whole source
radius would unnecessarily detach contacts. This remains a fixed-width shadow
softness approximation, not an area-light visibility integral. The additional
16 comparisons apply only to widened finite-source shadows and must be included
in the representative live measurement.

The sampling constraint follows the derivation in [Ignacio Castaño's Shadow
Mapping Summary](https://www.ludicon.com/castano/blog/articles/shadow-mapping-summary-part-1/).
Correct filtering does not repair folded source geometry. The follow-up clay
review exposed actual anatomical mesh and weight-transition defects, which
require separate geometry/binding validation.

## Field binding width

`AnatomySource.skinBlend` is an independent 0…1 metre skin-weight transition
width. Zero preserves the authored geometric CSG transition widths. A positive
value broadens only the joint-weight interpolation, leaving the field, regions,
material colors and correspondence coordinates unchanged. Geometry can therefore
retain a sharp muscle groove while its skin binding transitions over a wider
region. Unweighted cuts inherit that binding. This control does not guarantee
an injective posed surface: broad overlaps may exceed the four-influence budget,
and redundant ancestor/descendant weights should be simplified in the source.

`AnatomySkinContinuityTests` verifies unchanged geometry/color/region identity,
reduced weight gradients, unchanged distant binding, cut inheritance, source
round trips, legacy decoding and invalid-width rejection.
