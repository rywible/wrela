// Scene: a close-up of the wolf's head (wolf.wgsl) on a forest floor, sun low behind the head.

const G_YMIN: f32 = -0.12;
const G_YMAX: f32 = 0.12;
const G_SLOPE: f32 = 0.12;
const G_HALF: f32 = 60.0;
const FOG: f32 = 0.006;
const PUDDLE_R: f32 = 0.3;
const LAYER_S: f32 = 1.0;

const EYE_U = normalize(cross(vec3f(0.0, 1.0, 0.0), EYE_GAZE));
const EYE_W = cross(EYE_GAZE, EYE_U);

/** Head-frame (mirrored) coordinates → the eye's frame, z along the gaze. */
fn eye_basis(e: vec3f) -> vec3f { return vec3f(dot(e, EYE_U), dot(e, EYE_W), dot(e, EYE_GAZE)); }

fn ground_h(x: f32, z: f32) -> f32 {
  return 0.05 * sin(1.3 * x + 0.4) * cos(1.1 * z) + 0.03 * sin(2.3 * z + 1.0) * cos(1.7 * x);
}

// Part bounds (centre, radius), so far parts cost one length() (the union is unchanged, see below).
const TORSO_C = vec3f(0.0, 0.63, -0.08);
const TORSO_R: f32 = 0.56;
const NECK_C = vec3f(0.0, 0.82, 0.40);
const NECK_R: f32 = 0.32;
const HEAD_C = vec3f(0.0, 0.94, 0.66);
const HEAD_R: f32 = 0.24;
const ALL_C = vec3f(0.0, 0.72, 0.10);
const ALL_R: f32 = 0.80;

/** The wolf's head, neck and torso with the original fur displacement (creature.wgsl's field()).
 *  A part whose bound sphere is past the blend radius is replaced by that bound's distance: smin then
 *  returns the other operand exactly, so the union is unchanged where it matters and stays a bound. */
fn wolf(p: vec3f) -> f32 {
  let bt = length(p - TORSO_C) - TORSO_R;
  let bn = length(p - NECK_C) - NECK_R;
  let bh = length(p - HEAD_C) - HEAD_R;
  var dt = bt;
  if (bt < 0.1) { dt = sd_torso(p); }
  var dn = bn;
  if (bn < 0.1) { dn = sd_neck(p); }
  var dh = bh;
  if (bh < 0.08) { dh = sd_head(p); }
  var d = smin(dt, dn, 0.08);
  d = smin(d, dh, 0.05);
  if (d > 0.03) { return 0.9 * (d - 0.0145); }   // the displacement is at most 14.5 mm
  let fur = fbm3(p * vec3f(34.0, 30.0, 13.0), 3);
  let amp = 0.0045 * smoothstep(0.12, 0.35, p.y) * mix(0.2, 1.0, smoothstep(-0.01, 0.03, dh));
  let tufts = 0.5 + 0.5 * noise3(p * vec3f(40.0, 9.0, 22.0));
  let belly = (1.0 - smoothstep(0.47, 0.56, p.y)) * smoothstep(-0.22, -0.10, p.z) * (1.0 - smoothstep(0.18, 0.30, p.z)) * smoothstep(0.30, 0.40, p.y);
  return 0.9 * (d - amp * fur - 0.010 * tufts * belly);
}

fn obj(p: vec3f) -> f32 {
  let b = length(p - ALL_C) - ALL_R;
  if (b > 0.05) { return b; }
  return wolf(p);
}

fn obj_range(o: vec3f, d: vec3f) -> vec2f { return ray_sphere(o, d, ALL_C, ALL_R + 0.02); }

fn part_field(p: vec3f, key: u32) -> f32 { return wolf(p); }

/** Everything except the part `key`: the wolf is the only object. */
fn obj_except(p: vec3f, key: u32) -> f32 { return 1e5; }
fn obj_except_k(p: vec3f, key: u32) -> vec2f { return vec2f(1e5, 0.0); }
fn part_key(p: vec3f) -> u32 { return 0u; }
fn part_medium(key: u32) -> u32 { return 1u; }

fn nose_d(p: vec3f) -> f32 {
  let q = head_local(p);
  return sd_ell(q - vec3f(0.0, 0.006, 0.188), vec3f(0.0155, 0.011, 0.011)) * HEAD_SCALE;
}

fn eye_d(p: vec3f) -> f32 {
  let m = mirror_x(head_local(p));
  return eye_sdf(eye_basis(m - EYE_C), 0.0100) * HEAD_SCALE;
}

fn obj_id(p: vec3f) -> u32 {
  if (length(p - HEAD_C) < HEAD_R + 0.02) {
    if (eye_d(p) < 4e-4) { return M_EYE; }
    if (nose_d(p) < 1.5e-3) { return M_NOSE; }
  }
  return M_HIDE;
}

fn eye_frame(p: vec3f) -> EyeF {
  let q = head_local(p);
  let mir = vec3f(select(-1.0, 1.0, q.x >= 0.0), 1.0, 1.0);
  let c = J_HEAD + rot_x(EYE_C * mir * HEAD_SCALE, HEAD_PITCH);
  return EyeF(c, rot_x(EYE_GAZE * mir, HEAD_PITCH), rot_x(EYE_U * mir, HEAD_PITCH), rot_x(EYE_W * mir, HEAD_PITCH),
              0.01 * HEAD_SCALE, 0.0);
}

fn wet_of(id: u32) -> f32 { return select(0.0, 0.6, id == M_NOSE); }

/** creature.wgsl's albedo(), trimmed to the body, neck and head (no tail, legs or claws). */
fn wolf_albedo(p: vec3f) -> vec3f {
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

  let q = head_local(p);
  let hm = mirror_x(q);
  let wh = smoothstep(-0.14, -0.06, q.z) * smoothstep(0.70, 0.78, p.y);
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
  let e = ear_local(hm);
  let inear = (1.0 - smoothstep(0.0, 0.006, sd_ear_cavity(e))) * smoothstep(0.0, 0.008, e.z);
  let earw = 1.0 - smoothstep(0.0, 0.01, sd_ear(hm));
  var ec = mix(mix(grey, tawny, 0.6), dark, 0.7 * smoothstep(EAR_H - 0.03, EAR_H, e.y));
  ec = mix(ec, cream, inear);
  hc = mix(hc, ec, earw * smoothstep(0.02, 0.05, q.y));
  c = mix(c, hc, wh);

  c = c * (0.85 + 0.15 * n2) * (0.9 + 0.2 * n1);
  return clamp(c, vec3f(0.0), vec3f(1.0));
}

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
  if (id == M_NOSE) {
    var s = surf0(vec3f(0.012, 0.010, 0.009), 0.3);
    let dt = detail(p, 400.0, 0.0002, 3, fp);   // the nose leather's pebbling
    s.dg = dt.g;
    s.dvar = dt.var_;
    return s;
  }
  if (id == M_EYE) {
    var s = surf0(vec3f(0.5), 0.3);
    s.hn = 2e-4;
    return s;
  }
  var s = surf0(wolf_albedo(p), 0.62);
  s.f0 = 0.03;
  s.tr = 1u;
  s.hn = 4e-4;
  return s;
}
