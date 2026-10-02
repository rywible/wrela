// The raster hybrid: the same wolf as a linear-blend-skinned mesh, shaded per pixel with the same
// material and light, sun shadows from a shadow map (D-086), and optional fur shells: the mesh drawn
// again N times, pushed out along its normal, alpha-blended with the same strand texture.
// Appended to common.wgsl + wolf.wgsl.

struct View {
  viewproj: mat4x4f,
  light: mat4x4f,       // the sun's view-projection, for the shadow map
  blob0: vec4f,         // ground AO occluders (world spheres): chest, hindquarters, neck
  blob1: vec4f,
  blob2: vec4f,
  shells: vec4f,        // x: shell count
}

@group(0) @binding(1) var<uniform> V: View;
@group(0) @binding(2) var shadow_map: texture_depth_2d;
@group(0) @binding(3) var shadow_samp: sampler_comparison;
@group(0) @binding(4) var strands: texture_2d_array<f32>;
@group(0) @binding(5) var samp: sampler;
@group(0) @binding(6) var<storage, read> palette: array<mat4x4f, 22>;

const LEAN: f32 = 2.2;     // as trace.wgsl
const SIGMA: f32 = 1100.0;
const LAYERS: f32 = 16.0;

struct VIn {
  @location(0) pos: vec3f,
  @location(1) bones: vec4u,
  @location(2) nrm: vec3f,
  @location(3) weights: vec4f,
  @location(4) ao: f32,
  @location(5) albedo: vec4f,
  @location(6) chan: vec4f,     // tail, head, coat length, skin depth
}

struct MOut {
  @builtin(position) clip: vec4f,
  @location(0) rest: vec3f,
  @location(1) world: vec3f,
  @location(2) n: vec3f,
  @location(3) r0: vec3f,
  @location(4) r1: vec3f,
  @location(5) r2: vec3f,
  @location(6) ao: f32,
  @location(7) chan: vec4f,
  @location(8) albedo: vec3f,
  @location(9) h: f32,
  @location(10) nr: vec3f,
}

fn skin_m(v: VIn) -> mat4x4f {
  return palette[v.bones.x] * v.weights.x + palette[v.bones.y] * v.weights.y
       + palette[v.bones.z] * v.weights.z + palette[v.bones.w] * v.weights.w;
}

fn skinned(v: VIn, out_h: f32, push: f32) -> MOut {
  let m = skin_m(v);
  let nw = normalize((m * vec4f(v.nrm, 0.0)).xyz);
  let wp = (m * vec4f(v.pos, 1.0)).xyz + nw * push;
  var o: MOut;
  o.clip = V.viewproj * vec4f(wp, 1.0);
  o.rest = v.pos;
  o.world = wp;
  o.n = nw;
  o.r0 = m[0].xyz;
  o.r1 = m[1].xyz;
  o.r2 = m[2].xyz;
  o.ao = v.ao;
  o.chan = v.chan;
  o.albedo = v.albedo.rgb * v.albedo.rgb;
  o.h = out_h;
  o.nr = v.nrm;
  return o;
}

@vertex
fn mesh_vs(v: VIn) -> MOut { return skinned(v, -1.0, 0.0); }

@vertex
fn shell_vs(v: VIn, @builtin(instance_index) ii: u32) -> MOut {
  let k = f32(ii + 1u) / V.shells.x;
  return skinned(v, (F.fur.x + k * F.fur.y) / (F.fur.x + F.fur.y), k * F.fur.y);
}

@vertex
fn shadow_vs(v: VIn) -> @builtin(position) vec4f {
  return V.light * (skin_m(v) * vec4f(v.pos, 1.0));
}

// ---- light ---------------------------------------------------------------------------------------

fn shadow_at(world: vec3f, n: vec3f) -> f32 {
  let lp = V.light * vec4f(world + n * 0.01, 1.0);
  let uv = lp.xy * vec2f(0.5, -0.5) + 0.5;
  if (any(uv < vec2f(0.0)) || any(uv > vec2f(1.0))) { return 1.0; }
  let texel = 1.0 / 2048.0;
  var s = 0.0;
  for (var k = 0; k < 4; k++) {
    let o = vec2f(f32(k & 1) - 0.5, f32(k >> 1) - 0.5) * 2.0 * texel;
    s += textureSampleCompareLevel(shadow_map, shadow_samp, uv + o, lp.z - 0.0015);
  }
  return s * 0.25;
}

/** Sky occlusion under the body from three spheres (analytic sphere AO): a cheap stand-in for the
 *  tracer's field-sampled ground AO. */
fn blob_ao(p: vec3f) -> f32 {
  var ao = 1.0;
  for (var k = 0; k < 3; k++) {
    var b = V.blob0;
    if (k == 1) { b = V.blob1; }
    if (k == 2) { b = V.blob2; }
    let dv = b.xyz - p;
    let d = length(dv);
    ao *= 1.0 - clamp(b.w * b.w / (d * d) * max(dv.y / d, 0.0), 0.0, 1.0) * 0.8;
  }
  return ao;
}

fn coat_tangent(rest: vec3f, nr: vec3f, ch: vec2f, R: mat3x3f) -> vec3f {
  var comb = comb_dir(rest, ch);
  comb += 0.35 * vec3f(noise3(rest * 18.0), noise3(rest * 18.0 + 11.0), noise3(rest * 18.0 + 23.0));
  let ct = normalize(comb - nr * dot(comb, nr) + vec3f(1e-5, 0.0, 0.0));
  return ct;
}

// ---- passes ----------------------------------------------------------------------------------------

@fragment
fn mesh_fs(i: MOut) -> @location(0) vec4f {
  let n = normalize(i.n);
  let v = normalize(F.eye.xyz - i.world);
  let R = mat3x3f(i.r0, i.r1, i.r2);
  let nr = normalize(i.nr);
  let ch = i.chan.xy;
  let alb = wolf_albedo(i.rest, ch);                 // per pixel, as the tracer
  let ct = coat_tangent(i.rest, nr, ch, R);
  let T = normalize(R * (ct * LEAN + nr));
  let sh = select(1.0, shadow_at(i.world, n), dot(n, F.sun.xyz) > -0.35);
  let col = shade_fur_surface(alb, n, T, v, sh, i.ao);
  return vec4f(tonemap(fog(col, -v, distance(F.eye.xyz, i.world))), 1.0);
}

@fragment
fn shell_fs(i: MOut) -> @location(0) vec4f {
  let len = i.chan.z;
  let hl = i.h / max(len, 1e-3);
  let velvet = 1.0 - smoothstep(0.32, 0.55, len);
  if (hl >= 1.0 && velvet < 0.5) { discard; }
  let n = normalize(i.n);
  let v = normalize(F.eye.xyz - i.world);
  let R = mat3x3f(i.r0, i.r1, i.r2);
  let nr = normalize(i.nr);
  let ch = i.chan.xy;
  let ct = coat_tangent(i.rest, nr, ch, R);
  let shell = F.fur.x + F.fur.y;
  let root = i.rest - nr * F.fur.x - ct * (LEAN * i.h * shell);
  var w = pow(abs(nr), vec3f(4.0));
  w /= (w.x + w.y + w.z);
  let layer = i32(min(hl * LAYERS, LAYERS - 1.0));
  let s = 1.0 / F.fur.z;
  var st = w.x * textureSample(strands, samp, root.zy * s, layer).rg
         + w.y * textureSample(strands, samp, root.xz * s, layer).rg
         + w.z * textureSample(strands, samp, root.xy * s, layer).rg;
  if (hl >= 1.0) { st = vec2f(0.0, 0.5); }
  st = mix(st, vec2f(select(0.0, 0.55, i.h < len), 0.5), velvet);
  let cov = st.x * min(i.chan.w * 4.0, 1.0);
  let dz = F.fur.y / V.shells.x;
  let a = 1.0 - exp(-SIGMA * cov * dz / max(abs(dot(n, v)), 0.2));
  if (a < 0.004) { discard; }
  let T = normalize(R * (ct * LEAN + nr));
  let fl = fur_light(T, n, v);
  let sh = shadow_at(i.world, n);
  let direct = fl.diff * fl.lam * sh;
  let spec = (0.07 * fl.spec1 + 0.16 * fl.spec2 * i.albedo * 3.0) * fl.lam * sh;
  let amb = ambient(n) * i.ao;
  let deep = 0.35 + 0.65 * i.h;
  let lit = (i.albedo * (2.0 * st.y) * (SUN_COL * direct + amb) + SUN_COL * spec) * deep;
  let c = tonemap(fog(lit, -v, distance(F.eye.xyz, i.world)));
  return vec4f(c * a, a);
}

struct EOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
}

@vertex
fn env_vs(@builtin(vertex_index) vi: u32) -> EOut {
  let p = vec2f(f32((vi << 1u) & 2u), f32(vi & 2u));
  var o: EOut;
  o.clip = vec4f(p * 2.0 - 1.0, 0.0, 1.0);
  o.uv = vec2f(p.x * 2.0 - 1.0, p.y * 2.0 - 1.0);
  return o;
}

struct FOut {
  @location(0) color: vec4f,
  @builtin(frag_depth) depth: f32,
}

/** Ground and sky, with the shadow map and blob AO. `ENV_SHADOW` false: no creature terms. */
override ENV_SHADOW: bool = true;

@fragment
fn env_fs(i: EOut) -> FOut {
  let d = normalize(F.cf.xyz + F.cr.xyz * i.uv.x + F.cu.xyz * i.uv.y);
  var o: FOut;
  if (d.y < -1e-6) {
    let t = -F.eye.y / d.y;
    let p = F.eye.xyz + d * t;
    var sh = 1.0;
    var ao = 1.0;
    if (ENV_SHADOW) { sh = shadow_at(p, vec3f(0.0, 1.0, 0.0)); ao = blob_ao(p); }
    o.color = vec4f(tonemap(shade_ground(p, d, t, sh, ao)), 1.0);
    let c = V.viewproj * vec4f(p, 1.0);
    o.depth = clamp(c.z / c.w, 0.0, 1.0);
  } else {
    o.color = vec4f(tonemap(sky(d)), 1.0);
    o.depth = 1.0;
  }
  return o;
}
