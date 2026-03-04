# Procedural Anime Engine — Master Plan v2

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a fully procedural anime game engine where zero predefined assets exist — all geometry, materials, animation, audio, and visual effects are runtime functions of game state. The engine renders an infinite, living world via bounded implicit fields cached into sparse brick volumes, lit by field-diffused global illumination, populated by evolutionarily adapting creatures, and verified by compile-time conservative proofs. Every frame is a unique painting. Light is a physical force that fights corruption. The compiler IS the renderer.

**Architecture:** Eighteen phase tracks (0, 1, 2a, 2b, 3, 4a, 4b, 5-16) plus cross-cutting infrastructure, ordered to minimize rework. Phase 1 is a thin anime style shell (cel/outline/palette/post/shadows) that exists only to hard-cut off PBR and feed Phase 2a output. Phase 2a is the first true gate — a falsification spike to prove compute ray marching hits perf targets on WebGPU before committing further. Phase 2b (after GO decision) replaces geometry with the full bounded implicit field engine (not "SDF" — the distinction matters and is explained below). The core invariant — Lipschitz-tracked implicit fields with conservative stepping — extends to spacetime (4D), giving guaranteed continuous collision detection and procedural anime smear frames from the same math. All subsequent phases build on the field engine without discarding prior work. Each phase produces a detailed sub-plan with per-task AC before execution begins.

**Tech Stack:** Rust/WASM, WebGPU/WGSL compute shaders, Wrela language compiler, Playwright for visual verification

**Target:** "Minecraft meets Guilty Gear" — infinite procedural world, anime combat, evolving creatures, persistent terrain damage, living ecosystems. Not AAA photorealism. AAA *stylized systemic depth*.

---

## The Core Mathematical Invariant

> **This section exists because the v1 plan had an internal contradiction that every reviewer caught independently. Read this before anything else.**

### The problem with "SDF everywhere"

A true signed distance function (SDF) is 1-Lipschitz and satisfies ||∇f(x)|| = 1 almost everywhere (the gradient has unit magnitude a.e.; nondifferentiable points exist at the medial axis). This property makes sphere tracing trivially correct: you can step by exactly f(x) and never overshoot the surface.

But most useful operations **break this property**:
- **Smooth blends / R-functions**: gradient magnitude deviates from 1 near the blend region
- **Domain warps** (twist, bend, noise displacement): gradient magnitude scales with the warp's Jacobian
- **Sampling into voxel bricks**: trilinear interpolation of distance samples can overestimate distance
- **Mipmapping**: averaging distances across a region overestimates distance to thin features

The v1 plan claimed "SDF" while using all of these operators. That's a contradiction. If you trust |grad(f)| = 1 and step by f(x), you WILL overshoot surfaces, producing holes, flicker, and missed intersections.

### The fix: Bounded Implicit Fields with Conservative Stepping

We don't have SDFs. We have **L-Lipschitz implicit fields**: functions where the rate of change is bounded.

**Definition:** A function f: R3 -> R is L-Lipschitz if for all points x, y: |f(x) - f(y)| <= L * |x - y|

For a true SDF, L = 1. For our composed, warped, sampled fields, L >= 1 and we track it explicitly.

**The universal stepping rule (region-valid bounds):**

```
b = d - epsilon
L_dir(v) = dot(abs(v), B)    if anisotropic bound B is available/valid for region
         = L_fallback         otherwise
L_safe = max(L_dir(v), epsilon_denom)
step_raw = max(0, b) / L_safe
step = min(step_raw, distance_to_region_exit)
```

where:
- `d` = sampled distance value at current point
- `epsilon` = conservative error bound from sampling/interpolation
- `B` = component-wise derivative bound over the entire active region (brick or mip cell):
  `|∂f/∂x|<=Bx, |∂f/∂y|<=By, |∂f/∂z|<=Bz`
- `v` = unit ray direction
- `L_fallback` = scalar Lipschitz bound (used where anisotropic bounds are unavailable)
- `epsilon_denom` = small floor (e.g. 1e-6) to prevent division by zero

Directional bounds are the primary denominator when available. This is only sound when the bound `B` is valid over the entire segment the step could traverse (region-valid); otherwise the step must be clamped to `distance_to_region_exit` or the kernel must fall back to a bound valid over a larger region. Scalar `L_fallback` is used when anisotropic bounds are unavailable (or explicitly invalid for the current region). `epsilon_denom` is only a divide-by-zero floor.

**With Lipschitz Certificate Portfolio (LCP, Med+ quality):** When both `B` and `L` are available with valid provenance, the fused stepping rule replaces the above:
```
b* = max( b_env_L1(p), b_env_L2(p) )     // tighter lower bound
L_dir*(v) = min( dot(abs(v), B), L )       // tighter directional bound
step = min( max(0, b*) / max(L_dir*(v), epsilon_denom), distance_to_region_exit )
```
This eliminates the ~1.73x diagonal tax on SDF-like regions while preserving full conservatism. See "Lipschitz Certificate Portfolio" section below.

**This is the contract that the entire engine is built on.** Every operator and cache layer must maintain conservative bound data: canonical lower-bound form `b = d - epsilon` plus either scalar `L` or anisotropic derivative bound `B` (or both for LCP). This replaces "unit gradient SDF" as the core invariant.

### B Provenance: Region-Valid vs Point-Valid

> **The most important hidden dependency in the architecture.** Everything above assumes `B_cell` (or `B_region`) bounds derivatives everywhere your step could possibly travel before you re-sample. That's easy for primitives and affine transforms, but it's the whole game for warps like twist/bend and for any noise displacement where the derivative bound depends on position.

**The rule:** Every time you compute a warp Jacobian bound, it must be derived from a bound on the input domain over the entire cell/brick AABB, not from the current point. If you ever accidentally compute "local B" and treat it as "cell B", cone-union stepping becomes a silent footgun — it will happily take you to the cell exit.

**Enforcement — provenance tracking:**

Every `B` value carries a provenance tag:
```
enum BoundProvenance {
    Analytic,    // derived from analytic formula valid over declared region (e.g., primitive, affine warp)
    Sampled,     // computed from worst-case of sampled Jacobians over region grid points
    Inflated,    // analytically derived but inflated by safety factor (e.g., noise bounds)
    Unknown,     // provenance not established (e.g., user-supplied assume_bound)
}
```

**Fail-closed policy:** When provenance is `Unknown`, the marching kernel MUST fall back to scalar `L_fallback` instead of using directional `L_dir(v) = dot(abs(v), B)`. This prevents "local B treated as cell B" from ever reaching the stepping hot path. The provenance tag is stored per-brick in metadata and propagated through composition rules.

**Type-level enforcement (reviewer upgrade):** `RegionValidBound` is a distinct newtype in the IR, not just a metadata tag. You cannot pass a raw `[f32; 3]` derivative bound into the marcher — you must explicitly construct a `RegionValidBound` via a validated constructor that checks provenance, or use an `inflate_to_region` operator that widens point-valid bounds to region-valid bounds with appropriate safety margin. This shifts "local B treated as cell B" from a runtime detection to a compile-time prevention:
```
struct RegionValidBound {
    b: [f32; 3],
    provenance: BoundProvenance, // must not be Unknown
}
impl RegionValidBound {
    fn from_analytic(b: [f32; 3]) -> Self { ... }           // Analytic provenance
    fn from_sampled(b: [f32; 3]) -> Self { ... }             // Sampled provenance
    fn inflate_point_to_region(point_b: [f32; 3], safety: f32) -> Self { ... }  // Inflated provenance
}
```
The marching kernel's `safe_step_from_lower_bound` accepts `RegionValidBound`, not raw arrays. Code that attempts to pass an unvalidated bound gets a type error. The `Unknown` provenance path is a separate function signature that forces the scalar `L_fallback` path — you literally cannot call the directional stepping function with Unknown bounds.

**Debug view:** B provenance overlay (see Phase 0f) visualizes provenance per brick as color-coded bitfield, making "unknown" regions immediately visible.

### The Four Truths (engine law)

The engine maintains four distinct "truths" for different consumers. Confusing them produces "render says outside, physics says inside, CCD says maybe" bugs.

1. **Truth for marching safety** = conservative lower bound `b_lower(p)`. Never allowed to exceed the real field value. This is the ONLY value that drives ray march step sizes. Sources: envelope reconstruction `b_env(p)` or epsilon fallback `b = d - epsilon`.

2. **Truth for contacts/animation** = distance estimate `d_est(p)`. Allowed to be an approximation, but MUST stay within drift budget relative to bounds: `b_lower(p) ≤ d_est(p) ≤ u_upper(p)`. Used by physics contact queries, CCD, foot placement. Re-distance modifies this value.

3. **Truth for shading stability** = stored normal and curvature. Sourced from `d_est` after re-distance. Used for lighting, outline detection, curvature-driven detail. May lag behind marching truth by one re-distance cycle.

4. **Truth for numerics** = f32 arithmetic is not exact, and the engine must own every last ULP. GPU arithmetic is round-to-nearest-even; `b_i - D_B(p, x_i)` can round UP by one ULP, which means a "conservative" lower bound can exceed the true field value by a sliver. The fix is explicit pessimistic padding on every conservative reconstruction:
   - **Lower bounds:** After computing a raw envelope `b_env_raw(p)`, subtract a pad: `b_env = b_env_raw - pad_b` where `pad_b = C * u * S`. Here `u = f32::EPSILON / 2` (unit roundoff), `S` is an upper bound on the sum of magnitudes of all terms used in the expression (for McShane: `|b_i| + |D_B| + |candidate|`; for quadratic envelope: `|b_i| + |grad·dx| + |(K/2)|dx|^2| + |candidate|`), and `C` is chosen from a conservative operation-count bound (γ_n = n*u/(1-n*u) style). For the current expressions (3-5 ops per candidate + max reduction), `C = 2/u ≈ pad = 2.0 * f32::EPSILON * max(max_abs_term, 1.0)` is a deliberate overestimate that absorbs the full expression tree. This is auditable: the pad formula is `O(u * |operands|)`, not a tuned magic number. If the expression complexity grows (e.g., additional terms in Whitney), `C` and `S` must be re-derived.
   - **Upper bounds:** After computing `u_env_raw(p)`, add the same-magnitude pad upward.
   - **Budget marching costs:** When computing `cost_raw = L_dir * Δt_exit`, add `pad_cost = 2.0 * f32::EPSILON * max(|cost_raw|, 1.0)` and use `cost = cost_raw + pad_cost`. This keeps cost accumulation conservative without mixing incompatible units.
   - **Philosophy:** Same as f16 directed rounding (already in plan), extended to f32 hot loop. You don't need directed rounding hardware — just be consistently pessimistic by a sliver.
   - **Enforcement:** The `lipschitz_envelope()`, `whitney_c1_envelope()`, `dual_envelope_lower_bound()`, and `budget_march_traverse()` functions apply their respective pads internally. No caller-side responsibility.

**Invariant:** `b_lower(p) ≤ d_est(p) ≤ u_upper(p)` at all times. Re-distance tightens this interval. Edit composition may widen it. If the interval is violated, the brick is flagged dirty with safety priority.

### Proof Scope and Claim Types

To keep correctness claims precise and auditable, the plan distinguishes proof-backed guarantees from measured/heuristic behavior:

- **Proof-backed guarantees:** Only apply on certified paths where bound provenance is region-valid (`Analytic`, `AnalyticBernstein`, or conservative `Sampled`), preconditions are satisfied (stencil scope, norm hygiene, dimensional tags), and fail-closed behavior is active.
- **Measured engineering claims:** Performance targets (fps, step counts, divergence reduction, ms budgets) are empirical acceptance criteria, not theorems.
- **Heuristic paths:** Any non-certified or approximation path is explicitly labeled heuristic, never used to justify conservative-safety guarantees, and must have a clear failover to certified conservative stepping or a non-certified gameplay/render fallback.
- **Documentation rule:** When language uses “guaranteed/provable,” it refers only to certified conservative properties (e.g., no overshoot under stated preconditions), not image quality or throughput outcomes.

### Lipschitz Certificate Portfolio (LCP)

> **The engine carries multiple conservative certificates in parallel and takes the tightest one at runtime.** This eliminates the "L1 diagonal tax" (~1.73x step penalty on diagonal rays) without compromising safety.

The current plan tracks both anisotropic `B = (Bx, By, Bz)` and scalar `L`. These induce *different norms*, and the choice of norm affects step size — sometimes dramatically. The key insight: you can be strict about norm hygiene *inside* each certificate while benefiting from multiple norms *globally*, because `max` of conservative lower bounds is conservative.

**Certificate A (Weighted L1, anisotropic):**
```
D_L1(p,q) = Bx|px-qx| + By|py-qy| + Bz|pz-qz|
b_L1(p)   = max_i( b_i - D_L1(p, x_i) )
L_dir_L1(v) = dot(abs(v), B)
```
(`b_i` are canonical lower-bound samples; same samples are reused across certificates. Only the metric changes.)

**Certificate B (Euclidean L2, scalar):**
```
D_L2(p,q) = L * ||p - q||_2
b_L2(p)   = max_i( b_i - L * ||p - x_i||_2 )
L_dir_L2(v) = L    (direction-independent for unit v)
```
Precondition: use only when scalar certificate is region-valid (`has_l2_certificate=true`, provenance not `Unknown`, and finite `L>0`). The L2 certificate must use the Euclidean metric and must be region-valid over the same step-valid region as the L1 certificate used for clamping. Otherwise fail closed to Certificate A only.

**Fusion theorem (one-line proof):**
```
b*(p) = max( b_L1(p), b_L2(p) )
```
Since `b_L1(p) ≤ f(p)` and `b_L2(p) ≤ f(p)`, their maximum is also `≤ f(p)`. QED.

**Fused denominator:**
```
L_dir*(v) = min( L_dir_L1(v), L_dir_L2(v) )
```
Both are valid upper bounds on `|∇f·v|`, so their minimum is also a valid upper bound.

**Fused safe step:**
```
step = min( max(0, b*(p0)) / max(L_dir*(v), eps), distance_to_region_exit )
```
Same shape as before, strictly better numbers.

**Why this matters:** For a sphere primitive, `L = 1` and `B = (1,1,1)`. Along a diagonal ray `v = (1,1,1)/√3`:
- L1: `L_dir_L1 = 1/√3 + 1/√3 + 1/√3 = √3 ≈ 1.732` → step = `b / 1.732`
- L2: `L_dir_L2 = 1.0` → step = `b / 1.0`
- Fused: `L_dir* = min(1.732, 1.0) = 1.0` → **1.73x larger step on diagonals**

After re-distance (where the field is close to SDF with `L ≈ 1`), the L2 certificate dominates and you get near-sphere-tracing step sizes. In warp-heavy regions where `L` is huge, L1 dominates and you're no worse than today.

**Mixed-norm cone-union stepping:**

Each envelope term defines safe balls in both norms:
- L1: octahedron `D_L1(p, x_i) ≤ b_i`
- L2: sphere `||p - x_i||_2 ≤ b_i / L`
(`b_i <= 0` or invalid `L` yields empty L2 ball; this is a safe no-op, not an error.)

Along a ray, each ball projects to a 1D interval. The connected component of the *full union* (L1 intervals ∪ L2 intervals) from `t=0` gives the maximal safe step. Same safety proof as current cone-union: for every `s ∈ [0, step]`, at least one certified ball contains `p(s)`, so `f(p(s)) ≥ 0`.

L2 intervals per sample are a cheap quadratic solve:
```
||w + tv||^2 ≤ r^2,  w = p0 - x_i,  r = b_i / L
t^2 + 2(w·v)t + (w·w - r^2) ≤ 0
Δ = (w·v)^2 - (w·w - r^2)
If Δ < 0: empty.  Else: t ∈ [-w·v - √Δ, -w·v + √Δ] ∩ [0, ∞)
```

**Quality ladder from math (not just budget knobs):**
- Low: L1 only (`b_L1`, `L_dir_L1`) — current path, cheapest
- Medium: Fused dual-envelope (`b* = max(b_L1, b_L2)`, `L_dir* = min(...)`) — for an 8-corner stencil, ~8 extra Euclidean distance evals (`sqrt`) per query
- High: Fused + mixed cone-union (L1 octahedra ∪ L2 spheres) — maximal step sizes

### Lipschitz Composition Rules

These are the "theorems" that make the engine work. They're simple algebra, but they must be implemented correctly in the field graph IR and enforced at compile time (Phase 14).

```
Primitive SDFs:              L = 1, B = (1,1,1), epsilon = 0
  sphere, box, plane, capsule, cylinder, torus, cone, rounded box

Addition:                    L(f + g) <= L(f) + L(g)
Scalar multiply:             L(alpha*f) = |alpha| * L(f)
Min (hard union):            L(min(f,g)) <= max(L(f), L(g))
Max (hard intersect):        L(max(f,g)) <= max(L(f), L(g))
Smooth blend:                L depends on blend function choice:
  Polynomial smooth-min:     formula-dependent.
                              Standard IQ clamped variant smin(a,b,k) = min(a,b) - h²k/4,
                                h = max(k - |a-b|, 0) / k:
                                |∂smin/∂a| <= 1, |∂smin/∂b| <= 1
                                => L(smin_poly(f,g,r)) <= max(L(f), L(g))   (same as log-sum-exp)
                                Proof: when a < b, ∂smin/∂a = 1 - h/2 ∈ [1/2, 1],
                                ∂smin/∂b = h/2 ∈ [0, 1/2]. Symmetric for b < a.
                                Outside blend region (|a-b| >= k): smin = min(a,b), partials ∈ {0, 1}.
                              WARNING: The variant smin(a,b,k) = a - s²/(4k), s=clamp(b-a,-k,k)
                                does NOT satisfy |partials| <= 1. Its ∂/∂a = 1 + s/(2k) ∈ [1/2, 3/2],
                                ∂/∂b = -s/(2k) ∈ [-1/2, 1/2]. The safe generic Lipschitz bound is
                                L ≤ (|∂/∂a| + |∂/∂b|) * max(L_f, L_g). At s=k: |∂/∂a|+|∂/∂b| = 1.5+0.5 = 2,
                                giving L up to 2× max(L(f), L(g)) (not 1.5× as previously stated).
                                A tighter mixed bound is L ≤ 1.5*max(L_f,L_g) + 0.5*min(L_f,L_g),
                                but if the compiler only tracks max-style constants, use the 2× bound.
                                The same 2× factor applies to anisotropic B composition for this variant:
                                B_out ≤ 2 * max(B_f, B_g) (or the tighter 1.5*B_f + 0.5*B_g form).
                                Do not use this form if the max(L_f, L_g) bound is needed;
                                use the IQ min(a,b)-based form above.
                              POLICY for OTHER polynomial smooth-min variants:
                              The engine whitelists a small set of smooth-min operators with
                              proven partial-derivative caps (c_a, c_b) such that |∂s/∂a| ≤ c_a,
                              |∂s/∂b| ≤ c_b over the blend region. The Lipschitz bound is then:
                                L ≤ c_a * L(f) + c_b * L(g)
                              Whitelisted operators (certified):
                                - IQ min(a,b)-based:  c_a=1, c_b=1  → L ≤ max(L_f, L_g)
                                - Log-sum-exp:        c_a=1, c_b=1  → L ≤ max(L_f, L_g)
                                - Regularized R-fn:   sup(|∂a|+|∂b|) = 2+√2 → L ≤ 3.414 * max(L_f, L_g)
                              Any smooth-min variant NOT on the whitelist gets Unknown provenance
                              (fail-closed: falls back to scalar L_fallback, no directional stepping).
                              The old default "L ≤ L(f) + L(g)" is NOT safe for arbitrary variants
                              because it assumes |∂s/∂a| ≤ 1, |∂s/∂b| ≤ 1, which is unverified.
                              To add a new variant: prove (c_a, c_b) caps, ship with tests, add to whitelist.
  Log-sum-exp smooth-min:    L(lse(f,g,r))  <= max(L(f), L(g))        (convex combination, no extra term)
  Regularized R-function:     R_eps(a,b)=a+b-sqrt(a^2+b^2+eps_r^2), eps_r>0
                              L(R_eps(f,g)) <= (2+sqrt(2)) * max(L(f), L(g))   (~3.414x, tight analytical bound)
                              Proof: partials dR/da=1-a/D, dR/db=1-b/D, D=sqrt(a^2+b^2+eps^2).
                              sup|dR/da|+|dR/db| at a=b=-t, t->inf: 2+2t/sqrt(2t^2) = 2+sqrt(2).
                              This bound is exact; the commonly cited 4x is unnecessarily loose.
  NOTE: unregularized R0(a,b)=a+b-sqrt(a^2+b^2) is not C1 at (a,b)=(0,0). Avoid in production.
  Choose log-sum-exp for tightest L; choose regularized R-function for style and smoothness.

Composition (f after g):     L(f compose g) <= L(f) * L(g)
Domain warp p' = p + w(p):   L(f(p')) <= L(f) * (1 + L(w))
  This is WHY warps break SDF-ness: if L(w) > 0, L(f(p')) > L(f).
  For twist with rate k: L(w) ~ k * radius, so L grows with distance from axis.
  For single-octave noise displacement with amplitude A and frequency omega:
    L(w) <= A * omega * L_noise_base.
  For FBM displacement with gain g and lacunarity lambda over N octaves:
    L(w) <= A * omega * L_noise_base * sum_{i=0}^{N-1}(g*lambda)^i
         = A * omega * L_noise_base * (1-(g*lambda)^N)/(1-g*lambda),  g*lambda != 1
         = A * omega * L_noise_base * N,                               g*lambda == 1

  Concrete L_noise_base values per noise primitive:
    Improved Perlin 3D:  L_noise_base = sqrt(3) ≈ 1.732
      (PROVENANCE: this is an empirically validated conservative bound, NOT analytically
       derived from gradient magnitudes. Standard improved Perlin uses gradient vectors from
       {(1,1,0), (1,-1,0), (-1,1,0), ...} and permutations, which have magnitude √2, not 1.
       The stated bound √3 accounts for the full tensor-product interpolant structure
       (quintic fade × gradient dot products) but the one-line "unit gradient" proof is
       insufficient. Assign this Sampled or Inflated provenance, not Analytic.
       Verify against Bernstein-certified tight bounds on reference cells before trusting.
       Bernstein certification supersedes this when available and is the preferred path.)
    Simplex 3D:          L_noise_base ≈ 0.96 * sqrt(3) ≈ 1.663
      (derive explicitly from your implementation's gradient table; slightly lower than
       Perlin due to simplex lattice geometry. Must be verified per implementation.)
    Worley/Voronoi:      L_noise_base = implementation-specific, use Sampled provenance.
      (not polynomial — Bernstein certification does not apply)
  NOTE: L_noise_base values above are temporary conservative estimates validated by
  sampling, NOT safe for provable paths. Assign Sampled or Inflated provenance, never
  Analytic. For provably correct bounds, use Bernstein-certified patch evaluation
  (AnalyticBernstein provenance) which computes tight per-cell derivative bounds from
  the actual polynomial structure. The fallback constants are only for non-certified
  gameplay/render paths where conservatism violation degrades quality but not safety.

Sampling into brick (voxel size h):

  PREFERRED: Lipschitz envelope reconstruction (no epsilon needed in hot loop).
  Given stored lower-bound samples b_i at lattice positions x_i and cell derivative
  bounds B_cell = (Bx, By, Bz), reconstruct the certified lower bound at query point p:

    D_B(p, q) = Bx*|px-qx| + By*|py-qy| + Bz*|pz-qz|   (weighted L1 distance)
    b_env(p)  = max_i ( b_i - D_B(p, x_i) )               (Lipschitz lower envelope)

  Safety proof: By derivative bounds, f(p) >= f(x_i) - D_B(p, x_i) >= b_i - D_B(p, x_i).
  Taking max over i preserves the inequality. So f(p) >= b_env(p).

  Optimality (McShane extension theorem): b_env(p) is the greatest L-Lipschitz function
  that is ≤ b_i at each sample x_i. No conservative estimator using only (b_i, B) can
  produce a larger safe value at any query point.

  For the 8-corner stencil of a cell, this costs 8 weighted-L1 distances + 8 subtractions
  + reduction to max. No epsilon subtraction, no interpolation damage to repair.

  Optional upper envelope (for uncertainty estimation):
    Given stored upper-bound samples u_i >= f(x_i):
    u_env(p) = min_i ( u_i + D_B(p, x_i) )
  The interval [b_env(p), u_env(p)] bounds f(p) and drives adaptive LOD, re-distance
  scheduling, and budget allocation (refine where interval crosses zero = surface).
  Contract: if interval-driven features are enabled, engine MUST provide conservative
  upper samples with quantization invariant `u_i >= true f(x_i)` (round up or +1 ULP).
  If upper samples are unavailable, treat u_env = +inf and disable interval-driven
  scheduling/debug metrics that rely on finite interval width.

  FALLBACK: epsilon subtraction (for when envelope stencil is unavailable):
  `epsilon_sample = (h/2) * (Bx + By + Bz)` is valid when each stored sample value
  is bound to the voxel CENTER (maximum coordinate offset per axis = h/2).
  This is the L1-norm-consistent bound (matches the weighted L1 metric used by L_dir).
  If a different sample convention is used (e.g. corner/cell-vertex semantics), the
  offset bound must be adjusted accordingly before applying this formula.
  Legacy L2-based bound epsilon = L_region * sqrt(3)/2 * h is still valid but uses
  a different norm and is not preferred.

NOTE ON NORMS: Conservative stepping is certificate-specific and must be internally consistent:
  Certificate A (L1/aniso): L_dir_L1(v) = dot(abs(v), B) = Bx|vx| + By|vy| + Bz|vz|,
  D_B distances, and L1 epsilon bounds must all use the same weighted L1 metric.
  Certificate B (L2/scalar, LCP): D_L2 distances and scalar `L` must use Euclidean norm.
  Fusion is allowed only across complete conservative certificates (`b_fused=max(...)`,
  `L_dir*=min(...)`), never by mixing terms inside one certificate.
  The L2 half-diagonal sqrt(3)/2 * h is an L2 quantity; the L1 half-diagonal is
  (h/2)*(1+1+1) = 3h/2 in the unweighted case. Mixing norms silently breaks conservatism.
  When in doubt, use the L1-consistent formula.
  Scalar fallback derivation policy (when `L_fallback` is derived from B rather than
  propagated independently):
    preferred: L_fallback = ||B||_2
    allowed:   L_fallback = ||B||_1   (looser but conservative)
    forbidden: using `max(B)` / `||B||_inf` as the primary denominator when anisotropic
               `B` is available for directional stepping (under-conservative for diagonal
               rays; can violate march safety and silently reintroduce overshoot).

Anisotropic derivative composition (componentwise):
  Addition:                  B(f+g) = B(f) + B(g)
  Scalar multiply:           B(alpha f) = |alpha| * B(f)
  Min/Max:                   B(min/max(f,g)) = max(B(f), B(g))
  Polynomial smooth-min:     Standard IQ clamped variant: B(smin_poly(f,g,r)) <= max(B(f), B(g))
                             (same proof as scalar case: partials bounded by 1; uses min(a,b)-based form)
                             Other variants: whitelist policy (same as scalar case above).
                             Each whitelisted variant provides certified (c_a, c_b) caps:
                             B_out ≤ c_a * B(f) + c_b * B(g). Non-whitelisted → Unknown provenance.
                             The old default B ≤ B(f) + B(g) assumed |∂s/∂a|,|∂s/∂b| ≤ 1; unsafe in general.
  Log-sum-exp smooth-min:    B(lse(f,g,r)) <= max(B(f), B(g))
  Warp/chain rule:
    if p' = g(p), and A_{j,i} bounds |∂p'_j/∂x_i| over region:
    B_out = A^T * B_in

Mip level n (downsampled by 2^n):
  Store b at lattice points (canonical lower bound b = d - epsilon at each sample position).
  Storage invariant for b-samples: serialized `b_i` MUST remain conservative lower bounds
  (`b_i <= true f(x_i)` after quantization). If using fp16 for storage, enforce downward
  rounding (or subtract 1 ULP after conversion) before upload.
  Store B_cell per mip cell (region-valid derivative bounds).

  PREFERRED: Envelope-based mip construction (Lipschitz closure).
  Instead of min-reduction (which poisons entire parent cells from one near-surface child):

  Step 1 — Decimate: pick every-other sample from the finer level (subsample at coarse
  lattice points). These are automatically conservative at those positions because they
  reuse known lower bounds.

  Step 2 — Tighten via separable Lipschitz closure (max-plus cone envelope):
  For each axis (x, then y, then z) with weight w = B_axis * voxel_size_at_mip:
    left[0]   = b[0]
    left[j]   = max(b[j], left[j-1] - w)
    right[N-1]= b[N-1]
    right[j]  = max(b[j], right[j+1] - w)
    b[j]      = max(left[j], right[j])
  This computes b[j] = max_i(b_orig[i] - w*|j-i|) exactly in O(N) per axis.
  In 3D with separable D_B, apply sequentially along x, y, z. Total: 6 linear passes.

  PRECONDITION: w must be constant across the pass domain (i.e., B_axis is the
  MAX derivative bound over ALL voxels along the 1D line being closed). If B varies
  spatially (warps, noise), use w = max(B_axis) over the full line/region. Using a
  per-voxel B that is too small for some segments makes the closure under-conservative.
  Alternative (tighter, same complexity): use per-segment weights w_{j→j+1} in the
  forward/backward passes: left[j] = max(b[j], left[j-1] - w_{j-1}). This stays
  O(N) and preserves conservatism with piecewise-varying bounds.

  Result: each mip sample is the greatest lower bound consistent with the
  retained coarse-lattice samples (after decimation) and the Lipschitz constraint.
  Thin features no longer smear pessimism across
  large regions. A single near-surface child pessimizes only its local neighborhood (within
  B * voxel_size), not the entire parent cell.

  At query time: use envelope reconstruction b_env(p) = max_i(b_i - D_B(p, x_i)) over the
  mip cell's lattice stencil. No global epsilon subtraction needed.
  Bound-validity rule:
  - If stencil points are confined to the current cell, `B_cell` is sufficient.
  - If stencil expands beyond the cell (e.g. 27-point neighborhood), use `B_region`
    valid over the full stencil support and clamp step to `distance_to_region_exit`.

  FALLBACK: min-reduction (for simplicity during initial bringup).
  b_mip = min_i(b_child_i). Safe but aggressively pessimistic. Replace with envelope
  construction once the brick pool is stable.

  Mip values MUST NOT be averaged. Averaging overestimates distance and breaks conservatism.

Cone stepping rule:
  At distance t from camera, use mip level n where 2^n * h ~ pixel_footprint(t).
  b = b_env(p)   (envelope reconstruction over mip cell stencil, or b_mip fallback)
  L_safe = max(L_dir(v), epsilon_denom)
  step_raw = max(0, b) / L_safe
  step = min(step_raw, distance_to_active_region_exit)   // mip-cell exit for in-cell stencil, region exit for expanded stencil

Connected Cone-Union Stepping (acceleration for envelope cells):

  The standard step uses a single scalar b_env(p0) and divides by L_dir. But the
  envelope representation contains more information: each stencil sample defines a
  weighted-L1 ball (octahedron) of provably-safe space:

    SafeSet = ⋃_i { p : D_B(p, x_i) ≤ b_i }

  Along a ray p(t) = p0 + t*v, each octahedron projects to a 1D interval I_i.
  The farthest safe step is the end of the connected component of ⋃ I_i containing t=0.

  Computing I_i: Each weighted-L1 ball has 8 halfspace inequalities (one per sign vector).
  Along the ray, each becomes a linear t-constraint:
    a_s + t * m_s ≤ b_i
  where a_s = s·(B⊙(p0-x_i)), m_s = s·(B⊙v), s ∈ {±1}^3.
  Intersect 8 half-lines → interval [t_enter_i, t_exit_i] (or empty).

  Connected component from t=0 (tiny loop, at most N iterations for N stencil points):
    end = 0
    repeat:
      find any I_i with start ≤ end and end_i > end
      end = max(end, end_i)
      if nothing extends, stop
    step = min(end - small_pad, distance_to_active_region_exit)

  Safety proof: If for every s ∈ [0, step], some stencil ball contains p(s), then
  b_env(p(s)) ≥ 0, so f(p(s)) ≥ 0. The ray cannot cross the surface. QED.
  This proof assumes the bound used in D_B is valid over the entire stencil support
  traversed by the ray segment; otherwise fallback to in-cell stencil or expand bound region.

  WHY this helps: Standard stepping pretends the lower bound must decrease at rate
  L_dir from the current point. Cone-union stepping allows the "responsible corner"
  to change along the ray, so the step can be dramatically larger — often reaching the
  active-region exit in one step even when b_env(p0) is small.

  Activation gates (keep heavy math off the hot path when it won't help):

  Gate 1 — standard step already near-optimal:
    L_safe = max(L_dir(v), epsilon_denom)
    step_std = min(max(0, b_env(p0)) / L_safe, distance_to_active_region_exit)
    if step_std >= 0.8 * distance_to_active_region_exit:
      step = step_std   // conservative fast path already near-optimal

  Gate 2 — pathological B (large bounds shrink weighted-L1 balls to nothing):
    median_b = median(b_i for stencil samples)
    if median_b < 0.05 * L_safe * distance_to_active_region_exit:
      step = step_std   // balls are tiny relative to cell, cone-union won't extend meaningfully
    else:
      step = cone_union_step(...)    // envelope geometry gives larger step

  Gate 2 prevents cone-union from paying ~150 ALU ops in scenes where B is huge
  (warped/high-frequency regions). When B is pathological, the weighted-L1 balls
  shrink and the union becomes tiny — cone-union can't extend beyond the standard
  step. The median_b threshold is a crude proxy; it can be tuned/learned later.

  Gate 3 — envelope slack (LCP-enabled, requires dual-envelope evaluation):
    b_fused = dual_envelope_lower_bound(p0, samples, b_cell, l_scalar, true)
    b_linear = max(0, b_env(p0))  // standard L1-only envelope at ray origin
    slack = b_fused - b_linear
    if slack < 0.05 * distance_to_active_region_exit:
      step = b_fused / fused_directional_bound(v, b_cell, l_scalar, true)
      // LCP fused stepping (cheaper than full cone-union)
    else:
      step = mixed_cone_union_safe_step(...)  // full mixed-norm cone-union

  Gate 3 uses the *difference* between the fused dual-envelope bound and the L1-only
  bound as a kernel-side activation signal. When the L2 certificate provides meaningful
  extra headroom (slack is large), the full mixed-norm cone-union pays off. When both
  certificates agree (slack is small), the cheaper fused stepping captures most of the
  benefit. This creates a three-tier cost ladder:
    - Gate 1 passes: ~5 ALU ops (standard step)
    - Gate 2 fails or Gate 3 slack is small: ~30 ALU ops (fused dual-envelope step)
    - Gate 3 slack is large: ~200 ALU ops (mixed L1+L2 cone-union, but saves 3-5 march iterations)

  Cost: 8 corners × 8 halfspaces = 64 linear t-bounds, collapse to 8 intervals,
  then the grow-end loop (at most 8 iterations). ~150 ALU ops for L1-only cone-union.
  Mixed cone-union adds N quadratic solves (~200 ALU total). Saves 2-5 main march
  iterations (each with texture fetch + envelope reconstruction) → net win.
```

### Spacetime Lipschitz Extension (4D)

> **The same invariant, lifted one dimension.** Everything above works in (x,y,z). Promoting the field from `f(p)` to `f(p,t)` and the derivative bound from 3 components to 4 gives guaranteed continuous collision detection and procedural anime smear frames from the same math.

**The 4D contract:** Promote B to `B4 = (Bx, By, Bz, Bt)` where `Bt` bounds `|∂f/∂t|` over the active region and time interval. The weighted L1 metric extends naturally:

```
D_B4((p,t), (q,s)) = Bx|px-qx| + By|py-qy| + Bz|pz-qz| + Bt|t-s|
```

**Spacetime stepping theorem (same proof, one dimension up):**
If `f` is B4-Lipschitz over a spacetime region, and at `(p,t)` you have conservative lower bound `b_lower ≤ f(p,t)`, then for any spacetime displacement `(Δp, Δt)` with `D_B4 ≤ b_lower`, you are guaranteed `f(p+Δp, t+Δt) ≥ 0`.

**CCD along a trajectory:** For a moving point `p(t) = p0 + v*t`, define `g(t) = f(p(t), t)`. A Lipschitz constant for g is:
```
L_path = Bx|vx| + By|vy| + Bz|vz| + Bt
```
(where `Bt` captures how the field itself changes over time; if the world is static over the tick, `Bt=0` and this simplifies).

Safe time step from an outside point:
```
Δt_safe = max(0, b_lower) / L_path
```

Proof (one line):
```
g(t+Δt) ≥ g(t) - L_path·Δt ≥ b_lower - L_path·(b_lower/L_path) = 0
```

The ray cannot cross the surface during the safe time step. When `b` is small, bracket and bisect.

**Computing Bt for rigid motion:** An object with object-space field `f_obj(x')` and rigid motion `x' = R(t)^T(x - p(t))`:
```
|∂f/∂t| = |∇f_obj(x') · dx'/dt|
dx'/dt = -R^T v - ω' × x'    (where ω' = R^T ω is angular velocity in body frame)
```
Given componentwise derivative bounds in object space `|∂f/∂x'_i| ≤ B_obj_i`:
```
|∂f/∂t| ≤ dot(abs(-R^T v - ω' × x'), B_obj)    (ω' = R^T ω, consistent with body-frame dx'/dt)
```
Over a cell/brick, bound `|x'|` from the AABB extents or radius. Region-valid `Bt_cell` with no heuristics.

**Swept-volume surface (anime smear frames):** Over shutter interval `[t0, t1]`, the swept field is:
```
F_sweep(x) = min_{t ∈ [t0,t1]} f(x, t)
```
The swept volume (everything the object occupied during the shutter) is `F_sweep(x) ≤ 0`. This is the exact set-theoretic union in implicit form — `min` corresponds to union for signed implicit solids.

Practical evaluation with certified bounds: For fixed x, define `g(t) = f(x,t)` with time-Lipschitz constant `L_t ≥ Bt_cell`. Sample at times `{t_i}` and compute `g_i = g(t_i)`:
- **Inside certificate:** if any `g_i < 0`, point was inside at `t_i`, so it's in the sweep.
- **Outside certificate (McShane in 1D time):**
  `g(t) ≥ max_i(g_i - L_t|t - t_i|)` (lower envelope of V-functions).
  If `min_t max_i(g_i - L_t|t - t_i|) > 0`, point was outside for the entire shutter interval.
  The minimum of the piecewise-linear lower hull is computable in O(M) for M time samples.

This gives smear frames that are: procedural, conservative, stable under topology changes, and artistically controllable (shutter interval, thickness clamp, combat intensity modulation).

**When Bt = 0 (static fields):** Everything simplifies. No overhead for static environment. Bt matters only for moving objects (characters, weapons, projectiles).

### Why this matters for the engine

With (B, b_env) tracked everywhere:
- **Ray marching never misses surfaces** (conservative stepping guarantee)
- **Curvature estimates are bounded** (we know how much to trust the Laplacian)
- **Phase 14 proofs become tractable** (prove L bounds, not unit gradient)
- **The quality ladder works** (quality changes adjust bounds/stencils/resolution in controlled ways, degrading quality before safety)
- **Dual contouring stays valid** (Hermite data has known error bounds)

With spacetime B4 additionally:
- **High-speed weapons never tunnel** through thin terrain (CCD from the same math, not a separate system)
- **Anime smear frames are mathematically correct** swept volumes, not animation hacks
- **Temporal correctness** for motion blur and accumulation without disocclusion lies

### Curvature: What We Can Actually Compute Cheaply

The v1 plan claimed "Hessian with 6 samples." That's mean curvature from the Laplacian, not the full Hessian (which needs 9+ samples and an eigen solve). Here's what's actually practical:

**Mean curvature (cheap, always available):**
For a field close to a distance field, mean curvature H ~ Laplacian(f) = d2f/dx2 + d2f/dy2 + d2f/dz2. This costs 6 extra texture fetches (+/-h along each axis) and one sum. It tells you concavity (H < 0) vs convexity (H > 0). That's enough for:
- Moss grows in concavities
- Erosion streaks on convexities
- AO-like darkening in crevices
- Outline width modulation

**Divergence of normalized gradient (robust curvature proxy):**
If the field deviates from a true distance field: H ~ div(grad(f) / |grad(f)|). More samples but stable when |grad(f)| != 1. Use this near R-function blend regions.

**Principal curvature directions (expensive, deferred):**
Full Hessian eigen solve. Reserve for Phase 6 (painterly stroke direction) at reduced resolution, or precompute per-brick and store as compressed 2-float direction field.

**Stored gradient + curvature in bricks (recommended):**
Per voxel, store:
- d: f32 — distance value
- n: [f16; 2] — octahedral-encoded gradient direction (unit normal)
- g_mag: f16 — gradient magnitude |∇f| (needed by Whitney quadratic envelope; see below)
- H: f16 — mean curvature
Total: 4 + 4 + 2 + 2 = 12 bytes per voxel (vs 4 bytes for distance only). The g_mag channel enables the Whitney quadratic envelope to use the actual gradient vector (g_mag * n_i) instead of the unit normal, which is required for conservatism when |∇f| < 1. If g_mag is omitted (10-byte format), the Whitney envelope MUST be disabled and only McShane C^0 used. This eliminates all finite-difference sampling at render time for primary shading. The gradient, magnitude, and curvature are computed once during brick population and reused for every ray that enters the brick.

---

## Mathematical Stack and Novel Methods (Implementation Reference)

This section is the explicit "what math are we actually using" map. Each entry includes the concrete object, the algorithm class, and where it appears in phases.

### 1) Bounded Implicit Geometry (Phases 0, 2, 3, 14, 16)

- **State carried per sample/region:** canonical lower bound `b = d - epsilon` plus derivative bounds (`L` scalar fallback or anisotropic `B=[Bx,By,Bz]`).
- **Conservative marching:** `step = min(max(0,b)/L_safe, distance_to_region_exit)` where `L_dir(v)=dot(abs(v),B)` (or `L_dir=L` fallback) and `L_safe=max(L_dir, epsilon_denom)`.
- **Novel angle:** explicit Lipschitz/derivative-bound calculus through the field graph + conservative cache representation at every mip level.
- **Why this is unusual:** most real-time SDF pipelines assume near-SDF behavior and rely on heuristics; this plan makes conservative bounds first-class and verifiable.

### 2) Multi-Resolution Field Caching (Phases 2, 10, 12, 16)

- **Sparse clipmaps and virtual pages:** world key `(level, coord)` with pool-slot indirection.
- **Conservative mip construction:** envelope-based Lipschitz closure (separable max-plus cone) preferred; `b_parent = min_i(b_child_i)` as fallback. Bounds valid over each mip cell.
- **Narrow-band re-distance maintenance:** local Eikonal relaxation around dirty edits.
- **Novel angle:** "git for bricks" content-addressed persistence plus conservative traversal math.

### 3) Neural Fields as Sources, Not Inner-Loop Renderers (Phase 3)

- **Canonicalization map:** `p_rest = T^{-1}(pose, p_world)` via motor/dual-quat inverse skinning.
- **Field evaluation:** `f(p_world, pose) ≈ f_canonical(p_rest) + f_residual(p_rest, local_pose)`.
- **Runtime strategy:** prebake to character-local brick volumes, then march cached field.
- **Morphing:** displacement-field advection `f_t(p) = f_source(p - t*D(p))` with closest-point correspondence and Laplacian smoothing. Physically correct mass flow (no ghostly cross-fades). Lipschitz tracked via standard warp rule.
- **Novel angle:** keeps neural continuity benefits while preserving WebGPU inner-loop feasibility. Displacement morphing gives Akira-style shape-shifting without optimal transport cost.

### 4) Transform Algebra (Phase 4a, optional 4b)

- **Motor/dual-quat interpolation:** `M(t) = exp(t * log(M))`.
- **Skinning blend:** normalized weighted motor blend.
- **Novel angle:** SE(3)-correct interpolation and volume behavior in a fully procedural field pipeline.

### 5) Spectral Basis Shading (Phase 5, 16)

- **Basis representation:** `S(lambda) = sum_i c_i * B_i(lambda)`.
- **Lighting/material operations:** low-dimensional matrix ops in basis space.
- **Observer mapping:** stylized `T_anime` transform to RGB.
- **Novel angle:** physically informed hue behavior under anime quantization, not RGB hacks.

### 6) Stochastic Painterly + Reconstruction (Phases 6, 16)

- **Noise model:** spatiotemporal blue noise for all temporal stochastic decisions.
- **Deterministic CI mode:** frozen/seed-locked stochastic sequence.
- **Novel angle:** expressive stochasticity in shipping mode with deterministic reproducibility in test mode.

### 7) PDE World Dynamics + Fracture (Phase 10)

- **Diffusion/reaction fields:** corruption, moisture, temperature, growth, irradiance.
- **Irradiance PDE:** screened diffusion with source/absorption coupling.
- **Fracture model:** event-driven stress intensity checks, not unstable global wave simulation.
- **Novel angle:** world deformation, lighting, and gameplay ecology coupled through shared field state.

### 7b) Spectral Region Sleep via DMD (Phases 10, 12, 15)

- **Dynamic Mode Decomposition:** truncated SVD of PDE state snapshot matrix extracts K dominant reduced dynamics directions.
- **Authoritative evolution form (real-valued):** evolve reduced state with real Schur form of reduced operator:
  `c_{k+1} = T c_k`, `x_k ≈ Ψ c_k`, and skip `m` epochs via `c_{k+m} = T^m c_k` (O(log m) via exponentiation by squaring).
- **Optional timelapse interpolation:** for presentation only, evaluate blockwise continuous interpolation from Schur blocks over `Δt_snap` (snapshot interval).
- **Tiered catch-up:** full PDE (short), DMD + corrective steps (medium), DMD + stochastic reseed (long).
- **Stability enforcement:** clamp Schur block spectral radius to `<= 1` in discrete time to prevent sleeping-region divergence.
- **Limitation boundary:** DMD is a linear approximation of nonlinear dynamics. Prediction quality degrades through bifurcations and near strongly nonlinear operating points. Corrective PDE steps and drift monitoring mitigate this. The engine explicitly does NOT claim exact evolution for nonlinear systems.
- **Novel angle:** no shipping game engine uses data-driven spectral decomposition for continuous-time world evolution. The combination of DMD compression, stability-clamped reduced dynamics, tiered catch-up with nonlinear correction, and drift monitoring is novel applied math for real-time persistent worlds.

### 7c) Lipschitz Envelope Reconstruction (Phases 0, 2, 14)

- **McShane extension (C^0, baseline):** `b_env(p) = max_i(b_i - D_B(p, x_i))` where `D_B` is the weighted L1 distance induced by anisotropic bounds `B`.
- **Optimality theorem (McShane 1934):** `b_env(p)` is the greatest L-Lipschitz function ≤ b_i at each sample x_i. No conservative estimator using only `(b_i, B)` can produce a larger safe value at any query point. This is a provable mathematical guarantee, not an empirical observation.
- **Whitney-style quadratic envelope (Medium+ quality, uses stored gradient/curvature):** Since we store per-voxel gradient direction `n_i`, gradient magnitude `g_mag_i = |∇f(x_i)|`, and a second-derivative bound `K_i` (12 bytes/voxel format), the C^0 McShane envelope leaves information on the table. A conservative first-order + quadratic-remainder lower envelope gives tighter bounds using this data:
  ```
  b_env_C1(p) = max_i(b_i + g_mag_i * dot(n_i, p - x_i) - (K_cell/2) * |p - x_i|^2)
  ```
  where `g_mag_i * n_i` reconstructs the actual gradient `∇f(x_i)` (not just the unit direction) and `K_cell` is the **Hessian operator-norm bound** (semiconvexity constant) for the cell: `K_cell >= sup_region ||Hf(x)||_op = sup_region max_eigenvalue(|Hf(x)|)`.

  **CRITICAL: Why g_mag is required.** The Taylor remainder inequality uses the actual gradient `∇f(x_i)`, not the unit normal. If `|∇f| < 1` (which occurs near R-function blends and before re-distance), using the unit normal `n_i` alone overestimates the linear term `dot(n_i, dx) > dot(∇f, dx)`, which can produce `b_env_C1(p) > f(p)` — silently breaking conservatism and allowing surface overshoot. The `g_mag_i` factor fixes this. If gradient magnitude data is unavailable (10-byte voxel format), the Whitney envelope MUST be disabled and only McShane C^0 used.

  **IMPORTANT: K_cell is NOT mean curvature.** Mean curvature `H = κ_1 + κ_2` is the trace of the shape operator. The Taylor remainder inequality requires the operator norm `||Hf||_op = max(|κ_1|, |κ_2|)`, which bounds the maximum directional second derivative. These differ: a saddle point with `κ_1 = -κ_2` has mean curvature zero but nonzero Hessian norm. Using mean curvature as the quadratic penalty would *overestimate* the lower bound, silently breaking conservatism.

  **How K_cell is computed (three options, in order of preference):**
  1. **Composed through field graph (preferred):** Propagate second-derivative bounds through the field graph IR alongside first-derivative bounds B, using composition rules analogous to the Lipschitz algebra (addition: K(f+g) <= K(f)+K(g), warp: chain rule on second derivatives with Jacobian and Hessian of the warp). This gives `Analytic` provenance. Derivation is Phase 14 work; initial implementation uses option 2.
  2. **Sampled over region grid (Phase 2b default):** Estimate the full symmetric Hessian (3 pure + 3 mixed partials) with central differences at each sample point in the region grid, then inflate by an explicit truncation/quantization margin before taking a conservative operator-norm upper bound (e.g., Gershgorin bound with safety inflation). This yields `Sampled` provenance.
  3. **Fail-closed fallback:** There is no generally valid conservative mapping from first-derivative bound `B` alone to `K_cell >= ||Hf||_op`. If `Analytic` or conservative `Sampled` `K_cell` is unavailable, disable this quadratic envelope for the region and fall back to McShane C^0 envelope.

  **Safety proof:** Taylor expansion with Hessian remainder: `f(p) >= f(x_i) + grad(f)(x_i) · (p - x_i) - (K_cell/2)|p - x_i|^2 >= b_i + g_mag_i * dot(n_i, p - x_i) - (K_cell/2)|p - x_i|^2` (using `grad(f)(x_i) = g_mag_i * n_i`). Taking max over `i` preserves the inequality. This requires `K_cell >= ||Hf||_op` over the region — the operator norm, not the trace.
  **Why tighter:** Near sample points, the linear gradient term keeps the bound close to the true field value longer before the quadratic correction dominates. McShane's linear decay `b_i - D_B(p, x_i)` drops faster.
  **Normal behavior at brick boundaries:** This envelope is piecewise smooth inside a fixed active sample branch, but the `max` across branches can introduce kinks at active-branch switches. It still tightens lower bounds; for shading continuity, use filtered/temporally stabilized normals from stored normal channels rather than assuming global C^1 continuity from the envelope alone.
  **Quality ladder:** Low = McShane C^0 (8 weighted-L1 distances). Medium+ = Whitney-style quadratic envelope (8 dot products + 8 quadratic corrections + reduction). Cost is ~2x per query and generally produces tighter lower bounds.
- **Separable mip construction:** max-plus cone envelope via 6 linear passes (2 per axis). Replaces min-reduction which poisons entire parent cells from one near-surface child. Thin features only pessimize their local `B * voxel_size` neighborhood.
- **Upper envelope:** `u_env(p) = min_i(u_i + D_B(p, x_i))` provides tightest upper bound. The interval `[b_env, u_env]` bounds the true field value and drives adaptive LOD, re-distance scheduling, and refinement allocation.
- **Connected cone-union stepping:** The envelope defines a union of weighted-L1 balls (octahedra) around stencil points. Along a ray, each ball projects to a 1D interval. The connected component of the interval union containing `t=0` gives a provably safe step that can be dramatically larger than the conservative baseline `step_std = min(max(0,b_env(p0))/L_safe, distance_to_active_region_exit)` — often reaching the active-region exit in one step. This converts the McShane representation from a pointwise estimator into a traversal accelerator.
- **Novel angle:** standard real-time SDF renderers use trilinear interpolation with heuristic epsilon correction. The envelope approach is provably optimal (well-known in extension theory but not previously applied to real-time field rendering) and eliminates epsilon subtraction on certified envelope paths; epsilon center-bound fallback remains for non-envelope bringup and fail-closed cases. The cone-union extension directly exploits the geometric structure of the envelope for ray traversal — this specific combination has not appeared in the literature. The Whitney-style quadratic upgrade using stored gradient/curvature data is a novel application of classical extension theory to real-time field rendering.

### 7d) Spacetime Lipschitz Envelopes (Phases 0, 9, 6, 11, 14)

- **4D extension:** Promote `B` to `B4 = (Bx, By, Bz, Bt)` where `Bt` bounds `|∂f/∂t|` over the region and time interval. Same weighted L1 metric, same McShane envelope, same conservative stepping — one dimension up.
- **CCD as "sphere tracing in time":** For a moving point `p(t) = p0 + v*t`, `L_path = dot(abs(v), B_spatial) + Bt`. Safe time step: `Δt_safe = max(0, b_lower) / L_path`. Proof is one line. No separate collision geometry needed. Works for non-SDF fields where traditional CCD assumes you have actual distance.
- **Bt from rigid body kinematics:** `|∂f/∂t| ≤ dot(abs(-R^T v - ω' × x'), B_obj)` (ω' = R^T ω in body frame) with `|x'|` bounded from AABB. Region-valid, no heuristics.
- **Swept-volume smear frames:** `F_sweep(x) = min_t f(x,t)` over shutter interval. Outside is certified from O(M) time samples via the 1D McShane lower hull; inside is sample-evidenced (`f(x,t_i) <= 0`), otherwise classification remains uncertain and uses refinement/fallback. Procedural, conservative, topologically stable, artistically controllable.
- **Static field optimization:** When `Bt = 0` (static environment), everything reduces to the 3D case. No overhead.
- **Novel angle:** No shipping engine treats time as a first-class Lipschitz dimension with certified stepping and envelope reconstruction. The combination of CCD + smear frames + motion blur under one conservative calculus, provable in Phase 14, is new.

### 7e) Lipschitz Certificate Portfolio (Phases 0, 2, 14, 16)

- **Dual-envelope fusion:** Carry both L1 (anisotropic `B`) and L2 (scalar `L`) certificates in parallel. Fusion theorem: `b* = max(b_L1, b_L2)` is conservative because both lower bounds are ≤ `f(p)`. Fused denominator: `L_dir* = min(dot(abs(v),B), L)` because both are valid upper bounds on `|∇f·v|`.
- **Diagonal tax elimination:** Pure L1 stepping with `B=(1,1,1)` penalizes diagonal rays by up to √3 ≈ 1.73x (because `dot((1/√3,1/√3,1/√3), (1,1,1)) = √3` while the true L2 Lipschitz constant is 1.0). Dual-envelope fusion selects `L_dir* = min(√3, 1.0) = 1.0` on diagonal rays — full step recovery.
- **Mixed-norm cone-union stepping:** Union of L1 octahedra AND L2 spheres along ray. The connected component of the combined interval union starting from `t=0` is strictly larger than either norm alone. Particularly effective in SDF-like regions where L2 spheres are dramatically larger than L1 octahedra.
- **Quality ladder integration:** Low = L1-only (zero overhead). Medium = fused dual-envelope (two envelope evaluations, cheap). High = fused + mixed cone-union (full interval computation when Gate 3 slack justifies it).
- **Three truths policy:** `b_lower` for marching safety, `d_est` for contacts/animation, stored normal for shading. Different truth sources for different consumers, all derived from the same field.
- **Novel angle:** Standard real-time field renderers commit to a single norm. The LCP fusion is a one-line theorem that is trivially correct but provides material performance improvement (20%+ step reduction on diagonal views) at negligible implementation cost. The mixed-norm cone-union extension is new.

### 7f) Bernstein-Certified Noise (Phases 0, 2b, 14)

- **Tight region-valid bounds from polynomial structure:** Improved (gradient) Perlin noise is piecewise polynomial (degree 6 per axis, not 5) over each lattice cell. The quintic fade s(t) = 6t^5 - 15t^4 + 10t^3 is degree 5, but it multiplies a linear gradient-dot-product term, yielding degree 6 total in each interpolated axis: noise(t) = a(t) + s(t)·(b(t) - a(t)) where a(t),b(t) are linear. Converting to Bernstein basis gives tight range and derivative bounds via the convex hull property — no sampling, no inflation factors.
  **NOTE:** If certifying VALUE noise (corner values constant, no gradient dots), degree is 5 and the counts below reduce. The degree-6 counts apply specifically to improved (gradient) Perlin.
- **Bernstein convex hull property:** For a polynomial `p(x) = sum c_i B_i(x)` in Bernstein form, `min(c_i) <= p(x) <= max(c_i)` for all `x` in the domain. Same applies to each partial derivative (degree 5 Bernstein for a degree-6 polynomial). These are guaranteed convex-hull bounds; they can be tightened further via de Casteljau subdivision without root-finding. Serialization/quantization still requires conservative rounding slack.
- **Frequency-bounded FBM evaluation:** Evaluate octaves low-to-high, maintaining residual tail interval from Bernstein-certified patch ranges. When `accumulated ± tail_max` can't cross the decision threshold, skip remaining octaves entirely. This is the primary performance lever for noise-heavy scenes — most voxels are far from the surface, so 1-2 octaves suffice.
- **Provenance: `AnalyticBernstein`** — region-valid by construction, not by sampling or inflation. Tighter than `Inflated` provenance by a factor that depends on how far the query cell is from noise extrema (typically 1.5-3x tighter).
- **Scope constraint:** `AnalyticBernstein` is valid for polynomial cell patches (improved Perlin in this plan). Non-polynomial noise (e.g. Worley/Voronoi) must use `Sampled` or `Inflated` provenance, never `AnalyticBernstein`.
- **Cost model:** ~1225 Bernstein coefficient evaluations per noise cell for gradient Perlin (degree 6 per axis): 343 for value range (7^3 tensor product) plus 882 for derivative bounds (3 partial derivatives × 6×7×7 = 294 coefficients each, degree 5 in differentiated axis × degree 6 in others). For value noise (degree 5): ~756 (216 + 540). Computed once during brick population. Amortized across all rays traversing the cell. The early-exit savings in the march inner loop dramatically exceed the bake cost.
- **Novel angle:** Bernstein basis for noise Lipschitz bounds exists in the approximation theory literature but has not been applied to real-time field rendering. The frequency-bounded FBM early-exit trick using certified tail intervals is new.

### 7g) Lipschitz Budget Marching (Phases 2a, 2b, 14)

- **The problem:** The current stepping rule clamps `step <= distance_to_region_exit`, forcing a full envelope reconstruction + texture fetch every time a ray crosses a brick boundary. In empty space (far from surfaces), rays spend most of their life paying this "region-exit clamp tax" repeatedly even when the initial `b_lower` is large enough to prove safety across many bricks.

- **The theorem (fundamental theorem of calculus + Lipschitz bound):**
  Let `p(t) = p0 + t*v` with `||v|| = 1`, and `g(t) = f(p(t))`. Suppose conservative lower bound `b0 <= g(0)` and piecewise directional bound `L(t)` such that `|g'(t)| <= L(t)` for all `t` in the traversal. Then:
  ```
  g(T) >= g(0) - ∫_0^T L(s) ds >= b0 - ∫_0^T L(s) ds
  ```
  Choose `T` such that `∫_0^T L(s) ds <= b0`, and `g(t) >= 0` for all `t ∈ [0, T]`. The ray cannot cross the surface anywhere in that interval.

- **Algorithm (brick-level DDA with budget spending):**
  ```
  1. Evaluate conservative lower bound once at p0: b = b_lower(p0) - pad_b  (pessimistic)
  2. Set budget = max(0, b)
  3. While budget > 0 and traversals < MAX_BUDGET_TRAVERSALS:
     a. Determine current brick AABB
     b. Fetch brick metadata: B_brick, L_brick, provenance
     c. If provenance is Unknown: STOP (fail closed, re-evaluate f)
     d. Compute directional bound for this brick:
        L1:  Lr = dot(abs(v), B_brick)
        LCP: Lr = min(dot(abs(v), B_brick), L_brick)
     e. Compute ray distance to exit brick AABB: Δt_exit
     f. Compute segment cost: cost = Lr * Δt_exit + pad_cost  (pessimistic, see Truth #4)
     g. If cost <= budget: advance to boundary, budget -= cost, continue
     h. Else: advance Δt = budget / Lr, budget = 0, stop
  4. Re-evaluate f at the new position (one expensive field eval instead of many)
  ```

- **Practical knob:** Cap `MAX_BUDGET_TRAVERSALS` at 8-32 bricks per outer iteration to control GPU thread divergence. If the cap is hit, stop early and re-evaluate — still a win.

- **Hierarchical option:** Traverse at brick AABB granularity first (cheap, big leaps). If stopped inside a brick with remaining budget, optionally refine within that brick using mip-cell bounds.

- **Numerical safety (Truth #4 integration):**
  - When computing `b_lower`, subtract `pad_b` (f32 ULP pad) so the budget never starts inflated.
  - When computing each segment cost, use `cost_raw = Lr * Δt_exit` and `pad_cost = 2.0 * f32::EPSILON * max(|cost_raw|, 1.0)`, then `cost = cost_raw + pad_cost` so costs are never undercounted.
  - Both pads are the same philosophy as f16 directed rounding — consistently pessimistic by a sliver.

- **LCP integration:** Budget marching uses `Lr = min(dot(abs(v), B_brick), L_brick)` per region when both certificates are available. This means diagonal rays in SDF-like regions spend budget at rate 1.0 instead of √3 — budget lasts √3x longer on diagonals.

- **What you get:** In empty space with `b0 = 80m`, the current code takes many 1-4m steps (one expensive field eval per brick crossing). Budget marching traverses dozens of bricks with cheap DDA + metadata reads, then does ONE expensive field eval at the end. Attacks p95 step counts and divergence because most rays spend most of their life in empty space.

- **Provenance fail-closed:** If any brick along the DDA path has `Unknown` provenance, the traversal stops immediately and falls back to the existing per-sample stepping. No budget is spent through Unknown regions.

- **Testing (property test):**
  1. Pick random fields from field-graph generator, random rays
  2. Compute `p_end` using budget marching from `p0` and `b_lower(p0)`
  3. Densely sample `f(p(t))` along `[0, t_end]` on CPU, assert `f >= 0` everywhere
  4. Assert `t_end >= t_std - tol` where `t_std` is the current conservative step (never meaningfully regresses)
  5. Test Unknown provenance regions: assert clean fallback, no budget traversal

- **Novel angle:** Standard sphere tracers stop at the sample point. Budget marching is the line-integral generalization — it spends a single conservative evaluation as a credit across many regions. The combination with per-brick provenance-gated fail-closed traversal and f32 ULP padding is new. The result is "global sphere tracing behavior from local bounds."

### 8) Distributed Determinism (Phases 0, 10, 12)

- **Authoritative simulation:** server fixed-tick evolution with canonical ordering and deterministic RNG keys.
- **Validation:** per-region hash stream for replay/divergence detection.
- **Novel angle:** strict gameplay determinism with intentionally non-deterministic client visuals.

### 9) Compiler-Driven Runtime (Phases 8, 14)

- **Compilation target:** GPU-resident DAG/bytecode, stable interpreter kernels.
- **Static analysis:** affine-arithmetic-based conservative proofs for bounds/cost/ranges.
- **Sensitivity propagation:** forward derivatives through DAG where tractable.
- **Novel angle:** "compiler is renderer" with conservative analysis and hot iteration.

### 10) Hyperfidelity Optimization Layer (Phase 16)

- **Wavefront compaction, reservoir reuse, directional caches, perceptual budgeting, cinematic accumulation, neural radiance cache, Gaussian splat vegetation, DEC ink wash, spatiotemporal blue noise reconstruction, field-exact temporal supersampling.**
- **Novel angle:** field-aware render scheduling that uses geometric uncertainty and saliency as optimization signals. Lipschitz-driven cache invalidation, hybrid field+splat rendering, DEC-exact fluid simulation, and motor-derived motion vectors are novel applications of the field engine's mathematical infrastructure to rendering quality.

---

## Rework Minimization Strategy

| Phase | What Survives Forever | What Gets Replaced Later |
|-------|----------------------|--------------------------|
| 0. Contracts + Infrastructure | Lipschitz algebra, envelope reconstruction (McShane C^0 + Whitney-style quadratic envelope), LCP dual-envelope + fused stepping + mixed cone-union, Bernstein noise certification + frequency-bounded FBM, **budget marching traversal**, spacetime stepping contracts, B provenance tracking (type-level `RegionValidBound`) + propagation rules, f16 conservative conversion, **f32 ULP padding contracts**, quality profiles, debug views, blue noise, determinism | Nothing — foundational |
| 1. Anime Style Shell | Cel shader, outlines, palettes, post-processing, shadow cascade wiring, VFX integration | Nothing significant — phase is intentionally thin and feeds Phase 2a |
| 2a. Falsification Spike | Field primitives, brick pool, envelope reconstruction, cone-union stepping, compute ray march, measurement harness | Spike renderer is intentionally minimal and rebuilt in 2b; measurement harness may be retained |
| 2b. Full Field Engine | Field graph IR, brick pool, clipmaps, compute ray march, conservative stepping, layered composition, dual contouring, GI probes | Matrix-based field transforms (motors in P4a), RGB material (spectral in P5) |
| 3. Neural Field Characters + Anatomy | Anatomy system, MLP training, character brick prebake, canonical-space conditioning, displacement-field morphing | Flat joint rotations (motors in P4a), hand-tuned anatomy params (evolved by P13) |
| 4a. Motor Transform Pipeline | Dual-quat/motor skinning, SE(3) log/exp interpolation | Nothing — foundational |
| 4b. Extended Geometric Algebra | CGA primitives, PGA domain ops, motor lattice repetition | Nothing — extends 4a |
| 5. Spectral Materials | Spectral basis representation, spectral cel shader, anime observer | Nothing — replaces RGB |
| 6. Stochastic Painting | Brush stroke model, ink wash, paper texture, swept-volume smear frames, deterministic + expressive modes | Nothing — purely additive |
| 7. Procedural Audio | Synthesis engine, voice architecture, sound recipes | Nothing — foundational |
| 8. Recipe DSL | DAG bytecode compiler, stable interpreter kernel, sensitivity propagation | Nothing — extends compiler |
| 9. Physics Animation | IK solver, variational integrators, Cosserat rod hair, Kirchhoff plate cloth, anime timing, smear frames, field-driven contact, spacetime CCD for fast contacts | Nothing — replaces keyframes |
| 10. Living World / PDE | Multi-res field simulation, stochastic PDE noise, layered edits, region epochs, irradiance PDE, fracture mechanics, DMD spectral sleep + tiered catch-up | Nothing — extends brick pool |
| 11. Emotional Rendering | Mood state machine, temporal crystallization, manga effects | Nothing — orchestrates everything |
| 12. Infinite World | Virtualized world address space, content-addressed bricks, biome hierarchy, streaming, DMD reduced-state persistence | Nothing — extends brick pool |
| 13. Ecology + Evolution | Genomes, mutation, selection, population dynamics, curated seed genomes, inverse design | Nothing — drives anatomy |
| 14. Conservative Proofs | Lipschitz/resource/range proofs, affine arithmetic, escape hatches | Nothing — compiler addition |
| 15. Temporal Archaeology | Region timelapse, forensic mode, epoch history navigation | Nothing — reads Phase 12 history |
| 16. Hyperfidelity WebGPU | Wavefront compaction, reservoir lighting, directional radiance cache, spectral super-sampling, perceptual budget, cinematic accumulation, neural radiance cache, Gaussian splat vegetation, DEC ink wash, blue noise reconstruction, field-exact temporal supersampling | Nothing — purely additive fidelity track. DEC ink wash upgrades Phase 6's finite-difference implementation. |

The planned rework: Phase 2a's spike renderer is intentionally throwaway — it validates the math/perf gate, then Phase 2b rebuilds properly. Measurement harness and profiling methodology may be retained. Phase 1 is now deliberately minimal so almost no work is invalidated by Phase 2. Phase 3's hand-tuned anatomy parameters get driven by Phase 13's evolution. This is intentional — each is cheap to build, necessary to prove the pipeline, and the infrastructure it validates is permanent. Phase 2a exists specifically because the reviewer is right: if compute ray marching can't hit perf targets on WebGPU, you want to know BEFORE building neural field training and DMD sleep systems.

---

## Phase 0: Contracts, Infrastructure, and Debug Tooling

**Purpose:** Define the mathematical contracts, quality ladder, determinism infrastructure, and debug tooling that every subsequent phase depends on. Small phase (~1 week) but it's the foundation that prevents the project from collapsing under its own complexity.

**Why this exists:** Every reviewer independently flagged the same gaps: no memory budgets, no quality profiles, no determinism strategy, no debug views. Adding these as afterthoughts in Phase 6+ is too late.

**What's built:**

### 0a. Lipschitz Algebra Library (Rust, CPU-side)
```rust
/// A bounded implicit field value with conservative tracking.
/// Every field computation produces one of these.
/// Provenance of derivative bounds — fail-closed to scalar L when not region-valid.
#[derive(Copy, Clone, PartialEq)]
enum BoundProvenance {
    Analytic,           // derived from analytic formula valid over declared region
    AnalyticBernstein,  // derived from Bernstein basis convex hull property over lattice cell
                        // (region-valid by construction — guaranteed convex-hull bounds; tightenable via subdivision)
    Sampled,            // computed from worst-case of sampled Jacobians over region grid
    Inflated,           // analytically derived but inflated by safety factor (noise bounds)
    Unknown,            // provenance not established (user-supplied assume_bound, etc.)
}

/// Type-safe region-valid derivative bound. Cannot be constructed from Unknown provenance.
/// The marching kernel accepts this type, not raw [f32; 3], preventing "local B as cell B" bugs.
struct RegionValidBound {
    b: [f32; 3],
    provenance: BoundProvenance, // Analytic, AnalyticBernstein, Sampled, or Inflated — never Unknown
}

impl RegionValidBound {
    /// Construct from analytically derived bounds (provenance: Analytic or AnalyticBernstein).
    fn from_analytic(b: [f32; 3], bernstein: bool) -> Self {
        Self { b, provenance: if bernstein { BoundProvenance::AnalyticBernstein } else { BoundProvenance::Analytic } }
    }
    /// Construct from worst-case sampled Jacobians over region grid (provenance: Sampled).
    fn from_sampled(b: [f32; 3]) -> Self { Self { b, provenance: BoundProvenance::Sampled } }
    /// Inflate a point-valid bound to region-valid by adding safety margin (provenance: Inflated).
    fn inflate_point_to_region(point_b: [f32; 3], safety_factor: f32) -> Self {
        Self { b: [point_b[0] * safety_factor, point_b[1] * safety_factor, point_b[2] * safety_factor],
               provenance: BoundProvenance::Inflated }
    }
}

struct FieldSample {
    distance: f32,    // sampled distance-like value
    region_bound: Option<RegionValidBound>, // None when provenance is Unknown (forces scalar L fallback)
    lipschitz: f32,   // scalar Lipschitz bound L (Euclidean)
    lipschitz_provenance: BoundProvenance, // tracks how L was derived (for LCP: must be region-valid)
    epsilon: f32,     // sampling error bound
}
// NOTE: For LCP (dual-envelope), L must be DIRECTLY propagated through composition rules,
// not loosely derived from B. A directly tracked L can be dramatically tighter than ||B||_2.
// Example: sphere primitive has L=1 (exact), but ||B||_2 = ||(1,1,1)||_2 = √3 ≈ 1.73.

/// Safe stepping from a canonical lower bound b (preferred envelope path).
/// This is THE invariant in canonical form.
fn safe_step_from_lower_bound(
    b_lower: f32,
    dfdxyz_bound: [f32; 3],
    has_anisotropic_bound: bool,
    lipschitz_fallback: f32,
    ray_dir_unit: [f32; 3],
    distance_to_region_exit: f32,
) -> f32 {
    let v = [ray_dir_unit[0].abs(), ray_dir_unit[1].abs(), ray_dir_unit[2].abs()];
    let l_dir = if has_anisotropic_bound {
        v[0] * dfdxyz_bound[0]
      + v[1] * dfdxyz_bound[1]
      + v[2] * dfdxyz_bound[2]
    } else {
        lipschitz_fallback
    };
    let l_safe = f32::max(l_dir, 1e-6);
    f32::min(f32::max(0.0, b_lower) / l_safe, distance_to_region_exit)
}

/// Compatibility helper for legacy distance+epsilon call sites.
/// Converts to canonical lower bound and delegates.
fn safe_step_from_sample(sample: &FieldSample, ray_dir_unit: [f32; 3], distance_to_region_exit: f32) -> f32 {
    let b_lower = sample.distance - sample.epsilon;
    safe_step_from_lower_bound(
        b_lower,
        sample.dfdxyz_bound,
        sample.has_anisotropic_bound,
        sample.lipschitz,
        ray_dir_unit,
        distance_to_region_exit,
    )
}

/// Lipschitz composition rules (compile-time and runtime).
fn lipschitz_union(a: f32, b: f32) -> f32 { f32::max(a, b) }
fn lipschitz_poly_smin(a: f32, b: f32, _radius: f32) -> f32 {
    a + b  // conservative default; use tighter formula-specific bound only if proven
}
fn lipschitz_lse_smin(a: f32, b: f32) -> f32 {
    f32::max(a, b)  // log-sum-exp: convex combination, no extra term
}
fn lipschitz_r_function(a: f32, b: f32) -> f32 {
    (2.0 + std::f32::consts::SQRT_2) * f32::max(a, b)  // tight: ~3.414x
}
fn lipschitz_warp(field_l: f32, warp_l: f32) -> f32 {
    field_l * (1.0 + warp_l)
}
fn lipschitz_composition(outer: f32, inner: f32) -> f32 { outer * inner }
fn scalar_fallback_from_bound_l2(b: [f32; 3]) -> f32 {
    (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt()
}
fn scalar_fallback_from_bound_l1(b: [f32; 3]) -> f32 {
    b[0] + b[1] + b[2]
}

/// Anisotropic derivative-bound rules.
fn bound_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn bound_max(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}
/// Chain rule with absolute Jacobian bound A (column-major indices shown explicitly).
/// B_out[i] = sum_j A[j][i] * B_in[j]
fn bound_chain(a_abs_jacobian: [[f32; 3]; 3], b_in: [f32; 3]) -> [f32; 3] {
    [
        a_abs_jacobian[0][0] * b_in[0] + a_abs_jacobian[1][0] * b_in[1] + a_abs_jacobian[2][0] * b_in[2],
        a_abs_jacobian[0][1] * b_in[0] + a_abs_jacobian[1][1] * b_in[1] + a_abs_jacobian[2][1] * b_in[2],
        a_abs_jacobian[0][2] * b_in[0] + a_abs_jacobian[1][2] * b_in[1] + a_abs_jacobian[2][2] * b_in[2],
    ]
}

/// Lipschitz envelope reconstruction (McShane/Whitney extension).
/// Computes the optimal (McShane) conservative lower bound at query point p
/// given stored lower-bound samples and derivative bounds.
/// Includes f32 ULP padding to absorb round-to-nearest arithmetic error.
fn lipschitz_envelope(
    p: [f32; 3],
    samples: &[([f32; 3], f32)],  // (position, lower_bound_value) at lattice points
    b_cell: [f32; 3],              // anisotropic derivative bound for the cell
) -> f32 {
    let mut b_env = f32::NEG_INFINITY;
    let mut max_abs_term = 1.0_f32;
    for &(x_i, b_i) in samples {
        let d_b = b_cell[0] * (p[0] - x_i[0]).abs()
                + b_cell[1] * (p[1] - x_i[1]).abs()
                + b_cell[2] * (p[2] - x_i[2]).abs();
        let candidate = b_i - d_b;
        b_env = f32::max(b_env, candidate);
        max_abs_term = max_abs_term
            .max(b_i.abs())
            .max(d_b.abs())
            .max(candidate.abs());
    }
    // f32 ULP padding: b_i - d_b can round UP by one ULP. Subtract a pessimistic pad
    // to guarantee the result never exceeds the true field value. (Truth #4: numerics)
    let pad = 2.0 * f32::EPSILON * max_abs_term.max(b_env.abs());
    b_env - pad
}

/// Whitney-style quadratic envelope reconstruction (Medium+ quality).
/// Uses stored gradients (with magnitude) and Hessian operator-norm bound for tighter bounds than McShane C^0.
/// Piecewise smooth within a branch; max-branch switches are not globally C^1.
/// Requires per-voxel gradient data (12 bytes/voxel format: d + n + g_mag + H) + per-cell K bound.
/// IMPORTANT: k_cell is the Hessian OPERATOR-NORM bound (semiconvexity constant),
/// NOT mean curvature. See "Whitney-style quadratic envelope" section for derivation.
/// IMPORTANT: g_mag_i (gradient magnitude) is REQUIRED for conservatism. Using unit normals
/// alone breaks the Taylor remainder inequality when |∇f| < 1.
fn whitney_c1_envelope(
    p: [f32; 3],
    samples: &[([f32; 3], f32, [f32; 3], f32, f32)],  // (position, lower_bound, unit_normal, _per_sample_curvature_unused, gradient_magnitude)
    k_cell: f32,  // Hessian operator-norm bound for the cell: K >= sup_region ||Hf||_op
) -> f32 {
    let mut b_env = f32::NEG_INFINITY;
    let mut max_abs_term = 1.0_f32;
    for &(x_i, b_i, n_i, _, g_mag_i) in samples {
        let dx = [p[0] - x_i[0], p[1] - x_i[1], p[2] - x_i[2]];
        // Use actual gradient = g_mag * unit_normal, NOT unit normal alone.
        // When |∇f| < 1, unit normal overestimates the linear term → non-conservative.
        let dot_grad_dx = g_mag_i * (n_i[0] * dx[0] + n_i[1] * dx[1] + n_i[2] * dx[2]);
        let dist_sq = dx[0] * dx[0] + dx[1] * dx[1] + dx[2] * dx[2];
        let quad = (k_cell / 2.0) * dist_sq;
        // Whitney-style quadratic lower bound candidate:
        // b_i + g_mag_i * dot(n_i, p - x_i) - (K_cell/2) * |p - x_i|^2
        let candidate = b_i + dot_grad_dx - quad;
        b_env = f32::max(b_env, candidate);
        max_abs_term = max_abs_term
            .max(b_i.abs())
            .max(dot_grad_dx.abs())
            .max(quad.abs())
            .max(candidate.abs());
    }
    let pad = 2.0 * f32::EPSILON * max_abs_term.max(b_env.abs());
    b_env - pad
}

/// Upper envelope for uncertainty estimation.
/// Returns the tightest upper bound at p from stored upper-bound samples.
fn lipschitz_upper_envelope(
    p: [f32; 3],
    samples: &[([f32; 3], f32)],  // (position, upper_bound_value) at lattice points
    b_cell: [f32; 3],
) -> f32 {
    let mut u_env = f32::INFINITY;
    for &(x_i, u_i) in samples {
        let d_b = b_cell[0] * (p[0] - x_i[0]).abs()
                + b_cell[1] * (p[1] - x_i[1]).abs()
                + b_cell[2] * (p[2] - x_i[2]).abs();
        u_env = f32::min(u_env, u_i + d_b);
    }
    u_env
}

/// Separable Lipschitz closure (max-plus cone envelope) for mip construction.
/// Tightens 1D lower-bound array in-place along one axis.
/// After forward+backward pass, each b[j] = max_i(b_orig[i] - w*|j-i|).
///
/// PRECONDITION: w must be >= max(B_axis * voxel_size) over the entire 1D line.
/// If B varies spatially, use the worst-case (max) weight over the pass domain.
/// Using a local B that is too small for some segments breaks conservatism.
/// For per-segment weights, use the variant with w_j per step instead.
fn separable_lipschitz_closure_1d(b: &mut [f32], w: f32) {
    let n = b.len();
    if n == 0 { return; }
    // Forward pass (left-to-right propagation)
    for j in 1..n {
        b[j] = f32::max(b[j], b[j - 1] - w);
    }
    // Backward pass (right-to-left propagation)
    for j in (0..n - 1).rev() {
        b[j] = f32::max(b[j], b[j + 1] - w);
    }
}

/// Connected cone-union stepping: compute farthest safe step along ray
/// by marching through the union of weighted-L1 balls from envelope stencil.
/// Returns the end of the connected safe component containing t=0.
fn cone_union_safe_step(
    p0: [f32; 3],         // ray origin
    v: [f32; 3],          // unit ray direction
    samples: &[([f32; 3], f32)],  // (position, lower_bound) stencil points
    b_cell: [f32; 3],     // anisotropic derivative bound
    active_region_exit: f32, // distance to active bound-valid region exit (cell or expanded region)
) -> f32 {
    // Compute interval [t_enter, t_exit] for each stencil ball
    let mut intervals: [(f32, f32); 27] = [(0.0, 0.0); 27]; // max 27 for high-quality stencil
    let n = samples.len().min(27);
    for idx in 0..n {
        let (x_i, b_i) = samples[idx];
        let mut t_lo = 0.0_f32;
        let mut t_hi = f32::INFINITY;
        // Intersect 8 halfspaces: for each sign vector s ∈ {±1}^3,
        // a_s + t * m_s ≤ b_i  →  t ≤ (b_i - a_s) / m_s  (if m_s > 0)
        //                          t ≥ (b_i - a_s) / m_s  (if m_s < 0)
        for s in 0u32..8 {
            let sx = if s & 1 != 0 { 1.0_f32 } else { -1.0 };
            let sy = if s & 2 != 0 { 1.0_f32 } else { -1.0 };
            let sz = if s & 4 != 0 { 1.0_f32 } else { -1.0 };
            let a = sx * b_cell[0] * (p0[0] - x_i[0])
                  + sy * b_cell[1] * (p0[1] - x_i[1])
                  + sz * b_cell[2] * (p0[2] - x_i[2]);
            let m = sx * b_cell[0] * v[0]
                  + sy * b_cell[1] * v[1]
                  + sz * b_cell[2] * v[2];
            // Constraint: a + t*m ≤ b_i
            if m.abs() < 1e-12 {
                if a > b_i { t_hi = f32::NEG_INFINITY; } // infeasible
            } else if m > 0.0 {
                t_hi = f32::min(t_hi, (b_i - a) / m);
            } else {
                t_lo = f32::max(t_lo, (b_i - a) / m);
            }
        }
        intervals[idx] = if t_lo <= t_hi { (t_lo, t_hi) } else { (0.0, -1.0) }; // empty = (0, -1)
    }
    // Grow connected component from t=0
    let mut end = 0.0_f32;
    let mut changed = true;
    while changed {
        changed = false;
        for idx in 0..n {
            let (lo, hi) = intervals[idx];
            if lo <= end && hi > end {
                end = hi;
                changed = true;
            }
        }
    }
    // Conservative bias: pull back by ULP-consistent pad (matches envelope padding philosophy)
    let pad = 2.0 * f32::EPSILON * end.abs().max(1.0);
    let end_safe = end - pad;
    f32::min(f32::max(end_safe, 0.0), active_region_exit)
}

/// Conservative f16 conversion — the ONLY code path for bound serialization.
/// Direction-aware: upper bounds round toward +inf, lower bounds toward -inf.
/// Invariant across CPU targets.
enum BoundDirection { Upper, Lower }

fn f16_conservative(value: f32, direction: BoundDirection) -> u16 {
    // Start with deterministic round-to-nearest-even conversion.
    let mut h = f16_from_f32_rne(value);
    // IMPORTANT: do NOT use raw `bits +/- 1` here. IEEE-754 half ordering over bit
    // patterns is not monotonic across sign boundaries, and NaN/Inf/subnormal handling
    // must be explicit. Step via numeric next-up/next-down primitives.
    match direction {
        BoundDirection::Upper => {
            while f16_to_f32(h) < value {
                h = f16_next_up(h);      // IEEE numeric successor (+0 -> min positive subnormal, etc.)
            }
        }
        BoundDirection::Lower => {
            while f16_to_f32(h) > value {
                h = f16_next_down(h);    // IEEE numeric predecessor
            }
        }
    }
    h
    // NOTE: production code must define behavior for NaN/Inf explicitly and include
    // cross-target tests that verify identical outputs for representative edge cases.
}
```

**Lipschitz Certificate Portfolio (LCP) — dual-envelope + fused stepping:**
```rust
/// Dual-envelope evaluation: compute conservative lower bound from both
/// L1 (anisotropic) and L2 (Euclidean) certificates, return the tighter one.
/// This is the core of the LCP optimization.
fn dual_envelope_lower_bound(
    p: [f32; 3],
    samples: &[([f32; 3], f32)],  // (position, lower_bound) stencil
    b_cell: [f32; 3],              // anisotropic derivative bound B
    l_scalar: f32,                 // scalar Lipschitz bound L (Euclidean)
    has_l2_certificate: bool,      // true when L is region-valid with known provenance and finite > 0
) -> f32 {
    // Certificate A: weighted L1 envelope (always available when B is valid)
    // IMPORTANT: Pad each certificate SEPARATELY before fusing, so that
    // fusion never regresses vs either individual padded certificate.
    let mut b_l1 = f32::NEG_INFINITY;
    let mut max_abs_l1 = 1.0_f32;
    for &(x_i, b_i) in samples {
        let d_l1 = b_cell[0] * (p[0] - x_i[0]).abs()
                 + b_cell[1] * (p[1] - x_i[1]).abs()
                 + b_cell[2] * (p[2] - x_i[2]).abs();
        let candidate = b_i - d_l1;
        b_l1 = f32::max(b_l1, candidate);
        max_abs_l1 = max_abs_l1
            .max(b_i.abs())
            .max(d_l1.abs())
            .max(candidate.abs());
    }
    // Pad L1 certificate independently
    let pad_l1 = 2.0 * f32::EPSILON * max_abs_l1.max(b_l1.abs());
    let b_l1_padded = b_l1 - pad_l1;

    if !has_l2_certificate || !l_scalar.is_finite() || l_scalar <= 0.0 {
        return b_l1_padded;
    }

    // Certificate B: Euclidean L2 envelope
    let mut b_l2 = f32::NEG_INFINITY;
    let mut max_abs_l2 = 1.0_f32;
    for &(x_i, b_i) in samples {
        let dx = p[0] - x_i[0];
        let dy = p[1] - x_i[1];
        let dz = p[2] - x_i[2];
        let d_l2 = l_scalar * (dx*dx + dy*dy + dz*dz).sqrt();
        let candidate = b_i - d_l2;
        b_l2 = f32::max(b_l2, candidate);
        max_abs_l2 = max_abs_l2
            .max(b_i.abs())
            .max(d_l2.abs())
            .max(candidate.abs());
    }
    // Pad L2 certificate independently
    let pad_l2 = 2.0 * f32::EPSILON * max_abs_l2.max(b_l2.abs());
    let b_l2_padded = b_l2 - pad_l2;

    // Fusion: max of individually-padded certificates.
    // This preserves: b_fused >= b_l1_padded AND b_fused >= b_l2_padded
    // so fusion NEVER regresses vs either certificate alone.
    f32::max(b_l1_padded, b_l2_padded)
}

/// Fused directional bound: take the tighter of L1 and L2 directional bounds.
fn fused_directional_bound(
    ray_dir_unit: [f32; 3],
    b_cell: [f32; 3],
    l_scalar: f32,
    has_l2_certificate: bool,      // requires finite positive l_scalar
) -> f32 {
    let v = [ray_dir_unit[0].abs(), ray_dir_unit[1].abs(), ray_dir_unit[2].abs()];
    let l_dir_l1 = v[0] * b_cell[0] + v[1] * b_cell[1] + v[2] * b_cell[2];
    if has_l2_certificate && l_scalar.is_finite() && l_scalar > 0.0 {
        f32::min(l_dir_l1, l_scalar)  // L2 is direction-independent for unit v
    } else {
        l_dir_l1
    }
}

/// Mixed cone-union stepping: union of L1 octahedra AND L2 spheres along ray.
/// Returns end of connected safe component from t=0.
fn mixed_cone_union_safe_step(
    p0: [f32; 3],
    v: [f32; 3],
    samples: &[([f32; 3], f32)],
    b_cell: [f32; 3],
    l_scalar: f32,
    active_region_exit: f32,
) -> f32 {
    let l_scalar_safe = if l_scalar.is_finite() {
        l_scalar.max(1e-6)
    } else {
        f32::INFINITY
    };
    let max_n = samples.len().min(27);
    // Allocate intervals for both norms: up to 27 L1 + 27 L2
    let mut intervals: [(f32, f32); 54] = [(0.0, -1.0); 54]; // empty by default
    let mut n_intervals = 0usize;

    for idx in 0..max_n {
        let (x_i, b_i) = samples[idx];

        // L1 octahedron intervals (8-halfspace method, same as cone_union_safe_step)
        let mut t_lo = 0.0_f32;
        let mut t_hi = f32::INFINITY;
        for s in 0u32..8 {
            let sx = if s & 1 != 0 { 1.0_f32 } else { -1.0 };
            let sy = if s & 2 != 0 { 1.0_f32 } else { -1.0 };
            let sz = if s & 4 != 0 { 1.0_f32 } else { -1.0 };
            let a = sx * b_cell[0] * (p0[0] - x_i[0])
                  + sy * b_cell[1] * (p0[1] - x_i[1])
                  + sz * b_cell[2] * (p0[2] - x_i[2]);
            let m = sx * b_cell[0] * v[0]
                  + sy * b_cell[1] * v[1]
                  + sz * b_cell[2] * v[2];
            if m.abs() < 1e-12 {
                if a > b_i { t_hi = f32::NEG_INFINITY; }
            } else if m > 0.0 {
                t_hi = f32::min(t_hi, (b_i - a) / m);
            } else {
                t_lo = f32::max(t_lo, (b_i - a) / m);
            }
        }
        if t_lo <= t_hi {
            intervals[n_intervals] = (t_lo, t_hi);
            n_intervals += 1;
        }

        // L2 sphere interval (quadratic solve)
        let r = b_i / l_scalar_safe;
        if r > 0.0 {
            let wx = p0[0] - x_i[0];
            let wy = p0[1] - x_i[1];
            let wz = p0[2] - x_i[2];
            let w_dot_v = wx * v[0] + wy * v[1] + wz * v[2];
            let w_dot_w = wx * wx + wy * wy + wz * wz;
            let discriminant = w_dot_v * w_dot_v - (w_dot_w - r * r);
            if discriminant >= 0.0 {
                let sqrt_d = discriminant.sqrt();
                let t_enter = f32::max(0.0, -w_dot_v - sqrt_d);
                let t_exit = -w_dot_v + sqrt_d;
                if t_exit > 0.0 && t_enter <= t_exit {
                    intervals[n_intervals] = (t_enter, t_exit);
                    n_intervals += 1;
                }
            }
        }
    }

    // Grow connected component from t=0 (same algorithm as cone_union_safe_step)
    let mut end = 0.0_f32;
    let mut changed = true;
    while changed {
        changed = false;
        for idx in 0..n_intervals {
            let (lo, hi) = intervals[idx];
            if lo <= end && hi > end {
                end = hi;
                changed = true;
            }
        }
    }
    let pad = 2.0 * f32::EPSILON * end.abs().max(1.0);
    let end_safe = end - pad;
    f32::min(f32::max(end_safe, 0.0), active_region_exit)
}
```

**Lipschitz Budget Marching (empty-space acceleration):**
```rust
/// Metadata stored per brick for budget marching (read from brick metadata buffer).
struct BrickBudgetMeta {
    b_max: [f32; 3],     // region-valid anisotropic derivative bound for this brick
    l_max: f32,           // scalar Lipschitz bound (for LCP)
    provenance: BoundProvenance,
    aabb_min: [f32; 3],
    aabb_max: [f32; 3],
}

/// Budget march result: how far the ray advanced and whether it needs a full field eval.
struct BudgetMarchResult {
    t_advance: f32,       // total distance advanced along ray
    budget_remaining: f32, // remaining budget (0 = fully spent)
    bricks_traversed: u32,
    stopped_by_unknown: bool, // true if traversal hit Unknown provenance
}

/// Traverse multiple bricks along a ray using a single conservative budget.
/// Returns the farthest point provably outside the surface without re-evaluating f.
/// MAX_BUDGET_TRAVERSALS controls divergence (8-32 recommended).
const MAX_BUDGET_TRAVERSALS: u32 = 16;

fn budget_march_traverse(
    p0: [f32; 3],
    v: [f32; 3],          // unit ray direction
    b_lower: f32,         // conservative lower bound at p0 (already padded)
    get_brick_meta: &dyn Fn([f32; 3]) -> Option<BrickBudgetMeta>,  // brick lookup
    use_lcp: bool,        // true for LCP fused directional bound
) -> BudgetMarchResult {
    let mut budget = f32::max(0.0, b_lower);
    let mut t_total = 0.0_f32;
    let mut traversals = 0u32;
    let mut stopped_by_unknown = false;

    while budget > 0.0 && traversals < MAX_BUDGET_TRAVERSALS {
        let p_current = [p0[0] + t_total * v[0], p0[1] + t_total * v[1], p0[2] + t_total * v[2]];

        // Look up brick at current position
        let meta = match get_brick_meta(p_current) {
            Some(m) => m,
            None => break,  // left the clipmap
        };

        // Fail closed on Unknown provenance
        if matches!(meta.provenance, BoundProvenance::Unknown) {
            stopped_by_unknown = true;
            break;
        }

        // Directional bound for this brick
        let abs_v = [v[0].abs(), v[1].abs(), v[2].abs()];
        let l_dir_l1 = abs_v[0] * meta.b_max[0] + abs_v[1] * meta.b_max[1] + abs_v[2] * meta.b_max[2];
        let l_dir = if use_lcp && meta.l_max.is_finite() && meta.l_max > 0.0 {
            f32::min(l_dir_l1, meta.l_max)
        } else {
            l_dir_l1
        };
        let l_safe = f32::max(l_dir, 1e-6);

        // Ray-AABB exit distance
        let dt_exit = ray_aabb_exit_distance(p_current, v, meta.aabb_min, meta.aabb_max);

        // Segment cost with f32 ULP padding (Truth #4: never undercount)
        let cost_raw = l_safe * dt_exit;
        let pad_cost = 2.0 * f32::EPSILON * f32::max(cost_raw.abs(), 1.0);
        let cost = cost_raw + pad_cost;

        if cost <= budget {
            // Full brick traversal: advance to boundary
            t_total += dt_exit;
            budget -= cost;
            traversals += 1;
        } else {
            // Partial: spend remaining budget within this brick
            let dt_partial = budget / l_safe;
            t_total += dt_partial;
            budget = 0.0;
            traversals += 1;
        }
    }

    BudgetMarchResult {
        t_advance: t_total,
        budget_remaining: budget,
        bricks_traversed: traversals,
        stopped_by_unknown,
    }
}

fn ray_aabb_exit_distance(p: [f32; 3], v: [f32; 3], aabb_min: [f32; 3], aabb_max: [f32; 3]) -> f32 {
    let mut t_exit = f32::INFINITY;
    for i in 0..3 {
        if v[i].abs() > 1e-12 {
            let t_min = (aabb_min[i] - p[i]) / v[i];
            let t_max = (aabb_max[i] - p[i]) / v[i];
            t_exit = f32::min(t_exit, f32::max(t_min, t_max));
        }
    }
    f32::max(t_exit, 0.0)
}
```

**Bernstein-certified noise (tight region-valid bounds for procedural noise):**
```rust
/// Bernstein basis evaluation for a polynomial patch over a lattice cell.
/// Improved (gradient) Perlin noise is piecewise polynomial (degree 6 in each axis) per
/// lattice cell. Degree 6 because: quintic fade s(t) (degree 5) × linear gradient-dot
/// (degree 1) = degree 6 in each interpolated axis.
/// In Bernstein form, the convex hull property gives tight, region-valid-by-construction
/// range and derivative bounds — no sampling, no inflation factors.
///
/// Key insight: the Bernstein coefficients of a polynomial over [a,b] bound its range
/// from above and below: min(coeffs) <= p(x) <= max(coeffs) for all x in [a,b].
/// The same applies to each partial derivative (also polynomial, degree 5).
/// These bounds can be tightened via de Casteljau subdivision if needed.

/// A certified polynomial patch over one noise lattice cell.
/// Precomputed during brick population (CPU-side or compute shader).
struct CertifiedNoisePatch {
    /// Range of noise value over the cell: [value_min, value_max]
    value_range: [f32; 2],
    /// Per-axis derivative bounds from Bernstein coefficients of ∂p/∂x_i
    /// These are TIGHT (convex hull property), not inflated.
    derivative_bound: [f32; 3],  // [max|∂p/∂x|, max|∂p/∂y|, max|∂p/∂z|]
    provenance: BoundProvenance, // always AnalyticBernstein
}

/// Compute certified noise patch for one lattice cell.
/// Input: the 8 gradient vectors at cell corners (from Perlin hash table).
/// Output: tight range and derivative bounds via Bernstein conversion.
fn certify_noise_patch(
    cell_origin: [i32; 3],
    gradients: &[[f32; 3]; 8],  // corner gradients from hash
) -> CertifiedNoisePatch {
    // 1. Build quintic polynomial coefficients from Perlin's improved interpolant
    //    s(t) = 6t^5 - 15t^4 + 10t^3 (the smoothstep that makes improved Perlin C2)
    // 2. Convert to tensor-product Bernstein basis over [0,1]^3
    //    (degree 6 in each axis for gradient Perlin → 7^3 = 343 Bernstein coefficients for value)
    //    (degree 5 for value noise → 6^3 = 216 coefficients)
    // 3. Tight range: min/max of coefficients (convex hull property)
    //    Can be tightened via de Casteljau subdivision if needed.
    // 4. For each partial derivative (degree 5 in that axis, 6 in others for gradient Perlin):
    //    compute Bernstein coefficients of derivative polynomial,
    //    bound as max(abs(coefficients))
    //
    // Cost: ~1225 coefficient evaluations per cell for gradient Perlin
    //    (343 value + 3×294 derivatives, done once during brick population,
    //    NOT per ray step). Amortized across all rays that traverse the cell.
    todo!("implementation in Phase 0/2b")
}

/// Frequency-bounded FBM evaluation with Bernstein-certified early exit.
/// Evaluates octaves low-to-high, maintaining a residual tail interval from
/// certified patches. Stops when the tail provably can't change the outcome.
///
/// This is THE performance trick for noise-heavy scenes: most voxels are far from
/// the surface, so low octaves already prove "definitely outside" and higher
/// octaves never execute.
fn fbm_frequency_bounded(
    p: [f32; 3],
    base_freq: f32,
    lacunarity: f32,
    gain: f32,
    num_octaves: u32,
    certified_patches: &[CertifiedNoisePatch],  // one per octave, for cell containing p
    threshold: f32,  // decision boundary (e.g., 0.0 for surface, or current b_lower for stepping)
) -> FbmResult {
    let mut accumulated = 0.0_f32;
    let mut amplitude = 1.0_f32;
    let mut accumulated_b = [0.0_f32; 3];  // running derivative bound
    let mut octaves_evaluated = 0u32;

    for octave in 0..num_octaves {
        // Residual tail bound: sum of all remaining octave amplitudes × their certified ranges
        let mut tail_max = 0.0_f32;
        let mut a_remaining = amplitude;
        for remaining in octave..num_octaves {
            let patch = &certified_patches[remaining as usize];
            let worst_case = a_remaining * f32::max(patch.value_range[1].abs(), patch.value_range[0].abs());
            tail_max += worst_case;
            a_remaining *= gain;
        }

        // Early exit: if accumulated ± tail_max can't cross threshold, we're done
        if accumulated - tail_max > threshold || accumulated + tail_max < threshold {
            break;
        }

        // Evaluate this octave (standard Perlin improved noise at scaled frequency)
        let freq = base_freq * lacunarity.powi(octave as i32);
        let noise_value = evaluate_improved_perlin(p, freq);
        accumulated += amplitude * noise_value;

        // Accumulate derivative bounds from certified patch
        let patch = &certified_patches[octave as usize];
        accumulated_b[0] += amplitude * freq * patch.derivative_bound[0];
        accumulated_b[1] += amplitude * freq * patch.derivative_bound[1];
        accumulated_b[2] += amplitude * freq * patch.derivative_bound[2];

        amplitude *= gain;
        octaves_evaluated += 1;
    }

    FbmResult {
        value: accumulated,
        derivative_bound: accumulated_b,
        bound_provenance: BoundProvenance::AnalyticBernstein,
        octaves_evaluated,
    }
}

struct FbmResult {
    value: f32,
    derivative_bound: [f32; 3],
    bound_provenance: BoundProvenance,
    octaves_evaluated: u32,
}

fn evaluate_improved_perlin(_p: [f32; 3], _freq: f32) -> f32 {
    todo!("standard improved Perlin noise evaluation")
}
```

**Spacetime extension (Phase 9+ CCD, Phase 6/11 smear frames):**
```rust
/// A field sample extended with time-derivative bound for spacetime operations.
/// Used by CCD (Phase 9) and swept-volume smear frames (Phase 6/11).
struct FieldSample4 {
    b_lower: f32,              // canonical lower bound at (p, t)
    dfdxyz_bound: [f32; 3],    // spatial derivative bounds B over region
    b_time: f32,               // time-derivative bound Bt over region and time interval
    has_spacetime_bound: bool,  // true when Bt is region-valid; false => fail closed / non-certified path
    spacetime_provenance: BoundProvenance, // tracks how Bt was derived
}

/// Safe spacetime step along a trajectory p(t) = p0 + v*t.
/// Returns how far in TIME the point can advance without crossing a surface.
/// This is the spacetime extension of safe_step_from_lower_bound.
fn safe_step_spacetime_along_path(
    b_lower: f32,
    dfdxyz_bound: [f32; 3],
    b_time: f32,               // Bt: |∂f/∂t| bound over region
    has_spacetime_bound: bool, // fail-closed guard
    velocity: [f32; 3],        // dp/dt for CCD path
    dt_remaining: f32,         // remaining time in tick
    dt_to_bound_region_exit: f32, // time until path leaves region where B/Bt are valid
) -> f32 {
    // Fail closed: no certified Bt => no certified spacetime advance.
    // Caller must route to non-certified collision path (e.g., discrete sampling / legacy CCD).
    if !has_spacetime_bound {
        return 0.0;
    }
    // L_path = Bx|vx| + By|vy| + Bz|vz| + Bt (because dt/dt = 1)
    let l_path = dfdxyz_bound[0] * velocity[0].abs()
               + dfdxyz_bound[1] * velocity[1].abs()
               + dfdxyz_bound[2] * velocity[2].abs()
               + b_time;
    let l_safe = l_path.max(1e-6);
    // Clamp to bound-valid region exit to keep the proof local (same rule as spatial march).
    (b_lower.max(0.0) / l_safe)
        .min(dt_remaining)
        .min(dt_to_bound_region_exit)
}

/// Compute Bt (time-derivative bound) for a rigidly moving object.
/// Given: object-space derivative bounds B_obj, linear velocity v, angular velocity omega,
/// rotation matrix R (world-to-object), and region AABB radius (bounding |x'| in object space).
fn compute_bt_rigid(
    b_obj: [f32; 3],          // object-space derivative bounds
    v_world: [f32; 3],        // linear velocity in world space
    omega_world: [f32; 3],    // angular velocity in world space
    r_transpose: [[f32; 3]; 3], // R^T (world-to-object rotation)
    aabb_radius: f32,          // bound on |x'| in object space over region
) -> f32 {
    // dx'/dt = -R^T v - ω' × x'  (ω' = R^T ω in body frame)
    // Per-component cross product bound: (ω' × x')_i = ω'_{i+1}*x'_{i+2} - ω'_{i+2}*x'_{i+1}
    // => |(ω' × x')_i| ≤ |ω'_{i+1}|*|x'_{i+2}| + |ω'_{i+2}|*|x'_{i+1}|
    // Using per-component AABB extents (tighter than scalar aabb_radius by up to √3×).
    let rt_v = [
        r_transpose[0][0] * v_world[0] + r_transpose[0][1] * v_world[1] + r_transpose[0][2] * v_world[2],
        r_transpose[1][0] * v_world[0] + r_transpose[1][1] * v_world[1] + r_transpose[1][2] * v_world[2],
        r_transpose[2][0] * v_world[0] + r_transpose[2][1] * v_world[1] + r_transpose[2][2] * v_world[2],
    ];
    // ω' = R^T ω (body-frame angular velocity; |ω'| = |ω| under orthogonal R)
    let omega_body = [
        r_transpose[0][0] * omega_world[0] + r_transpose[0][1] * omega_world[1] + r_transpose[0][2] * omega_world[2],
        r_transpose[1][0] * omega_world[0] + r_transpose[1][1] * omega_world[1] + r_transpose[1][2] * omega_world[2],
        r_transpose[2][0] * omega_world[0] + r_transpose[2][1] * omega_world[1] + r_transpose[2][2] * omega_world[2],
    ];
    // Per-component cross product bound using cyclic indices and scalar aabb_radius
    // (For tighter bounds, replace aabb_radius with per-axis AABB half-extents)
    let dxdt_bound = [
        rt_v[0].abs() + omega_body[1].abs() * aabb_radius + omega_body[2].abs() * aabb_radius,
        rt_v[1].abs() + omega_body[2].abs() * aabb_radius + omega_body[0].abs() * aabb_radius,
        rt_v[2].abs() + omega_body[0].abs() * aabb_radius + omega_body[1].abs() * aabb_radius,
    ];
    // |∂f/∂t| ≤ dot(|dx'/dt|, B_obj)
    b_obj[0] * dxdt_bound[0] + b_obj[1] * dxdt_bound[1] + b_obj[2] * dxdt_bound[2]
}

/// Evaluate swept-volume lower bound at a spatial point over shutter interval.
/// Returns the exact minimum over t of the sampled 1D McShane lower envelope.
/// If result > 0, point is certified outside the sweep for all t in [t0, t1].
/// If result <= 0, classification is not automatically "inside": treat as uncertain unless
/// a direct time sample has f(x, t_i) <= 0 (InsideSampled), then refine or fallback.
fn swept_volume_lower_bound(
    time_samples: &[(f32, f32)],  // (t_i, g_i = f(x, t_i)) time samples
    l_time: f32,                   // Bt: Lipschitz constant in time
    t0: f32,                       // shutter start
    t1: f32,                       // shutter end
) -> f32 {
    // Quick inside check: if any sample is negative, point is in the sweep
    for &(_, g_i) in time_samples {
        if g_i <= 0.0 { return g_i; }
    }
    // McShane lower envelope in 1D time:
    //   h(t) = max_i( g_i - l_time * |t - t_i| )
    // h is piecewise linear. Its global minimum on [t0, t1] occurs at:
    //   - interval boundaries (t0, t1), or
    //   - a breakpoint where active tent functions intersect, or
    //   - a tent apex t_i.
    // Build candidate set from boundaries + in-interval t_i + pairwise branch intersections.
    let mut candidates: Vec<f32> = vec![t0, t1];
    for &(t_i, _) in time_samples {
        if t_i >= t0 && t_i <= t1 {
            candidates.push(t_i);
        }
    }
    // Each sample contributes two lines:
    //   left branch (t <= t_i):  y = +l_time * t + (g_i - l_time * t_i)
    //   right branch (t >= t_i): y = -l_time * t + (g_i + l_time * t_i)
    for &(t_i, g_i) in time_samples {
        for &(t_j, g_j) in time_samples {
            // left(i) vs right(j)
            let denom_lr = 2.0 * l_time;
            if denom_lr > 0.0 {
                let t_lr = (g_j + l_time * t_j - (g_i - l_time * t_i)) / denom_lr;
                if t_lr >= t0 && t_lr <= t1 && t_lr <= t_i && t_lr >= t_j {
                    candidates.push(t_lr);
                }
            }
            // right(i) vs left(j)
            let denom_rl = 2.0 * l_time;
            if denom_rl > 0.0 {
                let t_rl = (g_i + l_time * t_i - (g_j - l_time * t_j)) / denom_rl;
                if t_rl >= t0 && t_rl <= t1 && t_rl >= t_i && t_rl <= t_j {
                    candidates.push(t_rl);
                }
            }
        }
    }
    let mut min_val = f32::INFINITY;
    for &t in &candidates {
        let mut h_t = f32::NEG_INFINITY;
        for &(t_i, g_i) in time_samples {
            h_t = f32::max(h_t, g_i - l_time * (t - t_i).abs());
        }
        min_val = f32::min(min_val, h_t);
    }
    min_val
}
```

### 0b. Quality Ladder Profiles
```rust
struct QualityProfile {
    // Ray marching
    ray_march_resolution_scale: f32,  // 0.5 (Low) to 1.0 (Ultra)
    max_ray_steps: u32,               // 48 (Low) to 128 (Ultra)

    // Brick pool
    brick_resolution: u32,            // 8 (Low) to 16 (High)
    max_resident_bricks: u32,         // 1024 (Low) to 4096 (Ultra)
    brick_update_budget_ms: f32,      // 1.0 (Low) to 3.0 (Ultra)
    clipmap_levels: u32,              // 3 (Low) to 5 (Ultra)

    // Shadows
    shadow_mode: ShadowMode,          // RasterMesh (Low) / FieldTrace (High)
    shadow_cascade_count: u32,        // 2 (Low) to 4 (Ultra)

    // Characters
    character_brick_resolution: u32,  // 32 cubed (Low) to 64 cubed (Ultra)
    character_update_frequency: u32,  // every 4 frames (Low) to every frame (Ultra)

    // GI
    gi_mode: GiMode,                  // ProbesOnly (Low) / ProbesPlusConeTrace (Med) / ProbesPlusConeTracePlusPDE (High)
    // Low: SH probe lookup only (nearest-probe interpolation with surface normal)
    // Med: SH probes (always on for propagation) + per-pixel cone tracing through radiance mips
    // High: SH probes + cone tracing + irradiance PDE (Phase 10 screened Poisson, light-corruption coupling)
    // Probes are ALWAYS active — they are the propagation mechanism. GI mode selects the per-pixel sampling method.
    gi_resolution_scale: f32,         // 0.25 (Low) to 0.5 (High)
    sh_probe_update_budget: u32,      // 16 probes/frame (Low) to 64 (High)

    // Spectral (Phase 5+)
    spectral_basis_dimension: u32,    // 3 (Low) to 6 (High)

    // Painterly (Phase 6+)
    painterly_resolution_scale: f32,  // 0.5 (Low) to 1.0 (Ultra)
    ink_wash_sim_steps: u32,          // 2 (Low) to 8 (Ultra)

    // Simulation (Phase 10+)
    sim_update_budget_ms: f32,        // 1.0 (Low) to 3.0 (Ultra)
    sim_clipmap_levels: u32,          // 2 (Low) to 4 (High)

    // NOTE: Later phases (11, 16) will extend this struct with phase-specific fields
    // (e.g. fidelity_track_enabled, wavefront_compaction, reservoir_reuse, sg_lobe_budget,
    // cinematic_accumulation_frames, emotional_render_quality). The struct as written covers
    // Phases 0-10. Each subsequent phase adds its quality-gated fields when it ships.
}
```

Define Low, Medium, High, Ultra presets. AC targets reference a specific profile. "60fps at 1080p" means "60fps at 1080p on Medium profile."

### 0c. GPU Memory Budget Spreadsheet

Target: 512MB total GPU memory budget (fits integrated GPUs).

```
Component               | Low     | Medium  | High    | Notes
------------------------|---------|---------|---------|------
Brick pool (distance)   | 20 MB   | 96 MB   | 160 MB  | Medium profile target; 12-byte logical payload/voxel (with g_mag for Whitney envelope), budget validated against measured physical GPU allocation
Brick pool (fields P10) | 2 MB    | 2 MB    | 4 MB    | Per-brick slow fields (31 bytes/brick excl. SH) + sparse stress (6 bytes x ~5%) + DMD snapshot buffers (~384 KB per active region, allocated from this or uniform/storage budget)
Radiance mip chain      | 2 MB    | 4 MB    | 8 MB    | SH probes (24 bytes/brick) + per-mip-voxel radiance for cone tracing (8 bytes/voxel at mip levels 1-3)
Page tables + metadata  | 2 MB    | 4 MB    | 8 MB    | Clipmap indirection
Dual contour meshes     | 4 MB    | 8 MB    | 16 MB   | Shadow + collision meshes
Character volumes       | 5 MB    | 10 MB   | 20 MB   | 2 chars x brick resolution x 12 bytes; +~0.8 MB (Med) / +6 MB (High) transient during active morphs (12 bytes/voxel displacement field)
Neural field weights    | 1 MB    | 2 MB    | 4 MB    | 2-4 characters x hierarchical
Render targets          | 32 MB   | 48 MB   | 64 MB   | Color + depth + aux at res scale
Post-process buffers    | 16 MB   | 24 MB   | 32 MB   | Bloom chain, outline, painterly
Blue noise textures     | 1 MB    | 1 MB    | 1 MB    | Spatiotemporal tables
Uniform/storage buffers | 4 MB    | 8 MB    | 16 MB   | Materials, instances, params, DMD snapshot buffers (~384 KB per active region)
TOTAL                   | ~89 MB  | ~207 MB | ~333 MB | All within 512 MB ceiling
```

Ultra column omitted: Ultra profiles use High brick resolution (16^3) with 4096 bricks (~160 MB brick pool alone). Ultra budgets are stretch targets validated during profiling, not pre-committed. The 512 MB ceiling applies to Low/Medium/High; Ultra may exceed it on discrete GPUs.

Memory-budget rule: logical payload tables are design intent; acceptance uses measured physical GPU allocations on target WebGPU backends (format/alignment-aware).

### 0d. Determinism Infrastructure
```rust
/// World seed -> deterministic sub-seeds for every system.
/// Record seed + frame + inputs -> exact reproduction of any bug frame.
struct DeterminismState {
    world_seed: u64,
    sim_tick: u64,      // authoritative fixed-tick simulation clock
    render_frame: u64,  // client-local presentation frame counter
}

impl DeterminismState {
    fn system_seed(&self, system_id: u32) -> u64 {
        hash(self.world_seed, self.sim_tick, system_id)
    }
    fn pixel_seed(&self, system_id: u32, x: u32, y: u32) -> u64 {
        hash(self.world_seed, self.render_frame, system_id, x, y)
    }
}
```

Determinism is split into two contracts:
- **Authoritative simulation determinism (MMO-critical):**
  - Server-authoritative fixed tick.
  - Canonical update order (stable entity/region iteration order).
  - Deterministic RNG keyed by `(world_seed, sim_tick, region_id, system_id, entity_id)`.
  - Gameplay-critical state uses deterministic numeric representation (integer/fixed-point or strictly constrained float subset with quantization at sync boundaries).
  - Per-region state hash emitted each tick for divergence detection/replay validation.
- **Rendering determinism (CI/debug only):**
  - Client presentation path may vary across hardware; this does not affect authoritative gameplay state.
  - Pixel-level reproducibility targets apply only on identical GPU/driver/browser builds.

Two rendering modes from day one:
- **Deterministic mode** (CI, debugging): fixed seeds, fixed timestep, seed-locked blue noise, temporal accumulation disabled or seed-driven. Playwright tests use perceptual thresholds (SSIM > 0.95), not pixel equality.
- **Expressive mode** (gameplay): full stochasticity, temporal accumulation, evolving noise.

### 0e. Spatiotemporal Blue Noise Service

All stochastic decisions draw from a single blue noise source:
- Precomputed 128x128x64 spatiotemporal blue noise texture (Georgiev & Fajardo 2016)
- Per-pixel, per-frame, per-system index into the table
- Used for: ray march jitter, brush stroke offset, ink wash sampling, paper grain, dithering, AO sampling, cone trace jitter, PDE noise injection
- In deterministic mode: freeze at frame 0

Why blue noise over white: blue noise distributes error perceptually uniformly. White noise clusters, producing visible patterns. The difference is "alive" vs "flickery." Critical for painterly rendering in Phase 6.

### 0f. Debug View System

Built as GPU-side overlay, toggled at runtime. These are survival infrastructure, not optional aids:

- **Step count heatmap**: color-coded ray march iterations per pixel (blue=few, red=many)
- **Bound heatmap**: local scalar fallback `L` and anisotropic bound magnitude visualization
- **Error epsilon visualization**: where sampling error is largest
- **Brick residency**: which bricks are populated, clipmap level, dirty status
- **Field channel overlays**: any scalar field as false-color overlay
- **Surface ID**: which field graph node generated nearest surface
- **Normal stability**: componentwise derivative-to-bound ratio
  `r = max(|df/dx|/Bx, |df/dy|/By, |df/dz|/Bz)` (should stay <= 1.0; >1 indicates under-estimated bounds)
- **Curvature overlay**: mean curvature as false color (blue=convex, red=concave)
- **Envelope slack**: `slack(p) = b_env(p) - b_fallback(p)` where `b_fallback = trilinear(d) - epsilon` (diagnostic heuristic only, never authoritative march input). Visualized as heatmap: green = envelope is tighter (positive slack = free step distance gained), gray = equivalent, red = fallback is tighter or conventions are mismatched. Persistent large red regions indicate bound/convention bugs worth investigation.
- **Uncertainty interval width**: `u_env(p) - b_env(p)` as heatmap (requires upper-bound channel enabled). Narrow (blue) = confident field value, wide (red) = uncertain. Surfaces live where the interval crosses zero; wide intervals near zero indicate regions that need higher-resolution bricks or re-distance passes.
- **B provenance overlay**: per-brick color-coded provenance bitfield. Green = Analytic (fully trusted), Cyan = AnalyticBernstein (Bernstein-certified, guaranteed convex-hull bounds), Blue = Sampled (empirically validated), Yellow = Inflated (conservative, safe but potentially loose), Red = Unknown (fallen back to scalar L — investigate). Persistent red bricks indicate missing or unimplemented analytic bounds for a warp/operator. This is the primary tool for catching "local B treated as cell B" bugs.
- **FBM octave utilization**: per-pixel heatmap of `octaves_evaluated / num_octaves` from frequency-bounded FBM. Blue = early exit (most voxels far from surface), red = all octaves evaluated (near surface or threshold). This shows where Bernstein early exit is saving work. Large uniformly-red regions far from surfaces indicate the certified tail bounds are too loose.
- **Budget marching traversal count**: per-pixel heatmap of bricks traversed per budget march iteration. Blue = many bricks skipped (budget marching working well), gray = 0-1 bricks (near surface, budget marching not applicable), red = stopped by Unknown provenance (investigate missing bounds). Shows where the empty-space acceleration is most effective.
- **Budget marching field eval savings**: per-pixel ratio of `actual_field_evals / hypothetical_field_evals_without_budget_march`. Blue = large savings, gray = no difference. Primary tool for validating budget marching ROI.

**Cross-phase benchmark corpus IDs (defined in Phase 0, reused by later AC):**
- `bench_forest_combat_path_A` (fast camera pans + melee clashes)
- `bench_many_lights_path_B` (16/32/64/128 dynamic lights)
- `bench_hero_closeup_path_C` (faces, hair, thin outlines)

**Parallel lanes:**
- Lane A: Lipschitz algebra library + composition rules + B provenance tracking (independent)
- Lane B: Quality ladder profiles + budget spreadsheet (independent)
- Lane C: Determinism infrastructure + seed system (independent)
- Lane D: Blue noise service + texture generation (independent)
- Lane E: Debug view shader system + B provenance overlay (independent)
- Lane F: Spacetime stepping contracts + `FieldSample4` + `compute_bt_rigid` + `swept_volume_lower_bound` (depends on A)
- Lane G: f16 conservative conversion utility (independent)
- Lane G2: LCP dual-envelope + fused stepping + mixed cone-union (depends on A)
- Lane G3: Bernstein noise certification + frequency-bounded FBM evaluation (depends on A)
- Lane G4: Budget marching traversal + brick metadata contract (depends on A, G)
- Lane H: Integration + verify all contracts defined (depends on all)

**AC:**
- Canonical step helpers exist: `safe_step_from_lower_bound(...)` (primary) and `safe_step_from_sample(...)` (compatibility)
- All Lipschitz composition rules implemented with unit tests
- Cone-union safe step function implemented with unit tests (verified: `step >= step_std - tol_step`, `step <= cell_or_region_exit`, and dense-sampled no-surface-crossing invariant holds; `step_std = min(max(0,b_env)/L_safe, cell_or_region_exit)` and `tol_step` accounts for conservative pullback pad)
- B provenance tracking: all composition rules propagate provenance; `Unknown` provenance forces scalar L fallback in stepping kernel
- f16 conservative conversion utility: single code path for all bound serialization, direction-aware (upper rounds up, lower rounds down), no direct f32-to-f16 casts on bound values
- f16 conservative conversion utility: explicitly defined behavior for NaN/Inf/+0/-0/subnormals, with deterministic cross-target tests on edge-case corpus
- Spacetime stepping helpers: `safe_step_spacetime_along_path()` and `compute_bt_rigid()` implemented with unit tests
  (includes invariant test: returned `Δt` is clamped by `dt_to_bound_region_exit`)
- Spacetime fail-closed invariant: if `has_spacetime_bound=false`, helper returns `Δt=0` and caller routes to non-certified collision path
- Swept-volume lower bound evaluator: `swept_volume_lower_bound()` computes exact min of sampled 1D McShane time-envelope (boundaries + apexes + branch intersections), tested on known swept geometries
- Quality profiles defined (Low/Med/High/Ultra) with concrete numbers
- GPU memory budget spreadsheet filled and verified against WebGPU limits
- Authoritative simulation replay harness: identical region hashes across runs and across supported server OS builds for a fixed 10k-tick corpus
- Determinism infrastructure defined: seed system, replay harness, mode toggle. Phase 0 exit requires harness/protocol readiness; a separate cross-phase acceptance hook runs full identical-frame verification at first renderer integration milestone (Phase 2a) against debug view output
- Blue noise texture generated and loadable
- LCP dual-envelope: `dual_envelope_lower_bound()` returns `>= lipschitz_envelope()` for all test cases (fusion never regresses — guaranteed by padding each certificate independently before fusing via max)
- LCP fused stepping: `fused_directional_bound()` returns `<= L_dir_L1` always (fused denominator never exceeds L1-only)
- LCP fail-closed behavior: non-finite/non-positive or unknown-provenance scalar certificate `L` forces L1-only path (no L2 terms used)
- LCP mixed cone-union: `mixed_cone_union_safe_step()` returns `>= cone_union_safe_step()` for all test cases (L2 spheres only extend the safe component)
- LCP quality ladder profiles defined: Low (L1-only), Medium (fused dual-envelope), High (fused + mixed cone-union)
- Bernstein noise certification: `certify_noise_patch()` produces tight range and derivative bounds verified against dense sampling (certified bounds are never violated)
- Bernstein derivative bounds are tighter than inflated fallback bounds on reference noise cells (quantified: average tightening ratio > 1.5x on test corpus)
- Frequency-bounded FBM: `fbm_frequency_bounded()` early-exits on > 60% of voxels in a benchmark scene with 6-octave FBM displacement
- FBM octave utilization debug view functional
- Budget marching: `budget_march_traverse()` implemented with property tests (dense-sampled no-surface-crossing invariant holds along full traversal; `t_advance >= t_std` for single-brick case; Unknown provenance bricks halt traversal cleanly)
- Budget marching: f32 ULP padding applied to both initial budget and per-segment costs (verified: padded result is strictly <= unpadded result for lower bounds)
- Debug view shaders compile and render
- cargo check and cargo test pass

---

## Phase 1: Anime Style Shell (Gate Prep)

**Purpose:** Hard cut from PBR to anime visual language with the minimum viable surface area. This phase exists only to provide stable style outputs and interfaces for Phase 2a.

**What's built:**
- Cel shader with toon ramp lookup (replaces Cook-Torrance BRDF)
- Screen-space anime outline pass (Sobel on depth + normals)
- Curated anime color palette system (flat colors with subtle noise)
- Post-processing tuned for anime (soft bloom, bright exposure)
- Resonance VFX adapted for anime (outline glow, palette shifts)
- Shadow cascade wiring that accepts ray-march depth/normal inputs from Phase 2a/2b
- Deterministic visual test harness for style stack regression (Playwright + SSIM)

**Explicitly NOT built in Phase 1:**
- Procedural triangle-mesh environment generators (trees/rocks/vegetation)
- Triangle-mesh character production path
- Keyframed combat clip production

**Parallel lanes:** 3 independent (cel shader, outlines, palette/post) -> 1 dependent (shadows + VFX wiring) -> integration -> review

**AC:**
- Style shell renders correctly on a minimal proxy scene and against a Phase 2a-compatible field I/O contract stub
- No PBR artifacts in any style-shell render path
- Deterministic visual verification passes (identical GPU/driver/browser build, SSIM > 0.95)
- Debug views remain wired and readable through the style stack
- Phase 2a integration preconditions met (cel/outline/palette/post/shadow interfaces stable)

**Detailed plan:** docs/plans/YYYY-MM-DD-phase-1-anime-style-shell.md (generate before execution)

---

## Phase 2: Bounded Implicit Field Engine

**Purpose:** Replace triangle mesh environment with bounded implicit field ray marching. This phase builds the core data structure that every subsequent phase extends.

### Phase 2a: Falsification Spike (GO/NO-GO gate)

**Purpose:** Answer the only question that matters before committing to Phases 3-16: can compute ray marching with the envelope + cone-union stack hit interactive framerates on WebGPU?

**What's built (minimal viable field renderer):**
- Field primitive library: sphere, box, plane only. R-function union/subtract. Each returns FieldSample with B=(1,1,1), L=1.0, epsilon=0.0.
- Tiny brick pool: 256 bricks, single clipmap level (1m voxel size), 8^3 voxels per brick.
- Envelope reconstruction: `b_env(p)` over 8-corner stencil with `B_cell`.
- Cone-union acceleration: `cone_union_safe_step()` with activation gate.
- Compute ray march: one thread per pixel, full-resolution. `safe_step` + cone-union + cell-exit clamp.
- Min-reduction mips (fallback): 2 mip levels per brick.
- Simple scene: 5 R-function trees (cylinders + spheres), 8 rocks (noise-displaced spheres with Bernstein-certified noise patches), flat ground plane. All within a 20x20m area.
- Step count heatmap debug view.
- Wire into Phase 1's cel shader + outline pass for visual output.
- No GI, no edits, no dual contouring, no re-distance. Just field → bricks → march → shade.
- Budget marching intentionally disabled in the 2a gate build; it is integrated in 2b to keep the 2a hypothesis single-variable (envelope + cone-union + compute march viability).

**Parallel lanes:**
- Lane A: Field primitives + R-functions + bound tracking (independent)
- Lane B: Brick pool + page table (independent)
- Lane C: Envelope reconstruction + cone-union step + LCP fused stepping (depends on A, Phase 0 Lane G2)
- Lane C2: Bernstein noise certification for rock displacement (depends on A, Phase 0 Lane G3)
- Lane D: Compute ray march + debug views (depends on B, C)
- Lane E: Scene generators + integration with Phase 1 style shell (depends on A, C2, D)
- Lane F: Performance measurement + GO/NO-GO evaluation (depends on E)

**Measurements (Lane F):**
Capture on target hardware (integrated GPU, Chrome, 1080p):
1. **Average steps per ray** (should be < 40 for Medium profile max_ray_steps=64)
2. **p95 steps per ray** (should be < 64 — if p95 hits the cap, divergence is killing you)
3. **Frame time** (target: < 16.6ms for 60fps)
4. **Divergence proxy metrics** (step histogram + per-tile timing variance via WebGPU timestamps; occupancy inferred, not directly measured)
5. **Step count with/without cone-union gate** (quantify the acceleration)
6. **Step count with/without LCP fused stepping** (quantify diagonal tax elimination)
7. **Step count heatmap** (identify where rays are crawling)

**GO/NO-GO decision:**
```
GO (proceed to Phase 2b):
  - 60fps at 1080p on integrated GPU (or close enough that GI/edit overhead fits in headroom)
  - Average steps < 40, p95 < 64
  - Cone-union provides measurable step reduction (>15% in pessimistic zones)

CONDITIONAL GO (proceed with mitigation):
  - 30-60fps: Pull minimal wavefront compaction from Phase 16 into Phase 2b
    (sorted ray buffer + prefix sum + indirect dispatch, ~2 additional lanes)
  - p95 hitting cap but average is fine: divergence problem, compaction is the fix

NO-GO (architectural reconsideration):
  - <30fps: Compute ray marching on WebGPU is not viable at this scene complexity
    without hardware ray tracing. Options:
    a) Reduce to field-first fallback: lower ray-march resolution scale + tighter step budgets + reconstruction/upscaling
    b) Fragment shader marching with reduced step budget (accept quality loss)
    c) Wait for WebGPU ray tracing extensions
  - This is the scenario where you want to know NOW, not after building
    neural field training and DMD sleep pipelines
```

**AC:**
- Simple scene renders via compute ray march from brick pool
- Envelope reconstruction produces tighter bounds than epsilon fallback (debug slack view: green)
- Cone-union gate activates and provides measurable step reduction
- Step count heatmap functional
- All 7 measurements captured and documented
- GO/NO-GO decision made with data

---

### Phase 2b: Full Field Engine (after GO decision)

**Purpose:** Complete the bounded implicit field engine with all production subsystems.

**Critical architectural decisions (changed from v1):**

1. **Compute shader ray marching, not fragment shader.** Fragment shaders have severe thread divergence — different pixels take different step counts, and the GPU executes in lockstep warps of 32-64 threads. A warp finishes when its LAST thread finishes, wasting 50-90% of GPU cycles. Compute ray marching writes to a screen-sized UAV and supports coherence control. In Phase 2b, ship lightweight coherence mitigation early (tile/brick-key ray binning and gated mini-compaction), with full queue-based wavefront compaction as an escalation path when divergence exceeds budget.

2. **Conservative mip hierarchy + region-valid bounds, not averaged mips.** Preferred mip construction is envelope-based Lipschitz closure (decimate + separable max-plus tightening). Min-reduction of child lower bounds is an allowed bringup fallback only. Store canonical lower bounds (`b`) per mip voxel plus derivative bounds valid over that mip cell. March with direction-aware denominator `L_dir(v)=dot(abs(v),B)` (floored via `L_safe`) and clamp to `distance_to_active_region_exit` (cell exit for in-cell stencil, region exit for expanded stencil). This guarantees cone stepping never overshoots thin features. Average mips WILL produce holes at grazing angles.

3. **Layered composition, not delta distance grid.** Persistent world edits are NOT additive deltas on distance (which produces sign garbage and non-physical bulges). They're composed during brick population:
   - **Base layer:** procedural generators (deterministic from world seed + brick coordinate)
   - **Edit layer:** list of CSG edit primitives per brick (spatially indexed). Each edit is an implicit primitive (sphere subtraction for craters, box subtraction for slashes, capsule addition for growth). During brick bake: f = compose(base, edit1, edit2, ...)
   - **Sim layer:** PDE field channels at their own (lower) resolution
   - Healing = fade out an edit primitive's influence over time. Clean, no sign weirdness.

4. **Stored gradient + curvature in bricks.** Logical payload per voxel: d (f32) + n ([f16; 2] octahedral normal) + g_mag (f16 gradient magnitude) + H (f16 mean curvature for shading) = 12 bytes/voxel. The g_mag channel is required by the Whitney quadratic envelope to reconstruct the actual gradient (g_mag * n); without it, using unit normals when |∇f| < 1 breaks conservatism. Eliminates finite-difference sampling at render time. Physical GPU stride may be 12-16 bytes depending on texture/buffer format alignment; budgets must use physical stride, not payload size. Per-cell (not per-voxel): K_cell (f16, Hessian operator-norm bound for the Whitney-style quadratic envelope). K_cell is stored once per mip cell in brick metadata, not per voxel — it's a region property. **Note:** The per-voxel `H` is mean curvature (trace of shape operator) used ONLY for shading (moss, erosion, AO, outline width). The per-cell `K_cell` is the Hessian operator-norm bound used ONLY for quadratic-envelope safety. These are different quantities — do not conflate them.

5. **Per-brick SH irradiance probes for GI (Tier 1, always on).** L1 spherical harmonics (4 coefficients x 3 RGB channels = 12 coefficients, packed as f16 = 24 bytes) stored per BRICK, not per voxel. Updated incrementally: each frame, update a rotating subset via short cone traces through the mip hierarchy. At shading time: sample nearest probes, compute SH with surface normal. One dot product.

   **The math (Ramamoorthi & Hanrahan 2001):** Diffuse reflectance acts as a cosine-lobe convolution on incident radiance. A cosine lobe is >99% captured by L2 SH (9 coefficients). We use L1 (4 coefficients) which captures ~92% of the cosine lobe energy — the missing ~9% is high-frequency detail that vanishes after quantization into 2-3 cel-shading bands. For our use case (indirect shadow hue shift, not photorealistic irradiance), L1 is sufficient.

   **GI cone tracing (Tier 2, Med+ quality):** 6-9 wide cones traced from each shading point through the radiance mip hierarchy. Each cone samples progressively coarser mips as it widens. The accumulated radiance follows standard volume rendering compositing:
   ```
   alpha_step = 1 - exp(-sigma_step * delta_t)
   L_accumulated += (1 - alpha_accumulated) * alpha_step * L_step
   alpha_accumulated += (1 - alpha_accumulated) * alpha_step
   ```
   where sigma_step derives from the mip's opacity (small |d| = high opacity, large |d| = transparent). Run at half resolution, upsample with depth-aware bilateral filter.

   **How GI integrates with anime cel shading:** Indirect light doesn't add luminance — it shifts shadow HUE:
   shadow_color = base_shadow + indirect_irradiance * bleed_factor
   A red wall tints nearby shadows pink. Green canopy tints ground shadows cool green. The indirect contribution passes through the toon ramp like everything else — stays quantized, stays anime.

6. **Narrow-band re-distance maintenance pass (first-class performance feature, not cleanup).** After edits or PDE deformation, the field near modified surfaces drifts from distance-like behavior, causing `B`/`L` to inflate and step sizes to collapse. Re-distance is the primary mechanism for preventing bound blow-up — without it, march step counts degenerate toward `max_ray_steps` and performance collapses. Run a localized approximate Eikonal solve (|grad(d)| = 1) via fast sweeping within the modified brick neighborhood. Stabilizes normals, curvature, and stepping. Scheduled by dirty flags AND bound-health metrics under a fixed per-frame ms budget. **Treat re-distance as part of the performance budget, not an optional cleanup pass.**

   **Re-distance safety invariants (treat like garbage collection — must not destroy semantics):**
   - **Narrow band only:** Re-distance operates within a bounded narrow band around the zero set (typically +/- 2-3 voxels). It MUST NOT modify voxels outside this band.
   - **Zero-set drift tolerance:** Re-distance MUST NOT move the zero set (isosurface) by more than `tau_drift = min(voxel_size * 0.25, u_env - b_env)` at any point. If the envelope interval `[b_env, u_env]` is available, use it as the tolerance bound. If the re-distance pass would drift the surface farther, clamp.
   - **d_est vs b/u separation:** Re-distance modifies `d_est` (the distance estimate stored per voxel) only. The conservative bounds `b` (lower) and `u` (upper) are tightened by re-distance but NEVER invalidated. Specifically: after re-distance, `b_new <= d_est_new` must still hold, and `u_new >= d_est_new` must still hold. Re-distance may tighten bounds (because the re-distanced field is closer to a true SDF), but it cannot widen them.
   - **Why this matters:** Without these invariants, re-distance can drift the surface past contact/shading tolerances, producing "physics says contact, rendering says no" desync in the combat layer. The separation of d_est from b/u ensures that rendering (which uses b for stepping) and physics (which uses d_est for contact) stay in agreement within the tolerance.
   - **B bound propagation after re-distance/edits:** When re-distance or an edit modifies a brick's field values and the recomputed `B_new` for that brick is LARGER than the previously stored `B_old` (looser bounds), neighboring bricks within `B_max * voxel_size` distance must be flagged dirty for B recomputation. If `B_new <= B_old` (tighter bounds), neighbors remain valid — conservative bounds stay conservative. This propagation rule prevents stale under-conservative B values in neighboring bricks from reaching the stepping hot path. Implementation: after any brick's B recomputation, if `any(B_new[i] > B_old[i])`, enqueue adjacent bricks (6-connected or 26-connected depending on stencil) for B revalidation with normal priority.

**What's built:**

- **Field primitive library:**
  - Primitives: sphere, box, cylinder, torus, cone, plane, capsule, rounded box
  - Each primitive returns FieldSample with (distance, B=(1,1,1), has_anisotropic_bound=true, L=1.0 fallback, epsilon=0.0)
  - R-function CSG operators: r_union, r_subtract, r_intersect
    - R-functions (Rvachev-style): Algebraic CSG with smooth transitions through boolean boundaries. Use a regularized form in production (e.g., r_union_eps(a, b) = a + b - sqrt(a^2 + b^2 + eps_r^2), eps_r > 0) to avoid singular behavior at the a=b=0 locus. Unregularized R0 is non-C1 there and is debug-only. Each operator outputs updated bounds; scalar bound uses the tight analytical factor `L <= (2+sqrt(2)) * max(L_f, L_g)` with anisotropic component bounds propagated in parallel.
  - Domain operators: twist, bend, repetition, FBM displacement — each outputs updated scalar L and anisotropic B via region-bounded absolute Jacobian propagation (`B_out = A^T * B_in`)
  - **Warp-bound tightening is the primary performance bottleneck.** Deriving region-valid Jacobian bounds for warps without inflating them into uselessness is the hardest practical problem in the engine. Twist with rate k over a brick of radius r produces `L(w) ~ k * r`, which can be enormous for large bricks far from the twist axis. The result: step sizes collapse and march iteration counts explode. Warp-bound tightening (smaller bricks near warp-heavy regions, Bernstein certification of polynomial warps where applicable, adaptive brick subdivision driven by bound health metrics) must be treated as a **first-class performance feature** from Phase 2b onward, not a Phase 16 polish item. The re-distance pass helps (restoring near-SDF behavior tightens bounds), but it cannot fix fundamental bound inflation from extreme warps — only smaller evaluation regions can.
  - **Recommended Phase 2b warp-bound strategy:** (1) Default: sampled worst-case Jacobian over region grid points (`Sampled` provenance). (2) For polynomial warps (twist, bend with polynomial profiles): Bernstein certification of the Jacobian entries over the brick AABB. (3) Adaptive brick subdivision: when a brick's `B_max` exceeds a threshold relative to its size (bound-health metric), split into 8 sub-bricks for tighter region-valid bounds. This is the primary quality/performance tradeoff for warp-heavy scenes.
  - FBM noise bound (general, conservative fallback): with gain g, lacunarity lambda, base frequency omega, base-noise derivative bound L_noise_base:
    `L_noise <= A * omega * L_noise_base * sum_{i=0}^{N-1}(g*lambda)^i`
    (closed form geometric series; falls back to `* N` only for `g*lambda == 1`)
  - **FBM noise bound (Bernstein-certified, preferred):** During brick population, compute `CertifiedNoisePatch` for each octave's lattice cell via Bernstein conversion (see Phase 0a). Use `fbm_frequency_bounded()` for:
    - **Tighter B per cell:** Bernstein derivative bounds are tight (convex hull), not inflated by worst-case-over-all-cells. A cell near a noise extremum has near-zero derivative → tighter B → larger steps.
    - **Frequency-bounded early exit:** Low octaves prove "definitely outside" for most voxels; higher octaves skip entirely. Primary performance lever for noise-heavy scenes.
    - **Provenance: `AnalyticBernstein`** — region-valid by construction, no sampling or inflation.
    - Cost: ~1225 Bernstein coefficients per noise cell per octave for gradient Perlin (343 value + 882 derivatives), computed once during brick bake (amortized). ~756 for value noise (216 + 540). Ray march inner loop uses cached per-brick B from certified patches.

- **Sparse brick pool + clipmap architecture:**
  - Camera-centered clipmap: 4 levels (0.25m, 0.5m, 1m, 2m voxel size)
  - Each level: sparse page table (level, brick_coord) -> brick_pool_slot
  - Brick pool: fixed-size GPU buffer, LRU eviction
  - Per brick: 8^3 or 16^3 voxels x 12 bytes/voxel = 6-48 KB/brick
  - Compute shader population: field graph into bricks, amortized over frames
  - Budget: brick_update_budget_ms from quality profile
  - Per-brick metadata: B_max (f16x3, CPU-side round-toward-+inf; see Phase 10 f16 rounding note), L_max fallback, dirty flag, edit list pointer

- **Conservative mip hierarchy:**
  - Per brick: 3-4 mip levels (8^3 -> 4^3 -> 2^3 -> 1^3)
  - Each mip voxel stores b (canonical lower bound) and B_cell (region-valid derivative bound for that mip cell)
  - b storage invariant: quantized/stored b values must remain conservative lower bounds (`b_i <= f(x_i)`). For fp16 storage, use explicit downward rounding or subtract 1 ULP after conversion.
  - Optional upper-bound channel for interval-driven features: if uncertainty/interval scheduling is enabled, maintain `u_i` samples with invariant `u_i >= f(x_i)` (upward rounding or +1 ULP). If omitted, set `u_env = +inf` and disable interval-width-driven scheduling/debug paths.
  - PREFERRED construction: envelope-based Lipschitz closure via `separable_lipschitz_closure_1d` (see Phase 0a). Decimate fine-level samples to coarse lattice, then tighten with 6 linear passes (2 per axis). Produces provably tight lower bounds for the retained coarse-lattice constraints; thin features only pessimize their local B-weighted neighborhood, not entire parent cells.
  - FALLBACK construction: b_mip = min(children_b). Safe but aggressively pessimistic. Use during initial bringup before envelope pipeline is validated.
  - At query time: `b_env(p)` envelope reconstruction over mip stencil
    - in-cell 8-corner stencil: use `B_cell` (default authoritative path)
    - expanded 27-sample stencil: require `B_region` valid over full stencil support; clamp to `distance_to_region_exit`
    - if expanded-bound precondition fails, fall back to in-cell stencil
  - Used for cone stepping: at distance t, use mip where voxel size ~ pixel footprint

- **Compute shader ray march:**
  - Dispatch: one thread per pixel (at ray_march_resolution_scale)
  - Per step: look up brick from page table -> reconstruct `b_env(p)` via envelope over cell stencil (preferred)
    - choose active bound/region pair from stencil scope: `B_active = B_cell` with `distance_to_active_region_exit = distance_to_cell_exit` for in-cell stencil, or `B_active = B_region` with `distance_to_active_region_exit = distance_to_region_exit` for expanded stencil
    - fallback (when envelope stencil unavailable): use conservative sampled lower bound `b = d_center - epsilon_center` (center-bound convention), NOT trilinear interpolation in authoritative march path
    -> apply `safe_step_from_lower_bound(b, B_active, has_B_active, L_fallback, ray_dir, distance_to_active_region_exit)`
  - **LCP fused stepping (Medium+ quality):** When L2 certificate is available (provenance != Unknown), use `dual_envelope_lower_bound()` for tighter `b_fused` and `fused_directional_bound()` for tighter `L_dir*`. This eliminates the ~1.73x diagonal tax in SDF-like regions where L2 is dramatically tighter than L1. On Low quality, use L1-only stepping (zero additional cost).
  - **Cone-union acceleration (with LCP Gate 3):** compute standard conservative step `step_std = min(max(0,b_env)/L_safe, distance_to_active_region_exit)` first. Gate 1: if `step_std >= 0.8 * distance_to_active_region_exit`, skip cone-union. Gate 2: if `median_b < 0.05 * L_safe * distance_to_active_region_exit`, skip (pathological B). Gate 3 (High quality, requires LCP): compare `b_fused - b_l1` slack to determine L1-only vs mixed L1+L2 cone-union. On High quality with significant envelope slack, run `mixed_cone_union_safe_step()` (union of L1 octahedra AND L2 spheres). On Medium or low slack, cheaper fused stepping captures most of the benefit. Cone-union must use the same bound validity scope as envelope reconstruction.
  - **Budget marching (empty-space acceleration):** Before each expensive field eval, attempt `budget_march_traverse()` from the current position using the just-computed `b_lower` as budget. If budget marching advances past the current brick, skip the per-brick envelope reconstruction for all traversed bricks. This converts the O(N_bricks) field-eval cost of traversing empty space into O(N_bricks) cheap metadata reads + ONE field eval at the end. Budget marching and cone-union are complementary: budget marching skips across bricks, cone-union extends within a brick's stencil. In the outer march loop, budget marching fires first (macro skip), then if the ray is near-surface, cone-union fires (micro extend).
  - **Early coherence mitigation (2b baseline):** Every `N_coherence` march iterations (default 2 on Medium), bin active rays by coarse brick/page key (tile-local radix/binning) before next iteration to increase wave coherence. This is cheaper than full queue compaction and reduces worst-case branch skew.
  - **Divergence budget guardrail:** Maintain `ray_march_branch_budget_ms` (Medium default 3.0ms at 1080p) for advanced branch stack (`LCP Gate 3` + cone-union + budget marching + coherence/binning). If exceeded, degrade in order within the same frame: (1) disable Gate 3 mixed cone-union, (2) reduce coherence frequency, (3) disable budget marching, (4) fall back to L1-only step path for remaining rays in over-budget tiles.
  - **Escalation path:** If `DivergenceProxy` remains above threshold for `M` consecutive frames despite guardrail degradation, enable minimal queue compaction lanes (prefix-sum + indirect dispatch) in 2b rather than waiting for Phase 16.
    - Medium defaults: escalate when `DivergenceProxy > 0.35` for `M = 12` consecutive frames and `active_rays_peak > 131072`.
    - De-escalate mini-compaction when `DivergenceProxy < 0.20` for 24 consecutive frames or `active_rays_peak < 32768`.
    - Profile policy: High/Cinematic start from the same thresholds and require benchmark-backed calibration before profile-specific overrides are allowed.
  - **Policy unification:** Phase 2b uses raw `DivergenceProxy` + consecutive-frame hysteresis for fast response; Phase 16 uses EWMA for stability. Both are modes of one `DivergenceController` with shared thresholds (`0.35/0.20`) and activity gates (`131072/32768`) to prevent control-law drift across phases.
  - Early termination: distance < threshold (hit), steps > max (miss), left clipmap (sky)
  - Output: position, normal, depth, material_id, curvature, step count
  - Blue noise jitter on initial ray offset

- **GI system (Tier 1: SH probes):**
  - Per-brick: L1 SH coefficients ([f16; 12] = 24 bytes)
  - Update pass: each frame, select N bricks for probe update
  - Per update: trace 8-16 short cones into mip hierarchy, accumulate radiance
  - Propagation: neighbors exchange energy -> multi-bounce convergence over frames
  - At shading: trilinear interpolation of nearest 8 probes -> SH with normal -> indirect color
  - Feed indirect into cel shader as shadow hue shift

- **GI system (Tier 2: Voxel Cone Tracing, Med+ quality):**
  - Radiance mip chain: per coarse mip, store (radiance_rgb, opacity)
  - 1 specular cone + 5-8 diffuse cones per pixel
  - Each cone: march through radiance mip, sample coarser mips as cone widens
  - Standard front-to-back alpha compositing
  - Run at gi_resolution_scale and upsample

- **Dual contouring mesh extraction:**
  - Triangle meshes from brick pool for shadow cascade rasterization and broad collision
  - Hermite data is reconstructed from sampled brick field (interpolated zero-crossings + gradients), not analytic primitive intersections
  - Compute shader output to vertex/index buffers
  - Re-extract only dirty bricks
  - Authoritative narrow-phase contacts remain field queries (distance + gradient), not dual-contour triangle contacts

- **Layered edit composition:**
  - Per-brick edit list: Vec<EditPrimitive> spatially indexed
  - EditPrimitive: type (subtract/add), shape, transform, blend radius, age
  - During population: f = base_field(p); for edit in edits: f = compose(f, edit)
  - Healing: edit age increments -> influence fades -> edit removed when expired
  - L bound updated during composition

- **Re-distance maintenance pass:**
  - Triggered on dirty bricks AND distance-likeness degradation metrics
  - 3 iterations of fast sweeping within brick +/- 1-brick neighborhood
  - Recomputes stored gradient and curvature
  - Tightens `B`/`L` bounds and restores distance-likeness where it degraded
  - Scheduling metric: histogram of `r = max(|df/dx|/Bx, |df/dy|/By, |df/dz|/Bz)` (or scalar fallback `|grad(f)|/L`) per brick; prioritize bricks with drift outside configured band
  - Trigger thresholds (initial defaults):
    - Safety trigger: if any sampled `r > 1.02`, enqueue immediate high-priority re-distance for that brick neighborhood
    - Performance trigger: if median `r < 0.35` and associated screen-tile `p95_steps > 0.8 * max_ray_steps`, enqueue opportunistic re-distance for tighter bounds
    - Cooldown: a brick cannot be re-distanced more than once every 8 frames unless safety trigger fires

- **Environment field generators:**
  - Tree: recursive branching as R-function-unioned cylinders + spheres
  - Rock: noise-displaced sphere with FBM
  - Terrain: heightfield as clamped planar field + erosion
  - Vegetation: grass as thin capsule fields, ferns as curved planes

- **Renderer baseline:** Field environment renderer is authoritative in Phase 2b.
- **Temporary proxy character path (owned in Phase 2b):** Minimal proxy/debug character path exists only for integration visibility (silhouette + shadow + hit proxy). It is explicitly non-gameplay-authoritative and removed in Phase 3.
- **Motor core handoff for Phase 3:** Ship a shared `motor_core` module (apply/compose/inverse/normalize + dual-quat packing rules) in Phase 2b. Phase 3 consumes this exact module for canonicalization; Phase 4a expands it to global transform cutover and interpolation/log-exp everywhere.

**Parallel lanes:**
- Lane A: Field primitive library + R-functions + Lipschitz/anisotropic-bound tracking (independent)
- Lane B: Sparse brick pool + clipmap architecture + page table (independent)
- Lane C: Conservative mip hierarchy + `b/B` mip generation (depends on B)
- Lane D: Compute shader ray march + conservative stepping (depends on B, C)
- Lane E: Stored gradient/curvature in bricks + population compute (depends on A, B)
- Lane F: GI Tier 1 — per-brick SH probes + update pass (depends on B, E)
- Lane G: GI Tier 2 — radiance mip chain + cone tracing (depends on C, D, F)
- Lane H: Dual contouring mesh extraction (depends on B, E)
- Lane I: Layered edit composition + CSG edit primitives (depends on A, B)
- Lane J: Re-distance maintenance pass (depends on A, B, E)
- Lane K: Environment field generators + Bernstein-certified noise patches during brick bake (depends on A)
- Lane K2: LCP integration into compute march — dual-envelope + fused stepping + Gate 3 (depends on D, Phase 0 Lane G2)
- Lane K3: Budget marching integration into outer march loop (depends on B, D, Phase 0 Lane G4)
- Lane L: Renderer integration + cel/outline/shadow/GI wiring (depends on D, F or G, H, K)
- Lane L2: Proxy/debug character path (integration-only; depends on L)
- Lane L3: `motor_core` module extraction + API freeze for Phase 3 reuse (depends on A, D)
- Lane M: Debug views (step count, L, epsilon, residency, fields, envelope slack, uncertainty interval, FBM octave utilization) (depends on D)
- Lane N: Review (depends on L, M)

**Dependencies from Phase 1:** Cel shader, outline pass, palette system, post-processing, shadow cascades

**AC:**
- Environment renders via compute ray march from cached brick pool
- Canonical safe-step helpers used everywhere — NO raw `step = d` anywhere
- Direction-aware denominator (`L_dir(v)=dot(abs(v),B)`) used where `B` is available; scalar `L` only as explicit fallback
- Step is clamped to active region exit (`distance_to_active_region_exit`) before advancing
- Conservative mips: envelope-based Lipschitz closure verified (min-reduction fallback acceptable during bringup), no averaged distance mips
- Envelope reconstruction: `b_env(p)` is generally tighter than diagnostic fallback on benchmark corpus; persistent large red slack regions are treated as convention/bound bugs
- Envelope slack debug view functional; uncertainty interval debug view functional when upper-bound channel is enabled
- Layered edits: carve crater -> persists -> heals -> no sign artifacts
- Dual contouring meshes validated as proxy geometry (shadow/broad collision); gameplay-critical narrow-phase uses field queries
- Brick population within budget without frame stutter
- Cone stepping: average steps < max_ray_steps for Medium profile
- Cone-union acceleration: measurable step-count reduction in pessimistic-but-empty zones (benchmark: warp-heavy scene, compare avg steps with/without cone-union gate)
- LCP fused stepping: on Medium+ quality, diagonal-ray step counts improve by >20% vs L1-only on SDF-like regions (benchmark: diagonal camera through sphere field)
- LCP mixed cone-union (High quality): step counts improve >10% beyond fused-only on scenes with mixed bound quality
- Bernstein-certified noise: brick bake produces `CertifiedNoisePatch` with `AnalyticBernstein` provenance for all noise cells; derivative bounds verified tighter than inflated fallback
- Frequency-bounded FBM early exit: > 60% of rays in noise-displaced scenes evaluate fewer than half the octaves (measured via FBM octave utilization debug view)
- Budget marching integrated in 2b: field eval count reduced by >50% for rays traversing >3 bricks of empty space (benchmark corpus)
- Re-distance maintenance: step-count histogram improves after re-distance passes (performance trigger fires and resolves)
- Re-distance invariants enforced: zero-set drift within `tau_drift` tolerance, b/u bounds tightened but never invalidated, narrow band only, d_est/b/u separation maintained
- GI: indirect light visible as shadow color shift
- GI: corners/crevices darker (AO from probes)
- Curvature-driven detail visible on Medium+
- Outlines work on field depth/normals
- Shadows via dual-contoured meshes
- Debug views functional
- Divergence guardrail active: advanced branch stack stays within `ray_march_branch_budget_ms` on Medium profile, with deterministic degrade order when over budget
- Coherence mitigation effective: `DivergenceProxy` improves vs unbinned baseline on stress path; if not, mini-compaction escalation is enabled in 2b
- Anime style stability (hero closeup path): silhouette temporal edge displacement p95 <= 1.0 px
- Anime style stability (micro-jitter camera, static lighting): toon-band flicker rate <= 2% pixels/frame for unchanged shading inputs
- Anime stability metric protocol (reproducible): run `bench_hero_closeup_path_C` in deterministic mode for 300 frames
- Anti-overfit check: run the same protocol on `bench_forest_combat_path_A` as a stress regression monitor; values may be looser but must not regress beyond agreed baseline deltas
- `SilhouetteDisp_p95` definition: p95 screen-space displacement of reprojected silhouette edge pixels over consecutive frames
- `ToonBandFlicker` definition: fraction of pixels in `stable_shading_mask` whose quantized toon band changes frame-to-frame
- `stable_shading_mask` definition: unchanged material ID and unchanged pre-quantized irradiance within `epsilon_irradiance = 1e-3` (absolute, linear domain)
- Proxy/debug character path is integration-only (no gameplay authority) and has explicit removal contract for Phase 3
- Phase 3 consumes shared `motor_core` from 2b without duplicate implementation
- 60fps at 1080p on Medium profile (with wavefront compaction if CONDITIONAL GO from Phase 2a)
- Memory within budget spreadsheet
- Phase 2a measurements reproduced on full scene (step counts should improve vs spike due to envelope mips replacing min-reduction)

---

## Phase 3: Neural Field Characters + Procedural Anatomy

**Purpose:** Ship neural implicit field characters as the authoritative character path. Characters are continuous functions with smooth morphing and topological changes. Includes procedural anatomy system for parametric body generation.

**Phase-order contract:** Phase 3 consumes the shared `motor_core` module shipped in Phase 2b (dual-quat apply/compose/inverse/normalize + packing conventions). Phase 4a is a global cutover/expansion phase, not the first introduction of motor math.

**Critical changes from v1:**

1. **Characters render from cached brick volumes, not per-ray MLP forward pass.** The math: a 4-layer, 64-wide MLP costs ~12-16K multiply-adds per query (3-6D input × 64 + 3 × 64×64 hidden + 64×1 output). Sphere tracing needs ~50 queries per pixel. At 1080p (2M pixels): 2M × 50 × 14K ≈ 1.4 TFLOP per frame. At 60fps that's ~84 TFLOPS throughput — 4-8x over a typical WebGPU device's 10-20 TFLOPS sustained throughput. Instead: maintain a character-local clipmap volume (capsule-aligned), run MLP into that volume via compute shader, ray march cached volume via texture fetches (fast).

2. **Canonical-space pose conditioning, not flattened joint rotations.** Feeding (x, y, z, 30_joint_angles) is high-dimensional and global. Instead:
   - Map query point to canonical (rest) space via inverse skinning using dual quats/motors
   - Run canonical neural field in rest space: f_canonical(p_rest) -> distance
   - Optionally add small pose-dependent residual: f_residual(p_rest, local_joint_params) -> delta
   - Cuts input dimension from ~33 to ~6, reduces training data needs, improves generalization

3. **Morphing via displacement-field advection, not scalar blending.** Lerping MLP weights produces mushy intermediates. Scalar field lerp (`f = lerp(f_A, f_B, t)`) is mathematically safe but visually wrong — it creates ghostly cross-fades where geometry dissolves and reappears instead of physically deforming. A human morphing into a wolf should show mass flowing and reconfiguring, not two transparent ghosts overlapping.

   **The fix: displacement-field morphing.** Compute a correspondence between source and target surfaces, derive a displacement field `D(p)` that maps source geometry to target geometry, and advect:
   ```
   f_t(p) = f_source(p - t * D(p))
   ```
   The displacement field `D` is computed once (during morph setup, not per-frame) via closest-point correspondence on the cached brick volumes:
   - For each near-surface voxel in source, find the closest surface point in target
   - Smooth the raw correspondence via 3-5 iterations of Laplacian diffusion on the brick grid
   - Store `D` as a vec3 field in a dedicated morph brick channel (~12 bytes/voxel for vec3 f32, only allocated during active morphs)

   **Lipschitz tracking:** If `D` has Lipschitz constant `L_D` (bounded by the Jacobian of the displacement field), then:
   `B_morph(t) <= B_source * (I + t * |J_D|)` where `|J_D|` is the componentwise absolute Jacobian bound of `D`.
   At `t=0`, bounds equal source. At `t=1`, bounds inflate by the warp factor. This is the standard domain-warp rule (`B_out = A^T * B_in`) already implemented in Phase 0a.

   **Why not optimal transport:** Full Wasserstein displacement interpolation (Brenier potential via Sinkhorn) would be provably optimal but costs 50-500 iterations of full-volume Gaussian convolution per morph setup — prohibitive on WebGPU. Closest-point + Laplacian smoothing gives 90% of the visual quality (mass flows, no ghosting) at 1% of the compute cost.

   **Fallback:** Scalar lerp `f = lerp(f_A, f_B, t)` remains available for cases where displacement is unnecessary (e.g., material-only transitions, subtle proportion changes). L = max(L_A, L_B), no inflation.

4. **Anatomy-derived skinning as analytic influence field, not "skin weights per vertex."** In a meshless world: weight(bone_i, p) = falloff(distance_to_bone_segment(bone_i, p)). Used for inverse skinning (canonicalization) and training point generation.

**What's built:**

- **Procedural anatomy generation system:** (unchanged from v1)
  - Spine graph: DAG of segments defining creature topology
  - Skeleton derivation: spine -> joint hierarchy with limits
  - Limb generation: arms, legs, wings, tentacles from limb specs
  - Body field generation: segments -> capsules/cylinders -> R-function unions
  - Correlated morphological variation: body_scale, musculature, mass, proportions with inter-parameter correlations
  - Weaponized anatomy: spikes, horns, claws, tail weapons, armor plates
  - Signature: generate_anatomy(spec: AnatomySpec) -> (FieldGraph, Skeleton, InfluenceField)

- **MLP training pipeline:**
  - Input: FieldGraph from anatomy system
  - Sample 1-5M points near surface, compute ground-truth distance
  - Canonical-space: sample points, apply inverse skinning for rest-space coordinates
  - Train 4-8 layer MLP (64-256 wide) in canonical space
  - Optional residual network (2-layer, 32-wide) for pose detail
  - Output: weight blobs (~100-500KB per character)
  - Training via WebGPU compute shaders (custom forward/backward pass in WGSL), NOT CPU-side (WASM CPU is ~1 GFLOP/s, too slow for 5M-sample training). Training runs on the same GPU as rendering but during loading screens or background idle. Alternative: server-side training with weight download for production builds.

- **Character brick prebake:**
  - Per character: capsule-aligned brick volume (32^3-64^3 from quality profile)
  - Update compute pass: when pose changes, run MLP into brick volume
  - Fast path: only re-run bricks where inverse skinning mapping changed (near joints)
  - Update frequency from quality profile
  - Same per-voxel format as environment bricks (d + normal + curvature)

- **Hierarchical detail:**
  - Coarse network (~50KB): body silhouette, always in coarse bricks
  - Fine network (~200KB): surface detail, only for bricks near camera
  - Screen-space coverage determines level

- **Morphing via displacement-field advection:**
  - Cache two field volumes (source and target)
  - Compute closest-point correspondence from source surface voxels to target surface
  - Smooth correspondence via 3-5 Laplacian diffusion iterations on brick grid
  - Store displacement field `D(p)` in dedicated morph brick channel (vec3 f32, ~12 bytes/voxel, only during active morphs)
  - Runtime morph evaluation: `f_t(p) = f_source(p - t * D(p))`
  - Lipschitz tracking: `B_morph(t) <= B_source * (I + t * |J_D|)` via standard warp rule
  - Morph setup is amortized (compute `D` once when morph begins, reuse across frames)
  - Scalar lerp fallback for material-only or subtle proportion transitions: `f = lerp(f_A, f_B, t)`, L = max(L_A, L_B)

**Parallel lanes:**
- Lane A: Anatomy — spine graph + skeleton + influence field (independent)
- Lane B: Anatomy — limb + body field generation (depends on A)
- Lane C: Anatomy — morphological variation + weaponized anatomy (depends on B)
- Lane D: Canonical-space conditioning — inverse skinning via dual quats (depends on A, Phase 2b Lane L3 `motor_core`)
- Lane E: MLP training pipeline + data generation from field graph (depends on C, D)
- Lane F: Character brick prebake — compute shader MLP -> brick volume (independent of anatomy)
- Lane G: Pose-dependent residual network training (depends on D, E)
- Lane H: Hierarchical detail + LOD selection (depends on F)
- Lane I: Morphing via displacement-field advection + correspondence compute (depends on F)
- Lane J: Integration — remove temporary proxy character path, wire cel/outline/shadow (depends on F, H)
- Lane K: Review (depends on J)

**AC:**
- Anatomy generates valid humanoid, quadruped, serpentine, winged variants
- Correlated parameters produce coherent variation
- Characters render from cached brick volumes (no per-ray MLP in inner loop)
- Canonical-space conditioning: network input is 3-6D, not 33D
- Pose changes produce smooth deformation within update budget
- Close-up shows fine detail (hierarchical LOD)
- Morphing via displacement-field advection: geometry flows physically (no ghostly cross-fade), Lipschitz bounds tracked via warp rule
- Morph displacement field `D` computed in < 50ms on Medium profile (amortized, not per-frame)
- Scalar lerp fallback available and produces no visual artifacts for subtle transitions
- Training produces valid networks in <5 minutes
- 60fps on Medium profile with 2 characters + field environment
- Memory within budget

---

## Phase 4a: Motor Transform Pipeline

**Purpose:** Perform the global hard cutover to dual-quat/motor transforms across the entire runtime. Phase 3 already consumes shared `motor_core`; Phase 4a expands that core to full-engine usage (camera/instances/joints/interpolation) and removes remaining matrix hot paths.

**Mathematical foundation:**

A **motor** in PGA is equivalent to a unit dual quaternion — an 8-component element representing rigid transformation (rotation + translation).

```
Motor application (sandwich product):
  p' = M p reverse(M)
  Applies rotation AND translation in one operation.

Motor interpolation (exponential map):
  M(t) = exp(t * log(M))
  Traces unique screw motion between identity and M.
  - Constant speed, shortest path, volume-preserving, no gimbal lock

Motor blending for skinning (DLB, Kavan et al. 2008):
  // Antipodal correction: motors have double-cover (M and -M = same transform).
  // Before blending, flip sign of M_i where dot(M_0, M_i) < 0 to ensure
  // all motors are in the same hemisphere. Without this, blending near
  // 180-degree rotations produces degenerate results (normalize of near-zero).
  for i in 1..N: if dot(M_0, M_i) < 0 { M_i = -M_i; }
  M_blend = normalize(sum(w_i * M_i))
  Unlike LBS with matrices:
  - Preserves volume (no candy-wrapper at 180 deg twist)
  - Interpolates through valid rigid transforms
```

**What's built:**
- Motor type in WGSL: 8 floats (dual quaternion layout)
- Operations: apply, compose, interpolate (log/exp), normalize, blend
- Replace mat4 transforms: camera, instances, joints
- Motor skinning path for character systems (authoritative transform path)
- Motor field transforms: domain operations use motor application

**AC:**
- All transforms use motors (no mat4 in hot path)
- Skinning: no candy-wrapper at 180 deg twist
- Motor interpolation follows screw motion
- Performance within 5% of matrix pipeline
- No visual regression

---

## Phase 4b: Extended Geometric Algebra (Research, Deferred)

**Purpose:** Explore CGA for field primitives and PGA for domain operations where they provide clear wins.

**What would be built:**
- CGA: sphere, circle, point pair as single algebraic elements
- PGA: domain repetition as motor lattice, symmetry as motor group
- Only ship if it measurably simplifies field graph IR. No-ship is acceptable.

---

## Phase 5: Spectral Material Model

**Purpose:** Replace RGB with spectral basis representation. Materials as functions over wavelength.

**Critical change from v1:** 3-6 spectral basis coefficients, not 16-32 wavelength samples.

**Mathematical foundation:**

**Spectral basis representation (Mallett & Yuksel 2019 style):**
```
S(lambda) = sum_i c_i * B_i(lambda)    for i = 1..N  (N = 3-6)
```
where B_i(lambda) are precomputed basis functions from PCA of natural material spectra.

**Key math that makes this cheap:**
- Light-surface interaction: S_reflected(lambda) = S_incident(lambda) * R(lambda) becomes c_reflected = M * c_incident where M is NxN. For N=4, it's a 4x4 matmul.
- Basis to RGB: precomputed 3xN matrix. T_anime instead of T_cie for stylized output.

**Fluorescence:** Non-diagonal NxN matrix (off-diagonal terms couple basis components).
**Iridescence:** Film interference at N wavelengths corresponding to basis peaks, or hero wavelength approach.

**What's built:**
- Spectral basis library: N-coefficient representation, PCA fitting
- Basis matrices: light x material, material -> RGB, anime observer
- Spectral cel shader: lighting in basis space, toon ramp, collapse to RGB
- Spectral palettes: basis coefficient sets instead of RGB tuples
- Fluorescence: off-diagonal basis matrices
- Iridescence: angle-dependent basis perturbation

**AC:**
- Materials as N-coefficient spectral basis vectors
- Shadow colors shift hue naturally (emerges from math, not manual tuning)
- Fluorescent materials visibly glow
- Iridescence shows angle-dependent color shift
- Cost: <=1 extra matrix multiply per shading point vs RGB
- No visual regression

---

## Phase 6: Stochastic Painterly Rendering

**Purpose:** Every frame is a unique painting.

**Critical change from v1:** Two modes (deterministic + expressive) as first-class. Blue noise everywhere. Perceptual testing (SSIM).

**What's built:** (largely unchanged, plus swept-volume smear frames)
- Brush stroke model (direction from screen-space gradient or stored curvature)
- Ink wash simulation (2D Navier-Stokes, compute shader)
- Paper texture response
- Temporal accumulation
- **Deterministic mode:** fixed blue noise seeds, SSIM-based CI
- **Expressive mode:** full stochasticity
- All stochastic sampling uses spatiotemporal blue noise, never white noise
- **Swept-volume smear frames (spacetime Lipschitz):** For fast-moving objects (weapons, limbs during attacks), evaluate the swept field `F_sweep(x) = min_t f(x,t)` over the shutter interval using time-envelope certificates (see Core Invariant — Spacetime Extension). The smear frame is rendered as a transient field layer that the existing field renderer already knows how to shade. Smear thickness scales with a local combat-intensity signal in Phase 6; Phase 11 later remaps that signal through mood-state orchestration. Implementation:
  - Scope: only explicitly tagged kinematic entities (weapons/limbs/projectiles); static terrain/world field remains 3D-only in this phase
  - Region of evaluation: actor-local swept AABB (with configurable dilation margin), never full-world 4D traversal
  - Per fast-moving entity: sample object field at 3-5 time points across shutter interval (Medium profile cap; High may increase)
  - At each shading point near the swept region, classify with sampled-time certificate:
    - if any time sample has `f(x, t_i) <= 0` -> `InsideSampled`
    - else compute `lb = swept_volume_lower_bound(...)`; `lb > 0` -> `OutsideCertified`; otherwise `Uncertain`
  - If `InsideSampled` (or refined-inside from the uncertain path) and outside current-frame object: shade as smear (reduced opacity, motion-direction color shift)
  - Budget control: `smear_ms_budget_per_frame` and `smear_max_active_entities` hard caps (Medium defaults: 1.0ms, 8 entities). If over budget: reduce time samples first, then clamp per-entity contribution radius, then fall back to deterministic 2D motion-streak proxy for overflow entities
  - For `Uncertain` classifications: refine temporal sampling if budget allows; otherwise use the deterministic overflow fallback path
  - Shutter interval is artistically controllable: narrow for crisp animation, wide for dramatic manga-style speed lines

**AC:** (unchanged, plus:)
- Deterministic mode (same GPU/driver/browser build): SSIM > 0.99 across runs
- No white noise anywhere
- Smear frames: fast weapon swing produces visible swept-volume trail consistent with sampled spacetime certificates (not a post-process blur)
- Smear frame thickness responds to local combat intensity parameter (Phase 11 mood remap plugs into this input later)
- Smear evaluation remains within budget: frame-time contribution <= `smear_ms_budget_per_frame` on Medium profile
- 4D smear path applies only to tagged kinematic entities; untagged/static world content never triggers full 4D sweep in Phase 6
- Overflow smear fallback rarity (Medium benchmark corpus): deterministic 2D fallback used in <= 5% of frames and <= 10% of tagged smear entities; exceedance fails AC and requires budget/sample retuning
- Overflow telemetry contract: expose per-frame counters for fallback frames/entities in runtime perf HUD and CI regression artifacts

---

## Phase 7: Procedural Audio Synthesis

(Unchanged from v1 — well-specified. Sound recipes, voice architecture, spatial audio, parameter-driven design, musical system.)

---

## Phase 8: Recipe DSL and LLM-to-Game Compiler Interface

**Purpose:** Wrela becomes the asset format. LLM writes code that compiles to game.

**Critical change from v1:** Compile to GPU-resident DAG bytecode interpreted by stable kernel, not fresh WGSL per edit.

**Why:** WebGPU pipeline compilation costs 100ms-2s per shader module. If every recipe edit regenerates WGSL, the <5s iteration target fails. Instead:

```
Wrela recipe code
    | (compiler)
    v
Compact bytecode / DAG representation
    | (upload to GPU buffer)
    v
Stable interpreter compute kernel processes DAG to populate bricks
```

Render shaders are static. Interpreter kernel is static. Only DAG buffer changes. Pipeline recompilation: zero. Fast-path local iteration (small dirty region): <100ms.

**Novel addition: Automatic sensitivity propagation.**

Propagate partial derivatives through a dedicated **sensitivity DAG** (dual-number forward mode) that may differ from the render DAG at nondifferentiable operators:
```
sphere(p, radius=r):  d = |p| - r,  dd/dr = -1
render_union(a, b):   d = min(a, b)                          // render path
sense_union(a, b):    d = -tau * log(exp(-a/tau)+exp(-b/tau)) // smooth surrogate for gradients
                      dd/dparam uses surrogate branch weights (stable near a ~= b)
warp(f, w):           chain rule through warp Jacobian
noise(p, amplitude):  dd/damplitude = noise_value(p)
```
`tau` is scheduled by profile (larger for stable global guidance, smaller for local fine edits).

This tells the LLM "increasing tree_twist by 0.1 most changes pixels in this region." Gradient-informed iteration, not blind trial-and-error. Fall back to finite differences for operators where analytic derivatives are impractical.

**What's built:**
- DAG bytecode format for field/material/animation/sound
- Stable interpreter kernel (compute shader)
- Wrela language extensions: field, material, anim, sound, world_recipe blocks
- Compiler lowering: Wrela -> bytecode (not Wrela -> WGSL)
- Sensitivity propagation: dual-number forward mode through sensitivity DAG with smooth surrogates for `min/max`
- Hot-patch: edit recipe -> emit bytecode -> upload GPU buffer -> bricks re-populate

**AC:**
- Complete world recipe compiles and runs without shader recompilation
- Local recipe edit (<=32 dirty bricks, no global reseed) -> visual update in <100ms on Medium profile desktop baseline
- Large recipe edit (global/world-scale invalidation) -> visible convergence in <5s via budgeted brick repopulation
- Sensitivity: changing a parameter shows derivative as debug overlay
- Sensitivity gradients remain stable near boolean seams (`a ~= b`) on regression corpus
- Compiler errors are clear and actionable
- All construct types compile to bytecode

---

## Phase 9: Physics-Driven Procedural Animation

**Purpose:** Replace keyframe animation with physics-based motion.

**Addition 1: Field-driven contact.**
Use distance field directly for:
- Penetration depth: `d_est(contact_point)` gives a bounded depth estimate with certificate `b_lower <= d_est <= u_upper`
- Contact normal: stored gradient/normal channel gives a stable normal estimate (bounded by re-distance drift + interval width)
- Foot placement: downward field query returns bounded ground height/normal; if interval width exceeds `tau_contact`, refine or fallback

Makes IK foot placement, impacts, wall contacts feel better with less code than mesh collision.

**Addition 2: Spacetime Lipschitz CCD (guaranteed no-tunneling for fast contacts).**

Traditional discrete collision detection samples distance at the start and end of a tick. High-speed anime weapons (sword tips moving at 10-30 m/s) can tunnel through thin terrain features (branches, rock edges) in a single tick. Bolting on swept-sphere or GJK CCD requires separate geometry representations.

With spacetime Lipschitz envelopes (see Core Invariant section), CCD becomes "conservative stepping in time" using the same math as ray marching:

```
For a moving point with trajectory p(t) = p0 + v*t over a tick:
  L_path = Bx|vx| + By|vy| + Bz|vz| + Bt
  Δt_safe = max(0, b_lower) / L_path
  Δt = min(Δt_safe, dt_remaining, dt_to_bound_region_exit)
```

The point can advance `Δt` in time without crossing any surface. `dt_to_bound_region_exit` is the time until the trajectory leaves the region where `(B_space, Bt)` are valid; clamping keeps the proof local. When it can no longer advance (b_lower is small), bracket and bisect to a configured tolerance (`tau_contact_time`, Medium default `1e-4 s`) for contact time.

**What's built for CCD:**
- `compute_bt_rigid()` for each moving rigid body (weapons, projectiles, limb endpoints)
- Spacetime marching along weapon tip trajectory during combat ticks
- Per-step clamp to `dt_to_bound_region_exit` before advancing
- Contact time refinement via bisection when `b_lower < contact_threshold`
- Weapon-vs-environment CCD is the **narrow vertical slice** — if it works here, it extends to foot contacts, projectiles, dash movement, etc.

**When Bt = 0 (static world, slow contacts):** CCD reduces to discrete distance sampling. No overhead for slow-moving contacts. Spacetime CCD only activates when `||v|| * dt > contact_margin` (high-speed contact risk).

**Addition 3: Variational integrators for all physics dynamics.**

All spring, IK, hair, and cloth dynamics use variational integrators (Marsden & West 2001) — derived from the discrete variational principle rather than discretizing the equations of motion. The midpoint rule discrete Lagrangian:
```
L_d(q_k, q_{k+1}) = h * L((q_k + q_{k+1})/2, (q_{k+1} - q_k)/h)
```
The discrete Euler-Lagrange equations derived from `L_d` give the integrator. Properties by construction:
- **Exact symplecticity** — preserves phase space volume exactly, not approximately
- **No energy drift** — bounded modified energy that stays stable indefinitely over long sessions
- **Momentum conservation** — if the Lagrangian has a symmetry, the discrete momentum is exactly conserved
This prevents character animations from drifting, spring systems from pumping/draining energy, and pendulum dynamics (limbs, hair, cloth) from accumulating error over hours of gameplay. This is how NASA integrates spacecraft trajectories.

**Addition 4: Cosserat rod hair dynamics.**

Hair strands are Cosserat rods — 1D elastic structures with both translational and rotational DOF at every point. Each strand is a chain of motors on SE(3) (plugging directly into the Phase 4a motor pipeline):
```
State per segment: M_i ∈ SE(3) (position + orientation as motor)
Strain measures:   κ (curvature), τ (twist), ε (stretch)
Elastic energy:    E = ∫ ½(EI₁κ₁² + EI₂κ₂² + GJτ² + EAε²) ds
Kinematics:        M_{i+1} = M_i * exp(h * ξ_i)  (discrete rod, ξ_i in se(3))
```
The variational integrator on SE(3) is embarrassingly parallel — each strand is independent except for collisions. N strands run simultaneously as a compute shader.

Collision: strand tip queries character body field -> bounded penetration estimate + stable normal estimate (same contact certificate path, no separate collision mesh).

Anime hair tuning: high bending stiffness = large coherent mass motion. Low damping = overshooting oscillation. Asymmetric material frame = preferred orientations. Emotional modulation: rest curvature shifts with mood state (tense = raised, calm = gravity-following).

Wind interaction: external forces are field queries — wind is a vector field on the brick substrate with Kolmogorov turbulence spectrum `E(k) ~ k^(-5/3)`. Large-scale coherent motion from low-frequency turbulence, individual strand variation from high-frequency. The `k^(-5/3)` spectrum produces the correct ratio of large to small scale motion seen in anime hair.

Hair as anisotropic implicit field: represent hair volume as a field with anisotropic `B = (B_perp, B_perp, B_along)` where `B_perp >> B_along`. Ray marching uses anisotropic stepping — rays along strands take large steps, rays across take small steps.

**Addition 5: Kirchhoff plate cloth dynamics.**

Garments are thin elastic plates governed by Kirchhoff plate theory:
```
Bending energy: E_bend = ∫∫ ½D(κ₁² + κ₂² + 2νκ₁κ₂) dA
Flexural rigidity: D = Et³/12(1-ν²)
Wrinkle wavelength: λ = 2π * (D/σ)^(1/4)  (Euler buckling threshold)
```
Different material parameters produce different aesthetics automatically:
```
Heavy wool coat:   High D, moderate damping → large dramatic wrinkles (λ ~ 8-12cm)
Silk:              Very low D, low damping  → fine numerous wrinkles (λ ~ 0.5-2cm)
Leather:           High D, high damping     → sharp structural wrinkles (λ ~ 4-8cm)
```
Anime style: scale material parameters slightly from physical values — lower damping (more oscillation), tuned D per garment type. The wrinkle wavelength formula determines the aesthetic automatically from material identity. No art direction needed to make wool look like wool.

Garment field layering: each garment is a thin shell implicit field offset from the body field. Body penetration constraint enforced by field geometry: `f_garment(p) >= f_body(p) + gap_distance`. Smooth union with gap keeps garments from penetrating. Layer ordering as a partial order: skin < base < mid < outer < accessories.

(Rest unchanged from v1 — IK, rigid body, springs, anime timing, smear frames, frame data, locomotion.)

**Additional AC:**
- Foot placement uses field-driven contact
- Impact response uses field penetration depth + gradient normal
- Spacetime CCD: fast sword swing against thin terrain feature (e.g. tree branch) detects contact — no tunneling. Verified on benchmark sweep with weapon tip velocity >= 15 m/s against features thinner than 2 voxels
- CCD activation gate: no overhead for slow contacts (gate triggers only when `||v|| * dt > contact_margin`)
- Variational integrators: spring/pendulum benchmark shows bounded energy over 10,000 simulation steps (no drift)
- Hair: N Cosserat rod strands run simultaneously on GPU compute, visually coherent large-scale motion with individual strand variation
- Hair collision: field-driven contact (no separate collision geometry), no strand-body interpenetration
- Cloth: Kirchhoff solver produces wrinkles at material-correct wavelength for at least 3 material types (heavy, medium, light)
- Cloth: garment field layers on body field without interpenetration via gap constraint

---

## Phase 10: Living World — Multi-Resolution PDE Simulation

**Purpose:** The world is alive as continuous physical simulation. Coupled fields evolve via PDEs on the brick pool substrate.

**Critical changes from v1:**

1. **Multi-resolution: not everything per voxel.** Distance stays high-res near surfaces. Slow scalars (corruption, moisture, temperature, growth, biome) at per-brick resolution (one value per brick). Stress is surface-biased and event-driven.

2. **Region epochs for infinite world (Phase 12).** Each region has a monotonic epoch. Near-player regions: full frequency. Far: freeze or coarse catch-up.

3. **Irradiance as coupled PDE — "light as a physical force."**

   Add irradiance field to simulation:
   ```
   du/dt = D_light * laplacian(u) - sigma * u + S
   ```
   u = irradiance, D_light = diffusion coeff, sigma = absorption (from material density), S = source emission.

   **Screened Poisson equation.** Steady state: -laplacian(u) + (sigma/D)*u = S/D

   **Discrete Green's function theorem:** Steady-state irradiance at distance r from point source decays as u(r) ~ exp(-sqrt(sigma/D) * r) / r. This bounds the influence radius per source: beyond r_max ~ 3/sqrt(sigma/D), contribution is negligible.

   **Convergence target (practical, not absolute):** Jacobi alone converges linearly and can be slow on coarse grids or high coefficient contrast. Use multigrid V-cycles on the brick hierarchy to accelerate. Acceptance target: >90% residual reduction in <=8 V-cycles on benchmark scenes for Medium profile.

   **Gameplay integration:**
   - Corruption blocks light (high corruption -> high sigma -> rapid attenuation). Corrupted regions are dark.
   - Soul Blade resonance glow is a source term. Higher resonance = more light = pushes back corruption.
   - Dawn: solar source increases -> irradiance expands -> corruption retreats. Coupled PDE interaction.
   - "Light fights darkness" is emergent from coupled PDEs, not a scripted mechanic.

4. **Stable diffusion integration.** Explicit diffusion: CFL constraint dt <= h^2 / (2d*D). For large timesteps (coarse region catch-up), use semi-implicit Jacobi (unconditionally stable).

5. **Stochastic PDE noise for emergent spatial patterns.**

   Add a Langevin noise term to the PDE stepping:
   ```
   u_{k+1} += sqrt(dt) * sigma_noise * W_k
   ```
   where `W_k` is spatially correlated noise drawn from the existing blue noise infrastructure (Phase 0/6). The `sqrt(dt)` scaling is Itô calculus — the correct scaling for Brownian motion.

   `sigma_noise` is a per-field quality parameter:
   - `sigma_noise = 0`: fully deterministic (regression/replay mode)
   - `sigma_noise > 0`: organic dynamics with emergent spatial structure

   **Why this matters:**
   - **Turing pattern nucleation:** Reaction-diffusion systems spontaneously form spatial patterns (Turing 1952) but ONLY in the presence of noise. Deterministic RD starting from uniform state stays uniform forever. With noise, corruption/growth patterns form organically — patches, rings, spots — without anyone placing them.
   - **Stochastic resonance:** Near threshold behaviors (corruption crossing yield strength, fracture initiation), noise pushes the system across thresholds it would never cross deterministically. Fractures initiate more naturally, corruption blooms emerge from seemingly stable regions.
   - **DMD compatibility:** The noise averages out over long times — the deterministic reduced dynamics captures the mean behavior. On wake, corrective PDE steps reintroduce the correct fluctuation statistics. No change to the DMD sleep system needed.
   - **Cost:** ~5 lines of code per PDE field per timestep. One blue noise lookup + multiply-add.

   **Authoritative boundary:** `sigma_noise` on the authoritative server uses a deterministic RNG seeded per (region, epoch, field) — stochastic but reproducible. Client presentation noise is purely visual.

6. **Stress as event-driven fracture, not wave equation.** v1 had "stress waves at 4x substep" — numerically fragile. Instead:
   - Impact deposits impulse (position, direction, magnitude)
   - Immediate local damage check: if damage > yield strength, add crack edit primitive along principal stress direction
   - Crack propagation: K_I = sigma * sqrt(pi * a) at crack tip. If K_I > K_Ic (fracture toughness), extend crack
   - Computed once per frame per active crack, not as PDE everywhere

6. **Spectral region sleep via Dynamic Mode Decomposition (DMD) — sublinear approximate time-skip.**

   The CFL condition `dt <= h^2 / (2d*D)` makes discrete PDE stepping O(N) in epoch count. When a player leaves a region and returns 5,000 epochs later, brute-force catch-up is infeasible. Semi-implicit Jacobi removes the stability constraint but still requires O(N) steps for accuracy. The plan's v1 "coarse summary dynamics" is a hand-wave — coarse integration of nonlinear coupled PDEs destroys fine structure and can diverge through bifurcations.

   **The fix: data-driven spectral decomposition of the local dynamics.**

   Use Dynamic Mode Decomposition (DMD) — a finite-rank approximation of the Koopman operator — to extract dominant reduced dynamics from the PDE trajectory while the region is active. When the region streams out, store reduced dynamics state instead of running PDE steps. On revisit, evaluate the reduced system at the target epoch, then run bounded corrective PDE steps.

   **Mathematical foundation:**

   Let `x_k ∈ R^N` be the per-brick PDE state vector (corruption, moisture, temperature, growth, biome, irradiance — N = 6 × num_bricks_in_region) at authoritative tick k. The nonlinear PDE update is `x_{k+1} = F(x_k)`.

   DMD approximates F as a linear operator by minimizing `||Y - A X||_F` where:
   ```
   X = [x_1, x_2, ..., x_{M-1}]   (N × (M-1) snapshot matrix)
   Y = [x_2, x_3, ..., x_M]       (N × (M-1) shifted matrix)
   ```

   Compute via truncated SVD: `X ≈ U_K Σ_K V_K^T` (rank-K truncation, K = 8-16), then:
   ```
   A_tilde = U_K^T Y V_K Σ_K^{-1}     (K × K reduced dynamics matrix)
   A_tilde = Q T Q^T                   (real Schur form; Q orthogonal, T quasi-upper-triangular)
   Ψ = Y V_K Σ_K^{-1} Q                (N × K real mode basis)
   ```

   Use snapshot interval `Δt_snap = S * Δt_tick` (S = snapshot stride in ticks). Any continuous-time diagnostic eigenvalues must use `Δt_snap`, not raw tick interval.

   **Authoritative prediction (discrete reduced system):**
   ```
   c_0 = Ψ^+ x_sleep   (project sleep-entry state into reduced basis via pseudoinverse)
   c_{k+1} = T c_k
   x_k ≈ Ψ c_k
   c_{k+m} = T^m c_k   (fast exponentiation by squaring)
   ```
   Pseudoinverse policy (authoritative path): compute `Ψ^+` via SVD/QR-based least-squares
   with singular-value cutoff (TSVD) and optional Tikhonov damping, never by explicit
   normal-equation inversion `(Ψ^T Ψ)^{-1} Ψ^T`. This avoids numerical instability/rank issues.

   This is effectively O(log m) in elapsed epochs (or O(1) for bounded precomputed powers), not O(m) stepping.

   **Critical limitation: this is an approximation, not an exact solver.** DMD captures the linearized dynamics around the observed trajectory. For strongly nonlinear systems (bistable corruption, threshold-driven fracture, ecological bifurcations), the prediction diverges from the true trajectory over time. The engine uses a **tiered catch-up strategy** that accounts for this:

   ```
   Tier 1 (< 100 epochs):    Full Jacobi PDE stepping. Exact. Already budgeted.
   Tier 2 (100-10K epochs):   DMD prediction as initial condition, then N_correct
                               corrective PDE steps (N_correct = 8-16) to re-establish
                               nonlinear invariants (reaction-diffusion pattern formation,
                               threshold behaviors, coupling equilibria).
   Tier 3 (> 10K epochs):     DMD for macroscopic distribution (which biome won, overall
                               corruption/irradiance levels). Re-seed microscopic structure
                               stochastically from macro state using procedural generators
                               conditioned on DMD output. Fine structure is lost — this is
                               physically reasonable at geological timescales.
   ```

   **Stability enforcement:** Before storing reduced dynamics, inspect Schur blocks in `T`.
   - For 1x1 block (real λ): clamp `|λ| <= 1`.
   - For 2x2 block (complex-conjugate pair): clamp block spectral radius `r <= 1` while preserving rotation angle.
   This guarantees sleeping regions do not diverge. The clamping is logged. If >50% of retained modes/blocks are clamped, mark region as "strongly nonlinear" and disable Tier 3 for that region.
   Strongly nonlinear wake policy is latency-safe: escalate Tier 2 (`K` up to 32, `N_correct` up to 64) instead of forcing full-gap Tier 1 replay.

   **Drift monitoring:** On region wake, compute
   `drift = ||x_reduced - x_corrected|| / max(||x_corrected||, eps_drift)` with `eps_drift = 1e-6` after corrective PDE steps.
   Track as a running metric per region. If `drift > 0.3` consistently, region dynamics are too nonlinear for current K/correction budget; increase `K` (up to 32) and/or correction steps.

   **Mode collapse detector:** If drift is repeatedly high (>0.3 on 3+ consecutive wakes) for the same region, the problem may be temporal resolution, not rank. Before escalating K further, automatically shorten `Δt_snap` (halve S, collect more frequent snapshots while the region is next active). This gives the DMD decomposition higher-frequency dynamics to capture. If shortened `Δt_snap` reduces drift below threshold, lock the new S for that region. If not, escalate K as before. This prevents wasting rank budget on aliased dynamics that just need more temporal samples.

   **Storage:** Per sleeping region:
   ```
   Mode basis Ψ:     N × K × f16 = 6 × R × K × 2 bytes
                     (R = bricks in region, K = retained rank)
                     For R=256, K=16: 256 × 6 × 16 × 2 = 48 KB
   Schur T:         K × K × f32 (or packed Schur blocks) ≈ 1 KB for K=16
   Coeff c0:        K × f32 = 64 bytes for K=16
   Initial epoch:   u64 = 8 bytes
   Clamping flags:  K bits (or block flags) = 2 bytes
   TOTAL: ~49-50 KB per sleeping region at K=16
   ```

   DMD storage is comparable to one full region snapshot at this scale. The primary win is computational and historical compression: no O(m) replay stepping for long sleeps, and no need to store long raw trajectories (`N × M` snapshots).

   **Decomposition compute cost (authoritative path):** Truncated SVD + real Schur on `(N × M)` snapshots: ~O(NMK) flops for retained rank K. For N=1536 (256 bricks × 6 fields), M=128, K=16: ~3M flops for the dominant term. Run asynchronously in deterministic server worker jobs at region unload; never on the render thread.
   **Determinism is core engineering, not a footnote.** Cross-OS "bit-for-bit" with SVD/Schur
   is not something you get for free even with the same source code. Without explicit measures,
   you'll get rare "region hash drift" reports that are impossible to reproduce locally.
   Mandatory determinism rules:
   - **Fixed-iteration deterministic algorithms:** Use Jacobi SVD (fixed iteration count, not
     convergence-based) for truncated SVD, and real Schur with fixed shifts (not adaptive).
     No randomized or probabilistic variants in the authoritative path.
   - **Schur shift specification:** Use Francis double-shift QR with explicit shift pinning:
     (a) Fixed iteration count per deflation step (e.g., 30 iterations, no convergence check).
     (b) Shifts are the eigenvalues of the trailing 2x2 submatrix (Wilkinson shifts) — these
         are deterministic given the matrix entries and do not depend on convergence state.
     (c) Deflation tolerance is a fixed constant (e.g., `eps_deflate = 1e-10`), not relative
         to machine epsilon or runtime-computed norms.
     This must be pinned before Phase 10 implementation — if left unspecified, different
     compilers/platforms may produce different Schur orderings, breaking cross-OS determinism.
   - **Canonical sign and ordering rules:** Before serialization, enforce:
     (a) Schur blocks sorted by decreasing spectral radius,
     (b) for each 1x1 real block: positive sign convention on corresponding Ψ column
         (flip both Ψ column and corresponding T row/col if Ψ's largest-magnitude entry is negative),
     (c) for each 2x2 complex-conjugate block: canonical rotation direction (positive imaginary part).
   - **Quantization at defined pipeline points:** Quantize `Ψ`, `T`, and `c0` to specified
     precision (f16 for Ψ, f32 for T and c0) BEFORE hashing/snapshot emission. Quantization
     points are explicit in the code — no implicit precision loss from intermediate operations.
   - **No randomness** in the authoritative decomposition path.

   **Snapshot collection:** While a region is active, maintain a circular buffer of M=64-128 PDE state snapshots sampled every S=4-8 authoritative ticks. Total snapshot buffer: N × M × f16 = 1536 × 128 × 2 = ~384 KB per active region. Allocated from the Phase 0 uniform/storage buffer budget. Only the most recent M snapshots are retained; older ones are overwritten.

   **Authoritative boundary:** DMD reduced data is computed server-side from authoritative PDE state. Serialized `Ψ`, `T`, and `c0` are part of the authoritative region snapshot. Clients receive pre-computed reduced data when streaming in a sleeping region; they do NOT independently compute SVD/Schur. This preserves the sim_tick/render_frame determinism split.

7. **Authoritative simulation boundary (MMO-safe).**
   - All gameplay-critical field evolution (damage, fracture state, corruption used by AI/combat, ecology state transitions) runs on server-authoritative fixed ticks.
   - Client-side simulation is presentation-only interpolation/extrapolation and must never author authoritative outcomes.
   - Region hashes emitted per authoritative tick for divergence detection and replay validation.

**Memory layout (revised):**

```
Per voxel (high-res, near-surface bricks):
  distance: f32           4 bytes
  normal: [f16; 2]        4 bytes (octahedral)
  grad_mag: f16           2 bytes (gradient magnitude, required for Whitney envelope)
  curvature: f16          2 bytes
  TOTAL: 12 bytes/voxel (logical payload; physical stride may be higher)
  Optional interval channel (debug/high quality when enabled):
    upper_delta: f16      2 bytes, where u_i = b_i + upper_delta and upper_delta >= 0
    (used only when uncertainty interval features are enabled; otherwise omitted)

Per brick (low-res metadata):
  B_max: [f16; 3]         6 bytes (anisotropic derivative bound for brick AABB; f16 MUST round toward +inf to stay conservative)
  L_max_fallback: f16     2 bytes (same rounding rule)
  NOTE ON f16 ROUNDING — THIS IS A PROOF OBLIGATION, NOT AN OPTIMIZATION DETAIL:
  GPU hardware uses IEEE 754 round-to-nearest-even (RNE), which can round DOWNWARD and
  silently break conservatism. For ANY channel that is a bound (lower or upper),
  serialization is a proof obligation: the converted value must be conservative in the
  correct direction.

  MANDATORY: A single conversion utility handles ALL bound serialization:
  ```rust
  /// Conservative f16 conversion. Direction-aware, invariant across CPU targets.
  /// This is the ONLY code path that converts bound values to f16.
  fn f16_conservative(value: f32, direction: BoundDirection) -> u16 {
      match direction {
          BoundDirection::Upper => f16_round_toward_pos_inf(value),  // bounds, B, L
          BoundDirection::Lower => f16_round_toward_neg_inf(value),  // lower bounds b
      }
  }
  ```
  ALL f16 bound conversions MUST go through this utility. Direct f32-to-f16 casts
  on bound values are forbidden — they use RNE and silently break safety.

  Implementation options for the rounding core (choose one):
  (a) CPU-side conversion: convert f32 to f16 using explicit directional rounding
      (e.g. Rust `f16::from_f32_round_up` / `f16::from_f32_round_down`), upload as
      raw u16 bits. GPU reads as f16 — no further rounding. Zero runtime cost.
  (b) Integer-encoded bounds: store as u16 fixed-point (e.g. 8.8 format,
      range [0, 255.996]) and decode in shader: `B = float(B_u16) / 256.0`. No
      floating-point rounding at any point. Costs one multiply per decode.
  (c) Epsilon pad: after f16 conversion, add 1 ULP (unit in the last place).
      Wastes ~0.1% stepping tightness but is trivially correct.
  Option (a) is preferred: zero runtime cost, exact conservative value.
  material_id: u8         1 byte
  corruption: f16         2 bytes
  moisture: f16           2 bytes
  temperature: f16        2 bytes
  growth: f16             2 bytes
  biome: f16              2 bytes
  irradiance_sh: [f16;12] 24 bytes (L1 SH)
  edit_list_ptr: u32      4 bytes
  epoch: u32              4 bytes
  dirty_flags: u32        4 bytes
  TOTAL: 55 bytes/brick

Per brick (stress, ONLY near damage, sparse ~5%):
  damage: f16             2 bytes
  fracture_dir: [f16; 2]  4 bytes (octahedral)
  TOTAL: 6 bytes/brick
```

16^3 bricks x 12 bytes/voxel = 48 KB/brick logical payload.
2048 bricks x (48 KB + 55 bytes) ~ 96 MB logical raw; budgeted at ~98 MB with per-brick field/metadata headroom (Phase 0: 96 MB + 2 MB). Physical allocation must be validated on target backend and remain within profile budget.

**What's built:**
- Multi-res field channels (per-voxel distance, per-brick slow fields)
- PDE diffusion engine: Jacobi iteration for slow fields at per-brick resolution
- Reaction-diffusion coupling (corruption<->growth, moisture<->temperature, corruption<->irradiance)
- Stochastic PDE noise: Langevin term with per-field sigma_noise parameter, deterministic RNG per (region, epoch, field)
- Irradiance PDE (screened Poisson): light diffusion with corruption absorption
- Event-driven fracture (impulse -> damage -> crack edit -> propagation test)
- Topology changes via edit composition
- Healing via edit age fadeout + growth -> regrowth edits
- Cross-modal parameter space (field values -> rendering/audio params)
- Region epoch system (prepare for Phase 12)
- Authoritative tick pipeline for gameplay-critical fields + per-region hash emission
- DMD spectral sleep system: snapshot collection, truncated SVD + real Schur reduced-state extraction, spectral-radius stability clamping
- Tiered catch-up dispatcher: Tier 1 (full PDE), Tier 2 (DMD + corrective steps), Tier 3 (DMD + stochastic reseed)
- Drift monitoring: per-region prediction quality tracking

**Parallel lanes:**
- Lane A: Multi-res field channels + memory layout (independent)
- Lane B: PDE diffusion engine — Jacobi compute shader (independent)
- Lane C: Reaction-diffusion coupling rules (depends on B)
- Lane D: Irradiance PDE — screened Poisson + light source injection (depends on B)
- Lane E: Irradiance-corruption coupling (depends on C, D)
- Lane F: Event-driven fracture mechanics (independent)
- Lane G: Topology changes from field thresholds (depends on A, B)
- Lane H: Healing via edit fadeout + growth dynamics (depends on C)
- Lane I: Cross-modal parameter bindings (depends on A)
- Lane J: DMD spectral sleep — snapshot buffer, deterministic truncated SVD + real Schur decomposition job, reduced-state storage, spectral-radius clamping (depends on B, C)
- Lane K: Tiered catch-up dispatcher — tier selection logic, corrective PDE pass, stochastic reseed, drift metric (depends on J, B)
- Lane L: Integration + gameplay wiring (depends on all)
- Lane M: Review (depends on L)

**AC:**
- Corruption visibly diffuses from sources
- Corruption blocked by dense materials
- Irradiance PDE: light source brightens surroundings
- Corruption-light coupling: bright areas resist corruption, corrupted areas darken
- Sword strikes produce directional cracks (fracture mechanics)
- Vegetation regrows in moist uncorrupted areas
- Stochastic PDE: with sigma_noise > 0, reaction-diffusion coupling produces visible emergent spatial patterns (spots, patches, or rings) that do not appear when sigma_noise = 0
- Stochastic PDE: sigma_noise = 0 produces bit-for-bit deterministic output matching non-stochastic baseline
- Stochastic PDE: authoritative noise is seeded deterministically per (region, epoch, field) — reproducible across runs
- Simulation within sim_update_budget_ms
- Memory within budget
- Single parameter change -> coherent cross-modal response
- Gameplay-critical field state advances only on authoritative server ticks
- Replay corpus: region hashes match bit-for-bit across supported server OS builds
- Client visual stochasticity does not alter authoritative gameplay state
- DMD: deterministic decomposition (fixed-iteration Jacobi SVD + fixed-shift real Schur) runs asynchronously on server unload path with p95 <= 25ms per region and no render-thread stalls
- DMD: cross-OS bit-for-bit determinism verified — identical region hashes for same snapshot input on all supported server OS builds (fixed-iteration algorithms + canonical sign/ordering + quantization at defined points)
- DMD: Schur-block spectral-radius clamping prevents divergence — no sleeping region produces NaN/Inf on wake
- DMD: Tier 2 catch-up (1000-epoch skip on benchmark region) produces drift < 0.3 after 16 corrective PDE steps
- DMD: Tier 3 catch-up (10K+ epoch skip) produces macroscopically plausible state (correct biome winner, corruption distribution within 20% of full-sim reference)
- DMD: strongly nonlinear regions (>50% clamped blocks) disable Tier 3 and escalate Tier 2 (`K` up to 32, up to 64 corrective steps) without wake-path stalls
- DMD: snapshot buffer fits within uniform/storage buffer budget
- DMD: reduced-state storage per sleeping region <= 64 KB for default `K<=16`; nonlinear escalation path (`K<=32`) <= 112 KB
- DMD: mode collapse detector triggers Δt_snap halving before K escalation when drift is repeatedly high (>0.3 on 3+ consecutive wakes)

---

## Phase 11: Emotional Rendering

**Purpose:** Visual style transforms with game state. Calm watercolor -> desperate charcoal -> crystallized impact painting. Orchestration of prior phases into a unified mood-reactive presentation.

(Detailed design unchanged from v1 — mood state machine, temporal crystallization, manga effects, audio-visual coupling.)

**What's built:**
- Mood state machine: emotional valence drives rendering parameters
- Temporal crystallization: time-stop painterly accumulation on dramatic beats
- Manga panel effects: radial lines, speed lines, impact frames as post-process
- Audio-visual coupling: combat intensity -> visual intensity mapping
- Style parameter interpolation: smooth transitions between mood palettes
- Smear frame intensity modulation: mood state drives swept-volume shutter interval width (Phase 6). Calm = no smear, combat = moderate (1-2 frame shutter), dramatic peak = exaggerated manga smear (3-4 frame equivalent shutter). Uses the same spacetime Lipschitz smear system — just controls the `[t0, t1]` interval.

**Parallel lanes:**
- Lane A: Mood state machine + parameter routing (independent)
- Lane B: Temporal crystallization shader (depends on Phase 6 painterly)
- Lane C: Manga panel post-process effects (independent)
- Lane D: Audio-visual coupling bindings (depends on Phase 7 audio)
- Lane E: Style interpolation + profile integration (depends on A)
- Lane F: Integration + tuning (depends on all)
- Lane G: Review (depends on F)

**AC:**
- Mood transitions produce visible style changes (palette shift, stroke weight, contrast)
- Temporal crystallization freezes and accumulates on dramatic hits
- Manga effects trigger on high-impact combat events
- Style interpolation is smooth (no popping between moods)
- All mood parameters exposed as quality-scalable knobs
- 60fps on Medium profile maintained during mood transitions

---

## Phase 12: Infinite World — Virtualized Brick Address Space

**Purpose:** Transform arena-scale engine into Minecraft-style infinite world. Brick pool becomes virtualized world address space with content-addressed pages.

**Key concept: "Git for Bricks"**

Every brick identified by stable **world key**: (level, brick_coord_xyz), not GPU pool slot. Pool is a cache. Key is identity.

```rust
struct BrickKey {
    level: u8,
    coord: [i32; 3],
}

struct BrickBlob {
    hash: u128,  // collision-resistant content address (e.g., BLAKE3-128)
    data: Vec<u8>,
}

// World map: key -> hash -> blob
// Unmodified: hash computed from procedural generation (deterministic, never stored)
// Modified: hash -> blob in persistence layer
// Hashing policy: use deterministic collision-resistant hash (BLAKE3-128 preferred).
```

**Why this matters:**
- Infinite streaming: evict far, generate near. Memory constant.
- Persistence without unbounded growth: only MODIFIED bricks store blobs. Unmodified regenerated from seed.
- Deduplication: identical bricks (common in procedural worlds) store one blob.
- Deterministic replay: record edit mutations. Base layer always regenerable.
- Multiplayer foundation: two players' edits are branches. Merge = compose edit lists with total ordering (Lamport timestamps per edit). CSG composition is NOT commutative (subtract-then-add != add-then-subtract), so edit order must be deterministic across clients.
- Authoritative networking boundary: server resolves region state and emits canonical deltas/snapshots; clients apply and reconcile. Clients never resolve conflicting edits authoritatively.
- Modding: share world modifications as compact brick diffs.

**Biome hierarchy for long-range structure:**
```
continent_type(seed, region_coord / 4096)  -> temperate / tropical / arctic / corrupted
  region_type(continent, region_coord / 512) -> ancient forest / volcanic / coastal / ruins
    biome(region, brick_coord / 64)          -> cathedral redwoods / moss valley / fungal grove
      micro_biome(biome, brick_coord)        -> individual brick generators + parameters
```

**Region epochs for living-world-at-scale:**
- Near players: full simulation frequency (Phase 10 Jacobi every authoritative tick)
- 1-2 regions away: reduced frequency (every 10 ticks)
- Far: freeze + DMD spectral sleep (Phase 10, item 6). On region unload, compute truncated SVD + real Schur reduced model and store `(Ψ, T, c0)`. On revisit, use tiered catch-up:
  - Tier 1 (< 100 epochs): full PDE stepping
  - Tier 2 (100-10K epochs): DMD prediction + corrective PDE steps
  - Tier 3 (> 10K epochs): DMD macro prediction + stochastic reseed of fine structure
- **Geological time:** returning after many epochs shows dramatic change. Reduced DMD dynamics capture dominant macroscopic evolution (which biome expanded, how corruption retreated, moisture equilibrium), while corrective steps and procedural reseed restore plausible microstructure

**What's built:**
- Virtualized brick address space (world key -> pool slot)
- Content-addressed persistence (brick hash -> blob storage)
- Brick streaming: generate ahead, evict behind
- Biome hierarchy (continent -> region -> biome -> micro-biome)
- Region epoch system with DMD tiered catch-up (integrates Phase 10 DMD sleep data)
- Persistence layer: save/load modified blobs + region headers + DMD reduced-state data `(Ψ, T, c0)`
- LRU memory management
- Authoritative region replication format (snapshot + delta + region hash)
- Deterministic merge/replay pipeline for ordered edit streams

**AC:**
- Walk indefinitely without running out of world
- Memory stays within budget regardless of distance
- Return to visited area: modifications persist
- Biome transitions visible
- Unvisited regions show time-advanced state (DMD tiered catch-up produces plausible results)
- Tier 2 catch-up (1000 epochs, non-flagged regions): region wakes within 50ms on Medium profile (reduced-model eval + 16 corrective steps)
- Tier 3 catch-up (10K+ epochs, non-flagged regions): region wakes within 20ms (reduced-model eval + stochastic reseed, no corrective stepping)
- Strongly nonlinear regions (>50% clamped Schur blocks): Tier 3 disabled; wake path uses escalated Tier 2 with bounded per-tick budget (no hard stall), converging over subsequent authoritative ticks
- DMD reduced-state data persists across save/load alongside brick blobs
- Save/load preserves modifications
- Generation keeps up with player movement speed
- Concurrent edits from multiple players resolve identically on all clients (server-authored order)
- Region snapshot+delta replay reproduces identical region hashes

---

## Phase 13: Ecology + Evolution

**Purpose:** Enemies evolve via Darwinian selection driven by player behavior. Ecosystems self-regulate on PDE substrate.

**Changes from v1:**

1. **Curated seed genomes per archetype.** Random initial genomes produce garbage. Each species starts from hand-authored seed genome that looks good. Evolution drifts FROM this.

2. **Diversity-preserving selection.** Standard tournament selection collapses to one optimum. Add MAP-Elites style diversity: population covers a behavior/morphology space, new genomes rewarded for exploring unoccupied regions.

3. **Inverse design ("Draw Your Enemy").** Field anatomy graph is differentiable-ish (primitives + smooth CSG with known gradients). Given target silhouette (player sketch), optimize AnatomySpec via CMA-ES on ~40-60 float parameter space. Result becomes seed genome for new species.

4. **Lyapunov stability monitoring (polish pass).** Simple heuristics for v1 (variance damping, minimum viable population, mutation clamping). Polish: compute Jacobian of population update, estimate largest Lyapunov exponent per region. Surface as player-facing signal: "evolutionary pressure: HIGH" where lambda_1 is large. Emergent narrative from dynamical systems theory.

(Rest unchanged — genomes, mutation, selection, population dynamics, speciation, extinction, ecosystem sim, anatomy/PDE/audio integration.)

**Additional AC:**
- Each species has curated seed genome that looks intentional
- Diversity: population covers morphology space (not all converged)
- Inverse design: player sketch -> creature in world within 2-3 generations

---

## Phase 14: Conservative Formal Guarantees

**Purpose:** Wrela compiler proves conservative bounds at compile time.

**Critical change from v1:** Prove conservative derivative/resource bounds, not unit-gradient idealism. Affine arithmetic (not just intervals).

**Why affine arithmetic over intervals:**
Intervals track [lo, hi]. When variables correlate (x - x should be 0, not [-2, 2]), intervals explode. Affine arithmetic: x_hat = x_0 + sum(x_i * epsilon_i) where epsilon_i are noise symbols in [-1, 1]. Same symbol in multiple variables tracks correlation.

```
a in [-1, 1]             ->  a_hat = 0 + 1*e1
b = a * 2.0              ->  b_hat = 0 + 2*e1                       (b in [-2, 2])
c = b - a                ->  c_hat = 0 + (2-1)*e1 = 0 + 1*e1       (c in [-1, 1], correct!)
                              vs naive interval: b - a = [-2,2] - [-1,1] = [-3, 3]  (3x too wide!)
                              true range: c = 2a - a = a in [-1, 1]
```

Reduces false positives by 50-90% vs intervals alone.

**What compiler proves:**
1. **Derivative bounds:** propagate anisotropic `B=(Bx,By,Bz)` through field graph operators.
2. **Directional march safety preconditions:** verify each generated march kernel uses:
   - canonical lower bound `b = d - epsilon` (or envelope reconstruction `b_env(p)`),
   - `L_dir(v)=dot(abs(v),B)` (or declared scalar fallback) and `L_safe=max(L_dir, epsilon_denom)`,
   - step clamp to region/cell exit.
3. **Envelope optimality invariant:** where envelope reconstruction is used, verify stencil coverage and per-certificate norm consistency:
   - L1 certificate: `D_B` with weighted-L1 directional bound from `B`
   - L2 certificate (LCP): `D_L2` with scalar Euclidean bound `L`
   For fused stepping, compiler verifies each certificate independently, then allows `b_fused=max(b_L1,b_L2)` and `L_dir*=min(L_dir_L1,L_dir_L2)`.
   The McShane extension theorem applies per certificate given these preconditions.
   - **Cone-union stepping precondition:** where cone-union acceleration is used, verify that all stencil intervals are computed from a bound valid over the full stencil support (`B_cell` for in-cell stencil, `B_region` for expanded stencil), and that the activation gate is applied to the conservative baseline `step_std = min(max(0,b_env)/L_safe, cell_or_region_exit)` with threshold `step_std < 0.8 * cell_or_region_exit`.
3b. **Norm hygiene as type-level constraint:** Every bound value in the IR carries a norm tag (`WeightedL1(B)`, `ScalarL`, `L2`, `Untagged`). The compiler rejects operations that mix incompatible norm types (e.g., subtracting an L2 epsilon from a WeightedL1 bound). Norm tags are erased at codegen — zero runtime cost. This prevents the subtle "L2 epsilon with L1 denominator" bugs that survive months and manifest as camera-angle-dependent holes.
3c. **Dimensional type safety for spacetime bounds:** When spacetime Lipschitz operations are used (Phase 9 CCD, Phase 6/11 smear frames), the IR carries dimensional tags on bound components: `Spatial(meters)` for Bx/By/Bz, `Temporal(seconds)` for Bt. The compiler rejects:
   - Adding a spatial bound to a temporal bound without the trajectory Jacobian (`L_path = dot(abs(v), B_spatial) + Bt` is valid because `v` has units of meters/second, making each term dimensionally consistent as `1/seconds`)
   - Using Bt in a purely spatial stepping context (and vice versa)
   - Passing raw `Bt` where `L_path` is expected (must go through `safe_step_spacetime_along_path`)
   This prevents the subtle "seconds mixed with meters" bugs that silently destroy conservatism in CCD codebases. Dimensional tags erased at codegen — zero runtime cost.
3d. **Spacetime region-validity precondition:** For spacetime stepping, verify generated kernels clamp time advance by the bound-valid interval (`Δt <= dt_to_bound_region_exit`) in addition to `dt_remaining`. `B_spatial` and `Bt` must both be valid over the same active spacetime region used for the step. If provenance/validity is missing, reject spacetime path or fall back to non-certified collision path for that op.
4. **Scalar fallback consistency:** if `L_fallback` is derived from anisotropic `B`, enforce allowed mappings (`||B||_2` preferred, `||B||_1` allowed). Reject `||B||_inf`/`max(B)` as sole denominator mapping for directional stepping.
5. **Resource bounds:** iteration count <= budget for generated loops.
6. **Parameter ranges:** outline width in [0.5, 4.0], blend radius > 0, etc.
7. **Escape hatches:** `assume_bound(Bx,By,Bz)`, `assume_lipschitz(L)`, `assume_bt(Bt)`, or `assume_cost(C)` for unanalyzable ops.
8. **LCP certificate consistency:** When dual-envelope stepping is used, verify that `L` (Euclidean) and `B` (anisotropic) are both region-valid with tracked provenance. Verify fusion rule: `b_fused = max(b_L1, b_L2)` uses compatible metric — L2 envelope uses `L`, L1 envelope uses `B`, both over the same stencil and region. Verify `fused_directional_bound = min(L_dir_L1, L)` — compiler checks that `L` is not loosely inflated (provenance must not be `Unknown`). Reject LCP stepping if either certificate has `Unknown` provenance (fall back to L1-only).
9. **Bernstein certificate verification:** When `AnalyticBernstein` provenance is claimed for a noise operator, verify the Bernstein conversion is well-formed: polynomial degree matches improved (gradient) Perlin (degree 6 per axis — quintic fade × linear gradient dot), domain is a unit lattice cell, and derivative bounds are computed from degree-5 Bernstein coefficients (derivatives of degree-6 polynomial). For value noise (degree 5), derivative coefficients are degree-4. Reject `AnalyticBernstein` provenance for non-polynomial noise functions (e.g., Worley/Voronoi — these need `Sampled` provenance).
10. **R-function rewrite rule safety:** R-functions do NOT satisfy associativity or distributivity of standard boolean algebra. The compiler MUST NOT apply algebraic rewrite rules that change CSG tree structure (e.g., flattening `r_union(a, r_union(b, c))` to `r_union(a, b, c)` is safe ONLY if using the same R-function form). Any IR optimization pass that restructures operator trees must be verified against the R-function derivative identity for the specific R-function variant used. Flag rewrite rules that change derivative behavior at the `a=b=0` locus.

**What compiler does NOT try to prove:**
- |grad(f)| = 1 (impossible with warps/blends/sampling)
- Exact visual appearance
- Float precision

**AC:**
- Anisotropic derivative bounds provable for primitive and CSG operations
- Warp Jacobian-bound propagation (`B_out = A^T * B_in`) implemented and verified on reference operators
- Generated march kernels enforce `step <= distance_to_active_region_exit`
- Warps/noise correctly flagged as raising scalar fallback `L` or component bounds `B`
- Resource bounds enforced
- Affine arithmetic reduces false positives >50% vs interval-only
- Envelope optimality preconditions verified: stencil coverage + norm consistency (weighted L1)
- Norm hygiene: IR bound values carry norm tags; mixed-norm operations rejected at compile time
- Dimensional type safety: spacetime bound values carry spatial/temporal dimension tags; mixed-dimensional operations rejected at compile time (e.g., raw Bt in spatial stepping context)
- Spacetime kernels enforce `Δt <= dt_to_bound_region_exit` and require shared validity scope for `(B_spatial, Bt)`
- Scalar fallback derivation from B validated against allowed norm mappings
- LCP dual-envelope stepping: compiler verifies both certificates have region-valid provenance before allowing fused stepping; `Unknown` provenance rejects to L1-only
- Bernstein certification: compiler verifies polynomial degree, domain, and derivative-bound construction for `AnalyticBernstein` provenance claims; rejects non-polynomial noise
- R-function rewrite safety: IR optimizer respects R-function tree structure; restructuring rewrite rules verified against derivative identity for specific R-function variant
- Escape hatches work
- Proof pass <1 second per recipe
- False-positive rate on valid recipe corpus <=1%

---

## Phase 15: Temporal Archaeology (Moonshot)

**Purpose:** Content-addressed world history from Phase 12 enables navigable world history.

**What's built:**
- **Region timelapse replay:** Select a region, play back epoch history as fast-forward visualization. For regions with stored reduced dynamics, timelapse uses Schur-block continuous interpolation over snapshot interval (`x(t) = Ψ exp(Ω τ) c_0`, `τ = t / Δt_snap`, presentation-only) to render smooth intermediate states at arbitrary frame rate, decoupled from authoritative tick rate. For regions with full edit history, replay actual epoch snapshots.
- **Geological time as gameplay:** Return after long absence -> "what happened here" cinematic replaying epoch history at accelerated speed.
- **Forensic mode:** Trace any world feature back to its cause. "Why is this cracked?" -> trace edit history -> "heavy attack 47 epochs ago, crack propagated along principal stress, corruption suppressed healing." Note: forensic tracing uses the causal edit history (Phase 12 content-addressed persistence), NOT DMD backward extrapolation. DMD backward evaluation (`t < 0`) diverges from the true nonlinear history and must NOT be used for causal attribution.
- **DMD spectral visualization:** Expose dominant reduced-spectrum diagnostics as player-facing signals. Growth/decay and cyclic behaviors are derived from Schur blocks (or equivalent diagnostic `ω` values using `Δt_snap = S * Δt_tick`). Surface as "evolutionary pressure" or "ecological stability" indicators.
- **Emergent narrative:** Epoch + evolution + PDE history + DMD spectral character = world story no one authored.

**AC:**
- Timelapse of region epoch history plays smoothly
- DMD-based timelapse renders intermediate states at arbitrary t without stepping
- Player can identify "what happened here" from forensics (edit history, NOT DMD backward extrapolation)
- World history persists across save/load

---

## Phase 16: Hyperfidelity WebGPU Renderer (Novel Fidelity Track)

**Purpose:** Push raw visual fidelity beyond typical AAA stylized output while staying procedural and WebGPU-native. This phase is not about new gameplay systems; it is about extracting maximum image quality from the existing field/spectral/painterly stack using advanced sampling, reconstruction, and scheduling.

**What makes it novel:** Use the field engine's conservative bounds, stored curvature, and deterministic simulation state to drive adaptive rendering decisions that are hard to do in mesh-first engines.

### Mathematical Specification

#### 16a) Wavefront Compaction for Implicit Ray March

Per ray state:
`RayState = {pixel_id, origin, dir, t, throughput, B_local: [f16; 3], has_B_local: bool, L_fallback: f16, active_flag, rng_state}`

Iteration `k`:
1. Evaluate field bound at `p = origin + t * dir`: get `b_k`, `B_k` (anisotropic) or `L_k` (scalar fallback).
2. Compute denominator:
   - if `has_B_local`: `L_dir_k = dot(abs(dir), B_k)`
   - else:             `L_dir_k = L_fallback`
   - `L_safe_k = max(L_dir_k, 1e-6)`
3. Advance `t <- t + min(max(0, b_k)/L_safe_k, distance_to_region_exit)`.
4. Mark done if hit/miss/max_steps.
5. Prefix-sum `active_flag` to compact surviving rays into next queue.

Queue model:
`Q_{k+1} = compact(Q_k where active_flag = 1)`

Termination:
- `Q_k` empty, or
- global max steps reached.

Implementation note: prefix-sum + scatter done in compute. Indirect dispatch on `|Q_k|` each iteration.
Adaptive activation policy (to avoid compaction overhead on easy scenes):
- Track EWMA of `DivergenceProxy` over last 32 frames.
- Enable compaction when `EWMA_DivergenceProxy > 0.35` AND `active_rays_peak > 131072`.
- Disable compaction when `EWMA_DivergenceProxy < 0.20` OR `active_rays_peak < 32768`.
- When enabled, compact every `N=2` march iterations by default (tunable per profile).
- Control ownership: these EWMA gates are the Phase 16 mode of the shared `DivergenceController` introduced in 2b (same thresholds/activity gates); 2b raw-consecutive hysteresis remains the fast-response mode.

#### 16b) Reservoir-Guided Direct Lighting (ReSTIR-style)

For a shading point `x`, candidate light sample `y_i` drawn from proposal `q(y)`:
- Define unnormalized target density:
  `p_hat(y) = L_e(y -> x) * f_s(x, omega_i, omega_o) * G(x,y) * V(x,y)`
- Candidate weight:
  `w_i = p_hat(y_i) / q(y_i)`
  (all terms must use a consistent sampling measure; convert area/solid-angle PDFs with the correct Jacobian before weighting)
- Reservoir maintains `(Y, W, M)`:
  - `W <- W + w_i`
  - replace representative sample `Y <- y_i` with probability `w_i / W`
  - `M <- M + 1`

Shading estimator (implementation options):
- **Biased low-variance mode (gameplay default):** standard temporal/spatial reservoir reuse with normalization clamps.
- **Unbiased validation mode:** evaluate with unbiased normalization and compare against reference path for regression tests.

Temporal/spatial reuse:
- Merge previous-frame reservoir (motion-compensated) and neighbor reservoirs in screen tile.
- Conservative visibility test against field bounds before accepting reused sample.

Estimator target:
- On benchmark suite, variance is lower than per-pixel independent light sampling at matched frame budget.

#### 16c) Directional Radiance Cache Extension

Baseline remains L1 SH per brick. Hero regions add sparse spherical-Gaussian lobes:

`L(omega) ~= sum_j a_j * exp(lambda_j * (mu_j dot omega - 1))`

where:
- `a_j` = RGB amplitude,
- `mu_j` = lobe direction,
- `lambda_j` = sharpness.

Update rule (incremental):
`theta <- (1 - eta) * theta + eta * theta_new`
for cache parameter vector `theta`.

Use SH for diffuse fallback, SG lobes for higher-frequency indirect/specular response near saliency regions.

#### 16d) Spectral Stochastic Super-Sampling

Estimator combines hero-wavelength and basis-space evaluation with temporal accumulation.

For frame `t`, pixel `p`:
`C_t(p) = (1 - alpha_t(p)) * C_{t-1}(reprojected p) + alpha_t(p) * C_new(p)`

Adaptive blending:
- `alpha_t` increases on disocclusion/high motion/high shading residual.
- `alpha_t` decreases on stable regions for noise reduction.

Wavelength budget:
- allocate higher hero-wavelength count in high-saliency/high-iridescence regions.
- keep low count in stable low-saliency regions.

#### 16e) Field-Native Silhouette Super-Resolution

Edge confidence map:
`E = w_n * |nabla depth| + w_nrm * |nabla normal| + w_k * |curvature|`

Reconstruction:
- anisotropic edge-aware upsample where filter axis aligns with tangent direction to preserve line continuity.
- reject history when edge orientation flips or confidence drops below threshold.

#### 16f) Perceptual Budget Allocator

Define saliency score per tile `s_i` and quality-response `q_i(b_i)` for budget allocation `b_i`.
Solve per frame:

maximize `sum_i s_i * q_i(b_i)`
subject to `sum_i b_i <= B_ms`, `b_i >= 0`

Practical solver:
- assume `q_i(b_i)` is monotone-concave; then greedy by marginal gain `d(s_i*q_i)/db_i` is near-optimal and stable.
- hard floor for gameplay-critical center/focus tiles.

### Novel Algorithmic Components

1. **Bound-aware reservoir reuse**
- Reused light samples are gated by conservative field visibility bounds (not just depth reprojection), reducing ghosted lighting under fast topology change.

2. **Curvature-conditioned reconstruction**
- Uses stored field curvature (from Phase 2 bricks) as a reconstruction prior for silhouette and outline stability.

3. **Saliency tied to combat semantics**
- Saliency map combines visual cues plus gameplay semantics (active hitboxes, face region, weapon contact zone) so quality follows player attention and gameplay value.

4. **Dual-mode fidelity output**
- Real-time mode obeys profile budgets.
- Cinematic accumulation mode uses the same pipeline with increased sample and accumulation budgets, preventing separate "offline renderer" divergence.

5. **Neural radiance cache (field-aware)**
- Tiny MLP (4 layers, 32 wide, ~200 multiply-adds per pixel) learns to predict indirect radiance from `(position, normal, roughness, time_encoding) -> L_indirect` as SH coefficients or SG lobes.
- Training signal: actual cone trace results computed at reduced rate (Phase 2b GI).
- Inference: every pixel every frame, replacing expensive cone traces for cached regions.
- **Lipschitz-driven invalidation:** `invalidation_priority(brick) = B_max * edit_age * screen_coverage`. Bricks with large B (rapidly changing fields) invalidate cache entries faster. Bricks with small B (stable geometry) keep entries for many frames. The Lipschitz infrastructure becomes the cache coherence system.
- Novel angle: no existing neural radiance cache uses field derivative bounds for invalidation scheduling.

6. **Gaussian splatting for vegetation (hybrid rendering)**
- Solid geometry (characters, terrain, structures) rendered via field engine. Vegetation and atmospheric effects rendered via 3D Gaussian splats fitted to brick field geometry during population.
- Per-blade/leaf: a few oriented Gaussians with translucency. Wind drives Gaussian positions via the Kolmogorov turbulence field.
- Constraint: Gaussians cannot penetrate solid field surfaces — field distance provides the barrier.
- Solves the "thin translucent geometry" problem that implicit fields handle poorly (grass, ferns, moss, leaves are thin, numerous, and highly translucent).
- Differentiable: Gaussians can be backpropagated through the rendering to optimize toward the spectral cel shader output — they learn the anime aesthetic.

7. **DEC Navier-Stokes ink wash (Phase 6 upgrade)**
- Upgrade Phase 6's 2D ink wash simulation from standard finite differences to Discrete Exterior Calculus (DEC) formulation.
- Velocity as 1-form, vorticity as 2-form, pressure as 0-form. Incompressibility via codifferential `d*ω¹ = 0`.
- DEC operators exactly satisfy `d∘d = 0`, `d*∘d* = 0` — no numerical diffusion of vorticity, exact incompressibility, exact energy conservation (inviscid case).
- Result: ink flows in sharp coherent streams (not blurry smears), develops genuine vortices at corners, maintains fine tendrils far from source. Looks like actual sumi-e brush technique.
- Boundary conditions from field depth/normal buffers — ink can't flow through surfaces.

8. **Spatiotemporal blue noise reconstruction (effective 4x performance)**
- Run expensive field renderer at half resolution. Reconstruct to full resolution using a small learned reconstruction filter (5x5 spatiotemporal neighborhood).
- Blue noise distributed optimally in `(x, y, t)` places rendering error in frequency bands where the human CSF has lowest sensitivity (high spatial + high temporal frequency).
- Lipschitz bounds drive adaptive sample allocation: high-B regions (rapidly changing appearance) get more samples, low-B regions (stable) get fewer.
- Effective 4x performance multiplier on the renderer with perceptually lossless quality.

9. **Field-exact temporal supersampling (anime motion clarity)**
- Motor transform differentials give exact 3D velocity at every surface point: `v_world(p) = motor_differential(p, t)`. Project to screen space for ground-truth motion vectors.
- No approximation, no mesh normal discontinuities. Perfect reprojection at every silhouette.
- Anime-specific: near silhouettes and at high motion, blend toward the current frame's edge-enhanced version instead of history — keeps silhouettes sharp during motion (characteristic anime clarity where characters are always readable even mid-combo).
- Blend factor driven by: disocclusion (exact from field depth), motion magnitude, field boundary proximity (near silhouettes = less history = preserve sharpness).

### Benchmark and Metric Definitions

Use fixed benchmark paths/scenes for all Phase 16 claims:
- `bench_forest_combat_path_A` (fast camera pans + melee clashes)
- `bench_many_lights_path_B` (16/32/64/128 dynamic lights)
- `bench_hero_closeup_path_C` (faces, hair, thin outlines)

Metrics:

1. **Temporal edge shimmer** (lower is better)
- Compute Canny edge maps on tone-mapped luma for frame `t` and motion-compensated `t-1` (same post-tonemap domain each run).
- `Shimmer = mean_p |E_t(p) - E_{t-1->t}(p)|`
- Report relative reduction vs pre-Phase-16 baseline.

2. **High-frequency detail retention** (higher is better)
- Edge-energy metric over Laplacian pyramid levels:
- `Detail = sum_{levels>=Lh} ||LaplacianLevel||_1 / N_pixels`
- Compare at equal frame budget.

3. **Divergence proxy (WebGPU-practical)** (lower is better)
- Per march iteration `k`, instrument:
  `ActiveRatio_k = active_rays_k / dispatched_threads_k`
- Aggregate:
  `DivergenceProxy = 1 - mean_k(ActiveRatio_k)`
- Track before/after wavefront compaction.
- Optional native builds may additionally report hardware occupancy counters; browser WebGPU CI uses this proxy metric.

4. **Lighting scaling exponent** (lower is better)
- Fit `T_lighting(L) = a * L^beta + c` over light counts `L`.
- Require `beta < 1` for sublinear scaling target.

5. **Ghosting error** (lower is better)
- Against disocclusion mask `D`, evaluate:
- `Ghost = mean_{p in D} |C_t(p) - C_new(p)|`

Implementation note: store metric capture scripts and scene seeds with deterministic replay harness to keep regressions attributable.

**What's built:**
- **Wavefront ray-march compaction (full):**
  - Promote Phase 2b lightweight coherence/mini-compaction path to a full queue-based wavefront pipeline.
  - Active rays compacted each iteration via prefix-sum + indirect dispatch.
  - Significantly reduces warp divergence in heavy scenes.
- **Reservoir-guided direct lighting for implicit fields:**
  - ReSTIR-style light sampling adapted to field intersections.
  - Temporal + spatial reservoir reuse with conservative visibility checks.
  - Many-light scenes without linear per-light shading cost.
- **Directional radiance cache upgrade:**
  - Keep per-brick SH for baseline propagation.
  - Add sparse anisotropic directional lobes (spherical Gaussian set) in hero regions for higher-frequency indirect/specular response.
  - Selectively enabled by quality profile and camera saliency.
- **Spectral stochastic super-sampling:**
  - Hero-wavelength sampling + temporal reconstruction for thin-film and fluorescence highlights.
  - Adaptive wavelength count based on motion/saliency to avoid chromatic shimmer.
- **Field-native silhouette super-resolution:**
  - Curvature/normal-guided edge reconstruction pass.
  - Subpixel line stability for outlines and high-contrast anime edges during motion.
- **Perceptual budget allocator:**
  - Runtime ms allocator shifts budget toward high-saliency regions (faces, focal combat zone, silhouette boundaries).
  - Degrades low-saliency regions first (far field, low contrast) to preserve perceived sharpness.
- **Cinematic accumulation mode (optional):**
  - Progressive refinement mode for photo mode/replays/cut-ins.
  - Uses extra temporal accumulation and higher sample counts to produce "hero frames" beyond real-time quality.
- **Neural radiance cache:**
  - Tiny online-trained MLP for indirect radiance prediction.
  - Lipschitz-driven invalidation from brick B metadata.
  - Replaces expensive cone traces in cached stable regions.
- **Gaussian splatting for vegetation:**
  - Hybrid field + splat rendering pipeline.
  - Gaussian fitting during brick population, wind-driven position updates.
  - Field-constrained to prevent surface penetration.
- **DEC ink wash upgrade:**
  - Replace Phase 6 finite-difference ink simulation with DEC formulation.
  - Exact vorticity/energy conservation for sharper ink behavior.
- **Spatiotemporal blue noise reconstruction:**
  - Half-res render + learned reconstruction filter for effective 4x performance.
  - Lipschitz-driven adaptive sample allocation.
- **Field-exact temporal supersampling:**
  - Motor-derived exact motion vectors for perfect temporal reprojection.
  - Anime clarity mode: edge enhancement near silhouettes during motion.

**Parallel lanes:**
- Lane A: Wavefront compaction pipeline + occupancy instrumentation (independent)
- Lane B: Reservoir lighting for implicit fields (depends on A, Phase 5 lighting)
- Lane C: Directional radiance cache extension (depends on Phase 10 irradiance + Phase 12 virtualized bricks)
- Lane D: Spectral stochastic supersampling + reconstruction (depends on Phase 5)
- Lane E: Silhouette super-resolution pass (depends on Phases 2, 6, 11)
- Lane I: Neural radiance cache — MLP training + Lipschitz-driven invalidation (depends on C, Phase 2b bricks)
- Lane J: Gaussian splatting for vegetation — fitting + hybrid render pipeline (depends on Phase 2b bricks, Phase 6)
- Lane K2: DEC ink wash upgrade (depends on Phase 6)
- Lane L: Spatiotemporal blue noise reconstruction (depends on Phase 6 blue noise, Phase 2b)
- Lane M: Field-exact temporal supersampling (depends on Phase 4a motors, Phase 2b)
- Lane F: Perceptual budget allocator + saliency map (depends on A, E)
- Lane G: Cinematic accumulation mode (depends on B, C, D, E)
- Lane H: Integration + profile tuning + benchmark suite (depends on all)

**AC:**
- Medium profile gameplay remains 60fps at 1080p with fidelity track disabled by default
- High profile: >=20% reduction in temporal edge shimmer vs pre-Phase-16 baseline on benchmark camera paths
- High profile: >=15% increase in measured high-frequency detail retention (edge-energy metric) at equal frame budget
- Many-light benchmark: lighting cost scales sublinearly with light count (reservoir reuse effective)
- Wavefront compaction reduces ray-march divergence stalls on stress scene (GPU timing proof in benchmark report)
- Adaptive compaction gating works as specified: compaction off in low-divergence scenes, on in stress scenes by threshold policy
- Low-divergence benchmark overhead from compaction system (including gating checks) <=0.3ms/frame on Medium baseline hardware
- Cinematic mode produces progressively improving frames over 16-64 accumulation frames without ghosting artifacts
- All fidelity upgrades preserve authoritative simulation invariants (render-only changes, no gameplay divergence)
- Neural radiance cache: indirect GI quality matches cone-trace reference within SSIM > 0.95 at >=3x lower cost on benchmark scene
- Neural radiance cache: Lipschitz-driven invalidation responds correctly to field edits (cache refreshes within 2 frames of brick B change)
- Gaussian splatting: vegetation renders with anime-consistent outlines and wind response; no penetration through field surfaces
- DEC ink wash: vorticity conservation verified — ink tendrils maintain structure over 60+ frames (no numerical diffusion vs finite-difference baseline)
- Spatiotemporal blue noise reconstruction: SSIM > 0.97 between half-res reconstructed and full-res reference on benchmark camera path
- Field-exact temporal supersampling: zero ghosting at silhouettes during fast motion; anime clarity preserved at weapon velocities >= 15 m/s

---

## Phase Dependency Summary

```
Phase 0 --- Contracts + Infrastructure (Lipschitz, profiles, determinism, debug)
  |
  v
Phase 1 --- Anime Style Shell (cel shader, outlines, palettes, post, shadows)
  |
  v
Phase 2a -- Falsification Spike (minimal field renderer, GO/NO-GO gate)
  |
  v
Phase 2b -- Full Field Engine (brick pool, compute ray march, conservative stepping,
  |          layered edits, GI probes, cone tracing, dual contouring, re-distance)
  |
  v
Phase 3 --- Neural Field Characters + Anatomy (canonical-space via shared motor_core
  |         from Phase 2b, brick prebake, morphing)
  |
  v
Phase 4a -- Motor Transform Pipeline (global cutover: dual-quat/motor everywhere)
  |
  |---------------------------------------------------------------------+
  |                                                                      |
  v            v            v            v              v                 |
Phase 4b     Phase 5      Phase 6      Phase 7       Phase 9             |
Extended GA  Spectral     Stochastic   Procedural    Physics             |
(optional)   Materials    Painting     Audio         Animation           |
               |            |            |              |                 |
               +------------+------------+--------------+                 |
               |            |            |              |                 |
               v            v            v              v                 |
Phase 8 ----- Recipe DSL (depends on ALL of 5,6,7,9)                    |
               |                                                         |
               v                                                         |
Phase 10 ---- Living World / PDE (multi-res, irradiance, fracture)      |
               |                                                         |
               +--------+---------+--------------------------------------+
               |        |         |
               v        v         v
Phase 12    Phase 13    Phase 11
Infinite    Ecology +   Emotional Rendering
World       Evolution   (orchestrates rendering stack)
               |        |         |
               +--------+---------+
               |
               v
Phase 14 ---- Conservative Proofs (Lipschitz, resources, affine arithmetic)
               |
               +----------------------------+
               |                             |
               v                             v
Phase 15    Phase 16
Temporal    Hyperfidelity WebGPU (wavefront compaction, reservoir lighting,
Archaeology directional radiance cache, cinematic accumulation)
[moonshot]  [novel fidelity]
```

**Parallelism:**
- Phase 0 is small (~1 week) and blocks downstream implementation work. A separate cross-phase verification hook (rendering determinism) closes at first renderer integration in Phase 2a
- Phases 4b, 5, 6, 7, 9 can ALL run in parallel after Phase 4a
- Phase 8 starts after ALL of Phases 5, 6, 7, 9 (compiles field, material, anim, sound blocks)
- Phases 11, 12, 13 ALL start after Phase 10 and run in parallel. Phase 11 (emotional rendering) orchestrates the rendering/audio stack (Phases 5, 6, 7, 9 — all satisfied transitively via Phase 8→10) but does NOT require infinite world (Phase 12) or ecology (Phase 13)
- Phase 14 starts after ALL of Phases 11, 12, 13
- Phases 15 and 16 are parallel after Phase 14 (Phase 16 does NOT depend on Phase 15)
- Phase 2a (falsification spike) runs immediately after the Phase 1 style shell. Phase 2b runs only after GO decision. Phase 2b always includes lightweight coherence mitigation; if CONDITIONAL GO (or persistent divergence over budget), Phase 2b enables minimal queue compaction lanes pulled forward from Phase 16
- Within each phase, 3-14 lanes run in parallel

---

## Per-Phase Plan Generation

Each phase gets its own detailed plan document before execution:
- docs/plans/YYYY-MM-DD-phase-N-name.md
- Full task breakdown with per-step AC
- Exact file paths and code
- Test strategy (TDD, Playwright in deterministic mode)
- Commit boundaries
- Memory budget verification against Phase 0 spreadsheet

Phase 1 style-shell plan is generated first. Subsequent plans are written after prior phase ships.

---

## Success Criteria — The Full Vision

When all phases are complete:

1. **Zero predefined assets.** No meshes, textures, audio samples, or animation clips as files. Everything runtime, cached into brick volumes.
2. **AAA stylized systemic depth.** Not photorealism — anime combat quality: clean silhouettes, punchy animation, readable action, stable image. Matches Guilty Gear Strive readability. Exceeds traditional AAA in systemic depth.
3. **Mathematically grounded rendering.** Bounded implicit fields with conservative stepping via Lipschitz envelope reconstruction (`b_env(p) = max_i(b_i - D(p, x_i))`, provably optimal by McShane extension theorem per certificate) or epsilon fallback (`b = d - epsilon`). Base L1 rule: `L_dir(v)=dot(abs(v),B)`, `L_safe=max(L_dir(v), epsilon_denom)`, `step=min(max(0,b)/L_safe, distance_to_region_exit)`. With LCP, fused certificates use `b_fused=max(b_L1,b_L2)` and `L_dir*=min(L_dir_L1,L_dir_L2)` with strict per-certificate norm hygiene. Every approximation has known error bounds. Conservative guarantees apply to certified paths with valid provenance/preconditions; heuristic paths are explicitly labeled and fail-closed.
4. **Living world as continuous simulation.** Coupled PDEs (corruption, moisture, temperature, growth, irradiance) on brick substrate. Light fights corruption. Dawn heals. Damage from stress mechanics. Healing from growth dynamics.
5. **Emotional rendering.** Visual STYLE transforms with game state. Calm watercolor -> desperate charcoal -> crystallized impact painting.
6. **Neural characters from procedural anatomy.** Continuous fields in cached brick volumes. Parametric anatomy (spine -> skeleton -> field body). Canonical-space conditioning. Displacement-field morphing for physical shape-shifting (mass flows, no ghostly cross-fades).
7. **Physically correct style.** Spectral basis materials produce correct color interactions under cel shading. Fluorescence, iridescence, hue-shifting shadows from spectral math.
8. **Every frame is a painting.** Stochastic brush strokes, ink wash, paper texture via spatiotemporal blue noise. Deterministic mode for testing.
9. **Procedural audio.** Every sound synthesized from game state. Zero samples.
10. **Physics-driven animation.** IK, dynamics, springs, anime timing, field-driven contact. Spacetime Lipschitz CCD guarantees no tunneling for fast weapon/limb contacts against non-SDF fields.
11. **Procedural anime smear frames.** Swept-volume surfaces from spacetime Lipschitz envelopes are the primary path for tagged kinematic entities — mathematically correct motion trails, not animation hacks or post-process blur. Overflow fallback proxies are budget safety valves only (not the target visual path) and must stay rare on target hardware.
12. **LLM to game.** Recipes compile to GPU-resident DAG bytecode. Local edits update visuals in <100ms on Medium-profile baseline hardware; large edits converge under budgeted streaming. Automatic sensitivity propagation.
13. **Conservative proofs.** Lipschitz, resource, and parameter bounds via affine arithmetic. Norm hygiene + dimensional type safety (spatial/temporal) enforced at compile time.
14. **Algebraically unified.** Motor transforms everywhere. Volume-preserving skinning. Stable SE(3) interpolation.
15. **Evolving ecosystem.** Enemies evolve via player-driven selection. Curated seeds, diversity-preserving selection, inverse design. Self-regulating predator-prey on PDE substrate.
16. **Infinite world.** Virtualized content-addressed bricks. Minecraft-style streaming. Biome hierarchy. Region epochs. DMD spectral sleep for sublinear time-skip of frozen regions with tiered catch-up (exact for short skips, approximate + corrected for medium, macro + reseed for geological).
17. **Temporal archaeology.** Navigate world history. Timelapse regions. Trace features to causes. DMD continuous-time interpolation for smooth timelapse; edit-history forensics for causal attribution (NOT DMD backward extrapolation).
18. **Global illumination from the field.** Per-brick SH probes + cone tracing + irradiance PDE. Light as physical force. Shadow hue shift from spectral indirect. AO from probes. Integrated into cel shader — stays anime.
19. **MMO-safe determinism.** Server-authoritative fixed-tick simulation produces identical region hashes across supported server builds; client rendering may vary visually without affecting authoritative gameplay state.
20. **Beyond-AAA raw fidelity mode.** Hyperfidelity WebGPU track delivers materially higher edge stability, lighting richness, and microdetail retention in High/Cinematic profiles while preserving 60fps Medium gameplay targets.
21. **Lipschitz Certificate Portfolio.** Dual L1+L2 envelope fusion eliminates diagonal tax (~1.73x penalty) in SDF-like regions. Quality ladder from math: Low=L1, Medium=fused, High=fused+mixed cone-union. Three truths policy keeps marching, contacts, and shading independently correct.
22. **Bernstein-certified noise.** Procedural noise bounds are tight by construction (convex hull of Bernstein coefficients), not inflated by worst-case heuristics. Frequency-bounded FBM evaluation early-exits on >60% of voxels, making noise-heavy scenes tractable without reducing octave count.
