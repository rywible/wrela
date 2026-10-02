// `pose`: one thread per instance builds its skeleton from the root state the CPU uploaded and
// writes its parts, rigid in world space, plus a bound sphere. crowd.js has the same code in JS
// (`poseJS`), to time posing on the CPU and to check this pass against.

@group(0) @binding(1) var<storage, read_write> inst: array<vec4f>;
@group(0) @binding(2) var<storage, read> state: array<vec4f>;    // S_STRIDE per instance
@group(0) @binding(3) var<storage, read> params: array<vec4f>;   // P_STRIDE per instance

const T_VILLAGER: u32 = 0u;
const T_ANIMAL: u32 = 1u;
const T_BIRD: u32 = 2u;

var<private> AX: vec3f;
var<private> AY: vec3f;
var<private> AZ: vec3f;
var<private> ROOT: vec3f;
var<private> OUT: u32;
var<private> LO: vec3f;
var<private> HI: vec3f;
var<private> K: f32;

fn xf(l: vec3f) -> vec3f { return ROOT + AX * l.x + AY * l.y + AZ * l.z; }

fn put(i: u32, a: vec3f, r1: f32, b: vec3f, r2: f32) {
  let wa = xf(a);
  let wb = xf(b);
  inst[OUT + I_PARTS + 2u * i] = vec4f(wa, r1);
  inst[OUT + I_PARTS + 2u * i + 1u] = vec4f(wb, r2);
  let r = max(r1, r2) + K;
  LO = min(LO, min(wa, wb) - vec3f(r));
  HI = max(HI, max(wa, wb) + vec3f(r));
}

/** A limb segment from o, swung by `ang` about the local x axis (forward positive). */
fn limb(o: vec3f, ang: f32, len: f32) -> vec3f { return o + len * vec3f(0.0, -cos(ang), sin(ang)); }

/** Parts: 0 torso, 1 head, 2–4 left thigh/shin/foot, 5–7 right, 8–9 left arm, 10–11 right arm,
 *  12 robe, 13 hat. p = (height, build, arm swing, stride). */
fn pose_villager(p: vec4f, en: u32, ph: f32) {
  let H = p.x;
  let w = p.y;
  let s = sin(ph);
  let c = cos(ph);
  let bob = 0.012 * H * cos(2.0 * ph);
  let hipY = 0.5 * H + bob;
  for (var side = 0u; side < 2u; side++) {
    let sg = select(-1.0, 1.0, side == 1u);
    let alpha = sg * p.w * s;
    let knee = 0.1 + 0.7 * max(0.0, sg * c);
    let hip = vec3f(sg * 0.085 * H * w, hipY, 0.0);
    let kn = limb(hip, alpha, 0.245 * H);
    let an = limb(kn, alpha - knee, 0.235 * H);
    let toe = an + vec3f(0.0, -0.012 * H, 0.07 * H);
    let i0 = 2u + 3u * side;
    put(i0, hip, 0.05 * H * w, kn, 0.038 * H);
    put(i0 + 1u, kn, 0.036 * H, an, 0.026 * H);
    put(i0 + 2u, an, 0.024 * H, toe, 0.02 * H);
    let beta = -sg * p.z * s;
    let sh = vec3f(sg * 0.118 * H * w, 0.765 * H + bob, 0.0);
    let el = limb(sh, beta, 0.17 * H) + vec3f(sg * 0.012 * H, 0.0, 0.0);
    let ha = limb(el, beta + 0.35, 0.16 * H);
    put(8u + 2u * side, sh, 0.034 * H * w, el, 0.028 * H);
    put(9u + 2u * side, el, 0.027 * H, ha, 0.022 * H);
  }
  put(0u, vec3f(0.0, hipY + 0.03 * H, 0.0), 0.09 * H * w, vec3f(0.0, 0.75 * H + bob, 0.01 * H), 0.105 * H * w);
  let hc = vec3f(0.0, 0.9 * H + bob, 0.015 * H);
  put(1u, hc - vec3f(0.0, 0.02 * H, 0.0), 0.058 * H, hc + vec3f(0.0, 0.012 * H, 0.004 * H), 0.062 * H);
  if ((en & (1u << 12u)) != 0u) {
    put(12u, vec3f(0.0, 0.56 * H + bob, 0.0), 0.085 * H * w, vec3f(0.0, 0.1 * H, 0.0), 0.15 * H * w);
  }
  if ((en & (1u << 13u)) != 0u) {
    put(13u, hc + vec3f(0.0, 0.045 * H, 0.0), 0.095 * H, hc + vec3f(0.0, 0.1 * H, 0.0), 0.05 * H);
  }
}

/** Parts: 0 body, 1 neck, 2 head, 3–10 legs (upper, lower; FL, FR, HL, HR), 11 tail.
 *  p = (body length, leg length, body radius, neck length), hs = head length, down = grazing. */
fn pose_animal(p: vec4f, hs: f32, down: f32, ph: f32) {
  let L = p.x;
  let legL = p.y;
  let rb = p.z;
  let bob = 0.015 * legL * cos(2.0 * ph);
  let by = legL + 0.35 * rb + bob;
  put(0u, vec3f(0.0, by, -0.5 * L), 0.88 * rb, vec3f(0.0, by + 0.04 * rb, 0.5 * L), rb);
  let na = mix(0.75, -0.8, down) + 0.06 * sin(2.0 * ph);
  let n0 = vec3f(0.0, by + 0.3 * rb, 0.5 * L + 0.2 * rb);
  let n1 = n0 + p.w * vec3f(0.0, sin(na), cos(na));
  put(1u, n0, 0.5 * rb, n1, 0.32 * rb);
  let ha = na - 0.9 + 0.5 * down;
  put(2u, n1, 0.34 * rb, n1 + hs * vec3f(0.0, sin(ha), cos(ha)), 0.2 * rb);
  let reach = by - 0.2 * rb;
  for (var l = 0u; l < 4u; l++) {
    let front = l < 2u;
    let sg = select(-1.0, 1.0, (l & 1u) == 1u);
    let off = select(0.0, PI, l == 1u || l == 2u);   // a trot: diagonal pairs together
    let lp = ph + off;
    let alpha = 0.36 * sin(lp);
    let flex = 0.55 * max(0.0, cos(lp));
    let j = vec3f(sg * 0.55 * rb, reach, select(-0.5 * L + 0.15 * rb, 0.5 * L - 0.1 * rb, front));
    let kn = limb(j, alpha, 0.5 * reach);
    let ft = limb(kn, alpha - flex, 0.5 * reach);
    put(3u + 2u * l, j, 0.06 + 0.13 * rb, kn, 0.04 + 0.07 * rb);
    put(4u + 2u * l, kn, 0.035 + 0.06 * rb, ft, 0.03 + 0.05 * rb);
  }
  let t0 = vec3f(0.0, by + 0.25 * rb, -0.5 * L - 0.75 * rb);
  let sway = 0.25 * sin(2.0 * ph + 1.0);
  put(11u, t0, 0.07 * rb + 0.02, t0 + 0.35 * L * vec3f(sway, -0.75, -0.45), 0.04 * rb + 0.012);
}

/** Parts: 0 body, 1 head, 2 beak, 3–4 left wing (inner, outer), 5–6 right, 7 tail.
 *  p = (span, flap amplitude, ..), glide in [0, 1] folds the flap away. */
fn pose_bird(p: vec4f, ph: f32, glide: f32) {
  let s = p.x;
  let th = p.y * (1.0 - glide) * sin(ph) + 0.12;
  put(0u, vec3f(0.0, 0.0, -0.16 * s), 0.035 * s, vec3f(0.0, 0.01 * s, 0.12 * s), 0.06 * s);
  put(1u, vec3f(0.0, 0.03 * s, 0.19 * s), 0.042 * s, vec3f(0.0, 0.04 * s, 0.22 * s), 0.04 * s);
  put(2u, vec3f(0.0, 0.035 * s, 0.25 * s), 0.016 * s, vec3f(0.0, 0.028 * s, 0.31 * s), 0.004 * s);
  for (var side = 0u; side < 2u; side++) {
    let sg = select(-1.0, 1.0, side == 1u);
    let sh = vec3f(sg * 0.04 * s, 0.025 * s, 0.05 * s);
    let el = sh + 0.27 * s * vec3f(sg * cos(th), sin(th), -0.03);
    let th2 = 1.35 * th - 0.04;
    let tip = el + 0.27 * s * vec3f(sg * cos(th2), sin(th2), -0.14);
    put(3u + 2u * side, sh, 0.04 * s, el, 0.028 * s);
    put(4u + 2u * side, el, 0.026 * s, tip, 0.006 * s);
  }
  put(7u, vec3f(0.0, 0.0, -0.14 * s), 0.02 * s, vec3f(0.0, 0.005 * s, -0.34 * s), 0.05 * s);
}

@compute @workgroup_size(64)
fn pose(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x;
  if (i >= F.dims.w) { return; }
  let s0 = state[i * S_STRIDE];
  let s1 = state[i * S_STRIDE + 1u];
  let p0 = params[i * P_STRIDE];       // type, enabled parts, secondary parts, blend k
  let p1 = params[i * P_STRIDE + 1u];  // shape
  let p2 = params[i * P_STRIDE + 2u];  // primary colour, shape
  let p3 = params[i * P_STRIDE + 3u];  // secondary colour, shape
  let ty = u32(p0.x);
  K = p0.w;
  OUT = i * STRIDE;
  var y = s0.y;
  if (ty != T_BIRD) { y = terrain_h(s0.x, s0.z); }
  ROOT = vec3f(s0.x, y, s0.z);
  let cy = cos(s0.w);
  let sy = sin(s0.w);
  let right = vec3f(cy, 0.0, -sy);
  let cb = cos(s1.y);
  let sb = sin(s1.y);
  AX = right * cb + vec3f(0.0, sb, 0.0);
  AY = -right * sb + vec3f(0.0, cb, 0.0);
  AZ = vec3f(sy, 0.0, cy);
  LO = vec3f(INF);
  HI = vec3f(-INF);
  let ph = 2.0 * PI * s1.x;
  if (ty == T_VILLAGER) {
    pose_villager(p1, u32(p0.y), ph);
  } else if (ty == T_ANIMAL) {
    pose_animal(p1, p2.w, p3.w, ph);
  } else {
    pose_bird(p1, ph, s1.z);
  }
  inst[OUT + I_HDR] = vec4f(K, p0.x, p0.y, p0.z);
  inst[OUT + I_COLA] = vec4f(p2.xyz, 0.0);
  inst[OUT + I_COLB] = vec4f(p3.xyz, 0.0);
  inst[OUT + I_BOUND] = vec4f(0.5 * (LO + HI), 0.5 * length(HI - LO));
}
