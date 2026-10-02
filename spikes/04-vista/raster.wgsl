// The comparison path for terrain only: a rasterized heightfield clipmap (geometry clipmaps, Losasso
// and Hoppe 2004). Each level is an M×M grid of quads drawn as frustum-culled 32×32 blocks; the
// heights come from a toroidal texture cooked from the same height function (cook.wgsl's hcook).
// Vertices morph toward the coarser level near a level's outer edge, so the two meet without
// cracks; a coarser level discards fragments inside the finer level's square. Writes the same
// G-buffer as march.wgsl. Appended to common.wgsl + field.wgsl (rfs_fn evaluates the field).

struct RLevel {
  org: vec4f,     // xz: the grid origin relative to the camera (m); w: sample spacing (m)
  box: vec4f,     // the level's square relative to the camera: min x, min z, max x, max z
  gi: vec4i,      // xy: absolute sample index of the grid origin
}

struct RFrame {
  vp: mat4x4f,
  info: vec4u,    // levels, M, T (texture period), unused
  camy: vec4f,    // xyz: camera (content); w: radians per pixel
  lv: array<RLevel, 16>,
}

@group(0) @binding(21) var<uniform> RF: RFrame;
@group(0) @binding(22) var hmap: texture_2d_array<f32>;
@group(0) @binding(23) var nmap: texture_2d_array<f32>;
@group(0) @binding(24) var nsamp: sampler;
@group(0) @binding(25) var<storage, read> blocks: array<vec4u>;

struct VOut {
  @builtin(position) pos: vec4f,
  @location(0) rel: vec3f,
  @location(1) uv: vec2f,
  @location(2) alpha: f32,
  @location(3) @interpolate(flat) level: u32,
}

fn hload(g: vec2i, L: u32) -> f32 {
  let T = i32(RF.info.z);
  return textureLoad(hmap, ((g % T) + T) % T, i32(L), 0).r;
}

@vertex
fn rvs(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut {
  let blk = blocks[ii];
  let L = blk.x;
  let lv = RF.lv[L];
  let gl = vec2i(i32(blk.y * 32u + vi % 33u), i32(blk.z * 32u + vi / 33u));
  let g = lv.gi.xy + gl;
  let s = lv.org.w;
  let rel = lv.org.xz + vec2f(gl) * s;
  var h = hload(g, L);
  let halfM = f32(RF.info.y) * 0.5;
  let cheb = max(abs(rel.x), abs(rel.y)) / s;
  let wdt = halfM * 0.25;
  var alpha = clamp((cheb - (halfM - 2.0 - wdt)) / wdt, 0.0, 1.0);
  if (L + 1u >= RF.info.x) { alpha = 0.0; }
  if (alpha > 0.0) {
    // The coarser level's surface at this vertex: its sample, or the midpoint of its edge or its
    // quad's diagonal (both levels split quads along the same diagonal).
    let odd = g & vec2i(1);
    let gc = (g - odd) / 2;
    let hc = 0.5 * (hload(gc, L + 1u) + hload(gc + odd, L + 1u));
    h = mix(h, hc, alpha);
  }
  var o: VOut;
  let p = vec3f(rel.x, h - RF.camy.y, rel.y);
  o.pos = RF.vp * vec4f(p, 1.0);
  o.rel = p;
  o.uv = (vec2f(g) + 0.5) / f32(RF.info.z);
  o.alpha = alpha;
  o.level = L;
  return o;
}

struct FOut {
  @location(0) g: vec4f,
  @location(1) t: f32,
}

@fragment
fn rfs(i: VOut) -> FOut {
  if (i.level > 0u) {
    let b = RF.lv[i.level - 1u].box;
    if (i.rel.x > b.x && i.rel.x < b.z && i.rel.z > b.y && i.rel.z < b.w) { discard; }
  }
  var dh = textureSampleLevel(nmap, nsamp, i.uv, i32(i.level), 0.0).xy;
  if (i.alpha > 0.0 && i.level + 1u < RF.info.x) {
    let uvc = (i.uv * f32(RF.info.z) - 0.5) * 0.5 / f32(RF.info.z) + 0.5 / f32(RF.info.z);
    dh = mix(dh, textureSampleLevel(nmap, nsamp, uvc, i32(i.level + 1u), 0.0).xy, i.alpha);
  }
  let n = normalize(vec3f(-dh.x, 1.0, -dh.y));
  var o: FOut;
  o.g = vec4f(oct_encode(n), 0.0, f32(i.level));
  o.t = length(i.rel);
  return o;
}

/** As rfs, with the normal from the field per pixel (D-089's per-pixel field shading on raster
 *  visibility): the terrain's height gradient at a footprint-sized offset, 4 height evaluations. */
@fragment
fn rfs_fn(i: VOut) -> FOut {
  if (i.level > 0u) {
    let b = RF.lv[i.level - 1u].box;
    if (i.rel.x > b.x && i.rel.x < b.z && i.rel.z > b.y && i.rel.z < b.w) { discard; }
  }
  let t = length(i.rel);
  let fp = t * RF.camy.w;
  let pc = RF.camy.xyz + i.rel;
  let e = max(fp, 0.002);
  let hx = terrain_h(pc.xz + vec2f(e, 0.0), fp) - terrain_h(pc.xz - vec2f(e, 0.0), fp);
  let hz = terrain_h(pc.xz + vec2f(0.0, e), fp) - terrain_h(pc.xz - vec2f(0.0, e), fp);
  let n = normalize(vec3f(-hx, 2.0 * e, -hz));
  var o: FOut;
  o.g = vec4f(oct_encode(n), 0.0, f32(i.level));
  o.t = t;
  return o;
}
