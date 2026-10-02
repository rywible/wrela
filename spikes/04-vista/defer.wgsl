// Deferred per-pixel field shading for raster visibility (the raster_dn variant): after the raster
// pass, one compute pass evaluates the terrain's normal from the field once per visible pixel, at a
// footprint-sized offset (4 height evaluations), and rewrites the G-buffer. No overdraw, unlike the
// forward variant. Appended to common + field + cache (for level_guess).

@group(1) @binding(0) var depthD: texture_2d<f32>;
@group(1) @binding(1) var gbufD: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn defnormal(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let px = vec2i(gid.xy);
  let t = textureLoad(depthD, px, 0).r;
  if (t >= INF) { return; }
  let dir = ray_dir(vec2f(gid.xy) + 0.5);
  let rel = dir * t;
  let pc = F.cam.xyz + rel - F.shift.xyz;
  let fp = t * F.cam.w;
  let e = max(fp, 0.002);
  let hx = terrain_h(pc.xz + vec2f(e, 0.0), fp) - terrain_h(pc.xz - vec2f(e, 0.0), fp);
  let hz = terrain_h(pc.xz + vec2f(0.0, e), fp) - terrain_h(pc.xz - vec2f(0.0, e), fp);
  let n = normalize(vec3f(-hx, 2.0 * e, -hz));
  textureStore(gbufD, px, vec4f(oct_encode(n), 0.0, f32(min(level_guess(rel), F.dims.z - 1u))));
}
