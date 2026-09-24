# Wrela rendering compiler
## Final research findings and implementation design

21 September 2026 · Research prototypes and proposed production architecture

**Recommendation:** make the compiler produce several ways to realize the same authored world, including programs that directly integrate its unresolved appearance. Select among them using their validity, measured error, and measured GPU cost. Preserve authored geometry, material correlations, phase relationships, and visibility facts until the compiler has used them.

The latest pass adds two useful constructions:

1. **A phase warp for coherent GGX highlights.** It removes the narrow highlight from the difficult part of the integration problem. Four evaluations follow changing lights and views without a response texture for every direction. In the tested domain, this is **21.8× faster and 52.7× lower in RMS error** than 256 regular phase samples at roughness 0.06.
2. **Stable spherical-lens moments for a finite sun.** A single analytic overlap supplies both visible solid angle and the directional moment needed for diffuse lighting. It costs **0.00410 ms versus 0.04608 ms** for 256 visibility rays over 16,384 queries, with much lower error.

These extend the earlier geometry, filtered PBR, crowd, and atmosphere experiments. The result is a concrete implementation program, with executable mathematical prototypes alongside the existing renderer. The production renderer has not been changed by this research.

**The ambition remains visual quality beyond today's AAA games on ordinary hardware. The evidence establishes narrower kernel wins, not that final product outcome.** In particular, the current renderer generally does much less shading than a 256-sample reference. The new paths can make better filtering affordable; the comparisons are not speedups over its current one-sample material shader.

### Reading guide

- Sections 1–3: decisions, evidence, and the common compiler model.
- Sections 4–8: implementable algorithms for geometry, water/PBR, sunlight, visibility, and atmosphere.
- Sections 9–10: further mathematical extensions and GPU execution.
- Sections 11–13: exact repository integration points, rollout, and acceptance.
- Appendices: prototype inventory, reproduction, and prior art.

The earlier reports remain useful derivation records: [field realization](/Users/ryanwible/projects/wrela/docs/research/field-realization.md) and [transport compilation](/Users/ryanwible/projects/wrela/docs/research/transport-compilation.md). This document is the consolidated integration recommendation and supersedes their sequencing where the latest experiments change the decision.

## 1. What to build

| Priority | Production capability | Why it earns its place | Status |
|---|---|---|---|
| 1 | Alternative static primitive realizations, including analytic quadrics and cheap parametric meshes | Large savings against current extracted meshes; analytic silhouettes remain accurate close up | CPU/GPU prototypes |
| 1 | Explicit phase and footprint metadata for water/material programs | Enables correct filtering of nonlinear, correlated appearance | CPU/GPU prototypes for restricted expressions |
| 2 | Four/eight-node warped phase integration for eligible sharp water highlights | Handles changing view/light without a high-dimensional response cache | New CPU/GPU prototype |
| 2 | Reusable phase-response programs for broad unresolved footprints | Very cheap runtime integration where parameter reuse is valid | CPU/GPU prototype |
| 2 | Rigid opaque interior proxies, survivor compaction, and indirect expensive work | Removes invisible work using information available to the field compiler | CPU/GPU prototype; compare against Hi-Z |
| 3 | Finite-sun lens evaluation for actual spherical blockers and bounded proxy cases | Stable soft-shadow coverage and integrated diffuse direction | New CPU/GPU prototype |
| 3 | Adaptive atmosphere-table construction using optical-depth intervals | Improves table generation and validation while retaining cheap runtime lookup | CPU/GPU building block |
| Later | General blended implicit rendering, phase-distribution atlases, residual GI, animation compression | Potentially broad gains, but substantial remaining error/cost questions | Mixed prototypes and derived proposals |

Start with a **single upgraded Winter valley scene**: the existing bunny and trail, wet stones, a brook, low sun, thin mist, and dense vegetation/actors behind terrain. Use it to connect geometry, water, sunlight, and visibility. Keep the playable game and existing capture/completeness contracts as acceptance criteria.

Do not begin by replacing every mesh, every shadow, or the entire material model. The conventional controls already show that there is no universally best representation.

## 2. Evidence and its practical meaning

### 2.1 Final-pass measurements

Hardware: Apple M4, macOS arm64, Chrome hardware WebGPU, Metal 3. Each GPU timing is the median of nine alternating trials after warmup, with 64 dispatches per trial. Approximately 1.024 microseconds of effective timestamp quantization remains. Preparation inside a shader is timed; driver compilation, CPU submission, readback, and offline reference generation are separate.

The moving-highlight fixture contains 16,384 independent view/light queries over a complete coherent wave orbit. All queries also have an independent CPU reference with 8,192 double-precision phase samples. Selected references were checked at 32,768 samples; the roughness-0.06 convergence discrepancy was below \(1.4\times10^{-12}\).

| Roughness / method | GPU ms | RMS radiance error against CPU reference | Interpretation |
|---|---:|---:|---|
| 0.06, 256 regular samples | 0.223232 | \(6.490\times10^{-4}\) | Accuracy/cost comparison |
| 0.06, 4 randomized warped samples | **0.010240** | **\(1.231\times10^{-5}\)** | **21.8× faster; 52.7× lower error** |
| 0.06, 8 randomized warped samples | 0.015360 | \(7.405\times10^{-6}\) | More work, lower error |
| 0.06, 4 fixed warped nodes | 0.009216 | \(4.880\times10^{-6}\) | Better in this fixture, but a biased quadrature rule |
| 0.06, analytic pole approximation only | 0.007168 | \(4.911\times10^{-5}\) | Useful predictor; no source-wide error certificate |
| 0.04, 256 regular / 4 randomized warped | 0.202752 / 0.009216 | \(3.783\times10^{-2}\) / \(3.380\times10^{-6}\) | Sharp stress case; 0.04 is below the current shader's roughness floor |
| 0.18, 64 regular samples | approximately 0.05 | \(4.182\times10^{-8}\) | A broad lobe is easy for ordinary quadrature |
| 0.18, 4 randomized warped samples | 0.009216 | \(3.021\times10^{-4}\) | Use another path when this error is unacceptable |

The enormous error ratio at roughness 0.04 is largely a comparison against severe uniform-sampling aliasing. The warped result also approaches FP32 precision limits. The roughness-0.06 comparison is the more useful headline.

Across 32 additional random seeds, four-node warped RMS error at roughness 0.06 averaged \(1.241\times10^{-5}\) in root-mean-square against the GPU reference. Eight nodes gave \(7.824\times10^{-6}\). These are seed-replicated kernel tests, not moving-camera video or temporal reconstruction tests. The GPU reference itself differs from the CPU reference by \(2.933\times10^{-6}\) RMS.

For sun visibility, the source radius is 0.00465 radians. Occluder angular radii range from 0.02 to 0.2 radians, with queries concentrated around the penumbra:

| Method | GPU ms / 16,384 queries | Visible-fraction RMS error |
|---|---:|---:|
| 9 precomputed sun rays | 0.004096 | \(4.824\times10^{-2}\) |
| 64 precomputed sun rays | 0.013312 | \(1.095\times10^{-2}\) |
| 256 precomputed sun rays | 0.046080 | \(3.644\times10^{-3}\) |
| Planar disk overlap | 0.003072 | \(1.264\times10^{-5}\) |
| Spherical lens, including first moment | **0.004096** | **\(1.801\times10^{-6}\)** |

The spherical lens is **11.25× faster than the 256-ray fixture**, with about 2,023× lower visible-fraction error. A planar lens is a strong cheaper control; the roughly one-microsecond difference is near timing resolution. These are sphere-direction tests, not scene ray traversals, shadow-map timings, or a comparison with production PCF.

![Final-pass measurements](/Users/ryanwible/projects/wrela/docs/research/rendering-compiler-final-evidence.png)

![Moving highlight parameter plots](/Users/ryanwible/projects/wrela/docs/research/rendering-compiler-final-highlights.png)

The images above plot radiance as view and light parameters change. They are **not game screenshots**. Numerical error uses unclipped linear radiance; the display uses a shared logarithmic scale.

### 2.2 Earlier results retained in the design

| Experiment | Result | Boundary of the claim |
|---|---|---|
| 4,096 reference stones: extracted mesh → exact quadric | 12.517 → 0.213 ms, **58.8×** | Geometry pass only |
| Same stones: good 120-triangle parametric LOD → quadric | 0.262 → 0.213 ms, **1.23×** | Much of the previous gain comes from choosing a better representation |
| One close-up stone | Depth RMS 7.393 mm → 0.0184 mm; normals 0.469° → 0.00135° | Independent geometric reference; one grazing analytic miss remains |
| Regional field evaluation | **5.14×** on bunny; **30.2×** on synthetic 64-part field | Includes region lookup; single primitive becomes slower |
| Factored blend approximation | About **10×** lower residual in accepted blend regions | Bunny coverage only 27.9%; all-fallback-inclusive improvement **1.27×** |
| Distant water, 16 phase modes | **80.4×** faster and **14×** lower error than 256 shading samples | 65,536 fixed-view/light queries; preprocessing excluded |
| Same water vs response LUT | LUT costs 0.00410 ms vs program 0.01024 ms | Program has 7.3× lower error; LUT wins if its error is acceptable |
| Correlated PBR material, 64 modes | **121×** lower RMS error than 256 samples | CPU accuracy, synthetic harmonic material, fixed light/view |
| Crowd, 99.3% hidden | 2.38285 → 0.02560 ms, **93.1×** | Culling, compaction, indirect four-influence skinning; favorable barrier scene |
| Crowd, 51.4% / 0% hidden | **2.06× / approximately 1.00×** | No universal large crowd speedup |
| Two-wave GGX slope atlas | **0.547%** relative L2 density error | Distribution test, not complete BRDF integration |
| Atmosphere intervals | Tight real-arithmetic bounds; **no general runtime win over a LUT** | Optical depth, not a finished sky |

Full snapshots: [final pass](/Users/ryanwible/projects/wrela/docs/research/rendering-compiler-final-results.json), [geometry](/Users/ryanwible/projects/wrela/docs/research/field-realization-results.json), [earlier transport](/Users/ryanwible/projects/wrela/docs/research/transport-compilation-results.json).

### 2.3 Failed cases that determine policy

- Nearly collinear phase ellipses break the cheap warped approximation. In a 128-case CPU stress set, four warped samples had RMS error **0.0259**, versus **0.00000202** for 256 regular samples. Eight warped samples were worse in that particular random realization. This is a conditioning/variance failure, not evidence that more samples universally hurt.
- At roughness 0.18, ordinary 64-node quadrature reaches the numeric floor while four warped nodes do not. A sharp-highlight technique should not become the default for every material.
- A 16-mode phase polynomial has close-up water error 0.0858, versus 0.0000409 for 64 direct samples. Compression succeeds after sufficient filtering.
- Dense phase-response baking grows exponentially with the number of independent phases and becomes expensive across moving light/view parameters.
- A slope distribution does not encode the full correlation of masking, visibility, material parameters, and height.
- The first spherical-lens moment implementation suffered FP32 cancellation. Its normalized lateral-moment RMS error was \(2.79\times10^{-4}\). Algebraic cancellation before evaluation reduced it to \(4.39\times10^{-9}\). Mathematical equivalence alone does not make a good GPU kernel.
- Small atmosphere FP32 errors exceeded nominal interval endpoints by up to \(5.4\times10^{-7}\). Real-arithmetic inclusion is not a floating-point certificate.

Do not multiply the speedup factors in these tables. If an improved pass occupies fraction \(f\) of frame time and speeds up by \(s\), the ideal total speedup is
\[
S=\frac{1}{1-f+f/s}.
\]
For example, a 59× improvement to 20% of a frame gives only about 1.24× overall.

## 3. The compiler's new target: a bounded rendering query

A useful compilation unit is:

> Given a spatial/parameter domain, evaluate geometry or integrated radiance with stated assumptions, error evidence, resource cost, and a fallback.

Separate four objects:

1. **Meaning:** authored field, water, material, motion, and environment.
2. **Facts:** value ranges, exact zero sets, phase identities, opaque interiors, derivative bounds, dependency margins.
3. **Realizations:** meshes, quadrics, short field programs, phase polynomials, warped quadrature, density atlases, atmosphere tables.
4. **Policy:** choose the cheapest currently valid realization satisfying the requested quality.

This separation prevents a convenient approximation from silently changing the authored world.

### Error has several units and several kinds of evidence

Keep geometry depth, silhouette coverage, normal angle, linear radiance, transmittance, and temporal history error distinct. Millimetres cannot be added to radiance RMS.

For a radiance approximation, a useful decomposition is
\[
\epsilon_{\rm total}\le
\epsilon_{\rm sourceFit}+
\epsilon_{\rm truncation}+
\epsilon_{\rm parameterReuse}+
\epsilon_{\rm footprint}+
\epsilon_{\rm numeric}.
\]
Only add quantities with compatible units and a valid compositional bound. A stochastic estimator instead carries variance/confidence information and any known bias; its RMS observation is not a deterministic maximum-error guarantee.

Record evidence as **proved in real arithmetic**, **bounded including numeric evaluation**, **measured on a stated domain**, or **unknown**. A certificate authorizing removal of potentially visible content requires the appropriate conservative bound. Empirical fit quality may authorize an explicitly approximate shading option.

### Preserve covariance

The object to filter is the complete response:
\[
\mathbb E[L\,V\,f_r\,(\mathbf n\cdot\mathbf l)_+],
\]
not a product of separately averaged light, visibility, BRDF, and normal. The compiler may separate factors only when independence, constancy, or an error bound justifies it.

This principle ties water, layered materials, shadows, and lighting together. It is also why simply increasing roughness after blurring the normal cannot be the final quality solution.

## 4. Geometry: exact surfaces with cheaper local programs

### 4.1 Factor the implicit value without moving its zero

Wrela's ellipsoid leaf uses
\[
q=\|p/s\|,\quad r=\|p/s^2\|,\quad f=q(q-1)/r.
\]
It is not a global signed distance. For anisotropic axes, its center limit depends on direction. Retain the current compiler's conservative distinction between implicit fields and distance bounds.

Factor
\[
Q=q^2-1,\qquad w=\frac{q}{r(q+1)},\qquad f=Qw.
\]
Away from the center, \(w>0\). Approximate only the positive multiplier:
\[
\widehat f=Q\widehat w,\qquad \widehat w>0.
\]
An isolated primitive's zero and sign remain exact. Hard min/max/subtraction CSG also preserves the zero set and sign under positive leaf rescaling. Material branch selection can still change, so retain exact provenance or a separate branch certificate.

For Wrela's polynomial smooth minimum
\[
S_k(a,b)=\frac{a+b}{2}-\frac{k}{4}
-\frac{(a-b)^2}{4k}\quad\text{when }|a-b|<k,
\]
the two partial derivatives are nonnegative and sum to one. Thus
\[
|S_k(a,b)-S_k(\widehat a,\widehat b)|
\le\max(|a-\widehat a|,|b-\widehat b|).
\]
Hard min/max and negation obey the same maximum-norm nonexpansion. Through an unchanged ordered expression,
\[
|F-\widehat F|\le\max_i |Q_i|\,|w_i-\widehat w_i|.
\]
The error does not accumulate once per tree level. Smooth-union reassociation is still forbidden: preserve authored order.

### 4.2 Regional specialization and polynomial rays

Propagate actual value intervals. A smooth-union branch with \(u_a+k<l_b\) is exactly \(a\) throughout the region; retain strict margins to protect ties/provenance. Rigid transforms fold into coefficients.

On a ray, constant-multiplier quadrics \(a(t),b(t)\) produce a quartic active-blend equation:
\[
(a-b)^2-2k(a+b)+k^2=0.
\]
Linear/quadratic multipliers raise this to degree six/eight. Isolate candidate roots within the spatial region, validate the active branch, and check the original-field residual. Unblended primitive roots must also be considered. Nested blends can explode in degree; impose a cap and use the mesh or short evaluator.

A bracket and certified one-sign derivative \(|dF/dt|\ge m>0\) convert a residual bound \(E\) into a depth bound \(E/m\). This fails near tangencies unless another bound is available. Root isolation, nearest-hit ordering, region coverage, and numeric margins are mandatory production work.

### 4.3 Realization policy

For a region that reduces to a static sphere/ellipsoid, compare:

- a conventional parametric mesh at a suitable geometric error;
- the existing extracted mesh;
- analytic ray intersection behind a conservative raster proxy.

The analytic prototype uses a 12-triangle box, exact quadric depth, and analytic normals. Proxy coverage must include near-plane intersections, camera-inside cases, and grazing silhouettes. Writing fragment depth can change early-depth efficiency; include overdraw and shadow passes in measurements.

Never deform a static field quadric per bone and call it equivalent to the current skinned mesh. Linear-blend skinning is a different realization with different semantics.

Implementation sources: [local-program.ts](/Users/ryanwible/projects/wrela/tools/field-research/local-program.ts), [gauge.ts](/Users/ryanwible/projects/wrela/tools/field-research/gauge.ts), [blend-polynomial.ts](/Users/ryanwible/projects/wrela/tools/field-research/blend-polynomial.ts).

## 5. Water and PBR: integrate the authored program

### 5.1 General finite-footprint phase integration

Let authored phases over a locally affine pixel and shutter be
\[
\boldsymbol\phi=\boldsymbol\phi_0+\mathbf a u+\mathbf b v+\mathbf c\tau,
\qquad u,v,\tau\in[-1/2,1/2].
\]
For an integer mode \(\mathbf k\),
\[
C_{\mathbf k}=
\operatorname{sinc}(\mathbf k\cdot\mathbf a/2)
\operatorname{sinc}(\mathbf k\cdot\mathbf b/2)
\operatorname{sinc}(\mathbf k\cdot\mathbf c/2).
\]
Compile the complete response as
\[
P(\boldsymbol\phi)=c_0+
2\operatorname{Re}\sum_{\mathbf k\in K}c_{\mathbf k}e^{i\mathbf k\cdot\boldsymbol\phi}.
\]
Then its filtered value is the same sum with every term multiplied by \(C_{\mathbf k}\). Discarded finite-polynomial terms have the bound
\[
\epsilon_{\rm omitted}\le2\sum_{\mathbf k\in D}|c_{\mathbf k}C_{\mathbf k}|.
\]
This is a truncation bound for the polynomial, not a source-fit proof. Select modes after applying the footprint: beat frequencies can survive while each original carrier disappears.

The earlier 16-mode/80.4× result uses this construction. It is an excellent path for broad footprints and reusable parameters. The roughly 33–37 ms browser preparation cost and view/light validity domain must be amortized explicitly.

For moving parameters, bound or measure response variation over a parameter cell. Do not build a cache keyed only by material ID when the coefficients also depend on view and light.

### 5.2 Exact slope moments remain useful metadata

For slopes \(s=\sum_i v_i\cos\phi_i\), the first and second moments over the affine footprint are
\[
\mu=\sum_i v_i\cos\phi_i\,C_i,
\]
\[
M=\frac12\sum_{i,j}v_iv_j^T
\left[\cos(\phi_i-\phi_j)C_{i-j}
+\cos(\phi_i+\phi_j)C_{i+j}\right],\quad
\Sigma=M-\mu\mu^T.
\]
Eight waves have only 36 unordered pairs. The compiler can precompute coefficients and sum/difference frequencies.

Use moments for selection, roughness estimates with explicit approximation status, and debugging. They do not determine the exact GGX-filtered response. GGX micro-slopes also have heavy tails; do not assume their untruncated second moment is a finite Gaussian variance.

### 5.3 New construction: exact coherent slope orbits

If the integration coordinate advances several phase carriers equally,
\[
s(\theta)=\mu+\sum_j v_j\cos(\theta+\delta_j)
=\mu+a\cos\theta+b\sin\theta,
\]
where
\[
a=\sum_jv_j\cos\delta_j,\qquad
b=-\sum_jv_j\sin\delta_j.
\]
Any number of such waves becomes one ellipse in slope space. This identity is exact.

**Eligibility is essential.** A complete coherent orbit is not an arbitrary eight-wave pixel footprint. It can arise along one pixel axis with equal projected phase rates, in a conditional phase integral, or over a complete common temporal period. A transverse footprint needs a separate integral. Different carrier rates generally do not form an ellipse. Equal spatial wave vectors in a true heightfield give collinear slope vectors and need the degenerate fallback.

For a phase interval of length \(L=2\pi m+r\), split the integral into \(m\) complete periods plus the remaining interval. Weight by their actual lengths. Discarding a partial period or treating almost-equal carrier rates as equal biases the result.

### 5.4 Factor the sharp GGX denominator exactly

Let \(v,l\) be unit view/light directions in the macro frame, \(h=(v+l)/\|v+l\|\), and
\[
n(s)=\frac{(-s_x,1,-s_z)}{\sqrt{1+\|s\|^2}},\quad
\alpha=\text{roughness}^2,\quad \beta=1-\alpha^2.
\]
Write \(h_t=(h_x,h_z)\), and define
\[
A=I-\beta h_th_t^T,\quad
d_A=h_y^2+\alpha^2\|h_t\|^2,\quad
s_c=-\frac{\beta h_yh_t}{d_A},\quad
\delta=\frac{\alpha^2}{d_A}.
\]
Then the isotropic GGX NDF factors as
\[
D(s)=\frac{\alpha^2(1+\|s\|^2)^2}{\pi Q(s)^2},
\qquad Q(s)=\delta+(s-s_c)^TA(s-s_c).
\]
For the full single-scattering direct specular contribution,
\[
R(s)=\frac{H(s)}{Q(s)^2},\qquad
H(s)=\frac{\alpha^2(1+\|s\|^2)^2G(s)F}{4\pi(n\cdot v)}.
\]
Set \(R=0\) when \(n\cdot v\le0\) or \(n\cdot l\le0\). Fresnel depends on \(v\cdot h\), so it is constant along this fixed-direction orbit. Smith masking remains inside \(H\); it was not replaced by an averaged-normal approximation.

The prototype compares this factorization with conventional normalized-vector GGX. Computing \(Q\) as a positive shifted quadratic avoids the catastrophic subtraction in \(1-(1-\alpha^2)(n\cdot h)^2\) near a sharp highlight.

### 5.5 Construct a normalized pole model

Let \(B=[a\ b]\), \(x=B^{-1}(s_c-\mu)\), \(M=B^TAB\), and
\[
m=\sqrt{\det M}=|\det B|\sqrt{d_A},\qquad
\gamma^2=\delta/m.
\]
An initial model is
\[
q_p(\theta)=m\left[\gamma^2+\|u(\theta)-x\|^2\right],
\quad u=(\cos\theta,\sin\theta).
\]
With the phase origin rotated toward \(x\), this becomes
\[
q_p=m[\ell+2r_x(1-\cos\theta)],
\quad
\ell=\gamma^2+(r_x-1)^2,\quad h_p=\gamma^2+(r_x+1)^2.
\]
The notation \(h_p\) here denotes the maximum scalar denominator, not the half vector.

Its inverse-square integral is known:
\[
J=\mathbb E_\theta[q_p^{-2}]
=\frac{(\ell+h_p)/2}{m^2(\ell h_p)^{3/2}}.
\]
The balanced model has a conditioning bound: with
\(\kappa=\sqrt{\operatorname{cond}M}\),
\(q_p/Q\) lies between \(1/\kappa\) and \(\kappa\). This controls the denominator ratio, not the entire response error.

Improve the model around a stationary minimum \(\theta_*\) of \(Q(s(\theta))\):
\[
q_p(\theta)=Q_*+Q_*''[1-\cos(\theta-\theta_*)].
\]
It matches the value and second derivative of the true denominator. The prototype takes five guarded Newton-like updates and checks positive curvature and a small final gradient. A failed fit keeps the balanced model. Preparation is performed inside the measured GPU kernel.

### 5.6 The phase warp: turn a sharp pole into a first harmonic

Write either model in the common form
\[
q_p(\theta)=m[A_p-B_p\cos\theta],\quad
\ell=A_p-B_p>0,\quad h_p=A_p+B_p,\quad \rho=\ell/h_p.
\]
Use a Möbius change of angle:
\[
\tan(\theta/2)=\sqrt{\rho}\tan(\psi/2).
\]
Avoid tangent evaluation in the final kernel. For \(c=\cos\psi,\ s=\sin\psi\), let
\[
d=(1+c)+\rho(1-c),
\]
\[
\cos\theta=\frac{(1+c)-\rho(1-c)}d,\qquad
\sin\theta=\frac{2\sqrt\rho\,s}d,\qquad
q_p=\frac{2m\ell}{d}.
\]
Rotate this angle by the model's axis. Its Jacobian gives the exact transformed integrand
\[
\boxed{
I=J\,\mathbb E_\psi
\left[
H(s(\theta(\psi)))
\left(\frac{q_p}{Q}\right)^2
\frac{d}{1+\rho}
\right].
}
\]
All factors are nonnegative for a nonnegative reflection response.

The key simplification is
\[
\frac{d}{1+\rho}=1+\frac{1-\rho}{1+\rho}\cos\psi.
\]
If \(Q=q_p\) and \(H\) is constant, the transformed function contains only a constant and first harmonic. **Every equally spaced lattice with at least two nodes integrates that ideal pole exactly, regardless of its width or lattice shift.**

That explains the improvement over spending samples uniformly in the original phase. Matching the denominator removes the difficult narrow feature before sampling.

For \(N=4\) or \(8\), choose
\[
\psi_j=2\pi(j+U)/N,\qquad U\sim\operatorname{Uniform}[0,1).
\]
The estimator is unbiased in real arithmetic because the randomized lattice integrates any integrable transformed response correctly in expectation. It stays nonnegative for this response. A fixed \(U=1/2\) is deterministic quadrature and can be useful with measured or bounded bias.

This also gives a compiler-facing error criterion. If the transformed response has Fourier coefficients \(t_k\), fixed-lattice error is bounded by \(\sum_{m\ne0}|t_{mN}|\), and randomized-lattice variance is
\[
\operatorname{Var}\widehat I_N=\sum_{m\ne0}|t_{mN}|^2.
\]
These are established Fourier/lattice identities applied to the compiler's transformed response. A production implementation can use a certified transformed tail or a measured domain sweep to choose \(N\). Four/eight agreement alone is not a proof: both grids can alias the same mode.

### 5.7 Why the earlier rejection proposal is retained only as a control

The same pole model yields a normalized inverse-square proposal. A bounded four-attempt rejection sampler plus uniform fallback was implemented and tested. Correcting the exact mixture density preserves expectation; silently truncating rejection and retaining the old density would not.

It is slower and noisier than the final warp on the principal fixtures. The useful lesson is to compile a smoother integration coordinate, not merely a more concentrated random sampler. The rejection implementation remains as an independent mathematical control.

### 5.8 Production guards and extensions

Start with isotropic GGX reflection, sharp lobes, a well-conditioned nondegenerate orbit, and a complete conditional phase period. The moderate stress domain had \(\kappa\le5.43\); thin failures reached \(\kappa>35{,}000\). A preliminary \(\kappa\le8\), roughness \(\le0.08\) admission rule is a **proposed engineering guard**, not a proven quality guarantee.

Use ordinary/adaptive integration for collinear orbits, broad lobes, horizon crossings outside the validated domain, unresolved secondary poles, or incomplete phase intervals. Do not force this path through arbitrary normal maps.

Colored Fresnel shares the denominator/warp across RGB. Integrate broad diffuse terms separately; concentrating all diffuse work around a sharp specular pole can increase variance. Visibility, refraction, or material variation along the footprint must remain in the integrand or have their own justified approximation.

The GPU experiment shades a slope orbit, not a full water surface. It does not solve geometry displacement, shore transitions, depth-dependent absorption, foam, caustics, refraction, or reflected-scene visibility.

Implementation: [coherent-ggx.ts](/Users/ryanwible/projects/wrela/tools/transport-research/coherent-ggx.ts), [gpu-coherent.ts](/Users/ryanwible/projects/wrela/tools/transport-research/gpu-coherent.ts), [tests](/Users/ryanwible/projects/wrela/tools/transport-research/coherent-ggx.test.ts), [adversarial sweep](/Users/ryanwible/projects/wrela/tools/transport-research/orbit-stress.ts).

### 5.9 Distribution and positive-response alternatives

For fully independent uniform phases and additive GGX micro-slopes,
\[
\Phi(k)=qK_1(q)\prod_jJ_0(k\cdot v_j),\qquad q=\alpha\|k\|.
\]
For one coherent group, replace the product by
\[
J_0\!\left(\sqrt{(k\cdot a)^2+(k\cdot b)^2}\right),
\]
with a translation factor for the mean slope. A circular orbit of radius \(R\) convolved with GGX has density
\[
p(s)=\frac{\alpha^2(\alpha^2+R^2+\|s\|^2)}
{\pi\{[\alpha^2+(\|s\|-R)^2][\alpha^2+(\|s\|+R)^2]\}^{3/2}}.
\]
The circular formula is CPU-tested against independent angular integration. It is a slope-density identity, not the full Smith/Fresnel BRDF.

The prior 256² density atlas is view-independent and inexpensive to query, but finite-domain images, spectral truncation, interpolation, and the NDF-to-BRDF closure require separate budgets. Retain finite-footprint resonances instead of declaring phase independence whenever a wave becomes subpixel.

A different option compiles nonnegative radiance as \(|A(\phi)|^2\). Its exact footprint integral is \(z^*Gz\), with
\(G_{ij}=C_{k_j-k_i}\). This Gram matrix is positive semidefinite. Grouping equal integer orbit frequencies yields exact sums of squares over complete periods. Eigenvalue truncation gives a spectral-norm error bound; arbitrary entrywise thresholding can destroy positivity. The algebra is implemented, but GPU eigensolver compression is not.

Sources: [spectral.ts](/Users/ryanwible/projects/wrela/tools/transport-research/spectral.ts), [positive-response.ts](/Users/ryanwible/projects/wrela/tools/transport-research/positive-response.ts), [slope-atlas.ts](/Users/ryanwible/projects/wrela/tools/transport-research/slope-atlas.ts).

## 6. Finite sunlight: compile geometry into angular moments

### 6.1 Spherical visibility domains

For a receiver outside an opaque sphere, the blocked directions form a spherical cap centered toward the sphere, with angular radius \(\arcsin(r/d)\). A distant uniform sun is another cap. Their intersection gives the blocked solid angle.

Let centers be \(c_1,c_2\), radii \(r_1,r_2\in[0,\pi/2]\), and separation \(d\). Handle disjoint and contained caps directly. For a partial lens, set \(s=(r_1+r_2+d)/2\) and evaluate
\[
\alpha=2\arcsin\sqrt{
\frac{\sin(s-d)\sin(s-r_1)}
{\sin r_1\sin d}},
\quad
\beta=2\arcsin\sqrt{
\frac{\sin(s-d)\sin(s-r_2)}
{\sin r_2\sin d}}.
\]
Use clamped roundoff-safe arguments. The spherical triangle excess is
\[
E=4\arctan\sqrt{
\tan(s/2)\tan((s-r_1)/2)
\tan((s-r_2)/2)\tan((s-d)/2)}.
\]
The lens area is
\[
\Omega=4\alpha\sin^2(r_1/2)+4\beta\sin^2(r_2/2)-2E.
\]
Half-angle expressions avoid subtracting nearly equal cosines for a small sun.

### 6.2 Stable first moment

Define
\[
\mathcal S(x)=x-\sin x\cos x.
\]
The lens first moment has the compact form
\[
\boxed{
M=\int_{\rm lens}\omega\,d\omega
=\sin^2r_1\,\mathcal S(\alpha)c_1+
\sin^2r_2\,\mathcal S(\beta)c_2.
}
\]
A derivation uses \(\frac12\oint\omega\times d\omega\) around the two cap arcs. The shared chord cancels analytically. Performing that cancellation symbolically is what made the FP32 implementation accurate.

For small \(x\), evaluate
\[
\mathcal S(x)=x^3\left(
\frac23-\frac{2x^2}{15}+\frac{4x^4}{315}
-\frac{2x^6}{2835}+\frac{4x^8}{155925}+\cdots\right).
\]
The prototype switches to this series below 0.25 radians. A complete cap has
\(\Omega=4\pi\sin^2(r/2)\) and \(M=\pi\sin^2r\,c\).

Subtract the blocked moment from the full sun moment. For a constant normal with the entire sun above its tangent horizon, Lambertian irradiance is exactly proportional to \(n\cdot M_{\rm visible}\). Affine angular radiance also integrates from area plus first moment.

If the sun crosses the receiver's horizon, clip the angular domain by that hemisphere or fall back. If a sharp specular response varies across the sun, multiplying it by mean visibility is not exact; integrate their product or correct a predictor.

### 6.3 From exact spheres to conservative field proxies

An interior sphere supplies a lower bound on blocked directions; an enclosing sphere supplies an upper bound. For several opaque shapes, safe union bounds are
\[
B_{\rm lower}=\max_i\Omega_{{\rm inner},i},\qquad
B_{\rm upper}=\min(\Omega_{\rm sun},\sum_i\Omega_{{\rm outer},i}).
\]
Consequently,
\[
1-B_{\rm upper}/\Omega_{\rm sun}
\le V\le1-B_{\rm lower}/\Omega_{\rm sun}.
\]
Do not sum inner overlaps unless disjointness is established. Union area bounds do not automatically become vector-component bounds, since directional components may have either sign.

Use the lens as an exact answer for actual opaque spheres; as a visibility bracket or control variate for certified general-field proxies; and as a fast zero/full-visibility classifier where the bracket collapses. Foliage alpha, holes, deformation, and multiple partially overlapping blockers need a richer representation or ordinary shadow work.

The lens geometry is established mathematics. The useful Wrela construction is combining field-derived inner/outer facts, stable angular moments, and residual lighting policy. Primary background: [Mazonka, intersecting spherical caps](https://arxiv.org/abs/1205.1396).

Implementation: [sun-caps.ts](/Users/ryanwible/projects/wrela/tools/transport-research/sun-caps.ts), [gpu-sun.ts](/Users/ryanwible/projects/wrela/tools/transport-research/gpu-sun.ts), [independent quadrature tests](/Users/ryanwible/projects/wrela/tools/transport-research/sun-caps.test.ts).

## 7. Crowded scenes: eliminate work before deformation

The compiler can prove an interior ball by enclosing it in a cube whose full ordered-field value interval has upper bound below zero. Subtraction and opacity must participate in the proof.

For a sphere centered at \(c\), radius \(r\), and camera ray \(d=(u,v,1)\), define
\[
q=\|c\|^2-r^2,\quad p=c\cdot d,\quad
\Delta=p^2-q\|d\|^2.
\]
Its front depth is \(t=q/(p+\sqrt\Delta)\).

When \(c_z>r\), both the discriminant and reciprocal front depth are concave on the projected domain. If all four corners of a rectangular screen tile intersect the forward sphere, the tile is covered, and
\[
t(u,v)\le\max_{\rm corners}t.
\]
A candidate's nearest possible depth being farther than this upper front-depth bound throughout every tile in its projected rectangle proves it hidden.

The prototype clears tiles, builds four-corner bounds, culls conservative instance bounds, compacts survivors, and skins them with an indirect dispatch. No visibility readback drives submission. Timings include all these passes.

Production requirements:

- Preserve separate camera and light visibility. Hidden from the camera does not imply irrelevant to shadows.
- Candidate bounds contain the current pose, wind, and displacement. Existing normalized nonnegative LBS bounds are a useful starting point.
- A bind-pose interior does not automatically stay inside a moving/skinned body.
- Treat nonfinite/ambiguous data as visible; prove numeric margins before relying on culling for strict completeness.
- Do not save render time by stopping gameplay simulation or required animation events.
- Compare with a conventional Hi-Z implementation and measure certificate construction/upload cost on changing scenes.

For visible crowds, investigate shared clip/phase pose caches before a novel deformation format. A further compiler option is described in section 9; it has no measured speedup yet.

Sources: [field-occluders.ts](/Users/ryanwible/projects/wrela/tools/transport-research/field-occluders.ts), [visibility experiment](/Users/ryanwible/projects/wrela/tools/transport-research/visibility.ts), [GPU implementation](/Users/ryanwible/projects/wrela/tools/transport-research/gpu-visibility.ts).

## 8. Procedural skies, mist, and clouds

### 8.1 Compile optical-depth intervals into tables

For a spherical exponential atmosphere, log density along a straight ray obeys
\[
\ell''(s)=-\frac{b^2}{H\,r(s)^3}\le0.
\]
The chord of log density lies below it, and the midpoint tangent lies above it. Exponentiating and analytically integrating supplies density-integral bounds.

For affine log endpoints \(A,B\) over length \(h\),
\[
J(A,B)=h\,e^{\max(A,B)}
\frac{1-e^{-|A-B|}}{|A-B|},
\]
with its continuous small-difference limit. Sum segment intervals and reverse their order through \(T=e^{-\sigma\tau}\).

Use this to adapt table resolution and produce reliable reference values, concentrating work where the **transmittance** interval is too wide. A zenith exponential column integrates in one segment. Horizon and small-scale-height media need many more.

The proposed runtime remains lookup-based for the common atmosphere. Optical-depth tables alone do not create a convincing sky: add validated Rayleigh/Mie phase functions, multiple-scattering approximation, planet shadow, sun disk, exposure-consistent aerial perspective, and art direction. [Hillaire's production atmosphere](https://sebh.github.io/publications/egsr2020.pdf) is the relevant baseline.

### 8.2 Extend the compiler to heterogeneous media carefully

For a homogeneous segment with extinction \(\sigma\), source \(j\), and incoming transmittance \(T_0\), exact accumulated radiance is
\[
\Delta L=T_0j\,\frac{1-e^{-\sigma\Delta s}}{\sigma},
\]
with limit \(T_0j\Delta s\) at \(\sigma=0\).

A cloud/fog compiler can identify zero-density regions, constant or analytically integrable segments, and conservative extinction majorants. Integrate that base and sample the remaining heterogeneous contribution. Arbitrary procedural cloud noise does **not** inherit the exponential atmosphere's log-concavity proof.

This is a proposed direction, not an implemented cloud renderer. Established [residual ratio tracking](https://studios.disneyresearch.com/2014/11/19/residual-ratio-tracking-for-estimating-attenuation-in-participating-media/) already handles analytic base plus residual transmittance. Wrela's opportunity is producing a good base and valid bounds from the authored density program.

Ship a coherent atmosphere/mist solution before expanding into volumetric cloud shadows, dense multiple scattering, or weather simulation.

## 9. Further mathematics worth pursuing

The following are derived extensions with explicit experiments needed before production. They are not additional measured wins.

### 9.1 Reduce many independent phases through conditional coherent orbits

For independent uniform phases, Haar measure on the phase torus permits
\[
\phi_1=\theta,\qquad \phi_j=\theta+\delta_j.
\]
Integrating over uniform \(\theta,\delta_2,\ldots,\delta_n\) is exactly the original uniform torus integral. For fixed offsets, the slope sum is the ellipse from section 5.

This suggests a conditional estimator: sample the \(n-1\) relative phases, reduce all waves to an ellipse, and perform the cheap warped inner integral. If that inner integral were exact, the law of total variance guarantees no more variance per outer sample than an ordinary sample with the same conditioning. With finite inner quadrature, include its variance and cost.

This avoids a dense \(N^n\) phase grid. It does not remove the outer dimensionality, and many conditional ellipses may be poorly conditioned. Finite physical footprints are not automatically uniform on a high-dimensional torus. Surviving resonance terms must remain represented.

**Cheapest experiment:** authored two/eight-wave fixtures, direct phase sampling versus conditional four-node warp at equal total GPU time, including all degenerate fallbacks and finite-footprint controls.

### 9.2 Compile a residual of the rendering equation

Let \(K\) be the light-transport operator and \(E\) emitted/external radiance:
\[
L=E+KL.
\]
For a cheap compiled predictor \(P\), define
\[
R=E+KP-P.
\]
Then
\[
L=P+(I-K)^{-1}R.
\]
Estimate the residual transport with valid path sampling while evaluating the cheap base directly. The residual may be signed; clipping it biases the answer.

If a valid operator norm satisfies \(\|K\|\le\rho<1\), then
\[
\|L-P\|\le\frac{\|R\|}{1-\rho}.
\]
The proof is a geometric series. It is useful only if the norm and residual bounds are available and sufficiently tight. Nearly lossless enclosed scenes make the bound weak; a guessed global albedo bound is not enough for every transport model.

For Wrela, compile static spatial/light/material structure into the predictor: analytic direct light, zero-transfer visibility regions, low-frequency environment response, and inexpensive indirect transfer. Dynamic geometry invalidates affected domains rather than silently reusing a baked solution.

Control variates and radiance predictors have extensive prior art, including [Neural Control Variates](https://research.nvidia.com/publication/2020-11_neural-control-variates). The research question is whether the semantic compiler can make the predictor cheap and valid without training a large scene-specific model.

**Cheapest experiment:** one fixed-camera diffuse room and one moving blocker, then an outdoor valley patch. Compare complete base-plus-correction cost against direct sampling. The prior 35.5× residual variance reduction is not a GI performance result.

### 9.3 Compile animation error into shared deformation work

For normalized nonnegative LBS weights,
\[
p_i(t)=\sum_bw_{ib}[R_b(t)p_i+t_b(t)].
\]
Approximating bone transforms gives
\[
\|\Delta p_i\|\le
\sum_bw_{ib}\bigl(\|\Delta R_b\|\|p_i\|+\|\Delta t_b\|\bigr).
\]
Authored motion curves can provide interpolation/derivative bounds. A shared clip-phase cache or a compact temporal basis can therefore carry a deformation-error envelope, projected to pixels with a valid camera Jacobian bound.

Possible realization: expand transforms in a small temporal basis, precompute corresponding vertex deformation modes, and evaluate the modes for many visible instances. Account for normal error, clip blending, contacts, authoring edits, and memory bandwidth; preserve the ordinary skinner as fallback.

Low-rank animation and compressed skinning are established techniques; see [James and Twigg](https://graphics.cs.cmu.edu/projects/sma/). The proposed contribution is tying the approximation to Wrela's motion products, measured screen error, and GPU work selection.

**Cheapest experiment:** 1,000 visible copies of the real bunny, including varied phases, transitions, and hero close-ups. Compare conventional GPU skinning, a shared pose cache, and the basis path. Hidden crowds do not answer this question.

### 9.4 Error-aware allocation of residual work

For tile \(j\), cost \(c_j\), residual standard deviation \(\sigma_j\), and importance weight \(w_j\), minimizing weighted variance under budget \(B\) gives
\[
n_j=
\frac{B\,\sigma_j\sqrt{w_j/c_j}}
{\sum_\ell\sigma_\ell\sqrt{w_\ell c_\ell}}.
\]
Use independent/past-frame statistics and a positive exploration floor where residual support is unknown. Account for integer counts, tile dispatch cost, temporal correlation, and disocclusion.

The allocation identity is standard. The opportunity is supplying a much smaller residual through compiled structure. Exposure, highlight saturation, and motion sensitivity should influence priorities, but must not erase error reporting in linear radiance.

## 10. WebGPU execution design

### Fixed small kernels beat one universal interpreter

Compile a bounded set of kernel families: analytic primitive, short field program, phasor response, four/eight-node phase warp, direct fallback, lens visibility, atmosphere lookup, and visibility compaction. Bucket work by family to reduce divergence.

For the new warp, compute one sine/cosine pair in the transformed phase, then rotate it by fixed complex multiplications. Four nodes use quarter-turns; eight use fixed \(\sqrt{1/2}\) coefficients. Do not evaluate trig once per node. The tested 64/128/256-thread workgroups do not establish a universal winner.

For phase polynomials, compute fundamental phasors and reuse integer powers. Staging a coefficient array in workgroup memory did little by itself in the previous benchmark. Reducing transcendental operations mattered more.

### Memory and pass organization

Suggested frame flow:

~~~mermaid
flowchart LR
  A[Authored state and resident products] --> B[Conservative visibility and light bounds]
  B --> C[Compact work and indirect deformation]
  C --> D[Mesh or analytic geometry]
  D --> E[Footprints and response classification]
  E --> F[Compiled base lighting]
  F --> G[Residual shadows or transport where needed]
  G --> H[Atmosphere and scene-linear composition]
  H --> I[Existing HDR display pipeline]
~~~

This is a dependency diagram, not a requirement for a full deferred rewrite. Begin by adding optional specialized material/geometry paths to the existing renderer. Introduce compute shading queues only when batching savings exceed classification, intermediate-buffer, and synchronization costs.

Suggested packed records:

| Record | Initial layout | Notes |
|---|---|---|
| Coherent orbit | Two vec4 values: mean.xy/a.xy, b.xy/domain scalars | Compile static coefficients; prepare view/light-dependent quadratic at runtime |
| Phase coefficient | Integer frequency pair plus complex coefficient, 16-byte record | Keep RGB coefficient packing explicit; split common frequencies from channels |
| Interior sphere | vec4 center/radius plus instance/domain key | Preserve opacity, transform, and proof identity |
| Quadric | Symmetric coefficients or inverse axes/transform, plus provenance | Choose layout by measured bandwidth and reuse |
| Work item | Stable instance ID, product index, variant, parameter offset | No hidden dependence on CPU traversal order |

Do not assume TypeScript struct layout maps to WGSL. Specify byte offsets, alignment, matrix orientation, buffer range, and index units, and round-trip the packing in tests.

### Portability and bounded cost

- Ordinary workgroups and storage buffers are the baseline. Subgroups and f16 are optional feature-gated variants.
- Never assume subgroup width equals 32 or maps in a fixed way to local invocation IDs. Check [WGSL's subgroup and numeric rules](https://www.w3.org/TR/WGSL/).
- Footprint derivatives must be computed in legal uniform fragment control flow or supplied explicitly. Divergent special-case shading cannot invent valid derivatives.
- Retain fixed iteration counts and explicit fallback status for Newton/root isolation. Overflowing a work queue must be reported, not silently drop pixels/objects.
- Avoid CPU readbacks in the rendering critical path. Use indirect dispatch/draw after GPU compaction, with counters reset every frame.
- Positive depth atomic-min requires finite positive values and a deliberately chosen monotonic encoding. Do not reuse it blindly with reverse depth, negative values, or NaNs.
- Count all storage traffic, queue construction, buffer clears, and indirect work in end-to-end timings.
- Timestamp queries are optional in production; they are required by these research benchmark tools. When unavailable, report unavailable timing rather than fabricated precision.
- Cap shader variants and generated source size. Measure cold pipeline creation and cache reuse; move expensive fitting into workers/cooking.

## 11. Repository integration plan

### 11.1 Existing architecture to preserve

The current code already has useful boundaries:

| Existing file | Current responsibility | Required extension |
|---|---|---|
| [compiler/ir.ts](/Users/ryanwible/projects/wrela/packages/compiler/src/ir.ts) | Field instructions, bounds, provenance, evaluation cost; distinguishes implicit values from safe distance bounds | Regional value facts, positive factors, exact primitive candidates, proof dependencies |
| [compiler/products.ts](/Users/ryanwible/projects/wrela/packages/compiler/src/products.ts) | Separate geometry, material binding, motion, and binding keys | Separate appearance, occlusion, atmosphere, and optional realization keys |
| [compiler/index.ts](/Users/ryanwible/projects/wrela/packages/compiler/src/index.ts) | Document compilation and transferable buffers | New optional product orchestration and complete transfer lists |
| [model/contracts.ts](/Users/ryanwible/projects/wrela/packages/model/src/contracts.ts) | Mesh/artifact/render contracts, fidelity, measurements | Alternative realizations with domains/evidence; runtime selected-path reporting |
| [compiler/cooked.ts](/Users/ryanwible/projects/wrela/packages/compiler/src/cooked.ts) | Validated persisted artifacts and compatibility | Encode/decode/cap new buffers, domains, versions, and fallback identities |
| [compiler/versions.ts](/Users/ryanwible/projects/wrela/packages/compiler/src/versions.ts) | Compiler, product, and cook format identities | Bump relevant algorithm/format versions on integration |
| [runtime/scene-host.ts](/Users/ryanwible/projects/wrela/packages/runtime/src/scene-host.ts) | Product installation, ownership, material/water surfaces | Cache/install/invalidate added products without recompiling unrelated geometry |
| [runtime/picking.ts](/Users/ryanwible/projects/wrela/packages/runtime/src/picking.ts) | Picking selected mesh geometry | Analytic hit path when the visible realization differs materially from its mesh fallback |
| [render-webgpu/visibility.ts](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/visibility.ts) | Separate camera/light frusta and deformation bounds | Certified occlusion stage with separate camera/light validity |
| [render-webgpu/batching.ts](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/batching.ts) | Immutable rigid mesh batching; skin/water are currently separate | Group compatible product/variant work; preserve draw-range semantics |
| [render-webgpu/detail.ts](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/detail.ts) | Projected detail with 15% hysteresis; skinning excluded | Error/cost selection among additional realizations |
| [render-webgpu/scene.wgsl](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/scene.wgsl) | Water, layered procedural PBR, PCF, sky, beauty/diagnostic paths | Optional integrated response and finite-sun paths with truthful diagnostics |
| [render-webgpu/index.ts](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/index.ts) | GPU ownership, rendering, completeness, attributed timing | New resources/passes and their memory/timing ownership |
| [render-webgpu/quality.ts](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/quality.ts) | Explicit low/balanced/high cost profiles | Additional quality budgets within existing memory/upload caps |

At the inspected base, the compiler is version 2, the cook format is 1, and existing geometry/material/motion products have their own versions. Do not simply append arrays to a cooked object: update serialization, validation, transfer ownership, byte accounting, source identities, and compatibility together.

### 11.2 Proposed contracts

The following illustrates the contract shape; it is a design proposal, not code already added to the model package:

~~~ts
type ErrorEvidence =
  | { kind: "numeric-bound"; metric: string; maximum: number; domain: string }
  | { kind: "real-bound"; metric: string; maximum: number; numericError: "unknown" }
  | { kind: "measured"; metric: string; rms: number; maximum: number; fixture: string }
  | { kind: "unknown"; reason: string };

interface CompiledRenderProduct {
  key: string;
  sourceKey: string;
  algorithmVersion: string;
  formatVersion: number;
  domainKey: string;
  assumptions: readonly string[];
  errors: readonly ErrorEvidence[];
  byteLength: number;
  fallbackKey: string;
}

type AppearanceProgram =
  | { kind: "direct"; sourceKey: string }
  | { kind: "phase-polynomial"; coefficients: ArrayBuffer; phaseBasisKey: string }
  | { kind: "coherent-ggx"; orbit: ArrayBuffer; guardKey: string; nodes: 4 | 8 }
  | { kind: "slope-atlas"; data: ArrayBuffer; interpretation: "ndf-predictor" };
~~~

Use machine-readable discriminated assumptions in production rather than relying on free-form strings. Examples include opacity, rigid transform, roughness range, carrier relation, complete-period count, pose revision, finite-source geometry, and maximum matrix condition.

Keep mathematical evidence separate from measured cost. Cost profiles depend on adapter/browser/kernel version, not just source meaning. Measured errors also need their sample domain and reference identity.

Initially retain the existing mesh as a fallback and attach optional products. This minimizes disruption to cooking, editor selection, collision, and unsupported hardware. Avoid uploading every fallback to the GPU when it is not needed, but keep its ownership and recovery path explicit.

### 11.3 Invalidation and reuse

| Change | Reuse | Invalidate/update |
|---|---|---|
| Material color/roughness edit | Geometry and unrelated motion | Response fits, distributions dependent on roughness, material-domain guards |
| Field shape/CSG edit | Unrelated materials and motion definitions | Geometry, regional facts, interior/exterior proxies, affected transport predictor |
| Rigid instance movement | Local quadric/mesh/phase source | World bounds, camera/light projections, spatial visibility facts |
| Water time advance | Wave coefficients and static phase program | Phase origins and shutter inputs |
| Water amplitude/frequency edit | Unrelated terrain/actors | Phase basis, slope/orbit coefficients, distribution/fit products |
| Camera/light direction | View-independent source products | Per-query GGX preparation; cached fixed-parameter response cells |
| Character pose | Authored mesh/binding/motion products | Posed bounds; pose-dependent occluders and deformation cache entries |
| Atmosphere composition | Geometry | Optical-depth and scattering products that depend on it |
| Exposure | Scene-linear response products | Display mapping and any exposure-dependent scheduling priorities |

A regional dominance proof with margin \(m\) can survive changes bounded by a total perturbation below \(m\). Reuse is permitted by that bound, not by a vague “small edit” heuristic.

History keys must include the relevant realization/domain revisions. A stable surface identity alone does not make old lighting valid after an approximation switch or disocclusion.

### 11.4 Memory and authoring latency

Existing renderer caps are 128/256/384 MiB owned GPU memory and 4/8/12 MiB upload per frame for low/balanced/high. New products fit inside those totals. Start with proposed appearance-cache sublimits of 8/16/24 MiB and revise from measurements; these are allocations, not measured requirements.

A 256² R16F scalar atlas is 128 KiB before mip overhead; RGBA16F is 512 KiB. Do not grow a view × light × roughness × phase atlas without a measured reuse model.

Compilation is cancellable, keyed, and off the render critical path. Editing roughness should update or invalidate appearance without remeshing the object. Show a complete direct/fallback result while a better product is cooking. Record cold/warm compile time and edit-to-visible latency in addition to steady-state GPU time.

## 12. Implementation sequence with completion gates

### Stage A — Contracts and baseline capture

Add optional product metadata, selected-realization diagnostics, domain rejection reasons, byte ownership, and persisted-format support. Establish fixed camera/light trajectories in the existing Winter valley scenario. Keep rendered/culled/uploading/rejected completeness accounting intact.

**Complete when:** direct fallback round-trips through workers/cooking; changing one product invalidates only its dependents; captures cannot pass by omitting content; baseline images and timings identify the source manifest.

### Stage B — Static primitive realization

Integrate static ellipsoid/sphere analytic rendering with parametric mesh controls, common materials, identity/depth/normal outputs, picking, and shadow consistency.

**Complete when:** near plane, camera-inside, grazing rays, overlaps, silhouettes, and world rebasing pass; the selector respects geometric error and beats the best valid mesh choice on targeted workloads.

### Stage C — Water phase metadata and response selection

Preserve authored carriers through lowering. Implement exact footprint factors, coherent-group detection, finite-period/remainder handling, and variants for direct, regular quadrature, phase program, and guarded four/eight-node warp.

Use the actual authored water generator, with macro displacement and micro appearance separated. Keep a consistent slope/normal frame under transforms.

**Complete when:** moving camera/light and roughness sweeps preserve the reference highlight; collinear/near-collinear cases select fallback; phase correlation survives; source fitting/selection costs are included; no temporal seams appear between paths.

### Stage D — Opaque visibility and finite sun

Extract interior/exterior proxy facts for rigid opaque fields. Add compaction before expensive visible work. Introduce exact finite-sun sphere lighting and proxy brackets with uncertain regions falling back to existing shadows.

**Complete when:** subtraction holes, transparency, moving instances, deformation, light/camera separation, and numerical margins are covered; no false occlusion is found in adversarial differential captures; Hi-Z comparison and whole-pass cost justify the addition.

### Stage E — Atmosphere and full water appearance

Build/validate atmosphere lookup products; integrate scene-linear aerial perspective and a consistent sun. Add a deliberately scoped water reflection/refraction/absorption solution with clear reference behavior.

**Complete when:** horizon/sunset/high-altitude cases, water/land edges, occlusion, and exposure transitions remain stable; the scene is visually convincing when played, not only under numerical plots.

### Stage F — Residual lighting and visible crowds

Only after the base paths are stable, prototype a cheap transport predictor and correction, plus visible-crowd pose sharing/compression. Compare their total cost to established alternatives.

**Complete when:** each technique reduces whole-scene cost at matched visual quality, with source changes, motion, and disocclusion accounted for. Reject techniques whose preprocessing, memory, or residual cost erases their local win.

Stages are ordered by integration risk and available evidence, not promised calendar dates.

## 13. Verification and the quality bar

The final research snapshot passes **245 tests, zero failures, 986,424 assertions**, both TypeScript configurations, package boundaries, and repository formatting/lint. New GPU runs report no validation errors. The new ten mathematical tests cover orbit reduction, exact GGX factorization, ring convolution, sampling normalization, randomized warp expectation, degenerate rejection, and spherical-lens moments/bounds.

These checks do not validate a production integration that has not happened yet.

### Required image and runtime matrix

| Dimension | Cases |
|---|---|
| Geometry | Hero close-up, grazing silhouettes, tiny distant stones, blended regions, subtraction, camera inside proxy |
| Water | Current two-wave source; eight waves; coherent, decorrelated, nearly equal carriers; narrow/broad lobes; finite shutter; degeneracy |
| Material | Roughness 0.06 through broad diffuse-like lobes; metal/nonmetal; correlated layers; discontinuities; local/world coordinate domains |
| Lighting | Moving sun, small/large source, horizon clipping, overlapping blockers, colored lights, source-domain changes |
| Visibility | 0/50/99% occluded; holes; alpha foliage; pose/wind changes; shadow-only contributors; disocclusion |
| Atmosphere | Zenith/horizon, low/high altitude, multiple scale heights, sunset, planet shadow |
| Runtime | Cold load, warm cache, edits, cancellation, streaming, budget pressure, device loss, quality changes |
| Hardware | Apple integrated, another integrated GPU, discrete Windows GPU, supported browser families |

Use independent references and measure several things separately: coverage mistakes, geometric depth/normal error, linear radiance error, temporal instability, CPU/GPU p50/p95, full-frame pacing, compilation hitches, bytes, and completeness.

Suggested initial engineering gates, to calibrate with actual art:

- Zero unexplained missing identities or false culls; uncertain proofs render through fallback.
- Zero NaN/Inf output and explicit reporting of queue/iteration exhaustion.
- For approximate shading on the acceptance scene, relative linear-radiance RMS below 1% with a documented denominator floor and a separate highlight-region maximum/percentile report.
- Approximation switches move silhouettes by less than 0.25 pixel in the targeted domain, or stay on the more accurate path.
- No material temporal-error regression against the direct baseline on the recorded motion trajectory.
- A representative modified workload improves complete-frame GPU p95 by at least 20% at matched judged quality before becoming the default.
- Continue targeting the project's 1080p/60 Hz desktop scenario, while measuring lower-power profiles explicitly. Do not infer unsupported-device performance from M4 results.

These are proposed gates, not achievements. The final judge remains the actual playable scene: quiet highlights during motion, convincing contact and indirect light, readable materials, plausible water, and an art-directed sky.

## Appendix A. Executable research map

| Files | Role |
|---|---|
| [field-research/local-program.ts](/Users/ryanwible/projects/wrela/tools/field-research/local-program.ts), [gauge.ts](/Users/ryanwible/projects/wrela/tools/field-research/gauge.ts), [blend-polynomial.ts](/Users/ryanwible/projects/wrela/tools/field-research/blend-polynomial.ts) | Regional field specialization, positive metric factors, polynomial root candidates |
| [field-research/wave-moments.ts](/Users/ryanwible/projects/wrela/tools/field-research/wave-moments.ts) | Exact finite-wave footprint/shutter moments |
| [transport-research/spectral.ts](/Users/ryanwible/projects/wrela/tools/transport-research/spectral.ts), [positive-response.ts](/Users/ryanwible/projects/wrela/tools/transport-research/positive-response.ts) | Filtered response polynomial and positive coherence algebra |
| [slope-atlas.ts](/Users/ryanwible/projects/wrela/tools/transport-research/slope-atlas.ts) | Characteristic-function distribution compiler |
| [coherent-ggx.ts](/Users/ryanwible/projects/wrela/tools/transport-research/coherent-ggx.ts) | New orbit factorization, pole model, rejection control, Möbius quadrature |
| [sun-caps.ts](/Users/ryanwible/projects/wrela/tools/transport-research/sun-caps.ts) | New stable spherical-lens moments and proxy visibility intervals |
| [field-occluders.ts](/Users/ryanwible/projects/wrela/tools/transport-research/field-occluders.ts), [visibility.ts](/Users/ryanwible/projects/wrela/tools/transport-research/visibility.ts) | Field interior proofs and conservative tile visibility |
| [atmosphere.ts](/Users/ryanwible/projects/wrela/tools/transport-research/atmosphere.ts) | Log-concavity optical-depth bounds |
| [gpu-coherent.ts](/Users/ryanwible/projects/wrela/tools/transport-research/gpu-coherent.ts), [gpu-sun.ts](/Users/ryanwible/projects/wrela/tools/transport-research/gpu-sun.ts) | New complete WebGPU kernels and controls |
| [final-accuracy.ts](/Users/ryanwible/projects/wrela/tools/transport-research/final-accuracy.ts), [orbit-stress.ts](/Users/ryanwible/projects/wrela/tools/transport-research/orbit-stress.ts) | Independent full-grid CPU references and adversarial domains |
| [final-snapshot.ts](/Users/ryanwible/projects/wrela/tools/transport-research/final-snapshot.ts), [final-plot.py](/Users/ryanwible/projects/wrela/tools/transport-research/final-plot.py) | Versioned evidence with source hashes and figures |

Research helpers accept their documented mathematical domains. They are not validated public compiler APIs yet. Production code needs explicit input/domain validation, bounded allocation, and error status contracts.

## Appendix B. Reproduction

Run from the repository root. The full test/check logs below are also inputs to the snapshot writer.

~~~sh
bun tools/transport-research/coherent-bench.ts
bun tools/transport-research/orbit-stress.ts
bun tools/transport-research/gpu-final.ts
bun tools/transport-research/final-accuracy.ts
bun run check > output/transport-research/final-check.log 2>&1
bun test > output/transport-research/final-tests.log 2>&1
bun tools/transport-research/final-snapshot.ts
python3 tools/transport-research/final-plot.py
~~~

The CPU coherent benchmark creates the output directory. The GPU runner uses the repository's isolated browser and cooperative hardware lease. It needs Chrome/Bun WebView support and WebGPU timestamp queries. Plotting needs NumPy and Matplotlib.

The --sun option reruns only sunlight while retaining the previous coherent GPU report. The snapshot writer currently checks the expected test-count snapshot explicitly; update that expectation with a new validated snapshot when the suite changes.

Earlier suites:

~~~sh
bun tools/field-research/bench.ts
bun tools/field-research/gpu.ts
bun tools/field-research/wave-bench.ts
bun tools/transport-research/bench.ts
bun tools/transport-research/algebra-bench.ts
bun tools/transport-research/slope-bench.ts
bun tools/transport-research/gpu.ts
~~~

Raw output is under output/field-research and output/transport-research. Versioned evidence is beside this document. The inspected repository base was 13e39e2ca8915f6287a062fb9e6a81e0cc007b2b; the final snapshot also hashes the transport research sources.

## Appendix C. Prior art and the novelty boundary

The following primary sources informed the design or identify established components. The references are not a proof of worldwide novelty.

| Established work | Relationship to this proposal |
|---|---|
| [Snyder, Interval Analysis for Computer Graphics](https://www.microsoft.com/en-us/research/publication/interval-analysis-computer-graphics/) | Interval facts and subdivision are established tools |
| [Keeter, Massively Parallel Rendering of Complex Closed-Form Implicit Surfaces](https://www.mattkeeter.com/research/mpr/) | Expression specialization and GPU implicit evaluation |
| [Barbier et al., Lipschitz Pruning](https://wbrbr.org/publications/LipschitzPruning/) | Spatial CSG pruning; Wrela must respect its non-SDF ellipsoid values |
| [Kalra and Barr, Guaranteed Ray Intersections](https://authors.library.caltech.edu/records/hf4d8-mwk07) | Bracketing/rate conditions for reliable implicit intersections |
| [Olano and Baker, LEAN Mapping](https://userpages.cs.umbc.edu/olano/papers/lean/) | Distribution moments for specular filtering |
| [Han et al., Frequency Domain Normal Map Filtering](https://www.cs.columbia.edu/cg/normalmap/index.html) | Normal distributions and nonlinear BRDF filtering |
| [Yang and Barnes, Approximate Program Smoothing](https://arxiv.org/abs/1706.01208) | Compiling procedural shader smoothing |
| [Filtering After Shading With Stochastic Texture Filtering](https://arxiv.org/abs/2407.06107) | Relevant shading/filtering baseline |
| [Heitz et al., Linearly Transformed Cosines](https://eheitzresearch.wordpress.com/415-2/) | Credible analytic area-light baseline |
| [Hart et al., Practical Product Sampling by Fitting and Composing Warps](https://research.nvidia.com/publication/2020-07_practical-product-sampling-fitting-and-composing-warps) | Importance warps are established; the new experiment targets authored phase integration |
| [Dupuy and Benyoub, Sampling Visible GGX Normals with Spherical Caps](https://arxiv.org/abs/2306.05044) | Angular GGX/VNDF sampling is distinct from integrating deterministic macro-wave phase |
| [Mazonka, Intersecting Spherical Caps](https://arxiv.org/abs/1205.1396) | Spherical lens area is established geometry |
| [Greene et al., Hierarchical Z-Buffer Visibility](https://www.cs.cmu.edu/afs/cs/academic/class/15869-f11/www/readings/greene93_hierarchicalz.pdf) | Required visibility comparison |
| [Walter et al., Lightcuts](https://www.graphics.cornell.edu/~bjw/lightcuts.pdf) | Hierarchical light approximation and controlled error |
| [Hillaire, Production Atmosphere Rendering](https://sebh.github.io/publications/egsr2020.pdf) | Runtime sky/atmosphere architecture baseline |
| [Heitz, Understanding Masking-Shadowing](https://jcgt.org/published/0003/02/03/paper.pdf) | An NDF is not a complete microfacet model |
| [Tokuyoshi and Kaplanyan, Stable Geometric Specular Antialiasing](https://www.jcgt.org/published/0010/02/02/) | Practical specular antialiasing comparison |
| [NIST Bessel integral identities](https://dlmf.nist.gov/10.32), [ordinary Bessel identities](https://dlmf.nist.gov/10.9) | Established characteristic-function mathematics |
| [PBRT sampling and integration](https://pbr-book.org/4ed/Sampling_and_Reconstruction/Sampling_and_Integration) | Sampling/error analysis foundations |

The strongest candidate contribution is the **specific compiler construction**: retain Wrela's exact field factors and phase structure, transform difficult integrals into cheap low-frequency ones, derive angular visibility moments from semantic geometry, and carry validity/error information into GPU selection and residual correction.

The newly derived phase warp is an implementation-ready research lead for a restricted domain. The field/visibility/atmosphere facts provide complementary ways to remove work. Generalized GI, animated implicit surfaces, complete water transport, and AAA-level integrated scene quality remain work to validate through the staged implementation above.
