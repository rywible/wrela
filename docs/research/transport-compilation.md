# Compiling transport, not just geometry

Research continuation, 21 September 2026. These are implemented research prototypes and derived extensions for Wrela. They are not installed in the production renderer.

The strongest result is a different compilation target: **a program for the light integrated over a pixel**, with an explicit domain of validity. Wrela's authored phases, field composition, and material expressions give the compiler information that a triangle stream usually discards.

The experiments show substantial kernel wins and identify useful failure cases. They do not establish superiority over AAA games, a whole-game frame-time improvement, or worldwide novelty. Fourier filtering, microfacet slope distributions, control variates, and hierarchical visibility have substantial prior art. The proposed contribution is their specific composition into Wrela's semantic compiler, including executable approximation and visibility contracts.

## Results worth pursuing

Hardware: Apple M4, Chrome hardware WebGPU, Apple/Metal 3 adapter. GPU timings are medians of nine alternating trials after warmup. Each trial batches 64 dispatch sequences; this reduces the observed 65.536 µs timestamp quantization to approximately 1.024 µs per sequence. Small differences of that size are not meaningful.

| Experiment | Measured result | Scope |
|---|---|---|
| Distant water, 16 retained phase modes | 0.01024 ms versus 0.82330 ms for 256 shading samples: **80.4× faster**, with **14.0× lower RMS error** | 65,536 fixed-view/light GGX queries; preprocessing excluded |
| Same water versus anisotropic response texture | Texture: 0.00410 ms. Compiled program: 0.01024 ms, **7.3× lower error** | The texture is the cheaper alternative when its error is acceptable |
| Oblique water, 128 modes | **14.4× faster**, **1.47× lower error** than 256 samples | Harder footprint; much smaller quality advantage |
| Long correlated water footprint, 16 modes | **51.9× faster** than 256 samples; **568× lower RMS error than 64 stochastic samples** | Deliberate stress case with equal phase winding; not representative of every pixel |
| Correlated metal/roughness/color | 64 modes have **121× lower RMS error** than 256 samples | CPU accuracy experiment, fixed light/view; no material GPU speed claim |
| Crowd skinning with 99.3% hidden | 2.38285 ms → 0.02560 ms: **93.1× faster**, including visibility, compaction, and indirect dispatch | Synthetic 4,096-instance, four-influence, 1,024-vertex skin workload; compared with skinning every instance |
| Crowd with 51.4% hidden / nothing hidden | **2.06× / approximately 1.00×** | Benefit follows actual eliminated work |
| View-independent slope distribution | **0.547% relative L2 error**, normalized mass 1.000000000000003 | Two fully decorrelated waves plus GGX micro-slopes; 256² atlas, 256 independently integrated query points |
| Residual lighting estimator | **35.5× lower sample variance** in the tested integrand | Unbiased-estimator experiment; predictor cost prevents interpreting this as a speedup |
| Atmosphere | Real-arithmetic transmittance bounds work; **no universal runtime win over a lookup table** | Optical-depth building block, not a finished sky or multiple-scattering solution |

All 235 repository tests passed, with 590,607 assertions, at the validation snapshot. This extension contributes 12 tests / 90,210 assertions. GPU validation reported no errors. Full raw results are in [transport-compilation-results.json](/Users/ryanwible/projects/wrela/docs/research/transport-compilation-results.json).

![Measured evidence](/Users/ryanwible/projects/wrela/docs/research/transport-compilation-evidence.png)

## 1. Integrate the shaded phase program

### The mathematical object

Wrela's reference water has two authored waves. Its slopes are linear combinations of cosines. Let the local phase vector be

\[
\phi(u,v,\tau)=\phi_0+a u+b v+c\tau,\qquad
u,v,\tau\in[-1/2,1/2].
\]

Here a and b are the changes in phase across a pixel, and c includes shutter duration. Keep slowly varying parameters \(\theta\)—view, light, material constants, macro frame—fixed over this local integration domain.

Evaluate the **whole nonlinear response** \(R(\phi;\theta)\), including GGX, Smith masking, Fresnel, and any correlated material expressions. Compile a real Fourier polynomial:

\[
P(\phi)=c_0+2\operatorname{Re}\sum_{k\in K}c_k e^{i k\cdot\phi}.
\]

Its footprint integral is exact:

\[
\overline P(\phi_0)=c_0+
2\operatorname{Re}\sum_{k\in K}
c_k e^{i k\cdot\phi_0}
\underbrace{\operatorname{sinc}(k\cdot a/2)
\operatorname{sinc}(k\cdot b/2)
\operatorname{sinc}(k\cdot c/2)}_{C_k}.
\]

The compiler selects terms **after** applying \(C_k\). Dropping a set D has the phase-independent bound

\[
|\overline P-\overline P_{\rm kept}|
\le 2\sum_{k\in D}|c_k C_k|.
\]

This bounds the finite polynomial's truncation. It does not certify the original BSDF fit. That error is separately measured; water's unfiltered 256² Fourier fit has maximum error \(7.90\times10^{-13}\) on 1,000 independent phases in the CPU experiment. That is a sampled observation, not a proof over every phase and parameter.

### Why the cross terms matter

If both phases move by the same amount across a long pixel, \(\phi_1-\phi_2\) does not move at all. Its mode survives even when the individual waves are far below pixel resolution.

Averaging the normals, or independently blurring the inputs, loses this surviving interference. Averaging the shaded response with the exact phase footprint keeps it.

The correlated PBR experiment varies roughness, metallic fraction, color, and slope with shared phases. Average-input shading has RMS error 2.257; 64 compiled modes reduce this to 0.000726. The reference mean is 0.737. This is a deliberately correlated procedural material, not a claim about every material. It is a synthetic harmonic recipe; automatically lowering Wrela's current lattice-noise layers and arbitrary material graphs remains integration work.

For a concrete lighting example, if

\[
L=1+0.8\cos\phi,\quad B=0.5-0.4\cos\phi,
\]

then \(\mathbb E[LB]=0.34\), whereas \(\mathbb E[L]\mathbb E[B]=0.5\). The implementation multiplies phase programs by frequency convolution before integrating; it does not silently discard this covariance.

### Parameter and footprint validity

Changing the footprint changes the answer. From the integral representation of C and \(\mathbb E|u|=1/4\),

\[
|C_k(a,b,c)-C_k(a',b',c')|
\le \min\!\left(2,\frac{|k\cdot\Delta a|+
|k\cdot\Delta b|+|k\cdot\Delta c|}{4}\right).
\]

Multiplying by \(2|c_k|\) and summing gives a reuse bound for **unfiltered** coefficients. The tested helper intentionally requires those coefficients.

Changing view, light, or material also changes R. A production compiler needs a separate bound or measured guard for that parameter cell. A response baked at one view is not valid for arbitrary moving cameras. Sharp highlights can make those cells very small. The experiments hold \(\theta\) fixed to expose this issue rather than amortizing it invisibly.

The two-phase fit costs roughly 22 ms in Bun and 33–37 ms in the browser, including the browser planner. Driver pipeline creation ranged from roughly 1 ms for cached small programs to tens of milliseconds during earlier cold calls. These costs belong in authoring/cooking or a sufficiently reusable cache. For the distant workload, 37 ms of preparation needs roughly 46 repetitions of the tested 65,536-query batch to amortize against 256 samples, before additional cache management costs.

Eight arbitrary fast phases would make a dense phase grid prohibitive. Plausible lowerings are dominant phase subsets, sparse interaction terms with residual correction, and the distribution representation below. No eight-phase success is claimed here.

### GPU lowering

Three kernels were tested:

1. Loop over coefficients in storage memory.
2. Stage those coefficients in workgroup memory.
3. Emit a specialized phasor program: calculate the sine/cosine of the two fundamental phases, build integer powers by complex multiplication, and reuse them across terms.

The third method removes per-mode trigonometric evaluations. In the oblique case, storage evaluation took 0.12493 ms and the 64-thread phasor program 0.05734 ms. Workgroup staging alone made little difference. Workgroup sizes 64, 128, and 256 were measured; the results do not justify a universal choice across devices.

The experiment also includes one-sample shading, mean-normal shading, 64 and 256 regular samples, 64 stratified stochastic samples, and a 256² R16F response texture with mipmaps and anisotropy 16.

The dense GPU reference evaluates 256² samples per query and is checked against independent CPU integration at selected phases. GPU/CPU reference discrepancies are below \(8\times10^{-6}\) in the water cases. This is an empirical precision/reference floor, not a proven global error bound. The long-period reference analytically reduces eight complete equal phase periods to one before numerical integration; otherwise the reference itself can alias.

**Failure case:** close-up water does not compress into a few modes. With 16 modes its CPU RMS error is 0.0858, while 64 direct samples have error 0.0000409. A compiler must select another representation there.

![Filtering comparisons](/Users/ryanwible/projects/wrela/docs/research/transport-compilation-filtering.png)

These are phase-space radiance plots, not rendered game frames. Each row is contrast stretched to its own reference range; out-of-range values are clipped for display. Numerical comparisons use unclipped linear radiance.

Implementation: [spectral.ts](/Users/ryanwible/projects/wrela/tools/transport-research/spectral.ts), [gpu-spectral.ts](/Users/ryanwible/projects/wrela/tools/transport-research/gpu-spectral.ts).

## 2. Positive response algebra and coherent orbit classes

A Fourier approximation to radiance can ring below zero. Clamping it changes its integral and breaks an unbiased residual estimator.

A stronger representation is

\[
R(\phi)\approx |A(\phi)|^2,\qquad
A(\phi)=\sum_j a_j e^{i k_j\cdot\phi}.
\]

Its exact footprint integral is

\[
I=z^*Gz,\quad
z_j=a_j e^{i k_j\cdot\phi_0},\quad
G_{ij}=C_{k_j-k_i}.
\]

G is positive semidefinite: for any vector z,
\(z^*Gz=\mathbb E|\sum_j z_j e^{i k_j\cdot\delta\phi}|^2\ge0\).
Positivity comes from the representation, not a post-shading clamp.

This supplies two useful compiler transformations.

**Low-rank footprint factorization.** If \(G=Q\Lambda Q^*\), retain selected nonnegative eigenvalues and evaluate a sum of squared short programs. Discarding eigenvalues no larger than \(\lambda_{\rm tail}\) gives

\[
0\le I-I_{\rm kept}\le
\lambda_{\rm tail}\sum_j|a_j|^2.
\]

This spectral-norm bound is derived here; eigensolver compression and its GPU performance are not implemented. Entrywise thresholding of G is not an acceptable substitute: it can destroy positive semidefiniteness.

**Exact orbit grouping.** Suppose the pixel traverses a nonzero integer number of periods with integer winding vector w, and there is no transverse or temporal extent. All cross terms between different integer values \(k\cdot w\) vanish:

\[
I=\sum_r\left|
\sum_{j:k_j\cdot w=r} a_j e^{i k_j\cdot\phi_0}
\right|^2.
\]

This replaces pairwise coherence work with grouping by an integer key, while preserving the visible beat inside each group. It is implemented and agrees with the full Gram calculation to \(5.6\times10^{-16}\) in the measured fixture.

Near-periodic footprints require the sinc factors and a leakage budget; approximate equality of floating-point windings does not authorize exact grouping.

The square-root water fit with 129 signed amplitude modes remained positive in the experiment, but its filtered RMS error was 0.000660—worse than the direct 16-mode distant-radiance fit. Positivity is valuable; this is not yet the fastest or most accurate representation.

Implementation: [positive-response.ts](/Users/ryanwible/projects/wrela/tools/transport-research/positive-response.ts).

## 3. A view-independent GGX distribution compiler

The previous response program is expensive to cache across view/light parameters. A second target removes those parameters from the expensive distribution construction.

For additive heightfield slopes,

\[
s=\sum_j v_j\cos\phi_j+\xi,
\]

where \(\xi\) is a GGX micro-slope with density

\[
p_\alpha(\xi)=\frac{\alpha^2}
{\pi(\alpha^2+\|\xi\|^2)^2},
\]

the characteristic function of the fully decorrelated authored waves is

\[
\Phi_{\rm waves}(k)=\prod_j J_0(k\cdot v_j).
\]

The isotropic GGX characteristic function is

\[
\Phi_{\rm GGX}(k)=qK_1(q),\qquad q=\alpha\|k\|,
\]

with value 1 at q=0. Therefore

\[
\boxed{\Phi_{\rm total}(k)=qK_1(q)\prod_jJ_0(k\cdot v_j)}.
\]

For anisotropic micro-slopes, replace q by
\(\sqrt{\alpha_x^2k_x^2+\alpha_y^2k_y^2}\).
This follows by linear change of variables, not by fitting a new material model.

A short derivation: express the GGX denominator using
\(1/a^2=\int_0^\infty t e^{-ta}\,dt\), perform the two-dimensional Gaussian Fourier integral, and substitute \(u=\alpha^2t\). The remaining integral is
\(\int_0^\infty e^{-u-q^2/(4u)}\,du=qK_1(q)\).
The Bessel identities are established mathematics; see [NIST modified-Bessel integrals](https://dlmf.nist.gov/10.32) and [ordinary-Bessel integrals](https://dlmf.nist.gov/10.9).

The compiler can build this distribution once. Different lights and views query the resulting slope density at their half-vector slope. Converting a slope density p into an NDF uses
\(D(n)=p(s)(1+\|s\|^2)^2\), preserving projected-area normalization.

The implementation builds a periodic 256² density atlas. Its integral is 1 to floating-point precision; the tested density stays positive and its relative L2 error is 0.547%. Construction took approximately 376 ms in Bun. The independent reference integrates the two wave phases directly.

### Where the approximation enters

- Complete phase decorrelation is a domain assumption. Finite pixels can retain the beats from section 1.
- The FFT uses a finite frequency set and a periodic slope domain.
- Bilinear atlas interpolation has a separate error.
- Matching an effective NDF is not equivalent to integrating the entire BRDF. Visibility, masking, height correlation, Fresnel variation, and spatially varying materials still matter.

Periodic-copy contamination can be bounded separately. If query magnitude plus maximum macro-slope magnitude is at most B, and the slope period is L>B, the infinity-norm lattice ring r has 8r copies at distance at least Lr−B. Thus

\[
E_{\rm images}\le
\sum_{r=1}^\infty
\frac{8r\alpha^2}{\pi(Lr-B)^4}.
\]

The experiment's bound is 0.000690 in slope-density units. It does **not** bound frequency truncation or bilinear interpolation; those dominate the measured maximum error of 0.272 near the sharp density peak.

### Finite-footprint extension: derive coherence before discarding it

Jacobi–Anger expansion gives

\[
e^{i(k\cdot v_j)\cos\phi_j}
=\sum_{n_j\in\mathbb Z}
i^{n_j}J_{n_j}(k\cdot v_j)e^{in_j\phi_j}.
\]

Multiplying these expansions and applying the exact footprint factors from section 1 yields

\[
\Phi_{\rm pixel}(k)=
\sum_{\mathbf n}
i^{\sum n_j}
\left[\prod_jJ_{n_j}(k\cdot v_j)\right]
e^{i\mathbf n\cdot\phi_0}C_{\mathbf n}.
\]

This provides a mathematical connection between local coherent glints and the fully decorrelated distribution. The zero multi-index gives the product of \(J_0\) factors. Nonzero resonant terms supply the surviving beats.

This extension is derived, not yet implemented or bounded tightly enough for a production compiler. Its central risk is combinatorial growth. Sparse surviving resonances and conservative tail accounting must earn their cost.

Implementation: [slope-atlas.ts](/Users/ryanwible/projects/wrela/tools/transport-research/slope-atlas.ts).

## 4. Compiler-proven visibility before crowd skinning

A field compiler can extract **interior opaque occluders**, not merely exterior bounds. The prototype certifies a ball by enclosing it in a cube whose complete ordered CSG interval has upper bound below zero. A subtraction through the center correctly rejects the certificate.

For a sphere centered at c with radius r, viewed from the origin with \(c_z>r\), use unnormalized perspective ray \(d=(u,v,1)\). Define

\[
q=\|c\|^2-r^2,\quad p=c\cdot d,\quad
\Delta=p^2-q\|d\|^2.
\]

The front depth along the camera z axis is

\[
t_{\rm front}=\frac q{p+\sqrt\Delta}.
\]

The reciprocal front depth is concave over the projected sphere:

\[
w=\frac1{t_{\rm front}}=
\frac{p+\sqrt\Delta}{q}.
\]

Why: the Hessian of \(\Delta\) in (u,v) is
\(2(c_{xy}c_{xy}^T-qI)\), which is negative definite when \(c_z>r\). The square root is increasing and concave, and p is affine.

If all four tile corners intersect the forward sphere, concavity of \(\Delta\) establishes coverage throughout the tile. Concavity of w then gives

\[
t_{\rm front}(u,v)\le
\max_{\text{four corners}}t_{\rm front}.
\]

Store that farthest occluder-front bound per tile. A candidate's **nearest possible** depth being strictly farther in every tile of its conservative projected rectangle proves it hidden.

This is an inner-geometry certificate for whole tiles. Testing only the center depth would be unsafe.

### GPU execution

1. Clear a compact 160×90 tile buffer.
2. Assign one 64-thread group per interior sphere; visit its projected tile rectangle and compute four-corner bounds.
3. Merge positive depth bounds with integer atomic-min.
4. Test each candidate bound; compact survivors with an atomic counter.
5. Use that counter directly as an indirect dispatch count for four-influence skinning.

No CPU readback is used to dispatch survivors. The reported timings include all five stages. Validation readbacks happen outside the measured sequence.

The fixture's generated posed vertices are verified to remain inside each 0.16-radius candidate bound; the largest measured radius is 0.10698. Output is cleared before checking compacted results, preventing stale baseline data from hiding missing instances. All survivor outputs match the baseline exactly; there are no unsafe GPU/CPU culling disagreements.

The dense case has a nearly continuous foreground barrier: 77 spherical occluders hide 4,068 of 4,096 candidate bounds. This is a favorable workload, not an average scene. Conventional Hi-Z can also remove most of this work. The experiment compares with all-instance skinning, not a state-of-the-art Hi-Z implementation. The opportunity specific to Wrela is generating cheap valid interior proxies from semantic fields.

### Validity limits

The sphere must remain inside opaque geometry. Transparent materials, holes, and arbitrary deformation can invalidate it. Rigid transforms and uniform scale preserve this containment; a bind-pose sphere does not automatically remain valid under skinning. Animated occluders need a pose-domain proof or must be excluded.

Candidate bounds must contain the whole posed geometry. CPU animation/pose generation is not timed and is not eliminated in this experiment. Camera visibility is not a shadow certificate; a light requires its own ray-domain proof.

The concavity proof is exact in real arithmetic. GPU code shrinks proxies and adds depth margins, but those margins are empirical numerical precautions, not formally directed rounding.

Implementation: [field-occluders.ts](/Users/ryanwible/projects/wrela/tools/transport-research/field-occluders.ts), [visibility.ts](/Users/ryanwible/projects/wrela/tools/transport-research/visibility.ts), [gpu-visibility.ts](/Users/ryanwible/projects/wrela/tools/transport-research/gpu-visibility.ts).

## 5. Bounded optical depth for procedural skies

For a spherical exponential atmosphere,

\[
\rho(s)=\exp\left[
-\frac{\sqrt{r_0^2+2r_0\mu s+s^2}-R}{H}
\right].
\]

Writing \(\ell=\log\rho\) and
\(b^2=r_0^2(1-\mu^2)\),

\[
\ell''(s)=-\frac{b^2}{H r(s)^3}\le0.
\]

The endpoint chord lies below \(\ell\); its midpoint tangent lies above. Both are affine, so their exponentials integrate analytically. On every interval,

\[
\int e^{\ell_{\rm chord}}ds
\le \int \rho\,ds
\le \int e^{\ell_{\rm tangent}}ds.
\]

For affine log-density endpoints A and B over length h,

\[
J(A,B)=h\,e^{\max(A,B)}
\frac{1-e^{-|A-B|}}{|A-B|},
\]

with the continuous limit at A=B. The implementation uses a stable small-difference evaluation.

Summing segment bounds gives optical-depth bounds; for positive extinction \(\sigma\),

\[
e^{-\sigma\tau_{\rm upper}}\le T\le
e^{-\sigma\tau_{\rm lower}}.
\]

Adaptive subdivision can stop on **transmittance error**, the quantity rendering actually uses. The zenith ray has zero log-density curvature and integrates in one segment.

CPU tests include horizon rays, scale heights 1.2 and 8 km, heights 0–40 km, and 48 parameter combinations. The requested \(10^{-5}\) transmittance interval width needed 1–207 segments. Independent quadrature lies inside the real-arithmetic bounds up to the stated numerical tolerance.

Fixed 4/8/16/32-segment GPU cohorts use squared-distance spacing. For the 8 km medium, 16 bounded segments and 64 midpoint steps cost about the same; a lookup table is substantially faster. For the 1.2 km medium, the 4-segment estimate improves markedly over 16 midpoint steps, but its bound remains too wide to call that estimate certified at a tight tolerance.

GPU reference values exceed nominal computed bounds by up to \(5.4\times10^{-7}\), exposing floating-point/reference effects. A formal certificate needs directed-rounding or a proven evaluation-error budget. The code does not conceal that result.

The right initial use is **adaptive generation and validation of atmosphere tables**, plus special dynamic media where caching fails. It is not a reason to replace a good runtime atmosphere LUT. Adding Rayleigh and Mie optical depths preserves interval ordering, but single/multiple scattering, ozone, clouds, planet shadow, and aerial perspective still require their own treatment.

Implementation: [atmosphere.ts](/Users/ryanwible/projects/wrela/tools/transport-research/atmosphere.ts), [gpu-atmosphere.ts](/Users/ryanwible/projects/wrela/tools/transport-research/gpu-atmosphere.ts).

## 6. Lighting: exact cheap transport plus an unbiased residual

Approximation need not mean permanently losing difficult lighting. Given a fixed predictor P with known integral, estimate the remaining radiance:

\[
\widehat I=\int P(x)\,dx+
\frac1N\sum_{j=1}^{N}\frac{F(X_j)-P(X_j)}{p(X_j)}.
\]

It is unbiased if p has support wherever the residual is nonzero and the predictor is fixed independently of the samples used for correction. Reusing training samples without a correction or independent fold can introduce bias. Clipping negative corrections also changes the expectation.

For uniform samples, variance is
\(\operatorname{Var}(F-P)/N\). If the compiler proves
\(|F-P|\le\epsilon\), variance is at most \(\epsilon^2/N\).

The tested water-light response uses a 128-mode unfiltered predictor. On 65,536 independent seeded samples, raw variance is 0.03575 and residual variance 0.001006: a 35.5× reduction. The corrected mean is 0.069112 versus independent reference 0.069108; its estimated standard error is 0.000124. This agreement does not prove accuracy to the difference of those two means.

Predictor evaluation costs work. For this cheap GGX response, 128 terms may not be an economical control variate. The target use is expensive shadow/indirect-transport evaluation, where a compiler-produced base can be much cheaper than the residual's source computation. No global illumination renderer or GI speedup was demonstrated.

Useful compiler rules:

- Multiply correlated light, material, and visibility programs before integrating.
- Eliminate a zero-visibility domain only when its geometric certificate is valid for the light's rays.
- Keep an unoccluded analytic base when visibility is uncertain, then sample the full signed residual.
- Allocate residual samples according to measured variance and cost, not just the number of lights.

For tile j with importance weight \(w_j\), residual standard deviation \(\sigma_j\), and cost \(c_j\), minimizing weighted variance under budget B gives the usual optimal allocation

\[
n_j=
\frac{B\,\sigma_j\sqrt{w_j/c_j}}
{\sum_\ell\sigma_\ell\sqrt{w_\ell c_\ell}}.
\]

The research opportunity is producing a low-variance residual from authored structure. The allocation and control-variate identities themselves are established mathematics.

## 7. The compiler architecture this suggests

A realization should carry more than a mesh and material ID. It should carry:

| Contract | What it controls |
|---|---|
| Authored phase basis and affine footprint domain | Which spatial/temporal integrals are valid |
| Response or distribution representation | Phase polynomial, positive amplitude, slope atlas, direct shader |
| Approximation budget | Source fit, finite-spectrum truncation, parameter reuse, numerical evaluation |
| Visibility proof domain | Opaque interior, camera/light, transform and pose validity |
| Residual sampler | Unbiased correction when the base representation is insufficient |
| Measured kernel cost | Hardware-specific choice among valid lowerings |

The compiler should choose the cheapest representation that meets the current error target. Wrela's authored meaning stays independent of whether the realization uses triangles, analytic geometry, a distribution atlas, or an integrated response program.

A practical integration order follows the evidence:

1. Add phase and footprint metadata to water/material lowering. Preserve source expressions rather than committing to filtered normals.
2. Add the phasor response path for reusable, sufficiently unresolved regions, with direct or texture alternatives selected by measured error/cost.
3. Add field-derived rigid opaque occluders and survivor compaction before expensive geometry work. Benchmark against conventional Hi-Z before making claims about the broader industry.
4. Develop the view-independent distribution path, including finite-footprint coherence and masking validation. This addresses the largest cache-dimension problem.
5. Use atmosphere bounds to validate or adapt lookup construction.
6. Add lighting residuals only where measured total cost beats direct sampling.

Acceptance requires moving-camera and moving-light tests, disocclusion, material edits, roughness extremes, multiple scattering, full-frame timing, cross-vendor GPUs, and actual art-directed scenes. Those are concrete unanswered questions, not evidence that the measured kernel wins will transfer automatically.

## Prior art and novelty boundary

- [Frequency Domain Normal Map Filtering, Han et al.](https://www.cs.columbia.edu/cg/normalmap/index.html) already formalizes nonlinear normal-map filtering through distributions and BRDF convolution. This research does not claim to invent distribution filtering.
- [Approximate Program Smoothing, Yang and Barnes](https://arxiv.org/abs/1706.01208) already compiles shader programs into statistical smoothing approximations. Wrela's proposed phase algebra retains explicit correlations and footprint-dependent mode selection.
- [Filtering After Shading With Stochastic Texture Filtering](https://arxiv.org/abs/2407.06107) is directly relevant to shading before filtering. Stochastic sampling remains an important baseline and fallback.
- [Linearly Transformed Cosines, Heitz et al.](https://eheitzresearch.wordpress.com/415-2/) provides analytic approximations for polygonal-light integration. An area-light implementation should compare with LTC, not only brute-force samples.
- [Lightcuts, Walter et al.](https://www.graphics.cornell.edu/~bjw/lightcuts.pdf) already combines hierarchical light clustering with controlled approximation. No invention of light clustering is claimed.
- [Hierarchical Z-Buffer Visibility, Greene et al.](https://www.cs.cmu.edu/afs/cs/academic/class/15869-f11/www/readings/greene93_hierarchicalz.pdf) establishes hierarchical visibility rejection. The Wrela-specific question is compiler-generated interior certificates and their cost.
- [A Scalable and Production Ready Sky and Atmosphere Rendering Technique, Hillaire](https://sebh.github.io/publications/egsr2020.pdf) is a credible atmosphere baseline. The bounded optical-depth experiment complements such lookup-based architectures.
- [Understanding the Masking-Shadowing Function, Heitz](https://jcgt.org/published/0003/02/03/paper.pdf) makes clear why an NDF alone is not the complete microfacet model.
- [Stable Geometric Specular Antialiasing, Tokuyoshi and Kaplanyan](https://www.jcgt.org/published/0010/02/02/) is a relevant slope/projected-normal filtering comparison for future integration.

The candidate contribution is the combined compiler design: **exact finite-phase integration, positive coherence algebra, analytic GGX/field distribution lowering, CSG-derived visibility certificates, and independently corrected residual transport**. Originality beyond that specific construction has not been established by an exhaustive literature review.

## Reproduction

Run from the repository root:

~~~sh
bun test tools/transport-research/transport.test.ts
bun tools/transport-research/bench.ts
bun tools/transport-research/algebra-bench.ts
bun tools/transport-research/slope-bench.ts
bun tools/transport-research/gpu.ts
bun run check
bun test
~~~

The CPU benchmark creates the output directory. The GPU runner also creates it and uses the repository's isolated browser/GPU lease. Raw files go into output/transport-research. The --visibility option reruns only the visibility experiment while retaining the previous water/atmosphere results; it requires an existing GPU report.

The plotting script uses NumPy and Matplotlib. Versioned JSON is a measurement snapshot rather than a portable performance guarantee. Research files are isolated under tools/transport-research; production interfaces and the ongoing renderer work were not changed.
