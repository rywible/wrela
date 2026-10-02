// Visibility and shading kernels, the materials, their brute-force references and stats.
// Appended to common.wgsl + [wolf.wgsl] + one scene file, which provides:
//   const G_YMIN, G_YMAX, G_SLOPE, G_HALF, FOG, PUDDLE_R, LAYER_S
//   fn ground_h(x, z) -> f32, fn obj(p) -> f32, fn obj_range(o, d) -> vec2f, fn obj_id(p) -> u32,
//   fn surf(p, id, fp) -> Surf, fn part_field(p, key) -> f32, fn eye_frame(p) -> EyeF, fn wet_of(id) -> f32

@group(0) @binding(1) var<storage, read_write> vbuf: array<vec4f>;
@group(0) @binding(2) var out_tex: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var<storage, read_write> stats: array<atomic<u32>, 64>;

// Variants are pipeline-overridable constants: each compiles to its own specialized kernel.
override MATS: u32 = 0u;        // material switches (B_* in common.wgsl); 0 = plain shading everywhere
override REF: u32 = 0u;         // which switched-on materials use their brute-force reference instead
override REF_VIS: bool = false; // brute-force visibility and shadows too (half steps, 1/5 tolerance, huge caps)
override FILTER: bool = true;   // footprint-filtered detail (D-077); false = the naive unfiltered surface
override THICK_N: u32 = 4u;     // inward-march samples for thickness
override STATS: bool = false;
override SS: u32 = 4u;          // sub-pixel grid of the supersampled reference (SS × SS rays)
override TR_VIS: bool = true;  // transmitted light checks the sun's visibility from the far face (a shadow ray)
override DBG: u32 = 0u;         // development only: 1 = single-scattering lobe only, 2 = diffuse lobe only, 3 = nothing else occludes

const SOFT: f32 = 20.0;         // soft-shadow sharpness

// Stats slots.
const S_ID: u32 = 0u;           // 0..10: pixels per material id
const S_STEPS: u32 = 12u;       // primary steps (objects)
const S_CAPS: u32 = 13u;        // primary step-cap hits
const S_TH_MARCH: u32 = 14u;    // inward marches
const S_TH_STEPS: u32 = 15u;    // inward-march field evaluations
const S_TH_UNRES: u32 = 16u;    // inward marches that ended inside while still transmitting > 1/255
const S_SH_RAYS: u32 = 17u;     // shadow rays
const S_SH_STEPS: u32 = 18u;
const S_SH_CAPS: u32 = 19u;
const S_REF_CAPS: u32 = 20u;    // reference marches that hit their caps
const S_TR_SHADOW: u32 = 21u;   // extra shadow rays from transmission exit points
const S_EXTRA_EVALS: u32 = 22u; // field evaluations made by materials (curvature, occlusion)
const S_GCAPS: u32 = 23u;       // ground-trace caps
const S_PUDDLE: u32 = 24u;      // pixels with a puddle mask > 0.5
const S_MOSS: u32 = 25u;        // pixels with moss > 0.5
const S_BACKLIT: u32 = 26u;     // translucent pixels that are backlit

var<private> C: array<u32, 32>;
var<private> DBGV: vec3f;       // development: DBG 4 writes (chord in mm / 2, entry cosine, entry offset in mm) raw
var<private> DBGT: f32;
var<private> BUDGET: i32;       // field evaluations the current reference march may still spend (GPU safety)

const PART_BUDGET: i32 = 500;   // per march through the hit part (17 per pixel)
const OTHER_BUDGET: i32 = 3000; // for the one march past the part, through everything else

fn on(b: u32) -> bool { return (MATS & b) != 0u; }
fn isref(b: u32) -> bool { return (REF & b) != 0u; }

struct Surf {
  alb: vec3f,
  rough: f32,       // perceptual roughness
  f0: f32,
  dg: vec3f,        // detail displacement gradient (resolved octaves)
  dvar: f32,        // detail slope variance (octaves the footprint fades)
  key: u32,         // which part an inward march evaluates (a per-ray part mask)
  hn: f32,          // cap on the normal's finite-difference step (thin parts)
  tr: u32,          // translucent medium: 0 none, 1 hide, 2 leaf
  moss: f32,        // layered: bias toward moss
}

struct EyeF { c: vec3f, g: vec3f, u: vec3f, w: vec3f, R: f32, style: f32 }

fn surf0(alb: vec3f, rough: f32) -> Surf {
  return Surf(alb, rough, 0.04, vec3f(0.0), 0.0, 0u, 1.0, 0u, 0.0);
}

// ---- the whole field ----------------------------------------------------------------------------------

fn ground_k() -> f32 { return inverseSqrt(1.0 + G_SLOPE * G_SLOPE); }
fn ground_d(p: vec3f) -> f32 { return (p.y - ground_h(p.x, p.z)) * ground_k(); }
fn dist(p: vec3f) -> f32 { return min(obj(p), ground_d(p)); }

// ---- visibility -----------------------------------------------------------------------------------------

fn vis_eps() -> f32 { return select(0.25, 0.05, REF_VIS); }
fn vis_step() -> f32 { return select(1.0, 0.5, REF_VIS); }
fn vis_max() -> u32 { return select(192u, 3000u, REF_VIS); }

/** Spike 02's heightfield trace: along the ray the gap y − h shrinks at most at −d.y + slope·|d.xz|. */
fn ground_trace(o: vec3f, d: vec3f, pxa: f32, epsk: f32) -> f32 {
  let r = ray_box(o, d, vec3f(-G_HALF, G_YMIN, -G_HALF), vec3f(G_HALF, G_YMAX, G_HALF));
  if (r.x >= r.y) { return INF; }
  let rate = -d.y + G_SLOPE * length(d.xz);
  if (rate <= 0.0) { return INF; }
  var t = r.x;
  var t_prev = t;
  var gap_prev = INF;
  let maxs = select(160u, 4000u, REF_VIS);
  for (var i = 0u; i < maxs; i++) {
    let p = o + d * t;
    let gap = p.y - ground_h(p.x, p.z);
    if (gap < epsk * t * pxa) {
      if (gap_prev < INF && gap_prev > gap) { t += gap * (t - t_prev) / (gap_prev - gap); }
      return t;
    }
    t_prev = t;
    gap_prev = gap;
    t += gap / rate * vis_step();
    if (t > r.y) { return INF; }
  }
  C[S_GCAPS] += 1u;
  return t;
}

/** Primary hit: (t, id, steps, capped). */
fn primary(o: vec3f, d: vec3f, pxa: f32, epsk: f32) -> vec4f {
  let tg = ground_trace(o, d, pxa, epsk);
  var best = tg;
  var id = select(M_SKY, M_GROUND, tg < INF);
  let r = obj_range(o, d);
  var steps = 0u;
  var capped = 0.0;
  if (r.x < r.y && r.x < best) {
    var t = max(r.x, 0.0);
    let t1 = min(r.y, best);
    let sc = vis_step();
    var hit = false;
    var i = 0u;
    loop {
      if (i >= vis_max()) { capped = 1.0; break; }
      let p = o + d * t;
      let dd = obj(p);
      steps += 1u;
      if (dd < max(epsk * t * pxa, 1e-6)) { t += dd; hit = true; break; }
      t += dd * sc;
      if (t > t1) { break; }
      i++;
    }
    if (hit && t < best) {
      best = t;
      id = obj_id(o + d * t);
    }
  }
  return vec4f(best, f32(id), f32(steps), capped);
}

fn ray_dir(px: vec2f) -> vec3f {
  let u = px.x / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - px.y / f32(F.dims.y) * 2.0;
  return normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);
}

@compute @workgroup_size(8, 8)
fn visibility(@builtin(global_invocation_id) gid: vec3u) {
  let px = vec2u(gid.x, gid.y + F.dims.z);
  if (px.x >= F.dims.x || px.y >= min(F.dims.y, F.dims.w)) { return; }
  let d = ray_dir(vec2f(px) + 0.5);
  vbuf[px.y * F.dims.x + px.x] = primary(F.eye.xyz, d, F.eye.w, vis_eps());
}

// ---- field queries used by shading -------------------------------------------------------------------

const K0 = vec3f(1.0, -1.0, -1.0);
const K1 = vec3f(-1.0, -1.0, 1.0);
const K2 = vec3f(-1.0, 1.0, -1.0);
const K3 = vec3f(1.0, 1.0, 1.0);

/** Tetrahedral gradient (xyz) and the sum of the four taps (w), which with f(p) gives the Laplacian. */
fn tetra(p: vec3f, h: f32) -> vec4f {
  let f0 = dist(p + K0 * h);
  let f1 = dist(p + K1 * h);
  let f2 = dist(p + K2 * h);
  let f3 = dist(p + K3 * h);
  return vec4f(K0 * f0 + K1 * f1 + K2 * f2 + K3 * f3, f0 + f1 + f2 + f3);
}

/** Laplacian at scale r (stencil radius): four tetrahedral taps plus the centre. */
fn lap_tetra(p: vec3f, r: f32) -> f32 {
  let h = r * 0.57735;
  let t = tetra(p, h);
  C[S_EXTRA_EVALS] += 5u;
  return (t.w - 4.0 * dist(p)) / (2.0 * h * h);
}

/** The reference: the same Laplacian from the mean over n points of the sphere of radius r
 *  (mean − f(p) ≈ r²·Δf / 6). A spherical Fibonacci set. */
fn lap_sphere(p: vec3f, r: f32, n: u32) -> f32 {
  var acc = 0.0;
  for (var i = 0u; i < n; i++) {
    let z = 1.0 - (f32(i) + 0.5) * 2.0 / f32(n);
    let rr = sqrt(max(1.0 - z * z, 0.0));
    let a = f32(i) * 2.39996323;
    acc += dist(p + r * vec3f(rr * cos(a), z, rr * sin(a)));
  }
  return 6.0 * (acc / f32(n) - dist(p)) / (r * r);
}

/** Occlusion from distance samples along the normal: the mean over log-spaced h in [h0, 16·h0] of
 *  clamp((h − f(p + n·h)) / h, 0, 1), by k midpoint samples. */
fn occ_n(p: vec3f, n: vec3f, h0: f32, k: u32) -> f32 {
  var acc = 0.0;
  for (var i = 0u; i < k; i++) {
    let h = h0 * exp2(4.0 * (f32(i) + 0.5) / f32(k));
    acc += clamp((h - dist(p + n * h)) / h, 0.0, 1.0);
  }
  if (k < 8u) { C[S_EXTRA_EVALS] += k; }
  return acc / f32(k);
}

fn shadow_steps() -> u32 { return select(80u, 1500u, REF_VIS); }

/** Soft sun shadow against the objects (Quílez's closest-approach estimate). */
fn shadow(o: vec3f, l: vec3f) -> f32 {
  let r = obj_range(o, l);
  if (r.x >= r.y || r.y <= 0.0) { return 1.0; }
  C[S_SH_RAYS] += 1u;
  var res = 1.0;
  var t = max(r.x, 0.0);
  var ph = 1e10;
  let sc = vis_step();
  for (var i = 0u; i < shadow_steps(); i++) {
    C[S_SH_STEPS] += 1u;
    let h = obj(o + l * t);
    if (h < 1e-6) { return 0.0; }
    let y = h * h / (2.0 * ph);
    let dd = sqrt(max(h * h - y * y, 0.0));
    res = min(res, SOFT * dd / max(t - y, 1e-4));
    if (res < 0.004) { return 0.0; }
    ph = h;
    t += clamp(h, 1e-4, 0.5) * sc;
    if (t > r.y) { return res; }
  }
  C[S_SH_CAPS] += 1u;
  return res;
}

// ---- translucency: skin, hide, membranes, leaves -------------------------------------------------------

struct Medium {
  st: vec3f,      // extinction σt (1/m)
  ss: vec3f,      // scattering σs (1/m)
  g: f32,         // Henyey–Greenstein asymmetry
  sm: vec3f,      // diffuse-transmission extinction (1/m)
  am: vec3f,      // diffuse-transmission tint
  d0: f32,        // first inward step (m)
  hmax: f32,      // how far an inward march goes: past it, everything is opaque
}

fn medium(i: u32) -> Medium {
  if (i == 2u) {   // leaf: green transmits, red and blue are absorbed; veins are thicker, so darker
    let st = vec3f(3600.0, 1900.0, 5200.0);
    return Medium(st, st * vec3f(0.45, 0.75, 0.3), 0.35, vec3f(3000.0, 1500.0, 4600.0), vec3f(0.85, 1.0, 0.35), 0.00025, 0.006);
  }
  // hide and skin: red travels furthest (mean free path ~6 mm)
  let st = vec3f(170.0, 330.0, 460.0);
  return Medium(st, st * vec3f(0.5, 0.35, 0.25), 0.4, vec3f(180.0, 340.0, 480.0), vec3f(1.0, 0.62, 0.46), 0.0015, 0.05);
}

fn tr_bit(i: u32) -> u32 { return select(B_SKIN, B_LEAF, i == 2u); }

/** Fast path: march inward along u from the shaded point p, one field sample at p plus THICK_N
 *  inward. The sample at p says how far outside the surface p sits (up to the hit tolerance), so the
 *  chord starts at the real entry. Each step is |f| plus a growing increment, so the march crosses the
 *  far side; a secant on f places the exit, and its slope is the far face's cosine to u, free.
 *  `gs` is |∇f| (from the normal's taps): bound fields are often scaled, so slopes are divided by it.
 *  `k_in` is |u·n| at p. Returns (chord, resolved, far-face cosine). */
fn chord_fast(p: vec3f, u: vec3f, key: u32, m: Medium, k_in: f32, gs: f32) -> vec3f {
  C[S_TH_MARCH] += 1u;
  let kin = max(k_in, 0.1) * gs;          // how fast f falls along u at the entry
  let f0 = part_field(p, key);
  C[S_TH_STEPS] += 1u;
  let tin = max(f0, 0.0) / kin;
  DBGT = tin;
  // Along u, f is tent-shaped: the distance to the entry face, then to the far face. A secant only
  // places the exit (and its slope, the far face's cosine) when its inside sample is on the far
  // face's branch, i.e. when |f| there is less than its depth from the entry.
  var t = tin;
  var fprev = min(f0, 0.0);
  var inc = m.d0;
  var tout = -1.0;
  var fout = 0.0;
  for (var i = 0u; i < THICK_N; i++) {
    var tn = t + max(-fprev, 0.0) / gs + inc;   // |f|/|∇f| is a safe step inside
    if (tout > 0.0) {
      // Bracketed, but the inside sample is on the entry branch: sample just inside the exit
      // predicted by mirroring the entry (exact for a slab), which lands on the far branch.
      let tm = max(tout - fout / kin, t);
      tn = tm - 0.2 * (tm - t);
    }
    let f = part_field(p + u * tn, key);
    C[S_TH_STEPS] += 1u;
    if (f > 0.0) { tout = tn; fout = f; } else { t = tn; fprev = f; inc *= 1.6; }
    if (tout > 0.0 && fprev < 0.0 && -fprev < 0.85 * (t - tin) * kin) {
      let te = t + (tout - t) * (-fprev) / max(fout - fprev, 1e-9);
      return vec3f(te - tin, 1.0, clamp((fout - fprev) / ((tout - t) * gs), 0.0, 1.0));
    }
    if (tout < 0.0 && t > m.hmax) { break; }
  }
  if (tout > 0.0) {
    // No sample on the far branch: mirror the entry.
    return vec3f(max(tout - fout / kin, tin) - tin, 1.0, k_in);
  }
  return vec3f(t - tin + max(-fprev, 0.0) / gs, 0.0, 1.0);   // still inside: at least this far
}

/** Single scattering along the view ray through a slab, closed form:
 *  ∫₀ˢ e^(−σt(c + u(s−c)/s)) du = s·(e^(−σt·c) − e^(−σt·s)) / (σt·(s − c)). */
fn slab(st: vec3f, c: f32, s: f32) -> vec3f {
  let a = exp(-st * c);
  if (abs(s - c) < 1e-6) { return c * a; }
  return s * (a - exp(-st * s)) / (st * (s - c));
}

/** The far face's view of the sun, for transmitted light: a hard shadow ray that passes through
 *  translucent parts on its way, measuring each one's chord with the same few-sample inward march,
 *  and stops at opaque ones. Returns (interior length crossed, 1 if blocked). */
fn shadow_tr(o: vec3f, l: vec3f, sig: f32) -> vec2f {
  let r = obj_range(o, l);
  if (r.x >= r.y || r.y <= 0.0) { return vec2f(0.0); }
  C[S_SH_RAYS] += 1u;
  var t = max(r.x, 0.0);
  var len = 0.0;
  var crossings = 0u;
  for (var i = 0u; i < shadow_steps(); i++) {
    C[S_SH_STEPS] += 1u;
    let q = o + l * t;
    let h = obj(q);
    if (h < 1e-4) {   // within 0.1 mm: the crossing's own inward march measures the rest of the gap
      let key = part_key(q);
      let med = part_medium(key);
      if (med == 0u || crossings >= 3u) { return vec2f(len, 1.0); }
      let mo = medium(med);
      let ch = chord_fast(q, l, key, mo, 0.5, 1.0);
      len += ch.x;
      // a thick crossing (unresolved within the samples) or enough length is opaque: stop
      if (ch.y < 0.5 || len * sig > 6.0) { return vec2f(len, 1.0); }
      crossings += 1u;
      t += ch.x + max(2.0 * mo.d0, 2e-4);
      continue;
    }
    t += clamp(h, 1e-4, 0.5) * vis_step();
    if (t > r.y) { return vec2f(len, 0.0); }
  }
  C[S_SH_CAPS] += 1u;
  return vec2f(len, 0.0);
}

fn translucency_fast(p: vec3f, n: vec3f, v: vec3f, l: vec3f, s: Surf, m: Medium, gs: f32) -> vec3f {
  let nl = -dot(n, l);
  let nv = max(dot(n, v), 0.05);
  let ch = chord_fast(p, l, s.key, m, nl, gs);
  let c = ch.x;
  let sig = min(m.st.x, m.sm.x);
  if (ch.y < 0.5 && exp(-sig * c) > 1.0 / 255.0) { C[S_TH_UNRES] += 1u; }
  if (sig * c > 9.0) { return vec3f(0.0); }     // opaque: nothing gets through
  // Anything else between the exit point and the sun.
  // (from just past the exit; the same part's relief further on counts as a translucent crossing)
  var ext = 0.0;
  if (TR_VIS && DBG != 3u) {
    C[S_TR_SHADOW] += 1u;
    let sv = shadow_tr(p + l * (c + 2.0 * m.d0), l, min(m.sm.x, min(m.sm.y, m.sm.z)));
    if (sv.y > 0.0) { return vec3f(0.0); }
    ext = sv.x;
  }
  // Light enters through the far face: its cosine to l came free with the secant.
  let cin = ch.z;
  DBGV = vec3f(c * 500.0, cin, DBGT * 1000.0);
  let tin = 1.0 - fresnel(0.04, cin);
  let ss = m.ss * hg(dot(v, -l), m.g) * slab(m.st, c, c * nl / nv) * exp(-m.st * ext) * tin * select(1.0, 0.0, DBG == 2u);
  let dt = m.am * exp(-m.sm * (c + ext)) * cin * tin / PI * select(1.0, 0.0, DBG == 1u);
  return F.sunc.rgb * (ss + dt);
}

/** Interior length along l from x within the part `key` (the part that was hit), fine steps,
 *  re-entries into the part included; the cosine between l and the normal where the light entered
 *  (the first exit along l); and where that is. Stops once the medium would be opaque (len0 is the
 *  path already spent), or when the part is behind. */
fn part_depth(x: vec3f, l: vec3f, key: u32, hmin: f32, hout: f32, sig: f32, len0: f32) -> vec3f {
  BUDGET = PART_BUDGET;
  let tend = max(obj_range(x, l).y, 0.0);
  var t = 0.0;
  var len = 0.0;
  var cin = -1.0;
  var texit = 0.0;
  var was_in = false;   // a start just outside the part (within the hit tolerance) first steps in
  var tprev = 0.0;
  for (var i = 0u; i < 2000u; i++) {
    BUDGET -= 1;
    if (BUDGET <= 0) { C[S_REF_CAPS] += 1u; break; }
    let f = part_field(x + l * t, key);
    if (f < 0.0 && !was_in) {
      // entered during the last (outside) step: bisect the entry and count the part inside
      var a = tprev;
      var b = t;
      for (var k = 0; k < 10; k++) { let mm = 0.5 * (a + b); if (part_field(x + l * mm, key) < 0.0) { b = mm; } else { a = mm; } }
      len += t - b;
      BUDGET -= 10;
      if (cin < 0.0) { DBGT = b; }
    }
    if (f >= 0.0 && was_in && cin < 0.0) {
      // the first boundary: bisect it, then the normal there (tetrahedral taps)
      var a = max(t - hmin, 0.0);
      var b = t;
      for (var k = 0; k < 6; k++) { let mm = 0.5 * (a + b); if (part_field(x + l * mm, key) < 0.0) { a = mm; } else { b = mm; } }
      len -= t - b;
      texit = b;
      let qb = x + l * b;
      let e = 0.25 * hmin;
      let gn = normalize(K0 * part_field(qb + K0 * e, key) + K1 * part_field(qb + K1 * e, key)
                       + K2 * part_field(qb + K2 * e, key) + K3 * part_field(qb + K3 * e, key));
      cin = clamp(dot(gn, l), 0.0, 1.0);
      BUDGET -= 10;
    }
    was_in = f < 0.0;
    // fine steps inside (the length's precision); coarser outside, still too small to step over the part
    let st = select(max(f, hout), max(-f, hmin), f < 0.0);
    if (f < 0.0) {
      len += st;
      if ((len + len0) * sig > 10.0) { break; }   // contributes less than e^-10
    }
    tprev = t;
    t += st;
    if (t > tend || (f > 0.05 && (cin >= 0.0 || t > 0.05))) { break; }
  }
  return vec3f(len, max(cin, 0.0), texit);
}

/** Interior length along l from x through everything except the part `key`, fine steps; opaque
 *  (non-translucent) parts block it outright. */
fn other_depth(x: vec3f, l: vec3f, key: u32, hmin: f32, hout: f32, sig: f32) -> f32 {
  BUDGET = OTHER_BUDGET;
  let tend = max(obj_range(x, l).y, 0.0);
  var t = 0.0;
  var len = 0.0;
  var tprev = 0.0;
  var was_in = false;
  for (var i = 0u; i < 2000u; i++) {
    BUDGET -= 1;
    if (BUDGET <= 0) { C[S_REF_CAPS] += 1u; break; }
    let fk = obj_except_k(x + l * t, key);
    let f = fk.x;
    if ((f < 0.0) != was_in && i > 0u) {
      // crossed a boundary during the last step: bisect it, and count the part of the step inside
      var a = tprev;
      var b = t;
      for (var k = 0; k < 10; k++) {
        let mm = 0.5 * (a + b);
        if ((obj_except(x + l * mm, key) < 0.0) == was_in) { a = mm; } else { b = mm; }
      }
      BUDGET -= 10;
      if (was_in) { len -= t - b; } else { len += t - b; }
    }
    was_in = f < 0.0;
    let st = select(max(f, hout), max(-f, hmin), f < 0.0);
    if (f < 0.0) {
      if (part_medium(u32(fk.y)) == 0u) { return 1e3; }
      len += st;
      if (len * sig > 10.0) { break; }
    }
    tprev = t;
    t += st;
    if (t > tend) { break; }
  }
  return len;
}

/** Reference: integrate single scattering along the inward view ray numerically, each sample with its
 *  own fine optical depth toward the sun through the part (no slab, no secant); then the light's fine
 *  optical depth through everything else on its way (no soft-shadow estimate), once per pixel from
 *  where the light entered. 16 samples, importance-sampled on the slowest extinction. */
fn translucency_ref(p: vec3f, n: vec3f, v: vec3f, l: vec3f, s: Surf, m: Medium) -> vec3f {
  let hmin = m.d0 / 40.0;
  // Outside, the minimum step is half the thinnest part (a leaf's 0.5 mm lamina, a 2.4 mm membrane),
  // so no part can be stepped over.
  let hout = select(0.0012, 0.00025, s.tr == 2u);
  let d = -v;
  let stmin = min(m.st.x, min(m.st.y, m.st.z));
  let smmin = min(m.sm.x, min(m.sm.y, m.sm.z));
  // Where the view ray enters the part (p can sit up to the hit tolerance outside it), and where it
  // leaves (or the medium is opaque).
  var u = 0.0;
  for (var i = 0u; i < 200u; i++) {
    let f = part_field(p + d * u, s.key);
    if (f < 0.0) { break; }
    u += max(f, hmin);
  }
  let uin = u;
  let send = uin + 10.0 / stmin;
  var exitv = send;
  for (var i = 0u; i < 2000u; i++) {
    let f = part_field(p + d * u, s.key);
    if (f > 0.0) { exitv = u; break; }
    u += max(-f, hmin);
    if (u > send) { break; }
  }
  // Diffuse transmission, and the light's way past the part, from p.
  let pd0 = part_depth(p + l * hmin, l, s.key, hmin, hout, smmin, 0.0);
  let tent0 = DBGT + hmin;
  var ext = 0.0;
  if (DBG != 3u) { ext = other_depth(p + l * (hmin + pd0.z + hout), l, s.key, hmin, hout, smmin); }
  let dt = m.am * exp(-m.sm * (pd0.x + hmin + ext)) * pd0.y * (1.0 - fresnel(0.04, pd0.y)) / PI * select(1.0, 0.0, DBG == 1u);
  DBGV = vec3f((pd0.x + hmin) * 500.0, pd0.y, tent0 * 1000.0);
  // Single scattering: samples at quantiles of e^(−σmin·u) on [uin, exitv], weighted by 1/pdf.
  let M = 16u;
  let span = exitv - uin;
  let nrm = 1.0 - exp(-stmin * span);
  var ss = vec3f(0.0);
  let ph = hg(dot(v, -l), m.g);
  for (var j = 0u; j < M; j++) {
    let uj = -log(1.0 - (f32(j) + 0.5) / f32(M) * nrm) / stmin;
    let w = nrm * exp(stmin * uj) / (f32(M) * stmin);
    let pd = part_depth(p + d * (uin + uj), l, s.key, hmin, hout, stmin, uj + ext);
    ss += m.ss * ph * exp(-m.st * (uj + pd.x + ext)) * (1.0 - fresnel(0.04, pd.y)) * w * select(1.0, 0.0, DBG == 2u);
  }
  return F.sunc.rgb * (ss + dt);
}

// ---- eyes ----------------------------------------------------------------------------------------------------

const CORNEA_RC: f32 = 0.80;    // cornea sphere radius, in eyeball radii
const CORNEA_B: f32 = 0.12;     // the cornea's apex stands this far beyond the eyeball
const LIMBUS_Z: f32 = 0.7225;   // where the two spheres meet, along the gaze
const IRIS_Z: f32 = 0.66;       // the iris's base plane, along the gaze
const IRIS_R: f32 = 0.74;
const PUPIL_R: f32 = 0.22;
const IRIS_DOME: f32 = 0.07;    // the iris bulges toward the cornea by this much at its centre
const ETA_EYE: f32 = 1.376;

/** The eyeball with its corneal bulge, in an eye frame (z = gaze), absolute units. */
fn eye_sdf(q: vec3f, R: f32) -> f32 {
  let ball = length(q) - R;
  let cor = length(q - vec3f(0.0, 0.0, R * (1.0 + CORNEA_B - CORNEA_RC))) - R * CORNEA_RC;
  return smin(ball, cor, 0.03 * R);
}

fn iris_fibres(rho: f32, phi: f32) -> f32 {
  return noise3(vec3f(cos(phi) * 15.0, sin(phi) * 15.0, rho * 2.2)) * 0.6
       + noise3(vec3f(cos(phi) * 40.0, sin(phi) * 40.0, rho * 5.0 + 7.0)) * 0.4;
}

/** Iris albedo at normalized radius rho (1 = the iris's edge) and angle phi; `style` 0 is the wolf's
 *  amber, 1 the gallery eye's green-grey. Beyond the iris: sclera. */
fn iris_alb(rho: f32, phi: f32, style: f32, aa: f32) -> vec3f {
  let fib = iris_fibres(rho, phi);
  let inner = mix(vec3f(0.62, 0.34, 0.05), vec3f(0.42, 0.40, 0.16), style);
  let outer = mix(vec3f(0.48, 0.22, 0.03), vec3f(0.16, 0.30, 0.26), style);
  var c = mix(inner, outer, smoothstep(0.30, 0.75, rho));
  c *= 0.75 + 0.5 * fib;
  c = mix(c, c * 1.5 + 0.04, 0.6 * (1.0 - smoothstep(0.0, 0.08, abs(rho - 0.48))));   // collarette
  c *= 1.0 - 0.75 * smoothstep(0.78, 0.99, rho);                                         // limbal ring
  let pr = PUPIL_R / IRIS_R;
  c = mix(vec3f(0.006), c, smoothstep(pr - aa, pr + aa, rho));                           // pupil
  let sclera = mix(vec3f(0.07, 0.045, 0.025), vec3f(0.62, 0.58, 0.54), style);
  return mix(c, sclera, smoothstep(1.0 - aa, 1.0 + aa, rho));
}

/** Height of the iris surface above its base plane (in eyeball radii): dome plus fibre relief. */
fn iris_h(rho: f32, phi: f32) -> f32 {
  return IRIS_DOME * (1.0 - rho * rho) + 0.012 * iris_fibres(rho, phi);
}

fn eye_local(e: EyeF, x: vec3f) -> vec3f { return vec3f(dot(x, e.u), dot(x, e.w), dot(x, e.g)); }

/** Plain shading paints the iris onto the outer surface: orthographic along the gaze. */
fn eye_painted(p: vec3f, e: EyeF, fp: f32) -> vec3f {
  let q = eye_local(e, p - e.c) / e.R;
  let rho = length(q.xy) / IRIS_R;
  let alb = iris_alb(rho, atan2(q.y, q.x), e.style, max(fp / (e.R * IRIS_R), 0.01));
  let sclera = mix(vec3f(0.07, 0.045, 0.025), vec3f(0.62, 0.58, 0.54), e.style);
  return select(sclera, alb, q.z > LIMBUS_Z - 0.05);
}

/** Lighting of the iris seen through the cornea. */
fn iris_light(alb: vec3f, ni: vec3f, l: vec3f, sh: f32, tin: f32) -> vec3f {
  return alb * (F.sunc.rgb * max(dot(ni, l), 0.0) * sh * tin / PI + sky_light(ni) * 0.7);
}

fn eye_shade(p: vec3f, n: vec3f, v: vec3f, l: vec3f, e: EyeF, sh: f32, fp: f32, reference: bool) -> vec3f {
  let q = eye_local(e, p - e.c) / e.R;
  let corn = smoothstep(LIMBUS_Z - 0.04, LIMBUS_Z + 0.02, q.z);
  let nv = max(dot(n, v), 1e-3);
  let fr = fresnel(0.025, nv);
  // Wet surface: a sharp sun highlight and the sky's reflection, over cornea and sclera alike.
  let wet = F.sunc.rgb * ggx(n, v, l, 0.0009, 0.025) * sh + sky_env(reflect(-v, n), 0.03) * fr;
  // Sclera: plain diffuse.
  let sclera = mix(vec3f(0.07, 0.045, 0.025), vec3f(0.62, 0.58, 0.54), e.style);
  let col_s = sclera * (F.sunc.rgb * max(dot(n, l), 0.0) * sh / PI + sky_light(n)) * (1.0 - fr);
  // Cornea: refract through the field normal toward the iris.
  let rl = refract(-v, n, 1.0 / ETA_EYE);
  let r = vec3f(dot(rl, e.u), dot(rl, e.w), dot(rl, e.g));     // eye frame, unit length
  let aa = max(fp / (e.R * IRIS_R), 0.01);
  let tin = 1.0 - fresnel(0.025, max(dot(n, l), 0.0));
  var col_c = vec3f(0.0);
  if (!reference) {
    // Fast: the iris dome is a paraboloid, z = IRIS_Z + IRIS_DOME·(1 − ρ²): hit it exactly (a
    // quadratic), then shade with the relief's normal. Only the fibre relief's own parallax is left out.
    let A = IRIS_Z + IRIS_DOME;
    let Bq = IRIS_DOME / (IRIS_R * IRIS_R);
    let qa = Bq * dot(r.xy, r.xy);
    let qb = r.z + 2.0 * Bq * dot(q.xy, r.xy);
    let qc = q.z - A + Bq * dot(q.xy, q.xy);
    let disc = qb * qb - 4.0 * qa * qc;
    var tt = (IRIS_Z - q.z) / min(r.z, -1e-3);
    if (disc >= 0.0) {
      let sq = sqrt(disc);
      let t1 = select(-qc / qb, (-qb - sq) / (2.0 * qa), qa > 1e-7);
      let t2 = select(t1, (-qb + sq) / (2.0 * qa), qa > 1e-7);
      tt = select(t2, t1, t1 > 0.0);
    }
    let x = q + r * tt;
    let rho = length(x.xy) / IRIS_R;
    let phi = atan2(x.y, x.x);
    let alb = iris_alb(rho, phi, e.style, aa);
    let hh = 0.002;
    let h0 = iris_h(min(rho, 1.0), phi);
    let rx = length(x.xy + vec2f(hh, 0.0)) / IRIS_R;
    let ry = length(x.xy + vec2f(0.0, hh)) / IRIS_R;
    let gx = (iris_h(min(rx, 1.0), atan2(x.y, x.x + hh)) - h0) / hh;
    let gy = (iris_h(min(ry, 1.0), atan2(x.y + hh, x.x)) - h0) / hh;
    let nl_ = normalize(vec3f(-gx, -gy, 1.0));
    let ni = normalize(e.u * nl_.x + e.w * nl_.y + e.g * nl_.z);
    col_c = iris_light(alb, ni, l, sh, tin);
  } else {
    // Reference: march the refracted ray to the true domed, ridged iris surface, then bisect.
    var t = 0.0;
    var hit = false;
    var x = q;
    let dt = 0.002;
    for (var i = 0u; i < 1200u; i++) {
      x = q + r * t;
      let rho = length(x.xy) / IRIS_R;
      if (rho > 1.15 || x.z < IRIS_Z - 0.1) { break; }
      if (x.z < IRIS_Z + iris_h(min(rho, 1.0), atan2(x.y, x.x))) { hit = true; break; }
      t += dt;
    }
    if (hit) {
      var a = max(t - dt, 0.0);
      var b = t;
      for (var k = 0; k < 20; k++) {
        let mid = 0.5 * (a + b);
        let xm = q + r * mid;
        let rm = length(xm.xy) / IRIS_R;
        if (xm.z < IRIS_Z + iris_h(min(rm, 1.0), atan2(xm.y, xm.x))) { b = mid; } else { a = mid; }
      }
      x = q + r * b;
    }
    let rho = length(x.xy) / IRIS_R;
    let phi = atan2(x.y, x.x);
    let alb = iris_alb(rho, phi, e.style, aa);
    // Normal of the true iris surface, by finite differences of its height.
    let hh = 0.002;
    let h0 = iris_h(min(rho, 1.0), phi);
    let rx = length(x.xy + vec2f(hh, 0.0)) / IRIS_R;
    let ry = length(x.xy + vec2f(0.0, hh)) / IRIS_R;
    let gx = (iris_h(min(rx, 1.0), atan2(x.y, x.x + hh)) - h0) / hh;
    let gy = (iris_h(min(ry, 1.0), atan2(x.y + hh, x.x)) - h0) / hh;
    let nl_ = normalize(vec3f(-gx, -gy, 1.0));
    let ni = normalize(e.u * nl_.x + e.w * nl_.y + e.g * nl_.z);
    col_c = iris_light(alb, ni, l, sh, tin);
  }
  return mix(col_s, col_c * (1.0 - fr), corn) + wet;
}

// ---- shading -----------------------------------------------------------------------------------------------

fn shade_px(o: vec3f, d: vec3f, t: f32, id: u32, pxa: f32) -> vec3f {
  if (id == M_SKY) { return treeline(d, sky_rad(d, true)); }
  let p = o + d * t;
  let fp0 = t * pxa;                    // the pixel's footprint (m)
  let fp = fp0 * F.prm.x;               // the footprint used for filtering and variance (swept)
  let v = -d;
  let l = F.sun.xyz;
  var s = surf(p, id, fp);
  if (STATS) { C[S_ID] = id; }

  // Normal: tetrahedral gradient of the whole field, step tied to the footprint (fixed when unfiltered).
  var hn = clamp(0.5 * fp0, 1e-5, s.hn);
  if (!FILTER) { hn = min(1e-4, s.hn); }
  let tg = tetra(p, hn);
  let nb = normalize(tg.xyz);
  var n = normalize(nb - (s.dg - nb * dot(s.dg, nb)));
  let nv0 = dot(n, v);
  if (nv0 < 0.05) { n = normalize(n + v * (0.05 - nv0)); }

  var alb = s.alb;
  var rough = s.rough;
  var f0 = s.f0;
  var dvar = s.dvar;
  var ao = 1.0;
  var sheen = 0.0;

  // ---- wet: darker, glossier, puddles where the puddle-scale Laplacian is concave and n faces up
  let wet = select(0.0, wet_of(id) * F.prm.z, on(B_WET));
  if (wet > 0.0) {
    var lp = 0.0;
    if (isref(B_WET)) { lp = lap_sphere(p, PUDDLE_R, 64u); } else { lp = lap_tetra(p, PUDDLE_R); }
    let conc = smoothstep(0.12, 0.45, -lp * PUDDLE_R);
    let up = smoothstep(0.88, 0.97, nb.y);
    let pud = conc * up * wet;
    alb *= mix(1.0, 0.42, wet);              // porous darkening (Lagarde 2013)
    rough = mix(rough, select(0.13, 0.55, id == M_GROUND), wet);   // a water film on stone; damp soil
    f0 = mix(f0, 0.02, wet);
    n = normalize(mix(n, vec3f(0.0, 1.0, 0.0), pud));
    rough = mix(rough, 0.02, pud);
    alb *= mix(1.0, 0.4, pud);
    dvar *= 1.0 - pud;
    if (STATS && pud > 0.5) { C[S_PUDDLE] = 1u; }
  }

  // ---- layered stone: moss, grime, edge wear and lichen from normal, occlusion and curvature
  if (on(B_LAYER) && (id == M_STONE || id == M_MORTAR)) {
    let S = LAYER_S;
    var occ = 0.0;
    var lp = 0.0;
    if (isref(B_LAYER)) {
      occ = occ_n(p, nb, 0.0106 * S, 32u);
      lp = lap_sphere(p, 0.04 * S, 64u);
    } else {
      occ = occ_n(p, nb, 0.0106 * S, 4u);
      lp = lap_tetra(p, 0.04 * S);
    }
    let cav = smoothstep(0.08, 0.5, -lp * 0.04 * S);
    let edge = smoothstep(0.12, 0.6, lp * 0.04 * S);
    let up = smoothstep(0.1, 0.8, nb.y);
    let nz = fbm_f(p, 3.0 / S, 4, fp);
    let nz2 = fbm_f(p + vec3f(13.1, 7.7, 2.3), 18.0 / S, 3, fp);
    // moss: up-facing, occluded, concave and out of the sun, broken into patches by noise
    let shade = 1.0 - smoothstep(0.0, 0.5, dot(nb, l));
    let mossv = 1.2 * up + 0.8 * occ + 0.6 * cav + 0.35 * shade + s.moss + 2.2 * nz - 1.35;
    let moss = smoothstep(0.0, 0.3, mossv);
    let grime = clamp(0.8 * cav + 0.7 * occ, 0.0, 1.0) * (1.0 - moss);
    alb *= mix(vec3f(1.0), vec3f(0.42, 0.38, 0.33), grime);
    let wear = edge * (1.0 - moss);
    alb = mix(alb, alb * 1.4 + vec3f(0.03, 0.028, 0.024), 0.75 * wear);
    let lichen = smoothstep(0.50, 0.58, nz2 + 0.1 * (1.0 - up) - 0.4 * occ) * (1.0 - moss) * (1.0 - cav);
    alb = mix(alb, vec3f(0.47, 0.46, 0.37), 0.45 * lichen);
    let mt = fbm_f(p + vec3f(3.0, 1.0, 5.0), 40.0 / S, 3, fp);
    let moss_alb = mix(vec3f(0.035, 0.055, 0.012), vec3f(0.17, 0.22, 0.035), clamp(0.5 + 1.6 * mt + 0.4 * up, 0.0, 1.0));
    alb = mix(alb, moss_alb, moss);
    rough = mix(rough, 0.92, moss);
    f0 = mix(f0, 0.02, moss);
    dvar *= 1.0 - moss;
    ao = 1.0 - 0.85 * occ;
    sheen = moss;
    if (STATS && moss > 0.5) { C[S_MOSS] = 1u; }
  }

  // ---- eyes: plain shading paints the iris on the surface
  var eyef: EyeF;
  if (id == M_EYE) {
    eyef = eye_frame(p);
    if (!on(B_EYE)) { alb = eye_painted(p, eyef, fp0); rough = 0.3; }
  }

  // ---- specular AA: the slope variance the footprint doesn't resolve, added to roughness²
  var a2x = 0.0;
  if (on(B_SPECAA)) {
    // Resolved curvature across the footprint, from the normal's own taps plus f(p).
    let lapg = (tg.w - 4.0 * dist(p)) / (2.0 * hn * hn);
    C[S_EXTRA_EVALS] += 1u;
    let vg = 0.125 * lapg * lapg * fp * fp;
    a2x = dvar + vg;
  }

  // ---- light
  let ndl = dot(n, l);
  var sh = 0.0;
  if (dot(nb, l) > 0.0 && ndl > -0.1) { sh = shadow(p + nb * (2.0 * hn + 2e-4), l); }
  let a = rough * rough;
  let a2 = clamp(a * a + a2x, 1e-5, 1.0);
  let nv = max(dot(n, v), 1e-3);
  let fr = env_brdf(f0, rough, nv);
  var col = alb * (F.sunc.rgb * max(ndl, 0.0) * sh / PI + sky_light(n) * ao) * (1.0 - fr);
  col += F.sunc.rgb * ggx(n, v, l, a2, f0) * sh;
  col += sky_env(reflect(-v, n), sqrt(sqrt(a2))) * fr * ao;
  col += alb * sky_light(n) * sheen * 0.9 * pow(1.0 - nv, 3.0);

  if (id == M_EYE && on(B_EYE)) {
    col = eye_shade(p, n, v, l, eyef, sh, fp0, isref(B_EYE));
  }

  // ---- translucency (backlit only)
  if (s.tr != 0u && on(tr_bit(s.tr)) && dot(n, l) < 0.0) {
    if (STATS) { C[S_BACKLIT] = 1u; }
    let m = medium(s.tr);
    if (isref(tr_bit(s.tr))) { col += alb_tint(s) * translucency_ref(p, n, v, l, s, m); }
    else { col += alb_tint(s) * translucency_fast(p, n, v, l, s, m, clamp(length(tg.xyz) / (4.0 * hn), 0.3, 1.5)); }
  }

  let fogk = 1.0 - exp(-t * FOG);
  let fd = normalize(vec3f(d.x, max(d.y, 0.0), d.z));
  return mix(col, treeline(fd, sky_rad(fd, false)), fogk);
}

/** Translucent light leaves through the surface's own pigment, softened. */
fn alb_tint(s: Surf) -> vec3f { return sqrt(clamp(s.alb, vec3f(0.0), vec3f(1.0))) * 1.6; }

fn flush_stats(vflags: vec4f) {
  if (!STATS) { return; }
  atomicAdd(&stats[min(C[S_ID], N_IDS - 1u)], 1u);
  atomicAdd(&stats[S_STEPS], u32(vflags.z));
  atomicAdd(&stats[S_CAPS], u32(vflags.w));
  for (var i = S_TH_MARCH; i <= S_BACKLIT; i++) {
    if (C[i] != 0u) { atomicAdd(&stats[i], C[i]); }
  }
}

@compute @workgroup_size(8, 8)
fn shade(@builtin(global_invocation_id) gid: vec3u) {
  let px = vec2u(gid.x, gid.y + F.dims.z);
  if (px.x >= F.dims.x || px.y >= min(F.dims.y, F.dims.w)) { return; }
  for (var i = 0; i < 32; i++) { C[i] = 0u; }
  let vb = vbuf[px.y * F.dims.x + px.x];
  let d = ray_dir(vec2f(px) + 0.5);
  DBGV = vec3f(0.0);
  let hdr = shade_px(F.eye.xyz, d, vb.x, u32(vb.y), F.eye.w);
  textureStore(out_tex, vec2i(px), select(vec4f(tonemap(hdr), 1.0), vec4f(DBGV, 1.0), DBG == 4u));
  flush_stats(vb);
}

/** Supersampled reference for specular AA: SS×SS jittered sub-pixel rays, each marched and shaded
 *  at its own footprint, averaged in linear light before tonemapping. Built with FILTER = false. */
@compute @workgroup_size(8, 8)
fn shade_ss(@builtin(global_invocation_id) gid: vec3u) {
  let px = vec2u(gid.x, gid.y + F.dims.z);
  if (px.x >= F.dims.x || px.y >= min(F.dims.y, F.dims.w)) { return; }
  var acc = vec3f(0.0);
  let pxa = F.eye.w / f32(SS);
  for (var j = 0u; j < SS * SS; j++) {
    for (var i = 0; i < 32; i++) { C[i] = 0u; }
    let seed = u32(F.prm.y) * 7777u;   // a different jitter per seed: two renders measure the reference's own noise
    let jx = (f32(j % SS) + 0.5 + 0.8 * (hash_u(px.x * 7919u + px.y * 104729u + j * 31u + seed) - 0.5)) / f32(SS);
    let jy = (f32(j / SS) + 0.5 + 0.8 * (hash_u(px.x * 15485863u + px.y * 3571u + j * 17u + seed) - 0.5)) / f32(SS);
    let d = ray_dir(vec2f(px) + vec2f(jx, jy));
    let h = primary(F.eye.xyz, d, pxa, 0.25);
    acc += shade_px(F.eye.xyz, d, h.x, u32(h.y), pxa);
  }
  textureStore(out_tex, vec2i(px), vec4f(tonemap(acc / f32(SS * SS)), 1.0));
}
