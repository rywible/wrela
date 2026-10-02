// Per-frame cloth preparation, after the simulation. Appended to common.wgsl.
// - cloth_normals: per-particle normals from grid neighbours (every cloth technique uses them);
// - patch_bounds: each patch's box and slab (the traced techniques' bounding volumes);
// - sheet_bounds: per-sheet boxes for the reference render (not measured).

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<storage, read> pos: array<vec4f>;
@group(0) @binding(2) var<storage, read> pinfo: array<vec4u>;
@group(0) @binding(3) var<storage, read_write> attr: array<vec4f>;
@group(0) @binding(4) var<storage, read_write> prims: array<vec4f>;
@group(0) @binding(5) var<storage, read_write> sheetb: array<atomic<u32>>;

override P: u32 = 1u;

@compute @workgroup_size(256)
fn cloth_normals(@builtin(global_invocation_id) gid: vec3u) {
  let i = gid.x;
  if (i >= F.hair.z) { return; }
  let nb = pinfo[2u * i + 1u];
  let n = cross(pos[nb.y].xyz - pos[nb.x].xyz, pos[nb.w].xyz - pos[nb.z].xyz);
  let l = length(n);
  attr[2u * i] = vec4f(select(vec3f(0.0, 1.0, 0.0), n / l, l > 1e-12), 0.0);
}

/**
 * A patch's bound: its particles' box, and the slab |n·x − c| ≤ δ that holds them, where n is the
 * patch's mean normal. Triangles are convex hulls of their particles, so both contain the sheet;
 * both are inflated by half the cloth's thickness.
 */
@compute @workgroup_size(64)
fn patch_bounds(@builtin(global_invocation_id) gid: vec3u) {
  let pi = gid.x;
  if (pi >= F.grid.z) { return; }
  let inf = bitcast<vec4u>(prims[pr_pinfo(pi)]);
  var lo = vec3f(1e30);
  var hi = vec3f(-1e30);
  var c = vec3f(0.0);
  for (var j = 0u; j <= P; j++) {
    for (var i = 0u; i <= P; i++) {
      let p = pos[patch_vertex(inf, i, j)].xyz;
      lo = min(lo, p);
      hi = max(hi, p);
      c += p;
    }
  }
  c /= f32((P + 1u) * (P + 1u));
  let d1 = pos[patch_vertex(inf, P, P)].xyz - pos[patch_vertex(inf, 0u, 0u)].xyz;
  let d2 = pos[patch_vertex(inf, P, 0u)].xyz - pos[patch_vertex(inf, 0u, P)].xyz;
  let nn = cross(d1, d2);
  let nl = length(nn);
  var n = vec3f(0.0, 1.0, 0.0);
  var delta = 1e6;
  if (nl > 1e-10) {
    n = nn / nl;
    delta = 0.0;
    for (var j = 0u; j <= P; j++) {
      for (var i = 0u; i <= P; i++) {
        delta = max(delta, abs(dot(n, pos[patch_vertex(inf, i, j)].xyz - c)));
      }
    }
  }
  let base = PR_PATCH + PATCH_STRIDE * pi;
  prims[base] = vec4f(lo - vec3f(CLOTH_H), dot(n, c));
  prims[base + 1u] = vec4f(hi + vec3f(CLOTH_H), delta + CLOTH_H * 1.001);
  prims[base + 2u] = vec4f(n, 0.0);
}

fn fkey(f: f32) -> u32 {
  let b = bitcast<u32>(f);
  return select(b | 0x80000000u, ~b, (b & 0x80000000u) != 0u);
}

@compute @workgroup_size(64)
fn sheet_bounds(@builtin(global_invocation_id) gid: vec3u) {
  let pi = gid.x;
  if (pi >= F.grid.z) { return; }
  let inf = bitcast<vec4u>(prims[pr_pinfo(pi)]);
  let s = patch_sheet(inf);
  let base = PR_PATCH + PATCH_STRIDE * pi;
  let lo = prims[base].xyz;
  let hi = prims[base + 1u].xyz;
  for (var k = 0u; k < 3u; k++) {
    atomicMin(&sheetb[s * 6u + k], fkey(lo[k]));
    atomicMax(&sheetb[s * 6u + 3u + k], fkey(hi[k]));
  }
}
