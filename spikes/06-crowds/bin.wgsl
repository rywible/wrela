// Binning, made hierarchical so it doesn't cost tiles × instances (spike 02's one-level binning
// would be 32,400 tiles × 1,000 instances).
//
// `cull`: one thread per instance. Frustum test, then a conservative screen rectangle of the bound
//   sphere; the instance is appended to every overlapped coarse tile (64×64 px), and likewise to the
//   coarse cells (16²) of the sun's light grid.
// `bin_screen` / `bin_light`: one workgroup per coarse tile (cell) loads that list into shared
//   memory; each thread bins one fine 8×8 tile (light cell): sphere test, per-part capsule test,
//   near-to-far insertion keeping the nearest MAXE. Light bins inflate bounds by the penumbra, PEN.

@group(0) @binding(1) var<storage, read> inst: array<vec4f>;
@group(0) @binding(2) var<storage, read_write> ccount: array<atomic<u32>>;   // coarse list sizes
@group(0) @binding(3) var<storage, read_write> clist: array<u32>;            // CMAX per coarse list
@group(0) @binding(4) var<storage, read_write> tiles: array<u32>;
@group(0) @binding(5) var<storage, read_write> lbins: array<u32>;
@group(0) @binding(6) var<storage, read_write> stats: array<atomic<u32>, 32>;

const LBASE: u32 = 1024u;   // coarse light lists start here (screen coarse tiles ≤ 1024)

fn append(list: u32, i: u32, ovf: u32) {
  let slot = atomicAdd(&ccount[list], 1u);
  if (slot < CMAX) { clist[list * CMAX + slot] = i; } else { atomicAdd(&stats[ovf], 1u); }
}

@compute @workgroup_size(64)
fn cull(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x;
  if (i >= F.dims.w) { return; }
  let s = inst[i * STRIDE + I_BOUND];
  let r = s.w;
  // Screen: project the sphere's view-space box; that rectangle contains the sphere's.
  let tx = length(F.cr.xyz);
  let ty = length(F.cu.xyz);
  let rel = s.xyz - F.eye.xyz;
  let z = dot(rel, F.cf.xyz);
  let x = dot(rel, F.cr.xyz) / tx;
  let y = dot(rel, F.cu.xyz) / ty;
  if (z + r > 0.0) {
    var u = vec2f(-1.0, 1.0);
    var v = vec2f(-1.0, 1.0);
    if (z - r > 0.05) {
      let zn = z - r;
      let zf = z + r;
      u = vec2f(min((x - r) / zn, (x - r) / zf), max((x + r) / zn, (x + r) / zf)) / tx;
      v = vec2f(min((y - r) / zn, (y - r) / zf), max((y + r) / zn, (y + r) / zf)) / ty;
    }
    if (u.y >= -1.0 && u.x <= 1.0 && v.y >= -1.0 && v.x <= 1.0) {
      let cs = f32(TILE * CT);
      let gx = f32(F.grid.y - 1u);
      let gy = f32(F.grid.z - 1u);
      let x0 = u32(clamp(floor((u.x + 1.0) * 0.5 * f32(F.dims.x) / cs), 0.0, gx));
      let x1 = u32(clamp(floor((u.y + 1.0) * 0.5 * f32(F.dims.x) / cs), 0.0, gx));
      let y0 = u32(clamp(floor((1.0 - v.y) * 0.5 * f32(F.dims.y) / cs), 0.0, gy));
      let y1 = u32(clamp(floor((1.0 - v.x) * 0.5 * f32(F.dims.y) / cs), 0.0, gy));
      for (var cy = y0; cy <= y1; cy++) {
        for (var cx = x0; cx <= x1; cx++) { append(cy * F.grid.y + cx, i, S_COARSE_OVF); }
      }
    }
  }
  // Light grid: the sphere's disc in the sun's orthographic view, widened by the penumbra.
  let rl = r + PEN;
  let e = F.lc.w;
  let lrel = s.xyz - F.lc.xyz;
  let lx = dot(lrel, F.lr.xyz) + e;
  let ly = dot(lrel, F.lu.xyz) + e;
  if (lx + rl >= 0.0 && lx - rl <= 2.0 * e && ly + rl >= 0.0 && ly - rl <= 2.0 * e) {
    let lcell = 2.0 * e / f32(LCOARSE);
    let g = f32(LCOARSE - 1u);
    let x0 = u32(clamp(floor((lx - rl) / lcell), 0.0, g));
    let x1 = u32(clamp(floor((lx + rl) / lcell), 0.0, g));
    let y0 = u32(clamp(floor((ly - rl) / lcell), 0.0, g));
    let y1 = u32(clamp(floor((ly + rl) / lcell), 0.0, g));
    for (var cy = y0; cy <= y1; cy++) {
      for (var cx = x0; cx <= x1; cx++) { append(LBASE + cy * LCOARSE + cx, i, S_COARSE_LIGHT_OVF); }
    }
  }
}

var<workgroup> wsph: array<vec4f, CMAX>;
var<workgroup> widx: array<u32, CMAX>;
var<private> BL_I: array<u32, MAXE>;
var<private> BL_M: array<u32, MAXE>;
var<private> BL_K: array<f32, MAXE>;

fn load_list(list: u32, li: u32) -> u32 {
  let n = min(atomicLoad(&ccount[list]), CMAX);
  for (var k = li; k < n; k += 64u) {
    let i = clist[list * CMAX + k];
    widx[k] = i;
    wsph[k] = inst[i * STRIDE + I_BOUND];
  }
  return n;
}

/** Bins the shared list against four planes, keeping the `cap` nearest along `kd`. Returns
 *  (entries, dropped). */
fn bin_list(n: u32, pl: array<vec4f, 4>, kd: vec3f, cap: u32, inflate: f32) -> vec2u {
  var out = 0u;
  var dropped = 0u;
  for (var k = 0u; k < n; k++) {
    let s = wsph[k];
    if (!sphere_in(s + vec4f(0.0, 0.0, 0.0, inflate), pl)) { continue; }
    let i = widx[k];
    let base = i * STRIDE;
    let h = inst[base + I_HDR];
    var em = u32(h.z);
    var m = 0u;
    loop {
      if (em == 0u) { break; }
      let b = firstTrailingBit(em);
      em &= em - 1u;
      if (part_in(base, b, h.x + inflate, pl)) { m |= 1u << b; }
    }
    if (m == 0u) { continue; }
    let key = dot(s.xyz, kd) - s.w;
    if (out == cap) {
      dropped++;
      if (key >= BL_K[cap - 1u]) { continue; }
      out--;
    }
    var j = out;
    while (j > 0u && BL_K[j - 1u] > key) {
      BL_I[j] = BL_I[j - 1u];
      BL_M[j] = BL_M[j - 1u];
      BL_K[j] = BL_K[j - 1u];
      j--;
    }
    BL_I[j] = i;
    BL_M[j] = m;
    BL_K[j] = key;
    out++;
  }
  return vec2u(out, dropped);
}

fn ray_dir(px: vec2f) -> vec3f {
  let u = px.x / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - px.y / f32(F.dims.y) * 2.0;
  return F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v;
}

/** A plane through the eye containing directions a and b, facing toward `inside`. */
fn eye_plane(a: vec3f, b: vec3f, inside: vec3f) -> vec4f {
  var n = normalize(cross(a, b));
  if (dot(n, inside) < 0.0) { n = -n; }
  return vec4f(n, -dot(n, F.eye.xyz));
}

@compute @workgroup_size(8, 8)
fn bin_screen(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_id) lid: vec3u,
              @builtin(local_invocation_index) li: u32) {
  let n = load_list(wg.y * F.grid.y + wg.x, li);
  workgroupBarrier();
  let tx = wg.x * CT + lid.x;
  let ty = wg.y * CT + lid.y;
  if (tx >= F.dims.z || ty >= F.misc.y) { return; }
  let x0 = f32(tx * TILE);
  let y0 = f32(ty * TILE);
  let d00 = ray_dir(vec2f(x0, y0));
  let d10 = ray_dir(vec2f(x0 + 8.0, y0));
  let d01 = ray_dir(vec2f(x0, y0 + 8.0));
  let d11 = ray_dir(vec2f(x0 + 8.0, y0 + 8.0));
  let c = ray_dir(vec2f(x0 + 4.0, y0 + 4.0));
  let pl = array<vec4f, 4>(eye_plane(d00, d01, c), eye_plane(d10, d11, c), eye_plane(d00, d10, c), eye_plane(d01, d11, c));
  let r = bin_list(n, pl, F.cf.xyz, MAXE, 0.0);
  let o = (ty * F.dims.z + tx) * BIN_WORDS;
  tiles[o] = r.x;
  for (var e = 0u; e < r.x; e++) {
    tiles[o + 1u + 2u * e] = BL_I[e];
    tiles[o + 2u + 2u * e] = BL_M[e];
  }
  if (r.y > 0u) { atomicAdd(&stats[S_SCREEN_OVF], r.y); }
}

@compute @workgroup_size(8, 8)
fn bin_light(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_id) lid: vec3u,
             @builtin(local_invocation_index) li: u32) {
  let n = load_list(LBASE + wg.y * LCOARSE + wg.x, li);
  workgroupBarrier();
  let cx = wg.x * 8u + lid.x;
  let cy = wg.y * 8u + lid.y;
  let e = F.lc.w;
  let cell = 2.0 * e / f32(LGRID);
  let x0 = -e + f32(cx) * cell;
  let y0 = -e + f32(cy) * cell;
  let r = F.lr.xyz;
  let u = F.lu.xyz;
  let cr = dot(r, F.lc.xyz);
  let cu = dot(u, F.lc.xyz);
  let pl = array<vec4f, 4>(
    vec4f(r, -cr - x0), vec4f(-r, cr + x0 + cell),
    vec4f(u, -cu - y0), vec4f(-u, cu + y0 + cell));
  let res = bin_list(n, pl, -F.sun.xyz, LMAXE, PEN);
  let o = (cy * LGRID + cx) * LBIN_WORDS;
  lbins[o] = res.x;
  for (var k = 0u; k < res.x; k++) {
    lbins[o + 1u + 2u * k] = BL_I[k];
    lbins[o + 2u + 2u * k] = BL_M[k];
  }
  if (res.y > 0u) { atomicAdd(&stats[S_LIGHT_OVF], res.y); }
}
