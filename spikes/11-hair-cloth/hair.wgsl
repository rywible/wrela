// Per-frame hair preparation, after the simulation. Appended to common.wgsl.
// - hair_bounds: the guides' box, which places the density grid;
// - hair_children: N render strands interpolated from the guides, written as capsule segments and
//   splatted into the density grid (density and summed direction);
// - hair_resolve: the splat → a filterable 3D texture (σ, direction), and a coarse occupancy;
// - hair_skip: a coarse distance-to-hair grid, for skipping empty space;
// - hair_light: transmittance toward the sun, at half resolution;
// - strand_bounds: per-strand boxes for the reference render (not measured).

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<storage, read> pos: array<vec4f>;
@group(0) @binding(2) var<storage, read> chars: array<vec4f>;
@group(0) @binding(3) var<storage, read_write> prims: array<vec4f>;
@group(0) @binding(4) var<storage, read_write> segs: array<vec4f>;
@group(0) @binding(5) var<storage, read> children: array<vec4u>;
@group(0) @binding(6) var<storage, read_write> splat: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> occ: array<atomic<u32>>;
@group(1) @binding(0) var hvol_w: texture_storage_3d<rgba16float, write>;
@group(1) @binding(1) var hskip_w: texture_storage_3d<r32float, write>;
@group(1) @binding(2) var hvol: texture_3d<f32>;
@group(1) @binding(3) var lsamp: sampler;
@group(1) @binding(4) var hskip: texture_3d<f32>;
@group(1) @binding(5) var hlight_w: texture_storage_3d<rgba16float, write>;

const DENS_FIX: f32 = 16.0;        // splat fixed point: units of 1/16 m⁻¹
const TAN_FIX: f32 = 256.0;
const SIGMA_SCALE: f32 = 1.0;      // extinction = wisp cross-section per volume × this
const SKIP_R: i32 = 3;             // skip search radius, coarse cells

var<workgroup> wlo: array<vec3f, 256>;
var<workgroup> whi: array<vec3f, 256>;

@compute @workgroup_size(256)
fn hair_bounds(@builtin(local_invocation_index) lid: u32) {
  let n = F.hair.x * F.hair.y;
  var lo = vec3f(1e30);
  var hi = vec3f(-1e30);
  for (var i = lid; i < n; i += 256u) {
    let p = pos[F.bins.w + i].xyz;
    lo = min(lo, p);
    hi = max(hi, p);
  }
  wlo[lid] = lo;
  whi[lid] = hi;
  workgroupBarrier();
  for (var s = 128u; s > 0u; s >>= 1u) {
    if (lid < s) {
      wlo[lid] = min(wlo[lid], wlo[lid + s]);
      whi[lid] = max(whi[lid], whi[lid + s]);
    }
    workgroupBarrier();
  }
  if (lid == 0u) {
    let m = 0.035;                                    // clump spread + strand radius + a voxel
    let a = wlo[0] - vec3f(m);
    let b = whi[0] + vec3f(m);
    let v = max(max((b.x - a.x) / f32(hd().x), (b.y - a.y) / f32(hd().y)), (b.z - a.z) / f32(hd().z));
    let c = 0.5 * (a + b);
    let g = c - 0.5 * vec3f(hd()) * v;
    prims[PR_HAIR] = vec4f(g, v);
    prims[PR_HAIR + 1u] = vec4f(g + vec3f(hd()) * v, 1.0 / v);
  }
}

fn guide(g: u32, k: u32) -> vec3f { return pos[F.bins.w + g * F.hair.y + k].xyz; }

/** Point k (may be −1 or S, extrapolated) of child c, before smoothing. */
fn child_base(g: u32, g2: u32, w: f32, off: vec2f, phase: f32, k_in: i32) -> vec3f {
  let S = i32(F.hair.y);
  let k = clamp(k_in, 0, S - 1);
  let p = mix(guide(g, u32(k)), guide(g2, u32(k)), w);
  let ka = u32(max(k - 1, 0));
  let kb = u32(min(k + 1, S - 1));
  let tg = normalize(guide(g, kb) - guide(g, ka) + vec3f(0.0, -1e-6, 0.0));
  let right = chars[C_RIGHT].xyz;
  var b1 = cross(tg, right);
  if (dot(b1, b1) < 1e-6) { b1 = cross(tg, vec3f(0.0, 0.0, 1.0)); }
  b1 = normalize(b1);
  let b2 = cross(tg, b1);
  let v = f32(k) / f32(S - 1);
  let clump = 0.0015 + 0.017 * pow(v, 0.9);
  let frizz = 0.0012 * sin(v * 31.0 + phase) * v;
  return p + b1 * (off.x * clump + frizz) + b2 * (off.y * clump);
}

var<workgroup> wbase: array<vec3f, 16>;
var<workgroup> wpts: array<vec3f, 32>;

/**
 * One workgroup per render strand: its 16 guide-interpolated points once each, then 31 Catmull-Rom
 * points, then 30 segments, each written and splatted into the density grid.
 */
@compute @workgroup_size(32)
fn hair_children(@builtin(workgroup_id) wg: vec3u, @builtin(num_workgroups) nw: vec3u, @builtin(local_invocation_index) lid: u32) {
  let c = wg.x + wg.y * nw.x;
  if (c >= F.misc.x) { return; }
  let S = F.hair.y;
  let sps = F.misc.y;
  let c0 = children[2u * c];
  let c1 = bitcast<vec4f>(children[2u * c + 1u]);
  let w = bitcast<f32>(c0.z);
  if (lid < S) { wbase[lid] = child_base(c0.x, c0.y, w, c1.xy, c1.z, i32(lid)); }
  workgroupBarrier();
  let span = f32(S - 1u) * min(c1.w, 1.0);
  if (lid <= sps) {
    let x = span * f32(lid) / f32(sps);
    let k = min(u32(floor(x)), S - 2u);
    let f = x - f32(k);
    let p1 = wbase[k];
    let p2 = wbase[k + 1u];
    var p0 = p1 * 2.0 - p2;
    if (k > 0u) { p0 = wbase[k - 1u]; }
    var p3 = p2 * 2.0 - p1;
    if (k + 2u < S) { p3 = wbase[k + 2u]; }
    let f2 = f * f;
    let f3 = f2 * f;
    wpts[lid] = 0.5 * ((2.0 * p1) + (-p0 + p2) * f + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * f3);
  }
  workgroupBarrier();
  if (lid >= sps) { return; }
  let j = c * sps + lid;
  let x0 = span * f32(lid) / f32(sps);
  let x1 = span * f32(lid + 1u) / f32(sps);
  let p0 = wpts[lid];
  let p1 = wpts[lid + 1u];
  let rbase = F.wind.w;
  let r0 = rbase * (1.0 - 0.6 * x0 / f32(S - 1u));
  let r1 = rbase * (1.0 - 0.6 * x1 / f32(S - 1u));
  segs[2u * j] = vec4f(p0, r0);
  segs[2u * j + 1u] = vec4f(p1, r1);

  // Splat: extinction is the wisps' projected area per volume, 2r·Δs / v³.
  let g = prims[PR_HAIR];
  let v = g.w;
  let len = length(p1 - p0);
  let n = clamp(u32(ceil(len / (0.5 * v))), 1u, 32u);
  let ds = len / f32(n);
  let tg = (p1 - p0) / max(len, 1e-9);
  let wfix = u32(2.0 * 0.5 * (r0 + r1) * ds / (v * v * v) * DENS_FIX + 0.5);
  let tfix = vec3i(tg * TAN_FIX);
  for (var k = 0u; k < n; k++) {
    let q = mix(p0, p1, (f32(k) + 0.5) / f32(n));
    let x = vec3i(floor((q - g.xyz) / v));
    if (any(x < vec3i(0)) || any(x >= vec3i(hd()))) { continue; }
    let idx = 4u * (u32(x.x) + hd().x * (u32(x.y) + hd().y * u32(x.z)));
    atomicAdd(&splat[idx], wfix);
    atomicAdd(&splat[idx + 1u], bitcast<u32>(tfix.x));
    atomicAdd(&splat[idx + 2u], bitcast<u32>(tfix.y));
    atomicAdd(&splat[idx + 3u], bitcast<u32>(tfix.z));
  }
}

@compute @workgroup_size(4, 4, 4)
fn hair_resolve(@builtin(global_invocation_id) gid: vec3u) {
  if (any(gid >= hd())) { return; }
  let idx = 4u * (gid.x + hd().x * (gid.y + hd().y * gid.z));
  let dn = atomicExchange(&splat[idx], 0u);
  let tx = bitcast<i32>(atomicExchange(&splat[idx + 1u], 0u));
  let ty = bitcast<i32>(atomicExchange(&splat[idx + 2u], 0u));
  let tz = bitcast<i32>(atomicExchange(&splat[idx + 3u], 0u));
  let sigma = f32(dn) / DENS_FIX * SIGMA_SCALE;
  var t = vec3f(f32(tx), f32(ty), f32(tz));
  let tl = length(t);
  t = select(vec3f(0.0, -1.0, 0.0), t / tl, tl > 0.0);
  textureStore(hvol_w, gid, vec4f(sigma, t));
  if (sigma > 0.0) {
    // Trilinear filtering reaches one voxel further: mark every coarse cell within a voxel.
    let cd = hd() / HCOARSE;
    let a = (vec3i(gid) - vec3i(1)) / i32(HCOARSE);
    let b = (vec3i(gid) + vec3i(1)) / i32(HCOARSE);
    for (var z = max(a.z, 0); z <= min(b.z, i32(cd.z) - 1); z++) {
      for (var y = max(a.y, 0); y <= min(b.y, i32(cd.y) - 1); y++) {
        for (var x = max(a.x, 0); x <= min(b.x, i32(cd.x) - 1); x++) {
          atomicOr(&occ[u32(x) + cd.x * (u32(y) + cd.y * u32(z))], 1u);
        }
      }
    }
  }
}

@compute @workgroup_size(4, 4, 4)
fn hair_skip(@builtin(global_invocation_id) gid: vec3u) {
  let cd = hd() / HCOARSE;
  if (any(gid >= cd)) { return; }
  let cell = f32(HCOARSE) * prims[PR_HAIR].w;
  var best = SKIP_R + 1;
  for (var z = -SKIP_R; z <= SKIP_R; z++) {
    for (var y = -SKIP_R; y <= SKIP_R; y++) {
      for (var x = -SKIP_R; x <= SKIP_R; x++) {
        let q = vec3i(gid) + vec3i(x, y, z);
        if (any(q < vec3i(0)) || any(q >= vec3i(cd))) { continue; }
        if (atomicLoad(&occ[u32(q.x) + cd.x * (u32(q.y) + cd.y * u32(q.z))]) != 0u) {
          best = min(best, max(max(abs(x), abs(y)), abs(z)));
        }
      }
    }
  }
  // Chebyshev distance k in cells ⇒ the nearest occupied point is at least (k − 1) cells away.
  textureStore(hskip_w, gid, vec4f(f32(max(best - 1, 0)) * cell, 0.0, 0.0, 0.0));
}

/** What else shades the hero's hair from the sun: its own head, neck and torso, and the tower. */
fn hair_occlusion(p: vec3f, l: vec3f) -> f32 {
  for (var b = 0u; b < 4u; b++) {
    let a = chars[C_PARTS + 2u * b];
    let e = chars[C_PARTS + 2u * b + 1u];
    let iv = ray_capsule(p + l * 0.004, l, a.xyz, e.xyz, max(a.w, e.w));
    // Only if the ray enters the body ahead of the sample: a grid cell centred inside the scalp
    // isn't shaded by the head it straddles.
    if (iv.x > 0.0 && iv.x <= iv.y) { return 0.06; }
  }
  return tower_occlusion(p, l);
}

@compute @workgroup_size(4, 4, 4)
fn hair_light(@builtin(global_invocation_id) gid: vec3u) {
  let ld = hd() / HLIGHT_DIV;
  if (any(gid >= ld)) { return; }
  let g = prims[PR_HAIR];
  let v = g.w;
  let l = F.sun.xyz;
  let p = g.xyz + (vec3f(gid) * f32(HLIGHT_DIV) + 0.5 * f32(HLIGHT_DIV)) * v;
  var tau = 0.0;
  var t = 0.0;
  for (var i = 0u; i < 400u; i++) {
    let x = (p + l * t - g.xyz) / v;
    if (any(x < vec3f(-1.0)) || any(x > vec3f(hd()) + 1.0)) { break; }
    let sk = textureLoad(hskip, vec3i(clamp(x / f32(HCOARSE), vec3f(0.0), vec3f(hd() / HCOARSE) - 1.0)), 0).x;
    if (sk > 0.0) { t += sk; continue; }
    tau += textureSampleLevel(hvol, lsamp, x / vec3f(hd()), 0.0).x * v;
    if (tau > 7.0) { break; }
    t += v;
  }
  let T = exp(-tau) * hair_occlusion(p, l);
  textureStore(hlight_w, gid, vec4f(T, 0.0, 0.0, 1.0));
}

@compute @workgroup_size(64)
fn strand_bounds(@builtin(global_invocation_id) gid: vec3u) {
  let c = gid.x;
  if (c >= F.misc.x) { return; }
  var lo = vec3f(1e30);
  var hi = vec3f(-1e30);
  for (var s = 0u; s < F.misc.y; s++) {
    let j = c * F.misc.y + s;
    let a = segs[2u * j];
    let b = segs[2u * j + 1u];
    let r = max(a.w, b.w) + 0.01;
    lo = min(lo, min(a.xyz, b.xyz) - vec3f(r));
    hi = max(hi, max(a.xyz, b.xyz) + vec3f(r));
  }
  prims[pr_strand(c)] = vec4f(lo, 0.0);
  prims[pr_strand(c) + 1u] = vec4f(hi, 0.0);
}
