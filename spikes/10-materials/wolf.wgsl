// The wolf's head, neck and torso, copied from experiments/agent-authoring/wolf/creature.wgsl
// (an agent-authored field, D-095). Changes: legs, tail and paws are dropped; the eye spheres are
// replaced by eyeballs with a corneal bulge (eye_sdf, render.wgsl); the albedo is trimmed to match;
// a noisy smooth-union term is skipped when its bound (noise amplitude included) is at least d + k,
// where smin returns d exactly, so the field is unchanged.
// Helpers (sd_ell, smin, fbm3, ...) are fieldview's, in common.wgsl.

// ---------------- axial skeleton ----------------
const J_PELVIS  = vec3f(0.0, 0.632, -0.430);  // root: sacrum / hip centre
const J_LUMBAR  = vec3f(0.0, 0.655, -0.180);  // loin
const J_CHEST   = vec3f(0.0, 0.615,  0.095);  // thorax centre
const J_WITHERS = vec3f(0.0, 0.800,  0.200);  // top of the scapulae (shoulder height)
const J_NECK    = vec3f(0.0, 0.680,  0.310);  // neck base (C7)
const J_HEAD    = vec3f(0.0, 0.912,  0.592);  // skull centre
const HEAD_PITCH = 0.22;                      // nose-down tilt of the head bone (rad)
const HEAD_SCALE = 1.13;

// Round cone squashed sideways (x scaled by 1/sx), still a valid bound.
fn sd_flat_cone(p: vec3f, a: vec3f, b: vec3f, r1: f32, r2: f32, sx: f32) -> f32 {
  let s = vec3f(sx, 1.0, 1.0);
  return sd_round_cone(p * s, a * s, b * s, r1, r2) / sx;
}


fn head_local(p: vec3f) -> vec3f { return rot_x(p - J_HEAD, -HEAD_PITCH) / HEAD_SCALE; }

const EAR_BASE = vec3f(0.046, 0.040, -0.036);   // ear joint, head frame (left; mirrored)
// Ear frame: x across the ear, y from base to tip, z through it (+z = inner face).
fn ear_local(m: vec3f) -> vec3f {
  var e = m - EAR_BASE;
  e = rot_y(e, -0.35);  // opening faces forward and a little outward
  e = rot_z(e, 0.28);   // splay outward
  e = rot_x(e, 0.04);   // stand up (head pitch already tips them forward a little)
  return e;
}

// ---------------- parts ----------------
fn sd_torso(p: vec3f) -> f32 {
  var d = sd_ell(p - J_CHEST, vec3f(0.130, 0.180, 0.305));                                // ribcage
  d = smin(d, sd_ell(p - vec3f(0.0, 0.565, 0.320), vec3f(0.085, 0.105, 0.090)), 0.07);   // prosternum
  d = smin(d, sd_ell(p - (J_WITHERS + vec3f(0.0, -0.070, 0.010)), vec3f(0.075, 0.070, 0.140)), 0.06);
  d = smin(d, sd_ell(p - J_LUMBAR, vec3f(0.066, 0.088, 0.240)), 0.08);                   // loin
  d = smin(d, sd_ell(rot_x(p - (J_PELVIS + vec3f(0.0, 0.0, -0.010)), 0.30), vec3f(0.098, 0.112, 0.145)), 0.08); // croup, sloping to the tail
  return d;
}

fn sd_neck(p: vec3f) -> f32 {
  let b = J_HEAD + vec3f(0.0, -0.030, -0.070);
  var d = sd_round_cone(p, J_NECK, b, 0.122, 0.075);
  // ruff: fur mass on the throat and sides of the neck
  let r = sd_ell(p - (J_NECK + vec3f(0.0, 0.075, 0.140)), vec3f(0.112, 0.130, 0.105));
  if (r - 0.00525 < d + 0.05) { d = smin(d, r - 0.007 * fbm3(p * 16.0, 2), 0.05); }
  // mane over the withers
  let mn = sd_ell(p - (J_NECK + vec3f(0.0, 0.090, -0.040)), vec3f(0.105, 0.075, 0.120));
  if (mn - 0.0045 < d + 0.06) { d = smin(d, mn - 0.006 * fbm3(p * 16.0 + 3.0, 2), 0.06); }
  // hackles: raised fur along the top of the neck behind the head
  let hk = sd_ell(p - (J_NECK + vec3f(0.0, 0.165, 0.090)), vec3f(0.075, 0.050, 0.110));
  if (hk - 0.0045 < d + 0.05) { d = smin(d, hk - 0.006 * fbm3(p * 20.0 + 7.0, 2), 0.05); }
  return d;
}

const EAR_H = 0.086;
// Inner hollow of the ear (the opening), in ear frame.
fn sd_ear_cavity(e: vec3f) -> f32 {
  let ci = (e - vec3f(0.0, 0.012, 0.020)) * vec3f(1.12, 1.0, 1.45);
  return sd_round_cone(ci, vec3f(0.0), vec3f(0.0, EAR_H - 0.024, 0.0), 0.030, 0.004) / 1.45;
}
// Cupped ear: a cone flattened front-to-back, hollowed from the front.
fn sd_ear(m: vec3f) -> f32 {
  let e = ear_local(m);
  let outer = sd_round_cone(e * vec3f(1.0, 1.0, 1.45), vec3f(0.0), vec3f(0.0, EAR_H, 0.0), 0.038, 0.012) / 1.45;
  return smax(outer, -sd_ear_cavity(e), 0.004);
}

// Muzzle: tapered rounded box (flat top, flat sides), in head frame.
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
const EYE_C = vec3f(0.040, 0.018, 0.052);   // head frame, left eye
const EYE_GAZE = vec3f(0.74, 0.15, 0.656);  // unit, forward-outward

fn sd_head(p: vec3f) -> f32 {
  let q = head_local(p);
  let m = mirror_x(q);
  var d = sd_ell(q - vec3f(0.0, 0.010, -0.025), vec3f(0.064, 0.058, 0.072));             // cranium
  d = smin(d, sd_ell(m - vec3f(0.040, -0.018, 0.0), vec3f(0.046, 0.046, 0.056)), 0.03);  // cheeks (zygomatic + masseter)
  let cr = sd_ell(m - vec3f(0.056, -0.040, -0.050), vec3f(0.045, 0.066, 0.055));                        // cheek ruff
  if (cr - 0.003 < d + 0.03) { d = smin(d, cr - 0.004 * fbm3(q * 40.0, 2), 0.03); }
  d = smin(d, sd_muzzle(q), 0.035);                                                      // muzzle
  d = smin(d, sd_flat_cone(q, vec3f(0.0, -0.044, -0.010), vec3f(0.0, -0.036, 0.152), 0.032, 0.015, 1.25), 0.012); // lower jaw
  d = smin(d, sd_ell(q - vec3f(0.0, 0.006, 0.188), vec3f(0.0155, 0.011, 0.011)), 0.006);  // nose pad
  d = smin(d, sd_ell(m - vec3f(0.030, 0.034, 0.038), vec3f(0.022, 0.014, 0.022)), 0.02); // brow
  d = min(d, eye_sdf(eye_basis(m - EYE_C), 0.0100));                                    // eyes (spike 10: corneal eye)
  d = smin(d, sd_ear(m), 0.014);
  return d * HEAD_SCALE;
}

