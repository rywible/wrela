// Harness tools (not measured): probing the field from JS, and a top-down map for placing things.
// Appended to common + field.

@group(1) @binding(0) var<storage, read> pin: array<vec4f>;
@group(1) @binding(1) var<storage, read_write> pout: array<vec4f>;
@group(1) @binding(2) var mapout: texture_storage_2d<rgba8unorm, write>;

/** pin: (x, y, z, fp). pout: (h(x, z), |∇h| at fp, mountain mask, world(x, y, z)). */
@compute @workgroup_size(64)
fn probe(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= arrayLength(&pin)) { return; }
  let q = pin[gid.x];
  let fp = q.w;
  let e = max(fp, 0.01);
  let h = terrain_h(q.xz, fp);
  let hx = (terrain_h(q.xz + vec2f(e, 0.0), fp) - terrain_h(q.xz - vec2f(e, 0.0), fp)) / (2.0 * e);
  let hz = (terrain_h(q.xz + vec2f(0.0, e), fp) - terrain_h(q.xz - vec2f(0.0, e), fp)) / (2.0 * e);
  pout[gid.x] = vec4f(h, length(vec2f(hx, hz)), mountain_mask(q.xz), world(q.xyz, fp));
}

/** A 1080×1080 shaded relief of [-4096, 4096]², with 1 km grid lines and the ruins marked. */
@compute @workgroup_size(8, 8)
fn map(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= 1080u || gid.y >= 1080u) { return; }
  let s = 8192.0 / 1080.0;
  let q = (vec2f(gid.xy) + 0.5) * s - 4096.0;
  let h = terrain_h(q, s);
  let hx = (terrain_h(q + vec2f(s, 0.0), s) - terrain_h(q - vec2f(s, 0.0), s)) / (2.0 * s);
  let hz = (terrain_h(q + vec2f(0.0, s), s) - terrain_h(q - vec2f(0.0, s), s)) / (2.0 * s);
  let n = normalize(vec3f(-hx, 1.0, -hz));
  let shade = 0.35 + 0.65 * max(dot(n, normalize(vec3f(-0.6, 0.7, -0.4))), 0.0);
  let lo = vec3f(0.25, 0.42, 0.18);
  let mid = vec3f(0.55, 0.48, 0.32);
  let hi = vec3f(0.92, 0.92, 0.95);
  var c = mix(lo, mid, smoothstep(50.0, 700.0, h));
  c = mix(c, hi, smoothstep(1100.0, 1500.0, h));
  c *= shade;
  // slope over the assumed bound, at 1 m
  let e1 = 0.5;
  let gx = (terrain_h(q + vec2f(e1, 0.0), 1.0) - terrain_h(q - vec2f(e1, 0.0), 1.0)) / (2.0 * e1);
  let gz = (terrain_h(q + vec2f(0.0, e1), 1.0) - terrain_h(q - vec2f(0.0, e1), 1.0)) / (2.0 * e1);
  if (length(vec2f(gx, gz)) > T_G) { c = vec3f(1.0, 0.0, 1.0); } else if (length(vec2f(gx, gz)) > 2.0) { c = mix(c, vec3f(1.0, 0.5, 1.0), 0.5); }
  // contour every 100 m
  if (abs(fract(h / 100.0) - 0.5) > 0.47) { c *= 0.75; }
  // 1 km grid
  let gq = abs(fract(q / 1024.0 + 0.5) - 0.5) * 1024.0;
  if (min(gq.x, gq.y) < s * 0.6) { c = mix(c, vec3f(0.0, 0.0, 0.0), 0.35); }
  // origin
  if (length(q) < 3.0 * s) { c = vec3f(0.0, 0.0, 1.0); }
  // ruins
  let nr = u32(ruins[0].x);
  for (var i = 0u; i < nr; i++) {
    if (length(q - ruins[1u + 3u * i].xz) < 4.0 * s) { c = vec3f(1.0, 0.1, 0.1); }
  }
  textureStore(mapout, vec2i(gid.xy) + vec2i(420, 0), vec4f(c, 1.0));   // centred in 1920×1080
}
