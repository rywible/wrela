// Temporal accumulation, plus measurement helpers (accumulation, image differences, foliage coverage).
// Appended to common.wgsl. None of the helpers run in measured frames.

// ---- TAA ------------------------------------------------------------------------------------------------------
@group(0) @binding(1) var cur: texture_2d<f32>;
@group(0) @binding(2) var hist: texture_2d<f32>;
@group(0) @binding(3) var lsamp: sampler;
@group(0) @binding(4) var gAi: texture_2d<u32>;
@group(0) @binding(5) var outp: texture_storage_2d<rgba16float, write>;

fn ycocg(c: vec3f) -> vec3f {
  return vec3f(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b, -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}
fn from_ycocg(y: vec3f) -> vec3f { return vec3f(y.x + y.y - y.z, y.x + y.z, y.x - y.y - y.z); }

/** Reprojects the history through the G-buffer's depth (the opaque layer, or the foliage layer where
 *  it dominates), clamps it to the current frame's 3×3 neighbourhood in YCoCg, and blends. */
@compute @workgroup_size(8, 8)
fn taa(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let p = vec2i(gid.xy);
  let mx = vec2i(F.dims.xy) - 1;
  let c = textureLoad(cur, p, 0).rgb;
  var lo = vec3f(1e9);
  var hi = vec3f(-1e9);
  for (var dy = -1; dy <= 1; dy++) {
    for (var dx = -1; dx <= 1; dx++) {
      let s = ycocg(textureLoad(cur, clamp(p + vec2i(dx, dy), vec2i(0), mx), 0).rgb);
      lo = min(lo, s);
      hi = max(hi, s);
    }
  }
  let a = textureLoad(gAi, p, 0);
  let tf = unpack2x16float(a.y);
  var tr = select(bitcast<f32>(a.x), tf.x, tf.y > 0.5);
  tr = min(tr, 1e6);
  let wp = F.eye.xyz + ray_dir(vec2f(gid.xy) + 0.5) * tr;
  let v = wp - F.peye.xyz;
  let z = dot(v, F.pcf.xyz);
  let ndc = vec2f(dot(v, F.pcr.xyz) / dot(F.pcr.xyz, F.pcr.xyz), dot(v, F.pcu.xyz) / dot(F.pcu.xyz, F.pcu.xyz)) / z;
  let uv = vec2f(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
  var o = c;
  if (F.world.w > 0.5 && z > 0.0 && all(uv >= vec2f(0.0)) && all(uv <= vec2f(1.0))) {
    let h = textureSampleLevel(hist, lsamp, uv, 0.0).rgb;
    let hc = from_ycocg(clamp(ycocg(h), lo, hi));
    o = mix(hc, c, max(1.0 / (F.world.w + 1.0), F.jit.w));
  }
  textureStore(outp, p, vec4f(o, 1.0));
}

// ---- accumulation (reference and converged images) ---------------------------------------------------------------
struct AP { w: u32, h: u32, mode: u32, n: u32, rect: vec4u }   // rect: the region diff() counts (x0, y0, x1, y1)
@group(0) @binding(10) var asrc: texture_2d<f32>;
@group(0) @binding(11) var<storage, read_write> acc: array<vec4f>;
@group(0) @binding(12) var<uniform> ap: AP;
@group(0) @binding(13) var adst: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn accum(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= ap.w || gid.y >= ap.h) { return; }
  let i = gid.y * ap.w + gid.x;
  let s = textureLoad(asrc, vec2i(gid.xy), 0);
  if (ap.mode == 0u) { acc[i] = s; } else { acc[i] += s; }
}

@compute @workgroup_size(8, 8)
fn resolve(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= ap.w || gid.y >= ap.h) { return; }
  textureStore(adst, vec2i(gid.xy), acc[gid.y * ap.w + gid.x] / f32(ap.n));
}

// ---- differences (8-bit display values, as spike 02) and coverage --------------------------------------------------------
@group(0) @binding(14) var da: texture_2d<f32>;
@group(0) @binding(15) var db: texture_2d<f32>;
@group(0) @binding(16) var<storage, read_write> dres: array<atomic<u32>, 8>;
@group(0) @binding(17) var gAc: texture_2d<u32>;

var<workgroup> ws: array<atomic<u32>, 4>;

fn to_byte(v: vec3f) -> vec3f { return round(255.0 * pow(clamp(v, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2))); }

@compute @workgroup_size(8, 8)
fn diff(@builtin(global_invocation_id) gid: vec3u, @builtin(local_invocation_index) li: u32) {
  if (li < 4u) { atomicStore(&ws[li], 0u); }
  workgroupBarrier();
  if (all(gid.xy >= ap.rect.xy) && all(gid.xy < ap.rect.zw)) {
    let e = abs(to_byte(textureLoad(da, vec2i(gid.xy), 0).rgb) - to_byte(textureLoad(db, vec2i(gid.xy), 0).rgb));
    atomicAdd(&ws[0], u32(e.r + e.g + e.b));
    let m = max(e.r, max(e.g, e.b));
    if (m > 8.0) { atomicAdd(&ws[1], 1u); }
    if (m > 0.0) { atomicAdd(&ws[2], 1u); }
  }
  workgroupBarrier();
  if (li < 3u) { atomicAdd(&dres[li], atomicLoad(&ws[li])); }
}

/** Foliage coverage: the foliage layer's opacity, plus explicit leaves in the opaque layer. */
@compute @workgroup_size(8, 8)
fn coverage(@builtin(global_invocation_id) gid: vec3u, @builtin(local_invocation_index) li: u32) {
  if (li < 4u) { atomicStore(&ws[li], 0u); }
  workgroupBarrier();
  if (gid.x < ap.w && gid.y < ap.h) {
    let a = textureLoad(gAc, vec2i(gid.xy), 0);
    let af = unpack2x16float(a.y).y;
    let mat = u32(round(unpack4x8unorm(a.z).w * 255.0));
    let leaf = select(0.0, 1.0, mat == MAT_LEAF || mat == MAT_NEEDLE);
    let cov = af + (1.0 - af) * leaf;
    atomicAdd(&ws[0], u32(round(cov * 1024.0)));
    if (cov > 0.01) { atomicAdd(&ws[1], 1u); }
  }
  workgroupBarrier();
  if (li < 2u) { atomicAdd(&dres[li], atomicLoad(&ws[li])); }
}

/** |a − b| in display values, ×4, red where any channel is off by more than 8/255: the error image. */
@group(0) @binding(18) var errimg: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn diffimg(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= ap.w || gid.y >= ap.h) { return; }
  let e = abs(to_byte(textureLoad(da, vec2i(gid.xy), 0).rgb) - to_byte(textureLoad(db, vec2i(gid.xy), 0).rgb));
  let m = max(e.r, max(e.g, e.b));
  let g = clamp(m * 4.0 / 255.0, 0.0, 1.0);
  let v = select(vec3f(g * 0.6), vec3f(1.0, 0.15, 0.1) * max(g, 0.5), m > 8.0);
  textureStore(errimg, vec2i(gid.xy), vec4f(v * v, 1.0));   // squared: the blit re-encodes to sRGB
}
