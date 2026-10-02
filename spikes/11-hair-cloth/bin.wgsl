// Scatter binning: every item (body, cloth patch, strand segment) appends itself to the screen tiles
// (8×8 pixels) and light-grid cells its box covers. Count → prefix sum → fill, so lists are compact
// and never overflow. Appended to common.wgsl.
//
// Lists are indexed (cell × NCAT + category); screen cells first, then light cells from F.bins.x.
// The fill pass decrements the counts back to zero, so they needn't be cleared between frames.

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<storage, read> prims: array<vec4f>;
@group(0) @binding(2) var<storage, read> chars: array<vec4f>;
@group(0) @binding(3) var<storage, read> segs: array<vec4f>;
@group(0) @binding(4) var<storage, read_write> bcount: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> boff: array<u32>;
@group(0) @binding(6) var<storage, read_write> blist: array<u32>;
@group(0) @binding(7) var<storage, read_write> bsum: array<u32>;
@group(0) @binding(8) var<storage, read_write> big: array<atomic<u32>>;   // [count, items…]: items covering many cells

override FILL: bool = false;

const BIN_PATCH_SCREEN: u32 = 1u;
const BIN_PATCH_LIGHT: u32 = 2u;
const BIN_SEG_SCREEN: u32 = 4u;
const BIG_CELLS: i32 = 48;          // items covering more cells than this are binned by a whole workgroup
const BIG_MAX: u32 = 65535u;

fn add_entry(idx: u32, id: u32) {
  if (FILL) {
    let slot = boff[idx] + atomicSub(&bcount[idx], 1u) - 1u;
    if (slot < F.bins.y) { blist[slot] = id; }
  } else {
    atomicAdd(&bcount[idx], 1u);
  }
}

/**
 * Tiles covered by a box's screen projection (with a pixel and a half of margin), or an empty range.
 * The box is clipped to the near plane first: corners behind it are replaced by the points where
 * the box's edges cross it, so a box beside or behind the eye doesn't flood the screen.
 */
fn screen_rect(lo: vec3f, hi: vec3f) -> vec4i {
  let NEAR = 0.02;
  var h: array<vec4f, 8>;
  var mn = vec2f(1e30);
  var mx = vec2f(-1e30);
  var front = 0u;
  for (var k = 0u; k < 8u; k++) {
    let c = vec3f(select(lo.x, hi.x, (k & 1u) != 0u), select(lo.y, hi.y, (k & 2u) != 0u), select(lo.z, hi.z, (k & 4u) != 0u));
    h[k] = F.viewproj * vec4f(c, 1.0);
    if (h[k].w >= NEAR) {
      front += 1u;
      mn = min(mn, h[k].xy / h[k].w);
      mx = max(mx, h[k].xy / h[k].w);
    }
  }
  if (front == 0u) { return vec4i(0, 0, -1, -1); }
  if (front < 8u) {
    for (var k = 0u; k < 8u; k++) {
      for (var bit = 1u; bit < 8u; bit <<= 1u) {
        if ((k & bit) != 0u) { continue; }
        let a = h[k];
        let b = h[k | bit];
        if ((a.w < NEAR) == (b.w < NEAR)) { continue; }
        let q = mix(a, b, (NEAR - a.w) / (b.w - a.w));
        mn = min(mn, q.xy / NEAR);
        mx = max(mx, q.xy / NEAR);
      }
    }
  }
  let W = f32(F.dims.x);
  let H = f32(F.dims.y);
  let x0 = (mn.x * 0.5 + 0.5) * W - 1.5;
  let x1 = (mx.x * 0.5 + 0.5) * W + 1.5;
  let y0 = (0.5 - mx.y * 0.5) * H - 1.5;
  let y1 = (0.5 - mn.y * 0.5) * H + 1.5;
  if (x1 < 0.0 || y1 < 0.0 || x0 >= W || y0 >= H) { return vec4i(0, 0, -1, -1); }
  return vec4i(i32(max(x0, 0.0)) / 8, i32(max(y0, 0.0)) / 8,
               min(i32(min(x1, W - 1.0)) / 8, i32(F.dims.z) - 1), min(i32(min(y1, H - 1.0)) / 8, i32(F.dims.w) - 1));
}

/** Light-grid cells covered by a box (the column of cells its shadow can fall through). */
fn light_rect(lo: vec3f, hi: vec3f) -> vec4i {
  let c = 0.5 * (lo + hi) - F.lc.xyz;
  let ex = 0.5 * (hi - lo);
  let r = vec2f(dot(c, F.lr.xyz), dot(c, F.lu.xyz));
  let rx = dot(ex, abs(F.lr.xyz));
  let ry = dot(ex, abs(F.lu.xyz));
  let e = F.lc.w;
  let g = f32(F.grid.x);
  let a = (r - vec2f(rx, ry) + e) / (2.0 * e) * g;
  let b = (r + vec2f(rx, ry) + e) / (2.0 * e) * g;
  if (b.x < 0.0 || b.y < 0.0 || a.x >= g || a.y >= g) { return vec4i(0, 0, -1, -1); }
  return vec4i(i32(max(a.x, 0.0)), i32(max(a.y, 0.0)), i32(min(b.x, g - 1.0)), i32(min(b.y, g - 1.0)));
}

struct Item { lo: vec3f, hi: vec3f, cat: u32, id: u32, screen: bool, light: bool, ok: bool }

fn item(i: u32) -> Item {
  let nc = F.grid.y;
  let np = F.grid.z;
  let ns = F.grid.w;
  var it = Item(vec3f(0.0), vec3f(0.0), CAT_CHAR, i, true, true, true);
  if (i < nc) {
    it.lo = chars[i * CS + C_LO].xyz;
    it.hi = chars[i * CS + C_HI].xyz;
  } else if (i < nc + np) {
    it.id = i - nc;
    it.cat = CAT_PATCH;
    it.screen = (F.bins.z & BIN_PATCH_SCREEN) != 0u;
    it.light = (F.bins.z & BIN_PATCH_LIGHT) != 0u;
    it.lo = prims[PR_PATCH + PATCH_STRIDE * it.id].xyz;
    it.hi = prims[PR_PATCH + PATCH_STRIDE * it.id + 1u].xyz;
  } else if (i < nc + np + ns) {
    it.id = i - nc - np;
    it.cat = CAT_SEG;
    it.screen = (F.bins.z & BIN_SEG_SCREEN) != 0u;
    it.light = false;
    let a = segs[2u * it.id];
    let b = segs[2u * it.id + 1u];
    let r = max(a.w, b.w);
    it.ok = r > 0.0;
    it.lo = min(a.xyz, b.xyz) - vec3f(r);
    it.hi = max(a.xyz, b.xyz) + vec3f(r);
  } else {
    it.ok = false;
  }
  it.ok = it.ok && (it.screen || it.light);
  return it;
}

fn rect_cells(r: vec4i) -> i32 { return max(r.z - r.x + 1, 0) * max(r.w - r.y + 1, 0); }

/** Adds cells [c0, c1) of an item's screen rect and then its light rect, striding by `step`. */
fn add_cells(it: Item, sr: vec4i, lr: vec4i, c0: i32, step: i32) {
  let ns = rect_cells(sr);
  let nl = rect_cells(lr);
  for (var c = c0; c < ns + nl; c += step) {
    if (c < ns) {
      let w = sr.z - sr.x + 1;
      let tx = sr.x + c % w;
      let ty = sr.y + c / w;
      add_entry((u32(ty) * F.dims.z + u32(tx)) * NCAT + it.cat, it.id);
    } else {
      let k = c - ns;
      let w = lr.z - lr.x + 1;
      let x = lr.x + k % w;
      let y = lr.y + k / w;
      add_entry((F.bins.x + u32(y) * F.grid.x + u32(x)) * NCAT + it.cat, it.id);
    }
  }
}

fn rects(it: Item) -> array<vec4i, 2> {
  var sr = vec4i(0, 0, -1, -1);
  var lr = vec4i(0, 0, -1, -1);
  if (it.screen) { sr = screen_rect(it.lo, it.hi); }
  if (it.light) { lr = light_rect(it.lo, it.hi); }
  return array<vec4i, 2>(sr, lr);
}

/** One thread per item; items covering many cells are deferred to bin_big (count pass lists them). */
@compute @workgroup_size(64)
fn bin_items(@builtin(global_invocation_id) gid: vec3u, @builtin(num_workgroups) nw: vec3u) {
  let i = gid.x + gid.y * nw.x * 64u;
  let it = item(i);
  if (!it.ok) { return; }
  let r = rects(it);
  if (rect_cells(r[0]) + rect_cells(r[1]) > BIG_CELLS) {
    if (!FILL) {
      let k = atomicAdd(&big[0], 1u);
      if (k < BIG_MAX) { atomicStore(&big[1u + k], i); }
    }
    return;
  }
  add_cells(it, r[0], r[1], 0, 1);
}

/** Large items: one workgroup each (grid-stride over the list), its threads striding over the cells. */
@compute @workgroup_size(64)
fn bin_big(@builtin(workgroup_id) wg: vec3u, @builtin(num_workgroups) nw: vec3u, @builtin(local_invocation_index) lid: u32) {
  let n = min(atomicLoad(&big[0]), BIG_MAX);
  for (var b = wg.x; b < n; b += nw.x) {
    let it = item(atomicLoad(&big[1u + b]));
    let r = rects(it);
    add_cells(it, r[0], r[1], i32(lid), 64);
  }
}

// ---- exclusive prefix sum over the counts: per-block sums, a scan of those, then the blocks --------

const BLOCK: u32 = 1024u;          // 256 threads × 4
var<workgroup> wsum: array<u32, 256>;

fn total_cells() -> u32 { return (F.bins.x + F.grid.x * F.grid.x) * NCAT; }

/** In-place exclusive scan of wsum (256 entries); returns nothing, wsum[i] becomes the prefix. */
fn wg_scan(lid: u32) {
  // Hillis–Steele inclusive scan, then shift.
  for (var off = 1u; off < 256u; off <<= 1u) {
    var v = 0u;
    if (lid >= off) { v = wsum[lid - off]; }
    workgroupBarrier();
    wsum[lid] += v;
    workgroupBarrier();
  }
}

@compute @workgroup_size(256)
fn scan_reduce(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_index) lid: u32) {
  let n = total_cells();
  let base = wg.x * BLOCK + lid * 4u;
  var s = 0u;
  for (var k = 0u; k < 4u; k++) {
    if (base + k < n) { s += atomicLoad(&bcount[base + k]); }
  }
  wsum[lid] = s;
  workgroupBarrier();
  for (var st = 128u; st > 0u; st >>= 1u) {
    if (lid < st) { wsum[lid] += wsum[lid + st]; }
    workgroupBarrier();
  }
  if (lid == 0u) { bsum[wg.x] = wsum[0]; }
}

@compute @workgroup_size(256)
fn scan_blocks(@builtin(local_invocation_index) lid: u32) {
  let nb = (total_cells() + BLOCK - 1u) / BLOCK;
  var v = array<u32, 4>(0u, 0u, 0u, 0u);
  var s = 0u;
  for (var k = 0u; k < 4u; k++) {
    let i = lid * 4u + k;
    if (i < nb) { v[k] = bsum[i]; }
    s += v[k];
  }
  wsum[lid] = s;
  workgroupBarrier();
  wg_scan(lid);
  var run = wsum[lid] - s;
  for (var k = 0u; k < 4u; k++) {
    let i = lid * 4u + k;
    if (i < nb) { bsum[i] = run; }
    run += v[k];
  }
}

@compute @workgroup_size(256)
fn scan_down(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_index) lid: u32) {
  let n = total_cells();
  let base = wg.x * BLOCK + lid * 4u;
  var v = array<u32, 4>(0u, 0u, 0u, 0u);
  var s = 0u;
  for (var k = 0u; k < 4u; k++) {
    if (base + k < n) { v[k] = atomicLoad(&bcount[base + k]); }
    s += v[k];
  }
  wsum[lid] = s;
  workgroupBarrier();
  wg_scan(lid);
  var run = bsum[wg.x] + wsum[lid] - s;
  for (var k = 0u; k < 4u; k++) {
    if (base + k < n) {
      boff[base + k] = run;
      run += v[k];
      if (base + k == n - 1u) { boff[n] = run; }
    }
  }
}
