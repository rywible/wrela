// The grey wolf, in rest space, split into parts that each hang on one bone.
//
// Adapted from experiments/agent-authoring/wolf/creature.wgsl (the agent's field, unchanged in
// shape) with three changes:
// - The field is split into 15 part functions (limbs on the left side; the right side evaluates
//   them at the mirrored point). Each is rigid in one bone's frame when posed.
// - The millimetre fbm "fur" displacement and the noise on the ruff, mane, hackles, cheek ruff and
//   tail are left out of the marched surface. The volumetric fur layer replaces them.
// - The colour function takes channels from the fold (tail and head weights) instead of
//   re-evaluating the tail and body fields to find the tail.
//
// Units: metres. +y up, ground at y = 0, +z forward (nose), +x = the wolf's left.

// ---------------- skeleton (rest pose), from creature.wgsl ----------------
const J_PELVIS  = vec3f(0.0, 0.632, -0.430);
const J_LUMBAR  = vec3f(0.0, 0.655, -0.180);
const J_CHEST   = vec3f(0.0, 0.615,  0.095);
const J_WITHERS = vec3f(0.0, 0.800,  0.200);
const J_NECK    = vec3f(0.0, 0.680,  0.310);
const J_HEAD    = vec3f(0.0, 0.912,  0.592);
const HEAD_PITCH = 0.22;
const HEAD_SCALE = 1.13;
const J_TAIL0 = vec3f(0.0, 0.668, -0.570);
const J_TAIL1 = vec3f(0.0, 0.585, -0.660);
const J_TAIL2 = vec3f(0.0, 0.450, -0.715);
const J_TAIL3 = vec3f(0.0, 0.270, -0.690);
const J_SCAPULA  = vec3f(0.050, 0.750, 0.185);
const J_SHOULDER = vec3f(0.065, 0.565, 0.335);
const J_ELBOW    = vec3f(0.078, 0.415, 0.245);
const J_CARPUS   = vec3f(0.072, 0.128, 0.270);
const J_FPAW     = vec3f(0.070, 0.000, 0.300);
const J_HIP    = vec3f(0.070, 0.600, -0.420);
const J_STIFLE = vec3f(0.085, 0.410, -0.320);
const J_HOCK   = vec3f(0.078, 0.210, -0.495);
const J_HPAW   = vec3f(0.075, 0.000, -0.445);

// ---------------- helpers (creature.wgsl) ----------------
fn sd_ell(p: vec3f, r: vec3f) -> f32 {
  let k0 = length(p / r);
  if (k0 < 1.0) { return (k0 - 1.0) * min(r.x, min(r.y, r.z)); }
  let k1 = length(p / (r * r));
  return k0 * (k0 - 1.0) / k1;
}

fn sd_flat_cone(p: vec3f, a: vec3f, b: vec3f, r1: f32, r2: f32, sx: f32) -> f32 {
  let s = vec3f(sx, 1.0, 1.0);
  return sd_round_cone(p * s, a * s, b * s, r1, r2) / sx;
}

const TOE_IN  = vec3f(0.011, 0.015, 0.030);
const TOE_OUT = vec3f(0.029, 0.014, 0.017);
fn sd_paw(q: vec3f, s: f32) -> f32 {
  var d = sd_ell(q - vec3f(0.0, 0.022, -0.012) * s, vec3f(0.030, 0.024, 0.034) * s);
  let mq = vec3f(abs(q.x), q.y, q.z);
  d = smin(d, sd_ell(mq - TOE_IN * s, vec3f(0.0125, 0.015, 0.017) * s), 0.006 * s);
  d = smin(d, sd_ell(mq - TOE_OUT * s, vec3f(0.0115, 0.014, 0.016) * s), 0.006 * s);
  return smax(d, -q.y, 0.004);
}
fn claw_mask(pq: vec3f) -> f32 {
  let mq = vec3f(abs(pq.x), pq.y, pq.z);
  let a = mq - TOE_IN;
  let b = mq - TOE_OUT;
  let ca = smoothstep(0.011, 0.015, a.z) * (1.0 - smoothstep(0.004, 0.007, abs(a.x)));
  let cb = smoothstep(0.010, 0.014, b.z) * (1.0 - smoothstep(0.004, 0.007, abs(b.x)));
  return max(ca, cb) * (1.0 - smoothstep(0.010, 0.016, pq.y));
}

fn head_local(p: vec3f) -> vec3f { return rot_x(p - J_HEAD, -HEAD_PITCH) / HEAD_SCALE; }

const EAR_BASE = vec3f(0.046, 0.040, -0.036);
fn ear_local(m: vec3f) -> vec3f {
  var e = m - EAR_BASE;
  e = rot_y(e, -0.35);
  e = rot_z(e, 0.28);
  e = rot_x(e, 0.04);
  return e;
}

const EAR_H = 0.086;
fn sd_ear_cavity(e: vec3f) -> f32 {
  let ci = (e - vec3f(0.0, 0.012, 0.020)) * vec3f(1.12, 1.0, 1.45);
  return sd_round_cone(ci, vec3f(0.0), vec3f(0.0, EAR_H - 0.024, 0.0), 0.030, 0.004) / 1.45;
}
fn sd_ear(m: vec3f) -> f32 {
  let e = ear_local(m);
  let outer = sd_round_cone(e * vec3f(1.0, 1.0, 1.45), vec3f(0.0), vec3f(0.0, EAR_H, 0.0), 0.038, 0.012) / 1.45;
  return smax(outer, -sd_ear_cavity(e), 0.004);
}

fn sd_muzzle(q: vec3f) -> f32 {
  let z0 = 0.020;
  let z1 = 0.192;
  let t = clamp((q.z - z0) / (z1 - z0), 0.0, 1.0);
  let cy = mix(0.004, -0.005, t);
  let hx = mix(0.043, 0.022, t);
  let hy = mix(0.040, 0.026, t);
  let r = mix(0.028, 0.019, t);
  let c = vec3f(q.x, q.y - cy, q.z - 0.5 * (z0 + z1));
  return sd_box(c, vec3f(hx - r, hy - r, 0.5 * (z1 - z0) - r)) - r;
}
const EYE_C = vec3f(0.040, 0.018, 0.052);
const EYE_GAZE = vec3f(0.74, 0.15, 0.656);

// ---------------- parts (rest space) ----------------
// Base functions, by id: 0 croup, 1 loin, 2 chest, 3 neck, 4 head, 5–7 tail, 8 scapula,
// 9 upper arm, 10 forearm, 11 manus (pastern + paw), 12 thigh, 13 gaskin, 14 pes (metatarsus + paw).
// Joint parts for the bend warp: 9 with `warped` = elbow (upper arm ∪ forearm), 13 = hock (gaskin ∪ pes).

fn part_croup(q: vec3f) -> f32 {
  return sd_ell(rot_x(q - (J_PELVIS + vec3f(0.0, 0.0, -0.010)), 0.30), vec3f(0.098, 0.112, 0.145));
}
fn part_loin(q: vec3f) -> f32 { return sd_ell(q - J_LUMBAR, vec3f(0.066, 0.088, 0.240)); }
fn part_chest(q: vec3f) -> f32 {
  var d = sd_ell(q - J_CHEST, vec3f(0.130, 0.180, 0.305));
  d = smin(d, sd_ell(q - vec3f(0.0, 0.565, 0.320), vec3f(0.085, 0.105, 0.090)), 0.07);
  d = smin(d, sd_ell(q - (J_WITHERS + vec3f(0.0, -0.070, 0.010)), vec3f(0.075, 0.070, 0.140)), 0.06);
  return d;
}
fn part_neck(q: vec3f) -> f32 {
  var d = sd_round_cone(q, J_NECK, J_HEAD + vec3f(0.0, -0.030, -0.070), 0.122, 0.075);
  d = smin(d, sd_ell(q - (J_NECK + vec3f(0.0, 0.075, 0.140)), vec3f(0.112, 0.130, 0.105)), 0.05);   // ruff
  d = smin(d, sd_ell(q - (J_NECK + vec3f(0.0, 0.090, -0.040)), vec3f(0.105, 0.075, 0.120)), 0.06);  // mane
  d = smin(d, sd_ell(q - (J_NECK + vec3f(0.0, 0.165, 0.090)), vec3f(0.075, 0.050, 0.110)), 0.05);   // hackles
  return d;
}
fn part_head(p: vec3f) -> f32 {
  let q = head_local(p);
  let m = mirror_x(q);
  var d = sd_ell(q - vec3f(0.0, 0.010, -0.025), vec3f(0.064, 0.058, 0.072));
  d = smin(d, sd_ell(m - vec3f(0.040, -0.018, 0.0), vec3f(0.046, 0.046, 0.056)), 0.03);
  d = smin(d, sd_ell(m - vec3f(0.056, -0.040, -0.050), vec3f(0.045, 0.066, 0.055)), 0.03);
  d = smin(d, sd_muzzle(q), 0.035);
  d = smin(d, sd_flat_cone(q, vec3f(0.0, -0.044, -0.010), vec3f(0.0, -0.036, 0.152), 0.032, 0.015, 1.25), 0.012);
  d = smin(d, sd_ell(q - vec3f(0.0, 0.006, 0.188), vec3f(0.0155, 0.011, 0.011)), 0.006);
  d = smin(d, sd_ell(m - vec3f(0.030, 0.034, 0.038), vec3f(0.022, 0.014, 0.022)), 0.02);
  d = min(d, sd_sphere(m - EYE_C, 0.0100));
  d = smin(d, sd_ear(m), 0.014);
  return d * HEAD_SCALE;
}
fn part_tail(i: u32, q: vec3f) -> f32 {
  if (i == 5u) { return sd_round_cone(q, J_TAIL0, J_TAIL1, 0.040, 0.064); }
  if (i == 6u) { return sd_round_cone(q, J_TAIL1, J_TAIL2, 0.064, 0.054); }
  return sd_round_cone(q, J_TAIL2, J_TAIL3, 0.054, 0.014);
}
fn part_scapula(m: vec3f) -> f32 { return sd_round_cone(m, J_SCAPULA, J_SHOULDER, 0.045, 0.058); }
fn part_upperarm(m: vec3f) -> f32 {
  let h = sd_round_cone(m, J_SHOULDER, J_ELBOW + vec3f(-0.008, 0.010, 0.0), 0.058, 0.040);
  let t = sd_round_cone(m, vec3f(0.055, 0.555, 0.245), J_ELBOW + vec3f(-0.008, 0.008, -0.020), 0.055, 0.035);
  return smin(h, t, 0.05);
}
fn part_forearm(m: vec3f) -> f32 {
  return sd_flat_cone(m, J_ELBOW + vec3f(0.0, -0.010, 0.005), J_CARPUS, 0.050, 0.025, 1.2);
}
fn part_manus(m: vec3f) -> f32 {
  let d = sd_round_cone(m, J_CARPUS, J_FPAW + vec3f(0.0, 0.040, -0.008), 0.026, 0.024);
  return smin(d, sd_paw(m - J_FPAW, 1.25), 0.02);
}
fn part_thigh(m: vec3f) -> f32 {
  var d = sd_flat_cone(m, J_HIP + vec3f(-0.005, -0.005, -0.030), J_STIFLE + vec3f(-0.010, 0.010, -0.045), 0.112, 0.062, 1.7);
  d = smin(d, sd_flat_cone(m, vec3f(0.060, 0.610, -0.540), vec3f(0.072, 0.330, -0.505), 0.065, 0.045, 1.4), 0.06);
  d = smin(d, sd_flat_cone(m, vec3f(0.045, 0.580, -0.220), J_STIFLE + vec3f(-0.012, 0.020, -0.010), 0.045, 0.040, 1.5), 0.05);
  return smax(d, 0.022 - m.x, 0.02);
}
fn part_gaskin(m: vec3f) -> f32 {
  let d = sd_flat_cone(m, J_STIFLE + vec3f(-0.007, -0.010, -0.070), J_HOCK + vec3f(0.0, 0.020, -0.005), 0.058, 0.027, 1.6);
  return smin(d, sd_round_cone(m, vec3f(0.078, 0.360, -0.515), J_HOCK + vec3f(0.0, 0.005, -0.024), 0.030, 0.018), 0.04);
}
fn part_pes(m: vec3f) -> f32 {
  let d = sd_round_cone(m, J_HOCK, J_HPAW + vec3f(0.0, 0.040, -0.010), 0.024, 0.021);
  return smin(d, sd_paw(m - J_HPAW, 1.12), 0.02);
}

/** One part's rest-space value. `q` is already mirrored to the left side for right-side limbs. */
fn rest_part(f: u32, q: vec3f, warped: bool) -> f32 {
  switch (f) {
    case 0u: { return part_croup(q); }
    case 1u: { return part_loin(q); }
    case 2u: { return part_chest(q); }
    case 3u: { return part_neck(q); }
    case 4u: { return part_head(q); }
    case 5u, 6u, 7u: { return part_tail(f, q); }
    case 8u: { return part_scapula(q); }
    case 9u: {
      if (warped) { return smin(part_upperarm(q), part_forearm(q), 0.03); }
      return part_upperarm(q);
    }
    case 10u: { return part_forearm(q); }
    case 11u: { return part_manus(q); }
    case 12u: { return part_thigh(q); }
    case 13u: {
      if (warped) { return smin(part_gaskin(q), part_pes(q), 0.02); }
      return part_gaskin(q);
    }
    default: { return part_pes(q); }
  }
}

// ---------------- colour (creature.wgsl's albedo, with tail and head weights as channels) ----------------
// ch.x: how much the point belongs to the tail; ch.y: to the head (smooth-union blend weights).

fn wolf_albedo(p: vec3f, ch: vec2f) -> vec3f {
  let m = mirror_x(p);
  let n1 = fbm3(p * 14.0, 3);
  let n2 = noise3(p * vec3f(120.0, 30.0, 120.0));
  let cream = vec3f(0.66, 0.57, 0.43);
  let tawny = vec3f(0.45, 0.30, 0.15);
  let grey  = vec3f(0.26, 0.23, 0.19);
  let dark  = vec3f(0.032, 0.028, 0.024);
  let black = vec3f(0.014, 0.012, 0.010);

  var c = mix(grey, tawny, 0.45 * (1.0 - smoothstep(0.55, 0.68, p.y + 0.04 * n1)));
  let saddle = smoothstep(0.61, 0.71, p.y + 0.07 * n1) * smoothstep(-0.64, -0.52, p.z) * (1.0 - smoothstep(0.34, 0.50, p.z));
  c = mix(c, dark, 0.85 * saddle);
  var vline = mix(0.50, 0.60, smoothstep(0.12, -0.30, p.z));
  vline = mix(vline, 0.42, smoothstep(0.05, 0.09, m.x) * smoothstep(-0.26, -0.36, p.z));
  let ventral = 1.0 - smoothstep(vline - 0.03, vline + 0.04, p.y + 0.03 * n1);
  c = mix(c, cream, ventral);
  let bib = (1.0 - smoothstep(0.72, 0.84, p.y + 0.03 * n1)) * smoothstep(0.27, 0.37, p.z + 0.02 * n1) * (1.0 - 0.5 * smoothstep(0.07, 0.11, m.x));
  c = mix(c, cream, bib);
  let legw = 1.0 - smoothstep(0.38, 0.47, p.y + 0.03 * n1);
  let legc = mix(tawny, cream, clamp(smoothstep(0.25, 0.05, p.y) * 0.6 + 0.5 * smoothstep(0.08, 0.05, m.x), 0.0, 1.0));
  c = mix(c, legc, legw);
  let fa = J_CARPUS - J_ELBOW;
  let ft = clamp(dot(m - J_ELBOW, fa) / dot(fa, fa), 0.0, 1.0);
  let fo = m - (J_ELBOW + fa * ft);
  let stripe = smoothstep(0.012, 0.022, fo.z) * (1.0 - smoothstep(0.008, 0.016, abs(fo.x))) * smoothstep(0.05, 0.2, ft) * (1.0 - smoothstep(0.75, 0.95, ft));
  c = mix(c, dark, 0.75 * stripe);
  if (p.y < 0.03) {
    let claws = max(claw_mask((m - J_FPAW) / 1.25), claw_mask((m - J_HPAW) / 1.12));
    c = mix(c, vec3f(0.06, 0.05, 0.045), claws);
  }

  // tail
  if (ch.x > 0.001) {
    var tc = mix(grey, dark, 0.7 * smoothstep(-0.72, -0.62, p.z + 0.03 * n1));
    tc = mix(tc, cream, 0.5 * smoothstep(-0.68, -0.76, p.z));
    tc = mix(tc, black, smoothstep(0.40, 0.36, p.y + 0.02 * n1));
    c = mix(c, tc, ch.x);
  }

  // head
  let q = head_local(p);
  let wh = smoothstep(-0.14, -0.06, q.z) * smoothstep(0.70, 0.78, p.y);
  if (wh > 0.001) {
    let hm = mirror_x(q);
    var hc = mix(grey, dark, 0.35 * smoothstep(0.0, 0.06, q.y) * (1.0 - smoothstep(0.03, 0.06, hm.x)));
    hc = mix(hc, mix(tawny, dark, 0.25), 0.7 * smoothstep(0.02, 0.10, q.z));
    hc = mix(hc, cream, (1.0 - smoothstep(-0.032, -0.010, q.y + 0.01 * n1)) * (1.0 - 0.6 * smoothstep(0.012, 0.0, hm.x - 0.0) * smoothstep(-0.03, -0.02, q.y)));
    hc = mix(hc, cream, 0.85 * (1.0 - smoothstep(0.008, 0.016, length(hm - vec3f(0.030, 0.034, 0.050)))));
    let lt = clamp((q.z - 0.020) / 0.172, 0.0, 1.0);
    let lipy = mix(0.004, -0.006, lt) - mix(0.040, 0.024, lt) + 0.003 + 0.008 * smoothstep(0.09, 0.05, q.z);
    let lip = (1.0 - smoothstep(0.0015, 0.0035, abs(q.y - lipy))) * smoothstep(0.045, 0.06, q.z);
    hc = mix(hc, dark, lip);
    hc = mix(hc, black, 1.0 - smoothstep(0.0, 0.004, sd_ell(q - vec3f(0.0, 0.006, 0.188), vec3f(0.0155, 0.011, 0.011))));
    let ec0 = hm - EYE_C;
    let liner = length(vec2f(ec0.z / 0.0155, (ec0.y + 0.30 * ec0.z) / 0.0085));
    hc = mix(hc, dark, (1.0 - smoothstep(0.9, 1.1, liner)) * smoothstep(0.015, 0.025, hm.x));
    let gz = dot(normalize(ec0 + vec3f(1e-6)), EYE_GAZE);
    var eyec = mix(black, vec3f(0.60, 0.33, 0.05), smoothstep(0.50, 0.62, gz));
    eyec = mix(eyec, black, smoothstep(0.90, 0.94, gz));
    hc = mix(hc, eyec, 1.0 - smoothstep(0.0100, 0.0115, length(ec0)));
    if (q.y > 0.0) {
      let e = ear_local(hm);
      let inear = (1.0 - smoothstep(0.0, 0.006, sd_ear_cavity(e))) * smoothstep(0.0, 0.008, e.z);
      let earw = 1.0 - smoothstep(0.0, 0.01, sd_ear(hm));
      var ec = mix(mix(grey, tawny, 0.6), dark, 0.7 * smoothstep(EAR_H - 0.03, EAR_H, e.y));
      ec = mix(ec, cream, inear);
      hc = mix(hc, ec, earw * smoothstep(0.02, 0.05, q.y));
    }
    c = mix(c, hc, wh);
  }

  c = c * (0.85 + 0.15 * n2) * (0.9 + 0.2 * n1);
  return clamp(c, vec3f(0.0), vec3f(1.0));
}

// ---------------- fur regions (new) ----------------

/** The direction the coat lies, in rest space (projected onto the surface later): back toward the
 *  tail on the body and head, down the legs and the tail. */
fn comb_dir(q: vec3f, ch: vec2f) -> vec3f {
  var c = vec3f(0.0, -0.30, -1.0);
  let leg = 1.0 - smoothstep(0.36, 0.50, q.y);
  c = mix(c, vec3f(0.0, -1.0, -0.12), leg);
  let belly = (1.0 - smoothstep(0.42, 0.52, q.y)) * smoothstep(-0.30, -0.20, q.z) * (1.0 - smoothstep(0.15, 0.25, q.z)) * (1.0 - smoothstep(0.03, 0.08, abs(q.x)));
  c = mix(c, vec3f(0.0, -1.0, 0.0), belly);
  c = mix(c, vec3f(0.0, -1.0, -0.3), ch.x);
  return normalize(c);
}

/** Eyes and nose: no coat at all (1), checked per volume step on head rays. */
fn bare_mask(q: vec3f) -> f32 {
  let h = head_local(q);
  let hm = mirror_x(h);
  let eye = 1.0 - smoothstep(0.0105, 0.0125, length(hm - EYE_C));
  let nose = 1.0 - smoothstep(0.0, 0.003, sd_ell(h - vec3f(0.0, 0.006, 0.188), vec3f(0.0155, 0.011, 0.011)));
  return max(eye, nose);
}

/** The coat at a rest-space surface point: x = strand length as a fraction of the shell (long on the
 *  back, mane, ruff, britches and tail; short on the legs and face), y = how deep the skin sits, as a
 *  fraction of the shell's inner depth (0 = bare: eyes, nose, claws; thin on the ears). */
fn fur_coat(q: vec3f, ch: vec2f) -> vec2f {
  var l = 0.72;
  var depth = 1.0;
  l = mix(l, 1.0, smoothstep(0.60, 0.74, q.y));                                   // back, mane
  l = mix(l, 1.0, smoothstep(0.25, 0.35, q.z) * smoothstep(0.55, 0.65, q.y));    // ruff and chest
  let britches = smoothstep(-0.46, -0.54, q.z) * smoothstep(0.30, 0.40, q.y);
  l = mix(l, 1.0, britches);
  let leg = 1.0 - smoothstep(0.26, 0.46, q.y);
  l = mix(l, 0.30, leg);                                                          // legs
  depth = mix(depth, 0.6, leg);
  l = mix(l, 1.0, ch.x);                                                           // tail
  if (ch.y > 0.001) {
    let h = head_local(q);
    let hm = mirror_x(h);
    l = mix(l, 0.30, ch.y);                                                        // head
    depth = mix(depth, 0.5, ch.y);
    let ear = (1.0 - smoothstep(0.0, 0.012, sd_ear(hm))) * smoothstep(0.02, 0.04, h.y);
    l = mix(l, 0.22, ear);
    depth = mix(depth, 0.25, ear);
    let eye = 1.0 - smoothstep(0.0105, 0.0125, length(hm - EYE_C));
    let nose = 1.0 - smoothstep(0.0, 0.003, sd_ell(h - vec3f(0.0, 0.006, 0.188), vec3f(0.0155, 0.011, 0.011)));
    let bare = max(eye, nose) * ch.y;
    l = mix(l, 0.0, bare);
    depth = mix(depth, 0.0, bare);
  }
  if (q.y < 0.03) {
    let m = mirror_x(q);
    let claw = max(claw_mask((m - J_FPAW) / 1.25), claw_mask((m - J_HPAW) / 1.12));
    l = mix(l, 0.0, claw);
    depth = mix(depth, 0.0, claw);
  }
  return vec2f(l, depth);
}
