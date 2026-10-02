// The raster hybrid's mesh: the wolf's rest-pose field extracted by surface nets on a dense grid,
// with skin weights from part distances (as spike 01 does), and per-vertex baked AO, albedo and
// coat. Run once at startup. Appended to common.wgsl + wolf.wgsl + posed.wgsl.

struct Grid {
  origin: vec3f,
  cell: f32,
  dims: vec3u,          // cells per axis
  vcap: u32,
  icap: u32,
  z0: u32,              // first z slab of this dispatch (extraction runs in slabs, one submission each)
  pad1: u32, pad2: u32,
}

struct Vert {           // 48 bytes; also read as a vertex buffer
  pos: vec3f,
  bones: u32,           // 4 × u8
  nrm: vec3f,
  weights: u32,         // 4 × unorm8
  ao: f32,
  albedo: u32,          // rgb unorm8
  chan: u32,            // unorm8: tail, head, coat length, skin depth
  pad: u32,
}

struct Counts {         // the first five words are drawIndexedIndirect's arguments
  index_count: atomic<u32>,
  instance_count: u32,
  first_index: u32,
  base_vertex: u32,
  first_instance: u32,
  vertex_count: atomic<u32>,
  flags: atomic<u32>,   // 1: vertex capacity hit, 2: index capacity hit
  holes: atomic<u32>,
}

@group(0) @binding(1) var<storage, read> pose: array<vec4f>;   // unused here; keeps posed.wgsl happy
@group(0) @binding(2) var<uniform> grid: Grid;
@group(0) @binding(3) var<storage, read_write> corners: array<f32>;
@group(0) @binding(4) var<storage, read_write> cellv: array<u32>;
@group(0) @binding(5) var<storage, read_write> verts: array<Vert>;
@group(0) @binding(6) var<storage, read_write> counts: Counts;
@group(0) @binding(7) var<storage, read_write> indices: array<u32>;

const KF = array<f32, 15>(0.08, 0.08, 0.08, 0.08, 0.05, 0.04, 0.02, 0.03, 0.05, 0.05, 0.03, 0.015, 0.07, 0.05, 0.02);
const NO_VERTEX: u32 = 0xFFFFFFFFu;

/** The rest-pose wolf: the same 22-part fold the tracer evaluates, with every bone at rest. */
fn rest_field(q: vec3f) -> f32 {
  var kf = KF;
  var d = BIG;
  for (var i = 0u; i < 22u; i++) {
    let f = base_fn(i);
    var qm = q;
    if (i >= 15u) { qm.x = -qm.x; }
    d = smin(d, rest_part(f, qm, false), kf[f]);
  }
  return d;
}

fn rest_grad(q: vec3f) -> vec3f {
  let e = 4e-4;
  let k = vec2f(1.0, -1.0);
  return (k.xyy * rest_field(q + k.xyy * e) + k.yyx * rest_field(q + k.yyx * e)
        + k.yxy * rest_field(q + k.yxy * e) + k.xxx * rest_field(q + k.xxx * e)) / (4.0 * e);
}

fn corner_index(c: vec3u) -> u32 {
  let n = grid.dims + 1u;
  return (c.z * n.y + c.y) * n.x + c.x;
}

fn cell_index(c: vec3u) -> u32 {
  return (c.z * grid.dims.y + c.y) * grid.dims.x + c.x;
}

@compute @workgroup_size(4, 4, 4)
fn ex_corners(@builtin(global_invocation_id) id0: vec3u) {
  let id = id0 + vec3u(0u, 0u, grid.z0);
  if (any(id > grid.dims)) { return; }
  corners[corner_index(id)] = rest_field(grid.origin + vec3f(id) * grid.cell) + select(0.0, 1.0, grid.cell == SALT);
}

fn unorm8(x: f32) -> u32 { return u32(round(clamp(x, 0.0, 1.0) * 255.0)); }

@compute @workgroup_size(4, 4, 4)
fn ex_cells(@builtin(global_invocation_id) id0: vec3u) {
  let id = id0 + vec3u(0u, 0u, grid.z0);
  if (any(id >= grid.dims)) { return; }
  let ci = cell_index(id);
  var vals: array<f32, 8>;
  var signs = 0u;
  for (var j = 0u; j < 8u; j++) {
    vals[j] = corners[corner_index(id + vec3u(j & 1u, (j >> 1u) & 1u, (j >> 2u) & 1u))];
    if (vals[j] < 0.0) { signs |= 1u << j; }
  }
  if (signs == 0u || signs == 255u) { cellv[ci] = NO_VERTEX; return; }

  // Surface nets: the mean of the edge crossings, then projected onto the surface.
  let cmin = grid.origin + vec3f(id) * grid.cell;
  var mass = vec3f(0.0);
  var n = 0.0;
  for (var e = 0u; e < 12u; e++) {
    let axis = e >> 2u;
    let r = e & 3u;
    let ja = ((r & 1u) << ((axis + 1u) % 3u)) | ((r >> 1u) << ((axis + 2u) % 3u));
    let jb = ja | (1u << axis);
    if (((signs >> ja) & 1u) == ((signs >> jb) & 1u)) { continue; }
    let pa = vec3f(f32(ja & 1u), f32((ja >> 1u) & 1u), f32((ja >> 2u) & 1u));
    let pb = vec3f(f32(jb & 1u), f32((jb >> 1u) & 1u), f32((jb >> 2u) & 1u));
    mass += cmin + mix(pa, pb, vals[ja] / (vals[ja] - vals[jb])) * grid.cell;
    n += 1.0;
  }
  var p = mass / n;
  for (var it = 0; it < 2; it++) {
    let g = rest_grad(p);
    p -= rest_field(p) * g / max(dot(g, g), 1e-12);      // Newton step along the gradient
  }
  p = clamp(p, cmin - vec3f(0.5 * grid.cell), cmin + vec3f(1.5 * grid.cell));
  let nrm = normalize(rest_grad(p));

  // Skin weights from each part's own distance: near parts share the vertex (spike 01's rule).
  var pd: array<f32, 22>;
  var dmin = BIG;
  for (var i = 0u; i < 22u; i++) {
    var qm = p;
    if (i >= 15u) { qm.x = -qm.x; }
    pd[i] = rest_part(base_fn(i), qm, false);
    dmin = min(dmin, pd[i]);
  }
  var bw: array<f32, 22>;
  var tail = 0.0;
  var head = 0.0;
  var wsum = 0.0;
  for (var i = 0u; i < 22u; i++) {
    bw[i] = 1.0 - smoothstep(0.0, 0.03, pd[i] - dmin);
    wsum += bw[i];
    let f = base_fn(i);
    if (f >= 5u && f <= 7u) { tail += bw[i]; }
    if (f == 4u) { head += bw[i]; }
  }
  tail /= wsum;
  head /= wsum;
  var idx = vec4u(0u);
  var wq = vec4f(0.0);
  for (var k = 0u; k < 4u; k++) {
    var best = 0u;
    var bv = -1.0;
    for (var b = 0u; b < 22u; b++) {
      if (bw[b] > bv) { bv = bw[b]; best = b; }
    }
    idx[k] = best;
    wq[k] = max(bv, 0.0);
    bw[best] = -1.0;
  }
  wq /= max(wq.x + wq.y + wq.z + wq.w, 1e-9);
  var q8 = vec4u(round(wq * 255.0));
  q8.x = 255u - (q8.y + q8.z + q8.w);

  // Baked AO (rest pose), albedo and coat.
  var ao = 1.0;
  for (var s = 1; s <= 4; s++) {
    let h = 0.025 * f32(s);
    ao -= (h - rest_field(p + nrm * h)) * (1.6 / f32(s));
  }
  ao = clamp(ao, 0.2, 1.0);
  let ch = vec2f(tail, head);
  let alb = wolf_albedo(p, ch);
  let coat = fur_coat(p, ch);

  let vi = atomicAdd(&counts.vertex_count, 1u);
  if (vi >= grid.vcap) { atomicOr(&counts.flags, 1u); cellv[ci] = NO_VERTEX; return; }
  verts[vi] = Vert(p, idx.x | (idx.y << 8u) | (idx.z << 16u) | (idx.w << 24u), nrm,
                   q8.x | (q8.y << 8u) | (q8.z << 16u) | (q8.w << 24u), ao,
                   unorm8(sqrt(alb.r)) | (unorm8(sqrt(alb.g)) << 8u) | (unorm8(sqrt(alb.b)) << 16u),
                   unorm8(tail) | (unorm8(head) << 8u) | (unorm8(coat.x) << 16u) | (unorm8(coat.y) << 24u), 0u);
  cellv[ci] = vi;
}

fn vertex_at(c: vec3i) -> u32 {
  if (any(c < vec3i(0)) || any(c >= vec3i(grid.dims))) { return NO_VERTEX; }
  return cellv[cell_index(vec3u(c))];
}

@compute @workgroup_size(4, 4, 4)
fn ex_quads(@builtin(global_invocation_id) id0: vec3u) {
  let id = id0 + vec3u(0u, 0u, grid.z0);
  if (any(id >= grid.dims)) { return; }
  let v0 = cellv[cell_index(id)];
  if (v0 == NO_VERTEX) { return; }
  let in0 = corners[corner_index(id)] < 0.0;
  let gc = vec3i(id);
  for (var axis = 0u; axis < 3u; axis++) {         // the three edges leaving the cell's min corner
    var o = vec3u(0u);
    o[axis] = 1u;
    let in1 = corners[corner_index(id + o)] < 0.0;
    if (in0 == in1) { continue; }
    var o1 = vec3i(0);
    var o2 = vec3i(0);
    o1[(axis + 1u) % 3u] = 1;
    o2[(axis + 2u) % 3u] = 1;
    let v1 = vertex_at(gc - o1);
    let v2 = vertex_at(gc - o1 - o2);
    let v3 = vertex_at(gc - o2);
    if (v1 == NO_VERTEX || v2 == NO_VERTEX || v3 == NO_VERTEX) { atomicAdd(&counts.holes, 1u); continue; }
    let at = atomicAdd(&counts.index_count, 6u);
    if (at + 6u > grid.icap) { atomicOr(&counts.flags, 2u); continue; }
    if (in0) {
      indices[at] = v0; indices[at + 1u] = v1; indices[at + 2u] = v2;
      indices[at + 3u] = v0; indices[at + 4u] = v2; indices[at + 5u] = v3;
    } else {
      indices[at] = v0; indices[at + 1u] = v2; indices[at + 2u] = v1;
      indices[at + 3u] = v0; indices[at + 4u] = v3; indices[at + 5u] = v2;
    }
  }
}
