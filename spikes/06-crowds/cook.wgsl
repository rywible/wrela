// Re-cooking the brick cache after an edit, entirely on the GPU, nothing read back.
//
// `begin_batch` resets the cook list. `classify`: one thread per cell of the batch's box that an
// edit in the batch can change. It evaluates base + the overlapping log entries at the cell's
// centre, then allocates a brick (free stack), keeps it, or releases it, and appends brick cells
// to the cook list. `cook`: an indirect dispatch, one workgroup per listed brick, writes its 9³
// samples. `release` returns freed slots to the stack (after `cook`, so a slot freed in this batch
// can't be handed out in it).
//
// The source of truth is always base + log, so the cache never accumulates error.

struct Batch {
  lo: vec4i,     // box min cell
  size: vec4u,   // box size in cells (xyz), w: cell count
  info: vec4u,   // x: overlapping log entries, y: first new edit, z: end of new edits, w: 1 = cook every cell
}

@group(0) @binding(1) var<uniform> BT: Batch;
@group(0) @binding(2) var<storage, read_write> bmap: array<u32>;
@group(0) @binding(3) var<storage, read_write> ctrl: array<atomic<u32>, 8>;   // cook dispatch xyz, pending frees, free count
@group(0) @binding(4) var<storage, read_write> freeslots: array<u32>;
@group(0) @binding(5) var<storage, read_write> cooklist: array<u32>;         // (cell, slot) pairs
@group(0) @binding(6) var<storage, read_write> pending: array<u32>;
@group(0) @binding(7) var<storage, read> edits: array<vec4f>;
@group(0) @binding(8) var<storage, read> relevant: array<u32>;
@group(0) @binding(9) var atlas_w: texture_storage_3d<rg16float, write>;
@group(0) @binding(10) var<storage, read_write> stats: array<atomic<u32>, 32>;

const C_PENDING: u32 = 3u;
const C_FREE: u32 = 4u;

/** Base plus the log entries that overlap this batch's box, in log order. */
fn field_batch(p: vec3f) -> vec2f {
  var dg = vec2f(base_field(p), 0.0);
  for (var j = 0u; j < BT.info.x; j++) { dg = edit_apply(p, dg, relevant[j]); }
  return dg;
}

/** How far an edit can change a brick's samples from its centre: its blend and its splash of
 *  thrown earth. (Additions also shrink empty-cell distances out to MAXSKIP; see `affected`.) */
fn reach_surface(i: u32) -> f32 {
  let e0 = edits[2u * i];
  let e1 = edits[2u * i + 1u];
  var r = e0.w + e1.y;
  if (e1.z > 0.0 && e1.x < 0.5) { r = max(r, 1.9 * e0.w); }
  return r;
}

const MARGIN: f32 = 2.75;   // ≥ BAND + two half-diagonals (edits.js has the same)

/** 2: an edit in this batch can change this cell's surface (re-classify, re-cook); 1: only its
 *  empty-cell distance (an addition within MAXSKIP); 0: nothing. */
fn affected(c: vec3f) -> u32 {
  var a = 0u;
  for (var i = BT.info.y; i < BT.info.z; i++) {
    let e0 = edits[2u * i];
    let dist = length(c - e0.xyz);
    if (dist < reach_surface(i) + MARGIN) { return 2u; }
    if (edits[2u * i + 1u].x > 0.5 && dist < reach_surface(i) + MAXSKIP + 0.87) { a = 1u; }
  }
  return a;
}

fn empty_entry(dc: f32) -> u32 { return pack2x16float(vec2f(clamp(dc, -MAXSKIP, MAXSKIP), 0.0)) & 0xFFFFu; }

@compute @workgroup_size(1)
fn begin_batch() {
  atomicStore(&ctrl[0], 0u);
  atomicStore(&ctrl[1], 1u);
  atomicStore(&ctrl[2], 1u);
  atomicStore(&ctrl[C_PENDING], 0u);
}

@compute @workgroup_size(64)
fn classify(@builtin(global_invocation_id) id: vec3u) {
  let k = id.x;
  if (k >= BT.size.w) { return; }
  let c = BT.lo.xyz + vec3i(i32(k % BT.size.x), i32((k / BT.size.x) % BT.size.y), i32(k / (BT.size.x * BT.size.y)));
  if (any(c < vec3i(0)) || any(c >= vec3i(RX, RY, RZ))) { return; }
  let centre = RMIN + vec3f(c) + 0.5;
  var a = 2u;
  if (BT.info.w == 0u) { a = affected(centre); }
  if (a == 0u) { return; }
  let idx = cell_index(c);
  let e = bmap[idx];
  let had = (e & BRICK) != 0u;
  if (a == 1u && had) { return; }   // a brick's samples are out of that addition's reach
  atomicAdd(&stats[S_COOK_CELLS], 1u);
  let dc = field_batch(centre).x;
  if (abs(dc) < BAND) {
    var slot = e & SLOT_MASK;
    if (!had) {
      let old = atomicSub(&ctrl[C_FREE], 1u);
      if (old == 0u || old > CAP) {
        atomicAdd(&ctrl[C_FREE], 1u);
        atomicAdd(&stats[S_ALLOC_FAIL], 1u);
        bmap[idx] = empty_entry(max(dc, 0.0));
        return;
      }
      slot = freeslots[old - 1u];
    }
    bmap[idx] = BRICK | slot;
    let j = atomicAdd(&ctrl[0], 1u);
    cooklist[2u * j] = idx;
    cooklist[2u * j + 1u] = slot;
    atomicAdd(&stats[S_COOK_BRICKS], 1u);
  } else {
    if (had) {
      let j = atomicAdd(&ctrl[C_PENDING], 1u);
      pending[j] = e & SLOT_MASK;
      atomicAdd(&stats[S_FREED], 1u);
    }
    bmap[idx] = empty_entry(dc);
  }
}

// Per brick, the log entries that can reach it (positions in `relevant`, sorted back into log order).
const WL: u32 = 256u;
var<workgroup> wl: array<u32, WL>;
var<workgroup> wn: atomic<u32>;

@compute @workgroup_size(9, 9, 3)
fn cook(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_id) lid: vec3u,
        @builtin(local_invocation_index) li: u32) {
  let j = wg.x + wg.y * 65535u;
  let idx = cooklist[2u * j];
  let slot = cooklist[2u * j + 1u];
  let c = cell_of_index(idx);
  let o = slot_origin(slot);
  let lo = RMIN + vec3f(c);
  if (li == 0u) { atomicStore(&wn, 0u); }
  workgroupBarrier();
  for (var k = li; k < BT.info.x; k += 243u) {
    let i = relevant[k];
    let e0 = edits[2u * i];
    if (length(e0.xyz - clamp(e0.xyz, lo, lo + 1.0)) < reach_surface(i) + MARGIN) {
      let s = atomicAdd(&wn, 1u);
      if (s < WL) { wl[s] = k; }
    }
  }
  workgroupBarrier();
  let n = atomicLoad(&wn);
  if (li == 0u && n <= WL) {
    for (var a = 1u; a < n; a++) {
      let v = wl[a];
      var b = a;
      while (b > 0u && wl[b - 1u] > v) { wl[b] = wl[b - 1u]; b--; }
      wl[b] = v;
    }
  }
  workgroupBarrier();
  for (var z = lid.z; z < BS; z += 3u) {
    let s = vec3u(lid.x, lid.y, z);
    let p = RMIN + vec3f(c) + vec3f(s) / VPC;
    var dg: vec2f;
    if (n > WL) {
      dg = field_batch(p);
    } else {
      dg = vec2f(base_field(p), 0.0);
      for (var q = 0u; q < n; q++) { dg = edit_apply(p, dg, relevant[wl[q]]); }
    }
    textureStore(atlas_w, o + s, vec4f(dg, 0.0, 0.0));
  }
}

@compute @workgroup_size(64)
fn release(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= atomicLoad(&ctrl[C_PENDING])) { return; }
  let j = atomicAdd(&ctrl[C_FREE], 1u);
  freeslots[j] = pending[id.x];
}
