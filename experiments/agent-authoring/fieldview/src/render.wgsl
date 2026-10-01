// ---- fieldview's renderer: sphere tracing the user's field over a checkered ground plane ----

struct Cam {
  eye: vec4f,       // xyz eye
  look_at: vec4f,   // xyz point looked at
  params: vec4f,    // x: vertical fov (radians), y: tile size px, z: tile x0, w: tile y0
}

@group(0) @binding(0) var<uniform> cam: Cam;

@vertex
fn fv_vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
  let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}

// x: distance, y: 0 creature / 1 ground
fn fv_scene(p: vec3f) -> vec2f {
  let c = field(p);
  if (c < p.y) { return vec2f(c, 0.0); }
  return vec2f(p.y, 1.0);
}

fn fv_trace(ro: vec3f, rd: vec3f, tmax: f32) -> vec2f {
  var t = 0.01;
  for (var i = 0; i < 400; i++) {
    let s = fv_scene(ro + rd * t);
    if (abs(s.x) < 0.0003 * max(t, 1.0)) { return vec2f(t, s.y); }
    t += max(s.x * 0.8, 0.0004);
    if (t > tmax) { break; }
  }
  return vec2f(-1.0, -1.0);
}

fn fv_normal(p: vec3f) -> vec3f {
  let e = vec2f(0.0005, -0.0005);
  return normalize(e.xyy * field(p + e.xyy) + e.yyx * field(p + e.yyx) +
                   e.yxy * field(p + e.yxy) + e.xxx * field(p + e.xxx));
}

fn fv_shadow(ro: vec3f, rd: vec3f) -> f32 {
  var res = 1.0;
  var t = 0.02;
  for (var i = 0; i < 96; i++) {
    let h = field(ro + rd * t);
    res = min(res, 10.0 * h / t);
    t += clamp(h * 0.8, 0.01, 0.25);
    if (res < 0.002 || t > 12.0) { break; }
  }
  return clamp(res, 0.0, 1.0);
}

fn fv_ao(p: vec3f, n: vec3f) -> f32 {
  var occ = 0.0;
  var w = 1.0;
  for (var i = 1; i <= 5; i++) {
    let h = 0.03 * f32(i);
    occ += w * (h - min(field(p + n * h), p.y + n.y * h));
    w *= 0.7;
  }
  return clamp(1.0 - 4.0 * occ, 0.0, 1.0);
}

@fragment
fn fv_fs(@builtin(position) frag: vec4f) -> @location(0) vec4f {
  let size = cam.params.y;
  let uv = (frag.xy - cam.params.zw) / size * 2.0 - 1.0;
  let ro = cam.eye.xyz;
  let fw = normalize(cam.look_at.xyz - ro);
  let rt = normalize(cross(fw, vec3f(0.0, 1.0, 0.0)));
  let up = cross(rt, fw);
  let f = 1.0 / tan(cam.params.x * 0.5);
  let rd = normalize(fw * f + rt * uv.x - up * uv.y);

  let sun = normalize(vec3f(0.5, 0.8, 0.35));
  let sky_col = vec3f(0.55, 0.65, 0.80);
  var col = mix(vec3f(0.80, 0.84, 0.90), sky_col, clamp(rd.y * 2.0, 0.0, 1.0));
  let hit = fv_trace(ro, rd, 40.0);
  if (hit.x > 0.0) {
    let p = ro + rd * hit.x;
    var n = vec3f(0.0, 1.0, 0.0);
    var base = vec3f(0.0);
    if (hit.y < 0.5) {
      n = fv_normal(p);
      base = clamp(albedo(p), vec3f(0.0), vec3f(1.0));
    } else {
      let c = (i32(floor(p.x * 2.0)) + i32(floor(p.z * 2.0))) & 1;   // 0.5 m checks show scale
      base = select(vec3f(0.20, 0.22, 0.19), vec3f(0.27, 0.29, 0.25), c == 1);
    }
    let sh = fv_shadow(p + n * 0.002, sun);
    let ao = fv_ao(p, n);
    let dif = max(dot(n, sun), 0.0);
    let h = normalize(sun - rd);
    let spe = pow(max(dot(n, h), 0.0), 32.0) * dif * sh * 0.15;
    col = base * (vec3f(2.0, 1.9, 1.7) * dif * sh + sky_col * 0.55 * ao * (0.5 + 0.5 * n.y)) + vec3f(spe);
    col = mix(col, vec3f(0.80, 0.84, 0.90), 1.0 - exp(-0.0015 * hit.x * hit.x));
  }
  return vec4f(pow(clamp(col, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2)), 1.0);
}

// ---- diagnostics: the field on a grid around the framing box ----

struct Grid {
  origin: vec4f,    // xyz origin, w cell size
  dims: vec4u,
}

@group(0) @binding(1) var<uniform> grid: Grid;
@group(0) @binding(2) var<storage, read_write> samples: array<f32>;

@compute @workgroup_size(4, 4, 4)
fn fv_grid(@builtin(global_invocation_id) id: vec3u) {
  let n = grid.dims.x;
  if (any(id >= vec3u(n))) { return; }
  let p = grid.origin.xyz + (vec3f(id) + 0.5) * grid.origin.w;
  samples[id.x + n * (id.y + n * id.z)] = field(p);
}
