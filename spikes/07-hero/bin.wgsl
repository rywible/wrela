// Binning: which parts can matter in each 8×8 screen tile, each light-grid column, and each cell of
// a coarse volume grid (for ambient occlusion). One creature, so a bin is just a 22-bit PartMask.
// Appended to common.wgsl + wolf.wgsl + posed.wgsl.

@group(0) @binding(1) var<storage, read> pose: array<vec4f>;
@group(0) @binding(2) var<storage, read_write> tiles: array<u32>;
@group(0) @binding(3) var<storage, read_write> lights: array<u32>;
@group(0) @binding(4) var<storage, read_write> volm: array<u32>;

fn mask_planes(planes: array<vec4f, 4>) -> u32 {
  var m = 0u;
  for (var i = 0u; i < NP; i++) {
    let n = part_nobb(i);
    for (var k = 0u; k < n; k++) {
      if (obb_in_planes(P_OBB + 12u * i + 4u * k, planes)) { m |= 1u << i; break; }
    }
  }
  return m;
}

fn ray_dir(px: vec2f) -> vec3f {
  let u = px.x / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - px.y / f32(F.dims.y) * 2.0;
  return F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v;
}

fn eye_plane(a: vec3f, b: vec3f, inside: vec3f) -> vec4f {
  var n = normalize(cross(a, b));
  if (dot(n, inside) < 0.0) { n = -n; }
  return vec4f(n, -dot(n, F.eye.xyz));
}

@compute @workgroup_size(8, 8)
fn bin_screen(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= F.dims.z || id.y >= F.dims.w) { return; }
  let x0 = f32(id.x * 8u);
  let y0 = f32(id.y * 8u);
  let d00 = ray_dir(vec2f(x0, y0));
  let d10 = ray_dir(vec2f(x0 + 8.0, y0));
  let d01 = ray_dir(vec2f(x0, y0 + 8.0));
  let d11 = ray_dir(vec2f(x0 + 8.0, y0 + 8.0));
  let c = ray_dir(vec2f(x0 + 4.0, y0 + 4.0));
  let planes = array<vec4f, 4>(eye_plane(d00, d01, c), eye_plane(d10, d11, c), eye_plane(d00, d10, c), eye_plane(d01, d11, c));
  tiles[id.y * F.dims.z + id.x] = mask_planes(planes);
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
  let planes = array<vec4f, 4>(
    vec4f(r, -cr - x0), vec4f(-r, cr + x0 + cell),
    vec4f(u, -cu - y0), vec4f(-u, cu + y0 + cell));
  lights[id.y * F.grid.x + id.x] = mask_planes(planes);
}

// AO samples reach 0.25 m from the shaded point; the cell's sphere is grown by that.
const AO_REACH: f32 = 0.25;

@compute @workgroup_size(4, 4, 4)
fn bin_volume(@builtin(global_invocation_id) id: vec3u) {
  let n = F.vdims.xyz;
  if (any(id >= n)) { return; }
  let cs = F.vol.w;
  let c = F.vol.xyz + (vec3f(id) + 0.5) * cs;
  let r = 0.8660254 * cs + AO_REACH;
  var m = 0u;
  for (var i = 0u; i < NP; i++) {
    let nb = part_nobb(i);
    for (var k = 0u; k < nb; k++) {
      if (obb_near(P_OBB + 12u * i + 4u * k, c, r)) { m |= 1u << i; break; }
    }
  }
  volm[(id.z * n.y + id.y) * n.x + id.x] = m;
}
