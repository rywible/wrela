# Wrela: compile the surface, the metric, and the unresolved light separately

Research and executable experiments, 21 September 2026.

**The proposal:** Wrela should compile each region of an authored field into the cheapest representation that preserves the requested geometric and shading accuracy. Some regions become exact quadrics. Blends become short, zero-preserving polynomial programs. Unresolved waves become analytic slope distributions. Triangles remain useful where their measured cost is best.

The distinctive mathematical direction developed here is to **approximate the positive multiplier of a primitive's known surface equation, rather than approximating the surface equation itself**. It preserves isolated primitive geometry exactly, makes blend errors easier to bound, and turns common two-primitive blends into polynomial ray queries.

This is a research contribution proposal, not an assertion of worldwide priority. Spatial pruning, implicit-surface jets, quadric impostors, polynomial root isolation, and normal-distribution filtering have substantial prior art. The exact factorization, Wrela operator-specific error composition, polynomial lowering, and finite-wave cross-moment treatment below are the proposed synthesis. No production renderer switch has been made; the experiments are runnable beside it.

## What is measured

The full raw evidence, including timings and controls, is in [field-realization-results.json](field-realization-results.json); the [evidence figure](field-realization-evidence.svg) plots the comparisons. Hardware: one Apple adapter, Metal 3, Chrome/WebGPU, 1920×1080. CPU: Bun 1.4.2, macOS arm64.

| Experiment | Measured result | What it establishes |
|---|---:|---|
| 4,096 copies of the actual reference stone, current 7,548-triangle mesh vs exact quadric | 12.517 → 0.213 ms; **58.8×** | Geometry-pass speed on this workload |
| Same workload, minimum-resolution extracted mesh, 1,868 triangles | 3.211 → 0.213 ms; **15.1×** | Gain remains against the existing coarsest mesher setting |
| Same workload, new conventional 120-triangle parametric LOD | 0.262 → 0.213 ms; **1.23×** | Much of the speed opportunity is representation selection, not a new intersection formula |
| One stone close-up: mesh24 vs analytic | **401× lower RMS ray-depth error**, 7.393 mm → 0.0184 mm | Geometric accuracy, measured separately from the dense timing workload |
| Same close-up, normals | 0.469° → 0.00135° RMS | Normal accuracy against independent double-precision geometry |
| Reference bunny field queries | **5.14×** vs production; **4.62×** vs flattened control | Exact spatial reduction, including query traversal |
| Synthetic 64-part field queries | **30.2×** vs production; **26.5×** vs flattened control | Scaling with locally inactive authored structure |
| Eligible bunny blend samples: factored metric vs ordinary quadratic jet | **9.92× lower mean field residual** | The new approximation is better on the tested blend regions |
| Eligible two-ellipsoid neck samples: same comparison | **10.27× lower mean field residual** | A second, deliberately blended test case |

There are important negative results:

- A 120-triangle conventional LOD is faster than the analytic path at 1, 64, and 1,024 instances. It has 73.3 mm RMS close-up depth error, versus 0.0184 mm for the analytic path. The best choice depends on the error budget.
- The exact-query atlas slows down a single primitive: **0.75×** the production evaluator's speed. Bypass the atlas there.
- The bunny reconstruction experiment accepts only **27.9%** of triangle-interior samples. Including every fallback, its mean residual improves only **1.27×**. The neck accepts 70.8%, giving **3.13×** overall. Large accepted-region numbers do not establish whole-character quality.
- The analytic GPU path misses one grazing silhouette pixel in the close-up test. The current mesh misses 336; the 120-triangle LOD misses 3,770. “Exact quadric” describes the mathematical surface, not perfect floating-point raster coverage.

GPU timings use nine alternating samples, each measuring four passes, after three warmups per representation. All paths use one instanced draw, the same lighting, camera, viewport, culling, and depth. Values near 0.05 ms are quantized and should not be overinterpreted. The fixture excludes terrain, shadows, skinning, procedural materials, streaming, and CPU submission. **These are not whole-world frame-time improvements.** The close-up accuracy comparison uses all common hit pixels, with missing and extra pixels reported separately; it is not an image-perception metric.

## 1. Factored fields: keep geometry exact, approximate its metric

Wrela's ellipsoid leaf currently evaluates, in its own coordinates,

\[
q=\left\|p/s\right\|,\qquad
r=\left\|p/s^2\right\|,\qquad
f(p)=\frac{q(q-1)}{r}.
\]

This is an implicit value, not a safe sphere-tracing distance. For example, for axes \((2,1,1)\), its limit approaching the center is \(-2\) along X and \(-1\) along Y. A global unit-Lipschitz assumption is invalid. The existing compiler already correctly labels it as implicit.

Factor it instead:

\[
Q(p)=q^2-1,\qquad
w(p)=\frac{q}{r(q+1)},\qquad
\boxed{f(p)=Q(p)w(p).}
\]

Away from the singular center, \(w>0\). \(Q\) is a quadratic polynomial and its zero set is the **exact ellipsoid**. For a sphere of radius \(s\), \(w=s/(q+1)\). Rigid group transforms can be folded into \(Q\)'s coefficients.

Now replace only \(w\):

\[
\widehat f(p)=Q(p)\widehat w(p),\qquad \widehat w(p)>0.
\]

Consequences:

1. Every isolated primitive zero and inside/outside classification remains exact, regardless of the approximation error in the positive multiplier.
2. A tree containing only hard union, intersection, and subtraction keeps its zero set and sign when all leaves undergo this replacement. Smooth blends need the metric, so they require an error-controlled approximation instead.
3. Approximation error vanishes on each primitive's exact surface:
   \[
   |f-\widehat f|=|Q|\,|w-\widehat w|.
   \]
   A generic Taylor approximation of \(f\) has no such zero-preservation property.

Within a compiler region centered at \(c\), choose a constant, linear, or quadratic Taylor approximation of \(w\). A cheap sufficient positivity condition for the quadratic case, with coordinate half-widths \(h_i\), is

\[
w(c)-\sum_i |w_i(c)|h_i
-\frac12\sum_{i,j}|w_{ij}(c)|h_i h_j>0.
\]

The prototype enforces this condition. It rejects a primitive center at the expansion point or an insufficient positivity margin; the calling reconstruction experiment separately requires a smooth certified region.

### An error bound that does not grow with tree depth

For Wrela's polynomial smooth union of width \(k>0\),

\[
S_k(a,b)=
\begin{cases}
\min(a,b),&|a-b|\ge k\\
\frac{a+b}{2}-\frac{k}{4}-\frac{(a-b)^2}{4k},&|a-b|<k.
\end{cases}
\]

Its partial derivatives are \(h\) and \(1-h\), both nonnegative and summing to one. Thus

\[
|S_k(a,b)-S_k(\widehat a,\widehat b)|
\le\max(|a-\widehat a|,|b-\widehat b|).
\]

Min, max, and negation have the same maximum-norm nonexpansion property. Induction through the **ordered** authored expression therefore gives

\[
\boxed{|F(p)-\widehat F(p)|
\le \max_i \left(|Q_i(p)|\,|w_i(p)-\widehat w_i(p)|\right).}
\]

This is not the sum of all leaf errors. It applies for fixed blend widths and an unchanged expression; it does not justify regrouping smooth unions. Their authored left-fold order is preserved.

The multiplier can also be enclosed without assuming an SDF. Let \(q\in[q_l,q_h]\), \(r\in[r_l,r_h]\), and \(s_{\min},s_{\max}\) be the extreme axes. Since \(q/r\in[s_{\min},s_{\max}]\),

\[
R_l=\max(s_{\min},q_l/r_h),\quad
R_h=\min(s_{\max},q_h/r_l),
\]
\[
w\in\left[\frac{R_l}{q_h+1},\frac{R_h}{q_l+1}\right].
\]

Use an infinite ratio upper bound when \(r_l=0\), and explicitly include the current evaluator's center fallback. At a candidate ray hit, multiplying the multiplier-error interval by \(|Q_i|\) gives a computable field-error bound. The prototype currently implements value intervals, positivity tests, and sampled accuracy; it does **not** yet implement a complete directed-rounding error certificate for the approximation.

If a ray bracket has opposite endpoint signs and a certified directional derivative of one sign, \(|dF/dt|\ge m>0\), its root is unique. A candidate in the bracket with field residual bounded by \(E\) then satisfies

\[
|t-\widehat t|\le E/m.
\]

Near a silhouette \(m\) can vanish. Subdivide, isolate the polynomial root, or fall back; do not turn this into an unguarded marching step. A depth bound alone does not certify silhouette coverage or multiple nearby surfaces.

### Compile a blend into a polynomial ray query

With constant multipliers, each primitive restricted to a ray is a quadratic \(a(t)\) or \(b(t)\). In the active blend, its zero equation is

\[
\boxed{(a(t)-b(t))^2-2k(a(t)+b(t))+k^2=0.}
\]

That is a quartic. Linear multipliers give degree six; quadratic multipliers give degree eight. Solve or isolate this polynomial on the region's ray interval, then validate the active-branch condition \(|a-b|\le k\). Also consider the separate unblended primitive roots. This is an alternative to repeatedly evaluating the full authored graph while marching.

The code constructs these coefficients and uses Bernstein convex-hull subdivision to retain candidate intervals, including tangencies. It reports budget exhaustion. Nested active blends can double polynomial degree again; cap the degree and retain a short evaluator or mesh fallback. A polynomial candidate interval is not by itself a validated hit on the original field.

### Why this improves the measured blend approximation

The control uses an ordinary second-order jet of the exact field. For a smooth union its Hessian is

\[
H_S=hH_a+(1-h)H_b-
\frac{(\nabla a-\nabla b)(\nabla a-\nabla b)^T}{2k}.
\]

The extra curvature introduced by one blend is rank one. This identity supplies a cheap differential compiler and the comparison's gradients and Hessians.

On 429 accepted bunny **blend** samples, ordinary jets leave mean residual \(7.75\times10^{-7}\); the factored approximation leaves \(7.81\times10^{-8}\). On 1,173 accepted neck blend samples the values are \(5.27\times10^{-7}\) and \(5.13\times10^{-8}\). These figures exclude the easy exact-primitive cases. The experiment uses normal-direction reconstruction inside conservative triangle neighborhoods, not a complete per-pixel ray renderer. Its three Newton updates are experimentally validated here, not a universal convergence guarantee.

## 2. Compile regional facts into a hybrid renderer

For each region, propagate actual implicit-value intervals through the ordered graph. Examples:

- Union: \(u_a<l_b\) permits replacing the union by \(a\).
- Intersection: \(l_a>u_b\) permits replacing the intersection by \(a\).
- Smooth union: \(u_a+k<l_b\) permits replacing the blend by \(a\).
- Subtraction: negate the second child's interval, then use the intersection rule.

Strict inequalities protect source/material tie behavior. Group transforms compose into leaf transforms; material overrides survive exact reduction. In the reference bunny, 16 primitive evaluations become an average of 2.44 at extracted mesh vertices. That is a sample-weighted statistic, not visible screen area.

The proposed output is a region with:

- its spatial domain and parameter validity domain;
- exact field-value program and source/material provenance;
- range and dominance margins;
- optional exact zero polynomial;
- optional positive metric approximation and error bounds;
- a measured-cost choice among a mesh, analytic intersection, polynomial query, or short exact evaluator.

When a region reduces to an ellipsoid, render the exact ray/quadric intersection and analytic normal. The GPU prototype does this behind a conservative 12-triangle box and writes actual surface depth. The proxy supplies coverage, not surface geometry. It replaces 7,548 submitted surface triangles per stone with 12 proxy triangles, a **629× triangle-count reduction**, which becomes a smaller measured GPU-time reduction.

When a region remains a blend, use the factored program, a bounded polynomial solver, or a mesh. Hard branch boundaries, clipping, and silhouettes need explicit ownership and fallback rules. The experimental GPU fixture currently handles a single ellipsoid definition and instances, not the entire bunny atlas. Approximated field values do not automatically preserve blend material selection; retain exact source evaluation or a separate provenance certificate.

### Reuse proofs across edits and motion

Suppose a discarded blend child has margin \(m=l_b-u_a-k>0\). After a parameter update, let valid uniform perturbation bounds be \(\epsilon_a,\epsilon_b,\epsilon_k\). The same reduction remains valid whenever

\[
\epsilon_a+\epsilon_b+\epsilon_k<m.
\]

This permits cached regional decisions across a bounded range of edits or motion, rather than invalidating an entire object. Rigid instance transforms can transport the artifact directly. Wrela's existing linear-blend skinning is not a rigid transform of the field; transporting an ellipsoid per bone would change its meaning. Continuous deformation charts, deformation-error bounds, and refreshed certificates are a separate research task.

### Choose on error and measured cost

A starting cost model compares

\[
T_{\text{mesh}}\approx c_vV+c_tT+c_sP_{\text{hit}},
\]
\[
T_{\text{analytic}}\approx c_bN+c_iP_{\text{proxy}}+c_sP_{\text{hit}}.
\]

Fit the constants on the actual adapter. The conventional LOD control proves why this choice matters: analytic rendering has a fragment cost and can lose for isolated, cheaply tessellated objects. The compiler should minimize cost subject to depth, normal, and coverage error budgets, rather than force one representation.

Also apply Amdahl's law before promising a frame-rate gain. If this pass is only 20% of a frame, a 59× pass speedup gives about **1.24×** overall, not 59×. The prototype does not establish what fraction of the current winter-valley frame can use these routes.

## 3. Compile waves into subpixel slope moments

Wrela already authors water as at most eight analytic sine waves. Its current shader attenuates unresolved normal frequencies. That reduces aliasing but does not transfer their slope variance into the shading model.

Write the slope as

\[
s(x,t)=\sum_i v_i\cos\phi_i,\quad
v_i=A_i k_i,\quad
\phi_i=k_i\cdot x+\omega_i t+\theta_i.
\]

Let one pixel's locally affine world footprint have axes \(u,v\), and uniform shutter duration \(\Delta t\). The exact box-filter transfer function for a phase frequency is

\[
C(k,\omega)=
\operatorname{sinc}(k\cdot u/2)\,
\operatorname{sinc}(k\cdot v/2)\,
\operatorname{sinc}(\omega\Delta t/2).
\]

Then

\[
\mu=\sum_i v_i\cos\phi_i\,C(k_i,\omega_i),
\]

\[
M=\frac12\sum_{i,j}v_i v_j^T
\left[
\cos(\phi_i-\phi_j)C(k_i-k_j,\omega_i-\omega_j)
+\cos(\phi_i+\phi_j)C(k_i+k_j,\omega_i+\omega_j)
\right],
\qquad \Sigma=M-\mu\mu^T.
\]

These are exact first and second slope moments for that affine space-time footprint, including coherent interference. Dropping all cross terms would be wrong for Wrela's small deterministic wave set: two identical waves with opposite phase cancel completely, yet an independent-variance sum would invent roughness.

The compiler can precompute wave vectors, sum/difference frequencies, and coefficient matrices. With eight waves there are only 36 unordered pairs. A runtime footprint evaluates the remaining transfer factors. Similar-frequency pairs must remain even when their individual waves are unresolved, because their difference frequency can remain visible.

The implementation agrees with independent spatial/shutter integration and passes the cancellation test. Eight-wave moments took about 1.98 µs per footprint on this CPU. A deliberately expensive 131,072-sample quadrature reference took 13.52 ms, with maximum moment discrepancy \(6.67\times10^{-6}\). That comparison validates an analytic integral; it is **not** a 6,800× rendering claim.

Shading still needs a tested closure from \((\mu,\Sigma)\) to a normal distribution and BRDF. Moment matching is natural for a Gaussian/Beckmann slope model; blindly adding variance to GGX roughness does not produce exact filtered lighting. Perspective footprint curvature, occlusion, and nonlinear normal normalization also limit the affine-moment interpretation. GPU BRDF integration and temporal image-quality measurements remain unimplemented.

## Implementation and reproduction

All new executable code is under [tools/field-research](../../tools/field-research). Production source and formats are unchanged.

~~~sh
bun test tools/field-research
bun tools/field-research/bench.ts
bun tools/field-research/gpu.ts
bun tools/field-research/wave-bench.ts
~~~

The GPU script uses the repository's hardware browser launcher and cooperative GPU lease. It requires Chrome, Bun's WebView support, and hardware timestamp queries. Generated reports and captures are in output/field-research/. The versioned JSON beside this report preserves this run.

Key files:

- local-program.ts: transform lowering, non-SDF intervals, ordered CSG reduction, atlas, differential jets, quadric candidates.
- gauge.ts: positive metric approximations that preserve primitive zero sets.
- blend-polynomial.ts: ray-polynomial composition and bounded candidate isolation.
- gpu-fixture.ts: independent GPU comparison with extracted and parametric mesh controls.
- wave-moments.ts: exact finite-wave space-time slope moments.

Validation completed: bun run check; **136 tests passed**, including 11 new research tests; hardware GPU experiment completed with no reported validation errors. Numerical tests exercise more than 150,000 assertions in the new suite. This is meaningful experimental coverage, not a formal floating-point proof.

The production integration gates are specific: directed-rounding bounds; branch/region coverage and seam handling; nearest-root and grazing-ray guarantees; semantic picking/material selection after approximation; mesh/analytic shadow consistency; deformation semantics; and real winter-valley timing with an error-driven dispatch policy. A resolution-12 meshing floor is not an adequate conventional LOD baseline, so retain the direct parametric control.

My first production change would be an optional analytic surface artifact for static sphere/ellipsoid definitions, alongside a cheap parametric LOD and measured selection. Next, extend to certified multi-primitive regions and the factored blend solver. The wave-moment work is an independent route to better temporal shading, with its own GPU validation.

## Prior art and novelty boundary

- [Snyder, *Interval Analysis for Computer Graphics*, 1992](https://www.microsoft.com/en-us/research/publication/interval-analysis-computer-graphics/) establishes interval inclusion and subdivision as graphics tools. Regional certificates are not new by themselves.
- [Keeter, *Massively Parallel Rendering of Complex Closed-Form Implicit Surfaces*, 2020](https://www.mattkeeter.com/research/mpr/) combines interval evaluation, expression specialization, and GPU implicit rendering. Calling the atlas alone an invention would be incorrect.
- [Barbier et al., *Lipschitz Pruning*, 2025](https://wbrbr.org/publications/LipschitzPruning/) specializes hard and smooth CSG spatially. Wrela needs value intervals that respect its non-distance ellipsoid estimator rather than assuming the required Lipschitz property.
- [Kalra and Barr, *Guaranteed Ray Intersections with Implicit Surfaces*, 1989](https://authors.library.caltech.edu/records/hf4d8-mwk07) makes rate bounds central to guaranteed intersection. The error-to-depth argument above likewise needs explicit derivative and bracketing conditions.
- [NVIDIA, *True Impostors*](https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-21-true-impostors) is relevant background for proxy rasterization with fragment-level intersections. The analytic-stone experiment is evidence for Wrela's representation choice, not a claim to have invented impostors.
- [Olano and Baker, *LEAN Mapping*, 2010](https://userpages.cs.umbc.edu/olano/papers/lean/) uses normal-distribution moments for specular filtering. The proposal here directly integrates Wrela's finite authored wave sum, including pair phases and shutter time, rather than treating authored waves as independent unresolved noise.

The strongest next research question is whether **zero-preserving metric approximation plus regional error certificates** can cover enough of animated, blended characters to retain the analytic prototype's accuracy while beating both adaptive meshes and established pruned implicit renderers. The current results support pursuing that question; they do not yet settle it.
