// Position-based dynamics for cloth and hair. Appended to common.wgsl.
//
// One workgroup per sim group (a sheet, or a batch of 16 hair guides). Constraints never cross a
// group, so every substep and Jacobi iteration runs inside one dispatch: no per-iteration dispatch
// overhead. Positions ping-pong between the two halves of pbuf (X = [0, n), Y = [n, 2n)); with an
// odd iteration count the result lands back in X.
// Each barrier-separated step costs ~25 µs whatever the work (measured; README). Keeping a sheet in
// workgroup memory was tried and was slower: 32 KB per workgroup leaves one resident per core.

struct Sim {
  t: f32,            // time at the start of the frame (s)
  dt: f32,           // frame step (s)
  substeps: u32,
  iters: u32,        // odd
  wind: vec4f,       // xyz base wind (m/s); w gust strength
  n: u32,            // particles in X (offset of Y)
  group0: u32,       // first group of this dispatch
  npins: u32,
  pad: u32,
}

@group(0) @binding(0) var<uniform> S: Sim;
@group(0) @binding(1) var<storage, read_write> pbuf: array<vec4f>;
@group(0) @binding(2) var<storage, read_write> prev: array<vec4f>;
@group(0) @binding(3) var<storage, read> pinfo: array<vec4u>;
@group(0) @binding(4) var<storage, read> cons: array<vec4u>;
@group(0) @binding(5) var<storage, read> pins: array<vec4f>;      // [previous targets | current targets]
@group(0) @binding(6) var<storage, read> chars: array<vec4f>;
@group(0) @binding(7) var<storage, read> groups: array<vec4u>;

// F isn't bound here; common.wgsl's helpers that use it aren't called.
var<private> F: Frame;

const NONE: u32 = 0xffffffffu;
const OMEGA: f32 = 1.6;            // Jacobi over-relaxation
/** Position j from buffer X (0) or Y (1). */
fn ld(j: u32, which: u32) -> vec4f { return pbuf[which * S.n + j]; }
fn st(j: u32, which: u32, v: vec4f) { pbuf[which * S.n + j] = v; }

fn wind_at(p: vec3f, t: f32) -> vec3f {
  let gust = 0.55 + 0.45 * sin(0.83 * t + 0.21 * p.x - 0.1 * p.z) + 0.35 * noise3(p * 0.33 + vec3f(-1.9 * t, 0.2 * t, 0.7 * t));
  let height = 0.55 + 0.45 * clamp(p.y / 8.0, 0.0, 1.6);
  let swirl = vec3f(0.0, 0.35 * noise3(p * 0.5 + vec3f(0.0, t, 3.0)), 0.6 * noise3(p * 0.41 + vec3f(t * 1.3, 5.0, 0.0)));
  return (S.wind.xyz * (1.0 + S.wind.w * (gust - 0.55)) + swirl * length(S.wind.xyz) * 0.3) * height;
}

fn kind_wind(kind: u32) -> f32 {
  switch kind {
    case 0u: { return 0.3; }        // clothes
    case 1u: { return 0.55; }       // cape
    case 2u: { return 0.3; }        // banners hanging on the wall (sheltered)
    case 4u: { return 1.0; }        // flags
    default: { return 0.6; }        // hair
  }
}

fn pin_target(pin: u32, frac: f32) -> vec3f {
  return mix(pins[pin].xyz, pins[S.npins + pin].xyz, frac);
}

fn integrate(i: u32, frac: f32, time: f32, h: f32) {
  let inf = pinfo[2u * i];
  let x = ld(i, 0u);
  if (inf.w != NONE) {
    prev[i] = x;
    st(i, 1u, vec4f(pin_target(inf.w, frac), x.w));
    return;
  }
  let nb = pinfo[2u * i + 1u];
  let kind = inf.z >> 16u;
  var v = (x.xyz - prev[i].xyz) / h;
  let sp = length(v);
  if (sp > 25.0) { v *= 25.0 / sp; }
  prev[i] = x;
  var a = vec3f(0.0, -9.81, 0.0);
  let rel = wind_at(x.xyz, time) * kind_wind(kind) - v;
  if (kind == KIND_HAIR) {
    let tg = normalize(ld(nb.y, 0u).xyz - ld(nb.x, 0u).xyz + vec3f(0.0, 1e-6, 0.0));
    a += (rel - tg * dot(tg, rel)) * 1.6;
  } else {
    let n = cross(ld(nb.y, 0u).xyz - ld(nb.x, 0u).xyz, ld(nb.w, 0u).xyz - ld(nb.z, 0u).xyz);
    let nl = length(n);
    if (nl > 1e-9) {
      let nn = n / nl;
      let vn = dot(nn, rel);
      a += nn * vn * abs(vn) * 1.2 + rel * 0.06;
    }
  }
  let damp = 1.0 - 0.5 * h;
  st(i, 1u, vec4f(x.xyz + v * h * damp + a * h * h, x.w));
}

/** Pushes p out of a body's parts (each part separately), by margins that depend on what's colliding. */
fn collide(p_in: vec3f, inf: vec4u) -> vec3f {
  var p = p_in;
  let grp = inf.z & 0xffffu;
  let kind = inf.z >> 16u;
  if (grp < 0xff00u) {
    let base = grp * CS;
    let lo = chars[base + C_LO].xyz - vec3f(0.05);
    let hi = chars[base + C_HI].xyz + vec3f(0.05);
    if (all(p > lo) && all(p < hi)) {
      for (var b = 0u; b < NPARTS; b++) {
        var margin = 0.014;
        if (kind == KIND_HAIR) {
          if (b > 4u && b != 6u) { continue; }       // head, neck, torso, upper arms only
          margin = select(0.03, 0.005, b == P_HEAD);
          if (b == 2u) { margin = 0.012; }
          if (b == 4u || b == 6u) { margin = 0.03; }
        } else if (kind == KIND_CAPE) {
          margin = 0.013;
        }
        let a = chars[base + C_PARTS + 2u * b];
        let e = chars[base + C_PARTS + 2u * b + 1u];
        let g = round_cone_g(p, a.xyz, e.xyz, a.w, e.w);
        let pen = g.x - margin;
        if (pen < 0.0) { p -= g.yzw * pen; }
      }
    }
  } else if (grp == G_TOWER) {
    let q = p.xz - TOWER_C;
    let r = length(q);
    let rr = TOWER_R + 0.03;
    if (r < rr && p.y < TOWER_TOP) { p = vec3f(TOWER_C.x + q.x / r * rr, p.y, TOWER_C.y + q.y / r * rr); }
  }
  p.y = max(p.y, 0.01);
  return p;
}

fn solve(i: u32, src: u32, dst: u32, last: bool) {
  let inf = pinfo[2u * i];
  let x = ld(i, src);
  if (inf.w != NONE) { st(i, dst, x); return; }
  var acc = vec3f(0.0);
  var wsum = 0.0;
  for (var c = inf.x; c < inf.x + inf.y; c++) {
    let cc = cons[c];
    let xj = ld(cc.x, src);
    let rest = bitcast<f32>(cc.y);
    let k = bitcast<f32>(cc.z);
    let tether = bitcast<f32>(cc.w) > 0.5;
    let dv = x.xyz - xj.xyz;
    let L = length(dv);
    let C = L - rest;
    if (tether && C <= 0.0) { continue; }
    let ws = x.w + xj.w;
    if (L < 1e-9 || ws <= 0.0) { continue; }
    acc -= (x.w / ws) * C * (dv / L) * k;
    wsum += k;
  }
  let p = x.xyz + acc * (OMEGA / max(wsum, 1.0));
  // Collisions once per substep, on its last iteration (standard PBD practice).
  st(i, dst, vec4f(select(p, collide(p, inf), last), x.w));
}

override WG: u32 = 256u;

@compute @workgroup_size(WG)
fn simulate(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_index) lid: u32) {
  let gr = groups[S.group0 + wg.x];
  let start = gr.x;
  let end = gr.x + gr.y;
  let h = S.dt / f32(S.substeps);
  for (var s = 0u; s < S.substeps; s++) {
    let frac = f32(s + 1u) / f32(S.substeps);
    let time = S.t + h * f32(s + 1u);
    for (var i = start + lid; i < end; i += WG) { integrate(i, frac, time, h); }
    storageBarrier();
    for (var k = 0u; k < S.iters; k++) {
      let even = (k & 1u) == 0u;
      let src = select(0u, 1u, even);
      let dst = select(1u, 0u, even);
      let last = k + 1u == S.iters;
      for (var i = start + lid; i < end; i += WG) { solve(i, src, dst, last); }
      storageBarrier();
    }
  }
}
