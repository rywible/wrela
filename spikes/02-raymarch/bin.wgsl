// Binning: which instances, and which of their parts, can matter in each screen tile and each
// light-grid cell. Appended to common.wgsl + field.wgsl.
//
// One invocation per bin. It culls instance bound spheres against the bin's four planes, then each
// surviving instance's parts, and writes the bin a list of (instance, PartMask). Screen lists are
// sorted near to far, so a ray usually meets its hit first and skips what's behind it.
//
// (A first version used one 64-wide workgroup per bin with shared-memory lists: 0.52 ms for the
// screen, mostly fixed cost per workgroup. With 1–3 candidates per bin, one thread per bin is enough.)

@group(0) @binding(2) var<storage, read_write> bins: array<u32>;
@group(0) @binding(3) var<storage, read_write> stats: array<atomic<u32>, 32>;

fn bin(index: u32, planes: array<vec4f, 4>, overflow_slot: u32) {
  var li: array<u32, MAXE>;
  var lm: array<u32, MAXE>;
  var lk: array<f32, MAXE>;
  var out = 0u;
  var dropped = 0u;
  for (var i = 0u; i < F.dims.w; i++) {
    let base = i * STRIDE;
    let s = inst[base + I_BOUND];
    if (!sphere_in(s, planes)) { continue; }
    var m = 0u;
    for (var b = 0u; b < NPARTS; b++) {
      if (part_in(base, b, planes)) { m |= 1u << b; }
    }
    if (m == 0u) { continue; }
    if (out == MAXE) { dropped++; continue; }
    let key = dot(s.xyz - F.eye.xyz, F.cf.xyz) - s.w;
    var j = out;
    while (j > 0u && lk[j - 1u] > key) {
      li[j] = li[j - 1u]; lm[j] = lm[j - 1u]; lk[j] = lk[j - 1u];
      j--;
    }
    li[j] = i; lm[j] = m; lk[j] = key;
    out++;
  }
  let o = index * BIN_WORDS;
  bins[o] = out;
  for (var e = 0u; e < out; e++) {
    bins[o + 1u + 2u * e] = li[e];
    bins[o + 2u + 2u * e] = lm[e];
  }
  if (dropped > 0u) { atomicAdd(&stats[overflow_slot], dropped); }
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
fn bin_screen(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= F.dims.z || id.y * 8u >= F.dims.y) { return; }
  let x0 = f32(id.x * 8u);
  let y0 = f32(id.y * 8u);
  let d00 = ray_dir(vec2f(x0, y0));
  let d10 = ray_dir(vec2f(x0 + 8.0, y0));
  let d01 = ray_dir(vec2f(x0, y0 + 8.0));
  let d11 = ray_dir(vec2f(x0 + 8.0, y0 + 8.0));
  let c = ray_dir(vec2f(x0 + 4.0, y0 + 4.0));
  let planes = array<vec4f, 4>(eye_plane(d00, d01, c), eye_plane(d10, d11, c), eye_plane(d00, d10, c), eye_plane(d01, d11, c));
  bin(id.y * F.dims.z + id.x, planes, S_SCREEN_OVERFLOW);
}

@compute @workgroup_size(8, 8)
fn bin_light(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= F.grid.x || id.y >= F.grid.x) { return; }
  let g = f32(F.grid.x);
  let e = F.lc.w;
  let cell = 2.0 * e / g;
  let x0 = -e + f32(id.x) * cell;
  let y0 = -e + f32(id.y) * cell;
  let r = F.lr.xyz;
  let u = F.lu.xyz;
  let cr = dot(r, F.lc.xyz);
  let cu = dot(u, F.lc.xyz);
  // The cell's column along the sun: x0 ≤ r·(p − c) ≤ x0 + cell, and the same along u.
  let planes = array<vec4f, 4>(
    vec4f(r, -cr - x0), vec4f(-r, cr + x0 + cell),
    vec4f(u, -cu - y0), vec4f(-u, cu + y0 + cell));
  bin(id.y * F.grid.x + id.x, planes, S_LIGHT_OVERFLOW);
}
