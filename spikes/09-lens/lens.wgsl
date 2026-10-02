// ---- spike 09's kernels: render, probe, parameter gradients, silhouette minimum ----
// Appended after the lifted subject, lib.wgsl and the generated preamble (lens_lit, lens_tap).

struct LensView {
  o: vec4f,   // ortho: centre of the view plane; perspective: eye. w: ortho half-extent (m), 0 = perspective
  f: vec4f,   // forward (unit). w: 1 / tan(fov / 2) for perspective
  r: vec4f,   // right (unit). w: tile x0 (px)
  u: vec4f,   // up (unit). w: tile y0 (px)
}

struct LensU {
  views: array<LensView, 4>,
  a: vec4u,   // x: view count, y: tile size (px), z: mode, w: flags (1 shadows, 2 grid, 4 ground)
  b: vec4i,   // x: isolated part (-1 none), y: highlighted part (-1 none), z: influence literal (-1), w: 1 = gradients of d / |grad d|
  c: vec4f,   // x: unused, y: influence scale, z: silhouette ray half-span (m), w: unused
  d: vec4u,   // x: K literals per point, y: point or ray count, z: points offset, w: literal list offset
  e: vec4u,   // x: silhouette width, y: height, z: output offset (grad, silmin), w: output offset (probe)
  bmin: vec4f,  // creature bounds (padded) for culling rays; w > 0 when valid
  bmax: vec4f,
  tile: vec4u,  // this submission's pixel rectangle: x0, y0, width, height (GPU safety: <= ~100 ms per submit)
}

@group(0) @binding(0) var<uniform> LU: LensU;
@group(0) @binding(2) var<storage, read> lens_in: array<vec4f>;
@group(0) @binding(3) var<storage, read_write> lens_out: array<f32>;
@group(0) @binding(4) var lens_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(5) var<storage, read_write> lens_pts: array<vec4f>;

const LENS_PSTRIDE: u32 = 32u;   // probe output floats per ray: 12 fixed + up to 20 part values

// The field as the lens sees it: the whole field, or one part's value when a part is isolated.
fn lens_field(p: vec3f) -> f32 {
  let d = field(p);
  if (LU.b.x >= 0) { return lens_tap_v[LU.b.x]; }
  return d;
}

// The part with the smallest value after the last field() call: the union's winner there.
fn lens_winner() -> i32 {
  var best = 1e9;
  var k = -1;
  for (var i = 0; i < LENS_NPARTS; i++) {
    if (lens_tap_v[i] < best) { best = lens_tap_v[i]; k = i; }
  }
  return k;
}

struct LensRay { o: vec3f, d: vec3f, ortho: bool }

fn lens_ray(v: LensView, uv: vec2f) -> LensRay {
  if (v.o.w > 0.0) {
    let o = v.o.xyz + v.r.xyz * (uv.x * v.o.w) - v.u.xyz * (uv.y * v.o.w) - v.f.xyz * 6.0;
    return LensRay(o, v.f.xyz, true);
  }
  return LensRay(v.o.xyz, normalize(v.f.xyz * v.f.w + v.r.xyz * uv.x - v.u.xyz * uv.y), false);
}

// x: distance, y: 0 creature / 1 ground
fn lens_scene(p: vec3f) -> vec2f {
  let c = lens_field(p);
  if ((LU.a.w & 4u) != 0u && p.y < c) { return vec2f(p.y, 1.0); }
  return vec2f(c, 0.0);
}

// x: t, y: 0 creature / 1 ground; (-1, -1) for a miss. The creature is marched only inside its padded
// bounds (when known); the ground plane y = 0 is intersected analytically.
fn lens_trace(ro: vec3f, rd: vec3f, tmax: f32, ortho: bool) -> vec2f {
  var t0 = 0.01;
  var t1 = tmax;
  if (LU.bmin.w > 0.0) {
    let inv = 1.0 / select(rd, vec3f(1e-9), abs(rd) < vec3f(1e-9));
    let a = (LU.bmin.xyz - ro) * inv;
    let b = (LU.bmax.xyz - ro) * inv;
    t0 = max(t0, max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z)));
    t1 = min(t1, min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z)));
  }
  var tg = 1e9;
  if ((LU.a.w & 4u) != 0u) {
    if (ro.y <= 0.0) { tg = 0.01; } else if (rd.y < 0.0) { tg = -ro.y / rd.y; }
  }
  t1 = min(t1, tg);
  var t = t0;
  if (t0 < t1) {
    for (var i = 0; i < 400; i++) {
      let s = lens_field(ro + rd * t);
      let eps = select(0.0003 * max(t, 1.0), 0.0003, ortho);
      if (abs(s) < eps) { return vec2f(t, 0.0); }
      t += max(s * 0.8, 0.0004);
      if (t > t1) { break; }
    }
  }
  if (tg < tmax) { return vec2f(tg, 1.0); }
  return vec2f(-1.0, -1.0);
}

fn lens_normal(p: vec3f) -> vec3f {
  let e = vec2f(0.0005, -0.0005);
  return normalize(e.xyy * lens_field(p + e.xyy) + e.yyx * lens_field(p + e.yyx) +
                   e.yxy * lens_field(p + e.yxy) + e.xxx * lens_field(p + e.xxx));
}

fn lens_grad_p(p: vec3f) -> vec3f {
  let e = 0.0005;
  return vec3f(lens_field(p + vec3f(e, 0.0, 0.0)) - lens_field(p - vec3f(e, 0.0, 0.0)),
               lens_field(p + vec3f(0.0, e, 0.0)) - lens_field(p - vec3f(0.0, e, 0.0)),
               lens_field(p + vec3f(0.0, 0.0, e)) - lens_field(p - vec3f(0.0, 0.0, e))) / (2.0 * e);
}

fn lens_shadow(ro: vec3f, rd: vec3f) -> f32 {
  var res = 1.0;
  var t = 0.02;
  for (var i = 0; i < 96; i++) {
    let h = lens_field(ro + rd * t);
    res = min(res, 10.0 * h / t);
    t += clamp(h * 0.8, 0.01, 0.25);
    if (res < 0.002 || t > 12.0) { break; }
  }
  return clamp(res, 0.0, 1.0);
}

fn lens_ao(p: vec3f, n: vec3f) -> f32 {
  var occ = 0.0;
  var w = 1.0;
  for (var i = 1; i <= 5; i++) {
    let h = 0.03 * f32(i);
    occ += w * (h - min(lens_field(p + n * h), p.y + n.y * h));
    w *= 0.7;
  }
  return clamp(1.0 - 4.0 * occ, 0.0, 1.0);
}

fn lens_palette(k: i32) -> vec3f {
  switch (k % 10) {
    case 0: { return vec3f(0.90, 0.35, 0.30); }
    case 1: { return vec3f(0.30, 0.60, 0.90); }
    case 2: { return vec3f(0.95, 0.75, 0.20); }
    case 3: { return vec3f(0.35, 0.80, 0.40); }
    case 4: { return vec3f(0.75, 0.45, 0.90); }
    case 5: { return vec3f(0.20, 0.80, 0.80); }
    case 6: { return vec3f(0.95, 0.50, 0.75); }
    case 7: { return vec3f(0.60, 0.55, 0.30); }
    case 8: { return vec3f(0.55, 0.85, 0.95); }
    default: { return vec3f(0.85, 0.85, 0.85); }
  }
}

// Metric grid on the view plane behind the creature (orthographic views): 10 cm lines, 50 cm bold.
fn lens_grid(q: vec3f, rd: vec3f, px: f32) -> f32 {
  var g = 0.0;
  for (var a = 0; a < 3; a++) {
    if (abs(rd[a]) > 0.5) { continue; }
    let x = q[a];
    let d10 = abs(fract(x / 0.1 + 0.5) - 0.5) * 0.1 / px;
    let d50 = abs(fract(x / 0.5 + 0.5) - 0.5) * 0.5 / px;
    g = max(g, 0.35 * (1.0 - smoothstep(0.3, 0.9, d10)));
    g = max(g, 0.8 * (1.0 - smoothstep(0.5, 1.3, d50)));
  }
  return g;
}

const LENS_MODE_SHADED: u32 = 0u;
const LENS_MODE_PARTS: u32 = 1u;
const LENS_MODE_SILHOUETTE: u32 = 2u;
const LENS_MODE_INFLUENCE: u32 = 3u;
const LENS_MODE_TINT: u32 = 4u;

fn lens_shade(v: LensView, uv: vec2f, size: f32) -> vec3f {
  let ray = lens_ray(v, uv);
  let ro = ray.o;
  let rd = ray.d;
  let mode = LU.a.z;
  let sun = normalize(vec3f(0.5, 0.8, 0.35));
  let sky_col = vec3f(0.55, 0.65, 0.80);
  var col = mix(vec3f(0.80, 0.84, 0.90), sky_col, clamp(rd.y * 2.0, 0.0, 1.0));
  if (mode == LENS_MODE_SILHOUETTE) { col = vec3f(0.0); }
  let hit = lens_trace(ro, rd, select(40.0, 12.0, ray.ortho), ray.ortho);
  if (hit.x < 0.0 || (mode == LENS_MODE_SILHOUETTE && hit.y > 0.5)) {
    if (ray.ortho && (LU.a.w & 2u) != 0u && mode != LENS_MODE_SILHOUETTE) {
      let q = ro + rd * 6.0;
      col = mix(col, vec3f(0.25, 0.30, 0.38), lens_grid(q, rd, 2.0 * v.o.w / size));
    }
    return pow(clamp(col, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2));
  }
  let p = ro + rd * hit.x;
  if (mode == LENS_MODE_SILHOUETTE) { return vec3f(1.0); }
  var n = vec3f(0.0, 1.0, 0.0);
  var base = vec3f(0.0);
  var shadows = (LU.a.w & 1u) != 0u;
  if (hit.y < 0.5) {
    n = lens_normal(p);
    let d = field(p);
    let part = lens_winner();
    if (mode == LENS_MODE_PARTS) {
      base = lens_palette(part) * 0.8;
    } else if (mode == LENS_MODE_TINT) {
      let a = clamp(albedo(p), vec3f(0.0), vec3f(1.0));
      let grey = vec3f(dot(a, vec3f(0.3, 0.5, 0.2))) * 0.6 + 0.15;
      base = select(grey, mix(a, lens_palette(part), 0.55), part == LU.b.y);
    } else if (mode == LENS_MODE_INFLUENCE) {
      let h = 2e-4;
      lens_di = LU.b.z;
      lens_dh = h;
      let f1 = lens_field(p);
      lens_dh = -h;
      let f2 = lens_field(p);
      lens_di = -1;
      lens_dh = 0.0;
      let g = (f1 - f2) / (2.0 * h);
      let s = clamp(sqrt(abs(g) * LU.c.y), 0.0, 1.0);
      let grey = vec3f(0.22);
      // orange: raising the literal moves the surface outward here (d falls); blue: inward
      let hot = select(vec3f(0.15, 0.45, 1.0), vec3f(1.0, 0.45, 0.05), g < 0.0);
      base = mix(grey, hot, s);
      shadows = false;
    } else {
      base = clamp(albedo(p), vec3f(0.0), vec3f(1.0));
    }
    if (LU.b.y >= 0 && mode == LENS_MODE_PARTS && part != LU.b.y) { base = base * 0.35 + 0.1; }
  } else {
    let c = (i32(floor(p.x * 2.0)) + i32(floor(p.z * 2.0))) & 1;
    base = select(vec3f(0.20, 0.22, 0.19), vec3f(0.27, 0.29, 0.25), c == 1);
  }
  var sh = 1.0;
  var ao = 1.0;
  if (shadows) {
    sh = lens_shadow(p + n * 0.002, sun);
    ao = lens_ao(p, n);
  }
  let dif = max(dot(n, sun), 0.0);
  let hh = normalize(sun - rd);
  let spe = pow(max(dot(n, hh), 0.0), 32.0) * dif * sh * 0.15;
  col = base * (vec3f(2.0, 1.9, 1.7) * dif * sh + sky_col * 0.55 * ao * (0.5 + 0.5 * n.y)) + vec3f(spe);
  if (!ray.ortho) { col = mix(col, vec3f(0.80, 0.84, 0.90), 1.0 - exp(-0.0015 * hit.x * hit.x)); }
  return pow(clamp(col, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2));
}

@compute @workgroup_size(8, 8)
fn lens_render(@builtin(global_invocation_id) gid0: vec3u) {
  if (gid0.x >= LU.tile.z || gid0.y >= LU.tile.w) { return; }
  let gid = vec3u(gid0.xy + LU.tile.xy, 0u);
  let dims = textureDimensions(lens_tex);
  if (gid.x >= dims.x || gid.y >= dims.y) { return; }
  let size = f32(LU.a.y);
  var vi = 0u;
  for (var i = 0u; i < LU.a.x; i++) {
    let v = LU.views[i];
    if (f32(gid.x) >= v.r.w && f32(gid.x) < v.r.w + size && f32(gid.y) >= v.u.w && f32(gid.y) < v.u.w + size) { vi = i; }
  }
  let v = LU.views[vi];
  let px = vec2f(gid.xy) + 0.5 - vec2f(v.r.w, v.u.w);
  let uv = px / size * 2.0 - 1.0;
  var c = lens_shade(v, uv, size);
  if (LU.c.x == LENS_SALT && LENS_SALT != 0.0) { c.x += 1e-9; }   // a per-load salt defeats shader caches when timing compiles
  textureStore(lens_tex, gid.xy, vec4f(c, 1.0));
}

// Probe: per ray (lens_in[2i]: origin, w = tmax; lens_in[2i+1]: direction, w = 1 to snap the origin
// onto the surface instead of tracing). Writes hit, point, normal, value, gradient length, winning
// part and every part's value; and the point into lens_pts[i] for the gradient kernel.
@compute @workgroup_size(64)
fn lens_probe(@builtin(global_invocation_id) gid: vec3u) {
  let i = gid.x;
  if (i >= LU.d.y) { return; }
  let a = lens_in[2u * i];
  let b = lens_in[2u * i + 1u];
  var p = a.xyz;
  var hit = 0.0;
  var t = 0.0;
  if (b.w < 0.5) {
    let ortho = a.w < 20.0;
    let h = lens_trace(a.xyz, b.xyz, a.w, ortho);
    if (h.x > 0.0) {
      t = h.x;
      hit = 1.0 + h.y;
      if (h.y < 0.5) {
        for (var k = 0; k < 4; k++) { t += lens_field(a.xyz + b.xyz * t); }
      }
      p = a.xyz + b.xyz * t;
    }
  } else {
    for (var k = 0; k < 8; k++) {
      let g = lens_grad_p(p);
      p -= lens_field(p) * g / max(dot(g, g), 1e-6);
    }
    hit = 1.0;
  }
  let g = lens_grad_p(p);
  let n = normalize(g + vec3f(0.0, 1e-9, 0.0));
  let d = lens_field(p);
  let o = LU.e.w + i * LENS_PSTRIDE;
  lens_out[o + 0u] = hit;
  lens_out[o + 1u] = t;
  lens_out[o + 2u] = p.x;
  lens_out[o + 3u] = p.y;
  lens_out[o + 4u] = p.z;
  lens_out[o + 5u] = n.x;
  lens_out[o + 6u] = n.y;
  lens_out[o + 7u] = n.z;
  lens_out[o + 8u] = d;
  lens_out[o + 9u] = length(g);
  lens_out[o + 10u] = f32(lens_winner());
  lens_out[o + 11u] = f32(LENS_NPARTS);
  for (var k = 0; k < min(LENS_NPARTS, 20); k++) { lens_out[o + 12u + u32(k)] = lens_tap_v[k]; }
  lens_pts[LU.d.z + i] = vec4f(p, d);
}

// Parameter gradients by central differences. Invocation (pt, j): j = 0 writes the field value and
// the spatial gradient length at the point; j = 1..K writes d field / d literal for the j-th entry
// of the literal list (lens_in[litOffset + j - 1]: x = literal index, y = step h).
@compute @workgroup_size(64)
fn lens_grad(@builtin(global_invocation_id) gid: vec3u, @builtin(num_workgroups) nwg: vec3u) {
  let i = gid.x + gid.y * nwg.x * 64u;
  let K = LU.d.x;
  let pt = i / (K + 1u);
  let j = i % (K + 1u);
  if (pt >= LU.d.y) { return; }
  let p = lens_pts[LU.d.z + pt].xyz;
  let o = LU.e.z + pt * (K + 2u);
  if (j == 0u) {
    lens_out[o] = lens_field(p);
    lens_out[o + 1u] = length(lens_grad_p(p));
    return;
  }
  let L = lens_in[LU.d.w + j - 1u];
  let h = L.y;
  lens_di = i32(L.x);
  lens_dh = h;
  var f1 = lens_field(p);
  if (LU.b.w == 1) { f1 = f1 / max(length(lens_grad_p(p)), 0.25); }
  lens_dh = -h;
  var f2 = lens_field(p);
  if (LU.b.w == 1) { f2 = f2 / max(length(lens_grad_p(p)), 0.25); }
  lens_di = -1;
  lens_dh = 0.0;
  // b.w = 1: the derivative of the distance estimate d / |grad d|, which a pure scale factor on the
  // field (a Lipschitz safety factor) can't change; b.w = 0: of d itself.
  lens_out[o + 1u + j] = (f1 - f2) / (2.0 * h);
}

// Silhouette minimum: for each pixel of an orthographic view (views[0]), the minimum of the field
// along the pixel's ray and where it is. The pixel is inside the silhouette when the minimum is < 0.
// Outside the surface the march takes unrelaxed sphere steps; inside, steps of half the depth; then a
// golden-section search refines the minimum.
fn lens_ray_f(o: vec3f, rd: vec3f, t: f32) -> f32 { return lens_field(o + rd * t); }

@compute @workgroup_size(8, 8)
fn lens_silmin(@builtin(global_invocation_id) gid0: vec3u) {
  let W = LU.e.x;
  let H = LU.e.y;
  if (gid0.x >= LU.tile.z || gid0.y >= LU.tile.w) { return; }
  let gid = vec3u(gid0.xy + LU.tile.xy, 0u);
  if (gid.x >= W || gid.y >= H) { return; }
  let v = LU.views[0];
  let span = LU.c.z;
  let uv = (vec2f(gid.xy) + 0.5) / vec2f(f32(W), f32(H)) * 2.0 - 1.0;
  let rd = v.f.xyz;
  let o = v.o.xyz + v.r.xyz * (uv.x * v.o.w) - v.u.xyz * (uv.y * v.o.w) - rd * span;
  var t = 0.0;
  var best = 1e9;
  var bt = 0.0;
  var bstep = 0.004;
  for (var i = 0; i < 400; i++) {
    let d = lens_ray_f(o, rd, t);
    var step = d;
    if (d < 0.0005) { step = clamp(-d * 0.5, 0.002, 0.02); }
    if (d < best) { best = d; bt = t; bstep = max(step, 0.002); }
    t += step;
    if (t > 2.0 * span) { break; }
  }
  if (best < 0.05) {
    var lo = bt - bstep;
    var hi = bt + bstep;
    let gr = 0.381966;
    var x1 = lo + gr * (hi - lo);
    var x2 = hi - gr * (hi - lo);
    var f1 = lens_ray_f(o, rd, x1);
    var f2 = lens_ray_f(o, rd, x2);
    for (var k = 0; k < 12; k++) {
      if (f1 < f2) { hi = x2; x2 = x1; f2 = f1; x1 = lo + gr * (hi - lo); f1 = lens_ray_f(o, rd, x1); }
      else { lo = x1; x1 = x2; f1 = f2; x2 = hi - gr * (hi - lo); f2 = lens_ray_f(o, rd, x2); }
    }
    if (min(f1, f2) < best) {
      best = min(f1, f2);
      bt = select(x2, x1, f1 < f2);
    }
  }
  let pix = gid.x + gid.y * W;
  let pb = o + rd * bt;
  var gl = 1.0;
  if (best < 0.05) { gl = length(lens_grad_p(pb)); }
  lens_out[LU.e.z + pix] = best;
  lens_pts[LU.d.z + pix] = vec4f(pb, gl);
}

// Field samples on a grid (views[0].o = origin, views[0].f.xyz = dims, views[0].f.w = cell size), for
// counting separate pieces. tile.x = first cell of this submission, tile.z = cells in it.
@compute @workgroup_size(64)
fn lens_gridk(@builtin(global_invocation_id) gid: vec3u, @builtin(num_workgroups) nwg: vec3u) {
  let k = gid.x + gid.y * nwg.x * 64u;
  if (k >= LU.tile.z) { return; }
  let i = k + LU.tile.x;
  let n = vec3u(LU.views[0].f.xyz);
  if (i >= n.x * n.y * n.z) { return; }
  let c = vec3u(i % n.x, (i / n.x) % n.y, i / (n.x * n.y));
  let p = LU.views[0].o.xyz + (vec3f(c) + 0.5) * LU.views[0].f.w;
  lens_out[i] = lens_field(p);
}
