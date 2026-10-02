// Scene: the material gallery. One simple field object per material on a forest floor, under a low
// golden-hour sun from behind and to one side.

const G_YMIN: f32 = -0.12;
const G_YMAX: f32 = 0.12;
const G_SLOPE: f32 = 0.12;
const G_HALF: f32 = 60.0;
const FOG: f32 = 0.0045;
const PUDDLE_R: f32 = 0.05;   // the slab's hollows are ~10 cm across
const LAYER_S: f32 = 0.6;

// The shrub's twigs and leaves, written by main.js (shrubData()) into SD.
@group(0) @binding(4) var<storage, read> SD: array<vec4f>;
// NTWIG and NLEAF are injected by main.js.
const TB: u32 = 1u;                    // twigs: 3 vec4 each: (a, ra), (b, rb), (c, rc)
const LB: u32 = 1u + 3u * NTWIG;       // leaves: 4 vec4 each: (origin, length), (x, width), (y, curl), (petiole start, 0)
const GB: u32 = LB + 4u * NLEAF;       // grid: (lo, cell size), (dims, margin), cells (offset, count), indices (4 per vec4)

const SHRUB_C = vec3f(0.62, 0.40, 1.0);
const SHRUB_R: f32 = 0.62;
const BUST_P = vec3f(1.85, 0.0, 0.55);
const BUST_R: f32 = 0.75;
const SOCK_C = vec3f(0.02, 0.32, 0.36);
const EYE_C0 = vec3f(0.0, 0.40, 0.25);
const EYE_R0: f32 = 0.075;
const SLAB_C = vec3f(-0.75, 0.06, -0.25);
const ROCK_C = vec3f(-1.4, 0.0, 0.85);
const GLOSS_C = vec3f(-2.15, 0.34, 0.3);
const GLOSS_R: f32 = 0.2;

fn ground_h(x: f32, z: f32) -> f32 {
  return 0.05 * sin(1.3 * x + 0.4) * cos(1.1 * z) + 0.03 * sin(2.3 * z + 1.0) * cos(1.7 * x);
}

// ---- the bust: a fennec-eared creature with one bat wing (skin, hide, membrane) ----------------------------

fn ear_d(q: vec3f, side: f32) -> f32 {
  // ear frame: tilted outward, opening toward −z
  var e = q - vec3f(0.075 * side, 0.62, 0.0);
  e = rot_z(e, -0.42 * side);
  e = rot_x(e, 0.12);
  let outer = sd_ell(e, vec3f(0.062, 0.16, 0.032));
  let cav = sd_ell(e - vec3f(0.0, 0.012, -0.013), vec3f(0.054, 0.15, 0.026));
  return smax(outer, -cav, 0.004);
}

const WRIST = vec3f(-0.30, 0.46, 0.06);
const SHOULDER = vec3f(-0.13, 0.30, 0.04);

fn wing_frame(q: vec3f) -> vec3f {
  // the membrane's plane: x, y in the plane (around the wrist), z through it
  return rot_y(q - WRIST, 0.35);
}

fn finger_tip(i: i32) -> vec2f {
  var ang = array<f32, 4>(0.30, 0.85, 1.45, 2.05);
  var len = array<f32, 4>(0.40, 0.44, 0.38, 0.30);
  return vec2f(-cos(ang[i]), sin(ang[i])) * len[i];
}

fn wing_d(q: vec3f) -> f32 {
  let w = wing_frame(q);
  // arm and finger bones
  var d = sd_round_cone(q, SHOULDER, WRIST, 0.022, 0.014);
  for (var i = 0; i < 4; i++) {
    let t = finger_tip(i);
    d = min(d, sd_round_cone(w, vec3f(0.0), vec3f(t, 0.0), 0.009, 0.0035));
  }
  // membrane: between the first and last finger, scalloped between tips
  let rho = length(w.xy);
  let th = atan2(w.y, -w.x);
  let a0 = 0.30;
  let a3 = 2.05;
  let seg = clamp((th - a0) / (a3 - a0) * 3.0, 0.0, 2.999);
  let k = i32(floor(seg));
  let f = seg - f32(k);
  let r0 = length(finger_tip(k));
  let r1 = length(finger_tip(k + 1));
  let R = mix(r0, r1, f) - 0.07 * sin(PI * f);
  let dr = (rho - R) * 0.85;
  let da = max(a0 - th, th - a3) * rho;
  let d2 = max(dr, da);
  let billow = 0.025 * sin(PI * f) * smoothstep(0.0, 0.25, rho);
  let mem = extrude(d2, w.z - billow, 0.0012);
  return min(d, 0.8 * mem);
}

fn bust_d(p: vec3f) -> f32 {
  let q = p - BUST_P;
  let body = sd_ell(q - vec3f(0.0, 0.2, 0.02), vec3f(0.17, 0.22, 0.15));
  let head = sd_ell(q - vec3f(0.0, 0.47, -0.02), vec3f(0.115, 0.10, 0.115));
  let snout = sd_ell(q - vec3f(0.0, 0.445, -0.13), vec3f(0.048, 0.042, 0.075));
  var d = smin(body, head, 0.06);
  d = smin(d, snout, 0.03);
  d = smin(d, ear_d(q, 1.0), 0.02);
  d = smin(d, ear_d(q, -1.0), 0.02);
  d = smin(d, wing_d(q), 0.02);
  return d;
}

// ---- the shrub: twigs and leaves -----------------------------------------------------------------------------

/** One leaf in its frame: x across, y from base (0) to tip (L), z its normal (upper side +z). */
fn leaf_sd(q: vec3f, L: f32, W: f32, curl: f32) -> f32 {
  let u = clamp(q.y / L, 0.0, 1.0);
  let hw = 0.5 * W * pow(max(sin(PI * pow(u, 0.75)), 0.0), 0.8) + 1e-4;
  let z = q.z - (curl * 2.0 * q.x * q.x / W + 0.10 * L * u * u);
  let ax = abs(q.x);
  let d2 = max((ax - hw) * 0.8, max(-q.y, q.y - L));
  // thickness: a 0.5 mm lamina, a midrib and secondary veins sweeping toward the tip
  let midrib = 0.0008 * exp(-(ax / 0.0016) * (ax / 0.0016)) * (1.0 - 0.7 * u);
  let sv = (q.y - 0.8 * ax) / 0.012;
  let dv = abs(sv - round(sv)) * 0.012 * 0.78;
  let vein = 0.00016 * exp(-(dv / 0.00035) * (dv / 0.00035)) * smoothstep(0.0, 0.005, hw - ax) * smoothstep(0.003, 0.007, ax);
  return 0.7 * extrude(d2, z, 0.00024 + midrib + vein);
}

fn leaf_q(i: u32, p: vec3f) -> vec3f {
  let b = LB + 4u * i;
  let r = p - SD[b].xyz;
  let X = SD[b + 1u].xyz;
  let Y = SD[b + 2u].xyz;
  return vec3f(dot(r, X), dot(r, Y), dot(r, cross(X, Y)));
}

fn leaf_d(i: u32, p: vec3f) -> f32 {
  let b = LB + 4u * i;
  let pet = sd_capsule(p, SD[b + 3u].xyz, SD[b].xyz, 0.0016);
  return min(leaf_sd(leaf_q(i, p), SD[b].w, SD[b + 1u].w, SD[b + 2u].w), pet);
}

fn leaf_bound(i: u32, p: vec3f) -> f32 {
  let b = LB + 4u * i;
  let L = SD[b].w;
  return length(p - (SD[b].xyz + SD[b + 2u].xyz * (0.5 * L))) - (0.5 * L + 0.035);
}

fn twig_d(i: u32, p: vec3f) -> f32 {
  let b = TB + 3u * i;
  return min(sd_round_cone(p, SD[b].xyz, SD[b + 1u].xyz, SD[b].w, SD[b + 1u].w),
             sd_round_cone(p, SD[b + 1u].xyz, SD[b + 2u].xyz, SD[b + 1u].w, SD[b + 2u].w));
}

/** The shrub: (distance, part key). Key 200 + i is twig i, 100 + i leaf i. */
fn shrub(p: vec3f, skip: u32) -> vec2f {
  let lo = SD[GB].xyz;
  let cs = SD[GB].w;
  let gd = SD[GB + 1u].xyz;
  let mg = SD[GB + 1u].w;
  let gp = (p - lo) / cs;
  if (any(gp < vec3f(0.0)) || any(gp >= gd)) {
    // Every part is at least mg inside the grid's box.
    let q = max(max(lo - p, p - (lo + gd * cs)), vec3f(0.0));
    return vec2f(length(q) + mg, 0.0);
  }
  let gi = vec3u(gd);
  let ci = min(vec3u(gp), gi - 1u);
  let cell = SD[GB + 2u + ci.x + gi.x * (ci.y + gi.y * ci.z)];
  let off = u32(cell.x);
  let ib = GB + 2u + gi.x * gi.y * gi.z;
  var best = vec2f(mg, 0.0);      // unlisted parts are at least mg away
  for (var k = 0u; k < u32(cell.y); k++) {
    let e = off + k;
    let id = u32(SD[ib + e / 4u][e % 4u]);
    if (id >= 1000u) {
      let d = twig_d(id - 1000u, p);
      if (d < best.x) { best = vec2f(d, f32(200u + id - 1000u)); }
    } else {
      if (leaf_bound(id, p) > best.x || 100u + id == skip) { continue; }
      let d = leaf_d(id, p);
      if (d < best.x) { best = vec2f(d, f32(100u + id)); }
    }
  }
  return best;
}

// ---- the lidded eye ----------------------------------------------------------------------------------------------

const EYE_G0 = vec3f(0.24, 0.12, -0.963);

fn gallery_eye() -> EyeF {
  let g = normalize(EYE_G0);
  let u = normalize(cross(vec3f(0.0, 1.0, 0.0), g));
  return EyeF(EYE_C0, g, u, cross(g, u), EYE_R0, 1.0);
}

fn eyeball_d(p: vec3f) -> f32 {
  let e = gallery_eye();
  return eye_sdf(eye_local(e, p - e.c), e.R);
}

fn socket_d(p: vec3f) -> f32 {
  let e = gallery_eye();
  let blob = sd_ell(p - SOCK_C - vec3f(0.0, 0.0, 0.14), vec3f(0.24, 0.33, 0.20));
  let brow = sd_ell(p - (EYE_C0 + vec3f(0.0, 0.075, 0.03)), vec3f(0.13, 0.05, 0.08));
  // the almond opening the eye looks out of
  let q = eye_local(e, p - e.c);
  let open = sd_ell(q - vec3f(0.0, 0.0, 0.07), vec3f(0.092, 0.052, 0.12));
  return smax(smin(blob, brow, 0.05), -open, 0.018);
}

// ---- stones ------------------------------------------------------------------------------------------------------

fn slab_d(p: vec3f) -> f32 {
  let q = p - SLAB_C;
  var d = sd_round_box(q, vec3f(0.44, 0.075, 0.33), 0.045);
  d = smax(d, -sd_ell(q - vec3f(-0.13, 0.17, 0.02), vec3f(0.20, 0.12, 0.16)), 0.03);
  d = smax(d, -sd_ell(q - vec3f(0.19, 0.155, -0.10), vec3f(0.13, 0.10, 0.12)), 0.03);
  d = smax(d, -sd_sphere(q - vec3f(0.22, 0.165, 0.17), 0.095), 0.025);
  if (d < 0.04) { d -= 0.008 * fbm3(p * 7.0, 2); }
  return 0.85 * d;
}

fn rock_d(p: vec3f) -> f32 {
  let q = p - ROCK_C;
  var d = sd_ell(q - vec3f(0.0, 0.18, 0.0), vec3f(0.36, 0.30, 0.31));
  d = smin(d, sd_ell(q - vec3f(0.28, 0.12, -0.12), vec3f(0.22, 0.18, 0.22)), 0.06);
  d = smin(d, sd_round_box(rot_y(q - vec3f(-0.22, 0.08, -0.05), 0.5), vec3f(0.18, 0.12, 0.2), 0.05), 0.05);
  // cracks
  let c1 = abs(dot(q, normalize(vec3f(0.9, 0.15, 0.4))) - 0.04) - 0.006;
  let c2 = abs(dot(q, normalize(vec3f(-0.3, 0.2, 1.0))) + 0.05) - 0.005;
  d = smax(d, -min(c1, c2), 0.012);
  if (d < 0.05) { d -= 0.014 * fbm3(p * 5.0, 2) + 0.004 * noise3(p * 23.0); }
  return 0.8 * d;
}

fn gloss_d(p: vec3f) -> f32 {
  let sph = length(p - GLOSS_C) - GLOSS_R;
  let q = p - vec3f(GLOSS_C.x, 0.0, GLOSS_C.z);
  let ped = sd_round_box(q - vec3f(0.0, 0.08, 0.0), vec3f(0.11, 0.08, 0.11), 0.02);
  return min(sph, ped);
}

// ---- the scene -------------------------------------------------------------------------------------------------

/** Every object: (distance, part key). Keys: 1 bust, 2 shrub (see shrub()), 3 socket, 4 eyeball,
 *  5 slab, 6 rock, 7 gloss sphere, 8 pedestal. Far objects cost one bound each. */
fn scene_k(p: vec3f) -> vec2f { return scene_kx(p, 0u); }

/** scene_k without the part `skip` (0: nothing skipped). */
fn scene_kx(p: vec3f, skip: u32) -> vec2f {
  var best = vec2f(1e5, 0.0);
  let bb = length(p - (BUST_P + vec3f(-0.1, 0.4, 0.0))) - BUST_R;
  if (bb < best.x && skip != 1u) { let d = bust_d(p); if (d < best.x) { best = vec2f(d, 1.0); } }
  let bs = length(p - SHRUB_C) - SHRUB_R;
  if (bs < best.x) { let r = shrub(p, skip); if (r.x < best.x) { best = r; } }
  let be = length(p - (SOCK_C + vec3f(0.0, 0.0, 0.1))) - 0.42;
  if (be < best.x && skip != 3u) {
    let ds = socket_d(p);
    if (ds < best.x) { best = vec2f(ds, 3.0); }
    let de = eyeball_d(p);
    if (de < best.x) { best = vec2f(de, 4.0); }
  }
  let bl = length(p - SLAB_C) - 0.62;
  if (bl < best.x) { let d = slab_d(p); if (d < best.x) { best = vec2f(d, 5.0); } }
  let br = length(p - (ROCK_C + vec3f(0.0, 0.18, 0.0))) - 0.62;
  if (br < best.x) { let d = rock_d(p); if (d < best.x) { best = vec2f(d, 6.0); } }
  let bg = length(p - (GLOSS_C - vec3f(0.0, 0.12, 0.0))) - 0.36;
  if (bg < best.x) {
    let ds = length(p - GLOSS_C) - GLOSS_R;
    if (ds < best.x) { best = vec2f(ds, 7.0); }
    let q = p - vec3f(GLOSS_C.x, 0.0, GLOSS_C.z);
    let dp = sd_round_box(q - vec3f(0.0, 0.08, 0.0), vec3f(0.11, 0.08, 0.11), 0.02);
    if (dp < best.x) { best = vec2f(dp, 8.0); }
  }
  return best;
}

fn obj(p: vec3f) -> f32 { return scene_k(p).x; }

fn obj_except(p: vec3f, key: u32) -> f32 { return scene_kx(p, key).x; }

/** The part at (or nearest) p, and its translucent medium (0: opaque, 1: hide, 2: leaf). */
fn part_key(p: vec3f) -> u32 { return u32(scene_k(p).y); }
fn part_medium(key: u32) -> u32 {
  if (key == 1u || key == 3u) { return 1u; }
  if (key >= 100u && key < 200u) { return 2u; }
  return 0u;
}
fn obj_except_k(p: vec3f, key: u32) -> vec2f { return scene_kx(p, key); }

fn obj_range(o: vec3f, d: vec3f) -> vec2f {
  return ray_box(o, d, vec3f(-2.5, -0.15, -0.7), vec3f(2.75, 1.25, 1.75));
}

/** The field an inward march sees: only the part that was hit (a per-ray part mask). */
fn part_field(p: vec3f, key: u32) -> f32 {
  if (key == 1u) { return bust_d(p); }
  if (key == 3u) { return min(socket_d(p), eyeball_d(p)); }
  if (key >= 100u && key < 200u) { return leaf_d(key - 100u, p); }
  return obj(p);
}

fn obj_id(p: vec3f) -> u32 {
  let k = u32(scene_k(p).y);
  if (k == 1u || k == 3u) { return M_HIDE; }
  if (k == 4u) { return M_EYE; }
  if (k == 5u) { return M_SLAB; }
  if (k == 6u || k == 8u) { return M_STONE; }
  if (k == 7u) { return M_GLOSS; }
  if (k >= 200u) { return M_WOOD; }
  if (k >= 100u) { return M_LEAF; }
  return M_STONE;
}

fn eye_frame(p: vec3f) -> EyeF { return gallery_eye(); }

fn wet_of(id: u32) -> f32 { return select(0.0, 1.0, id == M_SLAB); }

fn surf(p: vec3f, id: u32, fp: f32) -> Surf {
  if (id == M_GROUND) {
    let nz = fbm_f(p, 3.0, 4, fp);
    let moss = smoothstep(-0.25, 0.15, fbm_f(p + vec3f(4.0, 0.0, 9.0), 0.9, 3, fp));
    let litter = mix(vec3f(0.045, 0.032, 0.02), vec3f(0.10, 0.065, 0.032), clamp(0.5 + nz, 0.0, 1.0));
    let green = mix(vec3f(0.030, 0.050, 0.014), vec3f(0.06, 0.08, 0.02), clamp(0.5 + 1.5 * nz, 0.0, 1.0));
    var s = surf0(mix(litter, green, moss), 0.9);
    let dt = detail(p, 9.0, 0.006, 5, fp);
    s.dg = dt.g;
    s.dvar = dt.var_;
    return s;
  }
  if (id == M_HIDE) {
    let k = u32(scene_k(p).y);
    var s: Surf;
    if (k == 3u) {
      s = surf0(vec3f(0.42, 0.27, 0.20) * (0.85 + 0.3 * fbm_f(p, 20.0, 3, fp)), 0.5);
      s.key = 3u;
    } else {
      let q = p - BUST_P;
      var c = vec3f(0.55, 0.38, 0.22) * (0.85 + 0.3 * fbm_f(p, 25.0, 3, fp));
      // pale inner ears
      let inear = (1.0 - smoothstep(-0.004, 0.004, min(ear_d(q, 1.0), ear_d(q, -1.0)))) * smoothstep(-0.01, 0.03, -q.z);
      c = mix(c, vec3f(0.62, 0.42, 0.34), inear * smoothstep(0.55, 0.62, q.y));
      // dark eyes and nose on the head
      let eyes = min(length(mirror_x(q) - vec3f(0.055, 0.49, -0.1)) - 0.016, length(q - vec3f(0.0, 0.455, -0.205)) - 0.015);
      c = mix(vec3f(0.01), c, smoothstep(0.0, 0.004, eyes));
      // the wing's membrane and bones are darker
      c = mix(c, vec3f(0.30, 0.19, 0.15), 1.0 - smoothstep(0.0, 0.006, wing_d(q)));
      s = surf0(c, 0.55);
      s.key = 1u;
    }
    s.f0 = 0.03;
    s.tr = 1u;
    s.hn = 3e-4;
    return s;
  }
  if (id == M_LEAF) {
    let k = u32(scene_k(p).y);
    let i = k - 100u;
    let q = leaf_q(i, p);
    let b = LB + 4u * i;
    let L = SD[b].w;
    let up = q.z - (SD[b + 2u].w * 2.0 * q.x * q.x / SD[b + 1u].w + 0.10 * L * (q.y / L) * (q.y / L));
    let hue = hash_u(i * 7u + 3u);
    let top = mix(vec3f(0.035, 0.085, 0.012), vec3f(0.06, 0.10, 0.012), hue);
    let under = mix(vec3f(0.10, 0.16, 0.05), vec3f(0.13, 0.17, 0.06), hue);
    var c = select(under, top, up > 0.0);
    let rib = exp(-(abs(q.x) / 0.0022) * (abs(q.x) / 0.0022));
    c = mix(c, vec3f(0.16, 0.20, 0.06), 0.6 * rib);
    var s = surf0(c, select(0.6, 0.38, up > 0.0));
    s.f0 = 0.035;
    s.tr = 2u;
    s.key = k;
    s.hn = 5e-5;
    return s;
  }
  if (id == M_WOOD) {
    return surf0(vec3f(0.10, 0.07, 0.045), 0.7);
  }
  if (id == M_EYE) {
    var s = surf0(vec3f(0.5), 0.3);
    s.hn = 5e-4;
    return s;
  }
  if (id == M_GLOSS) {
    var s = surf0(vec3f(0.015, 0.05, 0.07), 0.12);
    s.f0 = 0.05;
    let dt = detail(p, 22.0, 0.0015, 5, fp);   // hammered: wavelengths 45 mm down to 2.7 mm
    s.dg = dt.g;
    s.dvar = dt.var_;
    return s;
  }
  // stone: the slab, the boulder and the pedestal
  let grain = fbm_f(p, 11.0, 4, fp);
  let base = select(vec3f(0.33, 0.31, 0.28), vec3f(0.30, 0.27, 0.23), id == M_SLAB);
  var s = surf0(base * (0.9 + 0.25 * grain), 0.7);
  let dt = detail(p, 30.0, 0.0012, 5, fp);
  s.dg = dt.g;
  s.dvar = dt.var_;
  return s;
}
