// Cooking the field into the cache, on the GPU, with no readback. Appended to common + field.
//
//   classify   one thread per new brick: the field's bound at the centre proves most bricks empty.
//              The rest are candidates.
//   refine     27 threads per candidate: the field on a 3³ grid proves more bricks empty (every
//              point is within 0.433 of a brick of a sample, against 0.866 for the centre alone).
//              Releases slots of bricks that became empty; keeps the slot of bricks that stay.
//   allocate   surface bricks without a slot pop one from the free stack.
//   cook       one workgroup per brick: 9³ samples into the atlas.
//   args_*     turn counters into indirect dispatch sizes.
//   hcook      the raster path's heightfield clipmap: height and slope per sample.

struct CookU { jobs: u32, nboxes: u32, cap: u32, cookx: u32 }

@group(0) @binding(10) var<storage, read_write> pagew: array<u32>;
@group(0) @binding(11) var atlasw: texture_storage_3d<r16float, write>;
@group(0) @binding(12) var<storage, read> boxes: array<vec4i>;
@group(0) @binding(13) var<storage, read_write> counters: array<atomic<i32>, 16>;
@group(0) @binding(14) var<storage, read_write> freestack: array<u32>;
@group(0) @binding(15) var<storage, read_write> needl: array<vec4i>;
@group(0) @binding(16) var<storage, read_write> cookl: array<vec4i>;
@group(0) @binding(17) var<uniform> CU: CookU;
@group(0) @binding(18) var<storage, read_write> args: array<u32, 12>;
@group(0) @binding(26) var<storage, read_write> candl: array<vec4i>;
@group(0) @binding(19) var hmapw: texture_storage_2d_array<r32float, write>;
@group(0) @binding(20) var nmapw: texture_storage_2d_array<rgba16float, write>;

const C_COOK: u32 = 0u;
const C_NEED: u32 = 1u;
const C_OVERFLOW: u32 = 2u;
const C_RELEASED: u32 = 3u;
const C_CAND: u32 = 4u;
const C_REFINED: u32 = 5u;   // candidates the 3³ test proved empty
const C_FREE: u32 = 7u;      // free-stack top; persists across updates (0..6 are cleared per update)
const E_REFINED: u32 = 0x10000u;

const U_UNKNOWN: u32 = 0xFFFFFFFFu;
const U_ALLOC: u32 = 0x80000000u;

/** Job j → (level, absolute brick coordinate), through the box list (prefix in b1.w). */
fn job_brick(j: u32) -> vec4i {
  var bx = 0u;
  for (var k = 1u; k < CU.nboxes; k++) {
    if (j >= u32(boxes[2u * k + 1u].w)) { bx = k; } else { break; }
  }
  let b0 = boxes[2u * bx];
  let b1 = boxes[2u * bx + 1u];
  let local = j - u32(b1.w);
  let sx = u32(b1.x);
  let sy = u32(b1.y);
  let l = vec3i(i32(local % sx), i32((local / sx) % sy), i32(local / (sx * sy)));
  return vec4i(b0.yzw + l, b0.x);
}

fn page_index(L: u32, ab: vec3i) -> u32 {
  let t = vec3u(ab & vec3i(i32(F.dims.w) - 1));
  return ((L * F.dims.w + t.z) * F.dims.w + t.y) * F.dims.w + t.x;
}

@compute @workgroup_size(64)
fn classify(@builtin(global_invocation_id) gid: vec3u, @builtin(num_workgroups) nw: vec3u) {
  let j = gid.x + gid.y * nw.x * 64u;
  if (j >= CU.jobs) { return; }
  let jb = job_brick(j);
  let L = u32(jb.w);
  let cell = F.misc.x * exp2(f32(L));
  let center = (vec3f(jb.xyz) + 0.5) * (BRICK * cell) - F.shift.xyz;
  let d = world(center, 0.5 * cell);
  let idx = page_index(L, jb.xyz);
  if (abs(d) > F.lv[L].bnd.w + F.lv[L].bnd.z) {
    let old = pagew[idx];
    pagew[idx] = pack2x16float(vec2f(clamp(d, -60000.0, 60000.0), 0.0)) & 0xFFFFu;
    if (old != U_UNKNOWN && (old & U_ALLOC) != 0u) {
      let k = atomicAdd(&counters[C_FREE], 1);
      freestack[u32(k)] = old & 0x7FFFFFFFu;
      atomicAdd(&counters[C_RELEASED], 1);
    }
  } else {
    let k = atomicAdd(&counters[C_CAND], 1);
    if (u32(k) < 2u * CU.cap) { candl[u32(k)] = jb; }
    else { atomicAdd(&counters[C_OVERFLOW], 1); pagew[idx] = U_UNKNOWN; }
  }
}

var<workgroup> rvals: array<f32, 32>;
var<workgroup> rcount: u32;

@compute @workgroup_size(32)
fn refine(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_index) li: u32) {
  let ci = wg.x + wg.y * 512u;
  if (li == 0u) { rcount = min(u32(atomicLoad(&counters[C_CAND])), 2u * CU.cap); }
  let n = workgroupUniformLoad(&rcount);
  if (ci >= n) { return; }
  let jb = candl[ci];
  let L = u32(jb.w);
  let cell = F.misc.x * exp2(f32(L));
  let b = BRICK * cell;
  if (li < 27u) {
    let g = vec3f(f32(li % 3u), f32((li / 3u) % 3u), f32(li / 9u)) * 0.5;
    rvals[li] = world((vec3f(jb.xyz) + g) * b - F.shift.xyz, 0.5 * cell);
  }
  workgroupBarrier();
  if (li != 0u) { return; }
  var lo = INF;
  var pos = 0u;
  for (var i = 0u; i < 27u; i++) {
    lo = min(lo, abs(rvals[i]));
    if (rvals[i] > 0.0) { pos += 1u; }
  }
  let idx = page_index(L, jb.xyz);
  let old = pagew[idx];
  let had = old != U_UNKNOWN && (old & U_ALLOC) != 0u;
  if ((pos == 0u || pos == 27u) && lo > 0.4330127 * b + F.lv[L].bnd.z) {
    pagew[idx] = (pack2x16float(vec2f(clamp(rvals[13], -60000.0, 60000.0), 0.0)) & 0xFFFFu) | E_REFINED;
    atomicAdd(&counters[C_REFINED], 1);
    if (had) {
      let k = atomicAdd(&counters[C_FREE], 1);
      freestack[u32(k)] = old & 0x7FFFFFFFu;
      atomicAdd(&counters[C_RELEASED], 1);
    }
  } else if (had) {
    let k = atomicAdd(&counters[C_COOK], 1);
    cookl[u32(k)] = vec4i(jb.xyz, i32((L << 24u) | (old & 0xFFFFFFu)));
  } else {
    pagew[idx] = U_UNKNOWN;
    let k = atomicAdd(&counters[C_NEED], 1);
    if (u32(k) < CU.cap) { needl[u32(k)] = jb; } else { atomicAdd(&counters[C_OVERFLOW], 1); }
  }
}

@compute @workgroup_size(64)
fn allocate(@builtin(global_invocation_id) gid: vec3u) {
  let n = min(u32(atomicLoad(&counters[C_NEED])), CU.cap);
  if (gid.x >= n) { return; }
  let jb = needl[gid.x];
  let top = atomicSub(&counters[C_FREE], 1) - 1;
  if (top < 0) { atomicAdd(&counters[C_OVERFLOW], 1); return; }
  let slot = freestack[u32(top)];
  let L = u32(jb.w);
  pagew[page_index(L, jb.xyz)] = U_ALLOC | slot;
  let k = atomicAdd(&counters[C_COOK], 1);
  cookl[u32(k)] = vec4i(jb.xyz, i32((L << 24u) | slot));
}

@compute @workgroup_size(1)
fn args_refine() {
  let n = min(u32(max(atomicLoad(&counters[C_CAND]), 0)), 2u * CU.cap);
  args[8] = min(n, 512u);
  args[9] = (n + 511u) / 512u;
  args[10] = 1u;
}

@compute @workgroup_size(1)
fn args_alloc() {
  let n = min(u32(max(atomicLoad(&counters[C_NEED]), 0)), CU.cap);
  args[0] = (n + 63u) / 64u;
  args[1] = 1u;
  args[2] = 1u;
}

@compute @workgroup_size(1)
fn args_cook() {
  if (atomicLoad(&counters[C_FREE]) < 0) { atomicStore(&counters[C_FREE], 0); }
  let n = min(u32(max(atomicLoad(&counters[C_COOK]), 0)), CU.cap);
  args[4] = min(n, CU.cookx);
  args[5] = (n + CU.cookx - 1u) / CU.cookx;
  args[6] = 1u;
}

override NORMALIZE: bool = true;

var<workgroup> cs: array<f32, 729>;
var<workgroup> ccount: u32;

/** One workgroup per brick: 9³ samples into the atlas. With NORMALIZE, each sample is divided by
 *  the gradient magnitude of the cooked samples (central differences in workgroup memory): a
 *  first-order distance, up to 1/K times the field's bound on gentle terrain. It is an estimate,
 *  not a bound, so the hybrid (which needs a bound) uses a strict cook. Empty-brick classification
 *  always uses the strict field. */
@compute @workgroup_size(9, 9, 3)
fn cook(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_id) li: vec3u, @builtin(local_invocation_index) lii: u32) {
  let bi = wg.x + wg.y * CU.cookx;
  if (lii == 0u) { ccount = u32(max(atomicLoad(&counters[C_COOK]), 0)); }
  let n = workgroupUniformLoad(&ccount);
  if (bi >= n) { return; }
  let job = cookl[bi];
  let L = u32(job.w) >> 24u;
  let slot = u32(job.w) & 0xFFFFFFu;
  let cell = F.misc.x * exp2(f32(L));
  let sx = u32(F.misc.z);
  let sy = u32(F.misc.w);
  let s = vec3i(i32(slot % sx), i32((slot / sx) % sy), i32(slot / (sx * sy))) * 9;
  for (var z = li.z; z < 9u; z += 3u) {
    let si = job.xyz * 8 + vec3i(i32(li.x), i32(li.y), i32(z));
    let p = vec3f(si) * cell - F.shift.xyz;
    cs[(z * 9u + li.y) * 9u + li.x] = world(p, 0.5 * cell);
  }
  workgroupBarrier();
  for (var z = li.z; z < 9u; z += 3u) {
    var d = cs[(z * 9u + li.y) * 9u + li.x];
    if (NORMALIZE) {
      let x0 = max(li.x, 1u) - 1u; let x1 = min(li.x + 1u, 8u);
      let y0 = max(li.y, 1u) - 1u; let y1 = min(li.y + 1u, 8u);
      let z0 = max(z, 1u) - 1u; let z1 = min(z + 1u, 8u);
      let g = vec3f(
        (cs[(z * 9u + li.y) * 9u + x1] - cs[(z * 9u + li.y) * 9u + x0]) / f32(x1 - x0),
        (cs[(z * 9u + y1) * 9u + li.x] - cs[(z * 9u + y0) * 9u + li.x]) / f32(y1 - y0),
        (cs[(z1 * 9u + li.y) * 9u + li.x] - cs[(z0 * 9u + li.y) * 9u + li.x]) / f32(z1 - z0)) / cell;
      d = d / clamp(length(g), T_K, 1.0);
    }
    textureStore(atlasw, s + vec3i(i32(li.x), i32(li.y), i32(z)), vec4f(d, 0.0, 0.0, 0.0));
  }
}

// ---- the raster path's heightfield clipmap ----------------------------------------------------------
// Boxes here are (level, x0, 0, z0), (sx, 1, sz, prefix) in absolute sample indices; the sample
// spacing of level l is CU.cookx (as bits: f32) · 2^l. The texture is toroidal with period T = dims.w.

@compute @workgroup_size(64)
fn hcook(@builtin(global_invocation_id) gid: vec3u, @builtin(num_workgroups) nw: vec3u) {
  let j = gid.x + gid.y * nw.x * 64u;
  if (j >= CU.jobs) { return; }
  let jb = job_brick(j);
  let L = u32(jb.w);
  let s = bitcast<f32>(CU.cookx) * exp2(f32(L));
  let q = vec2f(f32(jb.x), f32(jb.z)) * s - F.shift.xz;
  let h = terrain_h(q, 0.5 * s);
  let e = 0.5 * s;
  let hx = (terrain_h(q + vec2f(e, 0.0), 0.5 * s) - terrain_h(q - vec2f(e, 0.0), 0.5 * s)) / (2.0 * e);
  let hz = (terrain_h(q + vec2f(0.0, e), 0.5 * s) - terrain_h(q - vec2f(0.0, e), 0.5 * s)) / (2.0 * e);
  let T = i32(F.dims.w);
  let tc = ((vec2i(jb.x, jb.z) % T) + T) % T;
  textureStore(hmapw, tc, i32(L), vec4f(h, 0.0, 0.0, 0.0));
  textureStore(nmapw, tc, i32(L), vec4f(hx, hz, 0.0, 0.0));
}

// ---- the rock scatter, cooked once: each cell's base height --------------------------------------

@group(0) @binding(27) var<storage, read_write> rockbase_w: array<f32>;

@compute @workgroup_size(64)
fn place_rocks(@builtin(global_invocation_id) gid: vec3u) {
  let nb = 2u * 86u;
  let nc = 2u * 18u;
  var i = gid.x;
  var k = boulders();
  var n = nb;
  if (i >= nb * nb) { i -= nb * nb; k = crags(); n = nc; if (i >= nc * nc) { return; } }
  let ci = vec2i(i32(i % n), i32(i / n)) - vec2i(i32(n / 2u));
  let r = rock_params(ci, k);
  rockbase_w[gid.x] = select(-1e9, rock_base(r), r.ok);
}
