// The fallback: simulated meshes rasterized over the marched world, composited by depth.
// Appended to common.wgsl + world.wgsl. Group 0 is the trace kernel's group 0.

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<uniform> M: Mats;
@group(0) @binding(2) var<storage, read> prims: array<vec4f>;
@group(0) @binding(3) var<storage, read> chars: array<vec4f>;
@group(0) @binding(4) var<storage, read> pos: array<vec4f>;
@group(0) @binding(5) var<storage, read> attr: array<vec4f>;
@group(0) @binding(6) var<storage, read> segs: array<vec4f>;
@group(0) @binding(7) var<storage, read> sheetb: array<u32>;
@group(0) @binding(8) var hvol: texture_3d<f32>;
@group(0) @binding(9) var hlight: texture_3d<f32>;
@group(0) @binding(10) var hskip: texture_3d<f32>;
@group(0) @binding(11) var lsamp: sampler;
@group(0) @binding(12) var smap: texture_depth_2d;
@group(0) @binding(13) var csamp: sampler_comparison;
@group(1) @binding(3) var<storage, read> boff: array<u32>;
@group(1) @binding(4) var<storage, read> blist: array<u32>;
@group(2) @binding(0) var tdepth: texture_2d<f32>;

// world.wgsl's switches, fixed for the raster path: bodies and tower are fields, cloth shadows come
// from the shadow map, hair shadows from its density grid.
override CLOTH: u32 = 0u;
override HAIRSH: bool = true;
override CLOTHSH: bool = true;
override FIELD_SOFT: bool = false;
override SMAP: bool = true;
override REF: bool = false;
override REF_LIGHT: bool = false;
override P: u32 = 1u;
override STEP_SCALE: f32 = 1.0;
override EPS_SCALE: f32 = 0.25;
override SHADOW_STEPS: u32 = 48u;

// ---- shadow map: cloth triangles from the sun ----------------------------------------------------------

@vertex
fn smap_vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
  return F.lightvp * vec4f(pos[vi].xyz, 1.0);
}

// ---- cloth -------------------------------------------------------------------------------------------------

struct CVOut {
  @invariant @builtin(position) clip: vec4f,
  @location(0) w: vec3f,
  @location(1) n: vec3f,
  @location(2) uv: vec3f,
}

@vertex
fn cloth_vs(@builtin(vertex_index) vi: u32) -> CVOut {
  var o: CVOut;
  let p = pos[vi].xyz;
  o.clip = F.viewproj * vec4f(p, 1.0);
  o.w = p;
  o.n = attr[2u * vi].xyz;
  o.uv = attr[2u * vi + 1u].xyz;
  return o;
}

// ---- the marched world's depth, where it's in front of the meshes -----------------------------------
// Written after the trace and before shading, so mesh shading can use an equal depth test with no
// discard (a discard would force late depth testing and shade every overdrawn fragment).

@vertex
fn full_vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
  let p = vec2f(f32((vi << 1u) & 2u), f32(vi & 2u));
  return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn world_depth_fs(@builtin(position) fc: vec4f) -> @builtin(frag_depth) f32 {
  let t = textureLoad(tdepth, vec2i(fc.xy), 0).x;
  if (t > 1e8) { discard; }
  let su = fc.x / f32(F.dims.x) * 2.0 - 1.0;
  let sv = 1.0 - fc.y / f32(F.dims.y) * 2.0;
  let d = normalize(F.cf.xyz + F.cr.xyz * su + F.cu.xyz * sv);
  let c = F.viewproj * vec4f(F.eye.xyz + d * t, 1.0);
  return clamp(c.z / c.w, 0.0, 1.0);
}

@fragment
fn cloth_fs(i: CVOut) -> @location(0) vec4f {
  let rel = i.w - F.eye.xyz;
  let t = length(rel);
  let d = rel / t;
  let nrm = normalize(i.n);
  let fp = t * F.eye.w;
  let sunside = select(-nrm, nrm, dot(nrm, F.sun.xyz) > 0.0);
  let vis = sun_vis(i.w, sunside, 0.25 * fp + CLOTH_H, true);
  var c = shade_cloth(i.w, d, nrm, i.uv.xy, u32(i.uv.z + 0.5), fp, vis, vis);
  c = fog(c, t, d);
  return vec4f(tonemap(c), 1.0);
}

// ---- hair: camera-facing ribbons, one quad per segment ----------------------------------------------------

struct HVOut {
  @invariant @builtin(position) clip: vec4f,
  @location(0) w: vec3f,
  @location(1) tg: vec3f,
  @location(2) @interpolate(flat) seg: u32,
}

@vertex
fn hair_vs(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> HVOut {
  var o: HVOut;
  let a = segs[2u * ii];
  let b = segs[2u * ii + 1u];
  let ends = array<f32, 6>(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);
  let sides = array<f32, 6>(-1.0, 1.0, -1.0, -1.0, 1.0, 1.0);
  let e = ends[vi];
  let p = mix(a.xyz, b.xyz, e);
  let r = mix(a.w, b.w, e);
  let tg = normalize(b.xyz - a.xyz + vec3f(0.0, 1e-7, 0.0));
  let side = normalize(cross(tg, F.eye.xyz - p) + vec3f(1e-7, 0.0, 0.0));
  let q = p + side * r * sides[vi] * select(1.0, 0.0, max(a.w, b.w) <= 0.0);
  o.clip = F.viewproj * vec4f(q, 1.0);
  o.w = q;
  o.tg = tg;
  o.seg = ii;
  return o;
}

@fragment
fn hair_fs(i: HVOut) -> @location(0) vec4f {
  let rel = i.w - F.eye.xyz;
  let t = length(rel);
  let d = rel / t;
  let lt = hair_light_at(i.w);
  let strand = i.seg / F.misc.y;
  let base = chars[C_HAIR].rgb * (0.75 + 0.5 * hash_f(strand * 7919u + 13u));
  let c = hair_shade(normalize(i.tg), -d, base, lt, 0.25 + 0.75 * lt);
  return vec4f(tonemap(fog(c, t, d)), 1.0);
}
