// The styles: realistic, cel and painterly. Reads the G-buffer and the light inputs, writes
// display-linear colour (the blit encodes it). Appended to common.wgsl + placement + wolf.wgsl + scene.wgsl.

@group(0) @binding(2) var gb: texture_2d<u32>;
@group(0) @binding(3) var lt: texture_2d<f32>;
@group(0) @binding(4) var out_tex: texture_storage_2d<rgba16float, write>;

override STYLE: u32 = 0u;           // 0 realistic, 1 cel, 2 painterly
override FIELD_LINES: bool = true;  // cel: field-native outlines and curvature creases
override HALO: bool = true;         // painterly: strokes overshoot silhouettes
override VIEW: u32 = 0u;            // 0 shaded, 1 outlines only (white, black lines), 2 primary-step heat map,
                                    // 3 shadow, 4 AO, 5 thickness, 6 curvature (debug)

const SUN_C = vec3f(3.1, 2.8, 2.35);

// ---- realistic --------------------------------------------------------------------------------------

fn sky_clouds(d: vec3f) -> f32 {
  let uv = d.xz / (d.y + 0.1) * 0.9;
  return fbm3(vec3f(uv.x + 3.0, uv.y, 1.7), 4);
}

fn sky_real(d: vec3f) -> vec3f {
  let l = F.sun.xyz;
  let sd = max(dot(d, l), 0.0);
  let y = max(d.y, 0.0);
  var c = mix(vec3f(0.60, 0.70, 0.84), vec3f(0.12, 0.27, 0.64), pow(y, 0.5));
  c += vec3f(1.0, 0.78, 0.5) * (0.22 * pow(sd, 6.0) + 0.5 * pow(sd, 64.0));
  if (d.y > 0.0) {
    let n = sky_clouds(d);
    let cov = smoothstep(0.02, 0.35, n) * smoothstep(0.0, 0.15, d.y);
    let lit = 0.6 + 0.4 * smoothstep(-0.1, 0.3, sky_clouds(normalize(d + l * 0.04)) - n + 0.1);
    c = mix(c, vec3f(1.05, 1.0, 0.95) * lit + vec3f(0.3, 0.2, 0.1) * pow(sd, 8.0), cov * 0.85);
  }
  c += vec3f(30.0, 26.0, 20.0) * smoothstep(0.99985, 0.99992, dot(d, l));
  return c;
}

fn fog_real(c: vec3f, d: vec3f, t: f32) -> vec3f {
  let sd = max(dot(d, F.sun.xyz), 0.0);
  let fc = mix(vec3f(0.56, 0.66, 0.80), vec3f(1.0, 0.86, 0.62), 0.5 * pow(sd, 6.0));
  return mix(c, fc, 1.0 - exp(-max(t - 15.0, 0.0) * 0.0028));
}

fn tower_q(p: vec3f) -> vec3f { return p - TOWER_P; }

/** Linear albedo and roughness for the realistic style. */
fn albedo_real(mat: u32, p: vec3f, n: vec3f) -> vec4f {
  switch (mat) {
    case 1u: {   // ground
      let r = length(p.xz);
      let n1 = noise3(vec3f(p.x, 0.0, p.z) * 0.35);
      let n2 = noise3(vec3f(p.x, 0.5, p.z) * 2.3);
      var c = mix(vec3f(0.050, 0.085, 0.020), vec3f(0.115, 0.135, 0.035), 0.5 + 0.5 * n1) * (0.85 + 0.3 * n2);
      let dirt = smoothstep(0.35, 0.6, noise3(vec3f(p.x, 3.0, p.z) * 0.15)) * (1.0 - smoothstep(6.0, 12.0, r));
      c = mix(c, vec3f(0.13, 0.10, 0.07) * (0.85 + 0.3 * n2), dirt);
      c = mix(c, vec3f(0.07, 0.055, 0.03) * (0.8 + 0.4 * n2), 0.75 * smoothstep(14.0, 21.0, r + 3.0 * n1));
      return vec4f(c, 0.9);
    }
    case 2u: {   // bark
      return vec4f(vec3f(0.08, 0.06, 0.045) * (0.7 + 0.5 * noise3(p * vec3f(9.0, 1.5, 9.0))), 0.85);
    }
    case 3u: {   // leaves
      let hue = tree_seed(p).y;
      let c = mix(vec3f(0.045, 0.095, 0.020), vec3f(0.095, 0.115, 0.022), hue) * (0.8 + 0.4 * noise3(p * 3.0));
      return vec4f(c, 0.78);
    }
    case 4u: {   // stone
      let q = tower_q(p);
      let ang = wrap_a(atan2(q.z, q.x) - DOOR_A);
      let m = masonry(q, ang);
      var c = vec3f(0.30, 0.285, 0.26) * (0.78 + 0.35 * m.y) * (0.9 + 0.2 * noise3(p * 6.0));
      c = mix(c, vec3f(0.36, 0.35, 0.32), 1.0 - smoothstep(0.015, 0.035, m.x));
      let moss = max(smoothstep(0.55, 0.85, n.y), 1.0 - smoothstep(0.3, 1.4, q.y + 0.6 * noise3(p * 1.7)));
      c = mix(c, vec3f(0.06, 0.085, 0.025), moss * 0.8);
      return vec4f(c, 0.8);
    }
    case 5u: {   // rock
      var c = vec3f(0.24, 0.23, 0.21) * (0.8 + 0.4 * noise3(p * 4.0));
      c = mix(c, vec3f(0.06, 0.085, 0.025), smoothstep(0.45, 0.8, n.y + 0.3 * noise3(p * 3.0)));
      return vec4f(c, 0.75);
    }
    case 6u: {   // the wolf
      let a = wolf_albedo(wolf_local(p));
      return vec4f(a, mix(0.3, 0.75, smoothstep(0.02, 0.06, luma(a))));
    }
    default: { return vec4f(0.5, 0.5, 0.5, 0.8); }
  }
}

fn shade_real(mat: u32, p: vec3f, n: vec3f, d: vec3f, li: vec4f) -> vec3f {
  let l = F.sun.xyz;
  let v = -d;
  let ar = albedo_real(mat, p, n);
  let alb = ar.rgb;
  let rough = ar.w;
  let ndl = max(dot(n, l), 0.0);
  let sh = li.x * li.x * (3.0 - 2.0 * li.x);
  let amb = mix(vec3f(0.13, 0.13, 0.08), vec3f(0.30, 0.42, 0.66), 0.5 + 0.5 * n.y) * li.y;
  var c = alb * (SUN_C * ndl * sh + amb);
  // GGX specular
  let hv = normalize(l + v);
  let nh = max(dot(n, hv), 0.0);
  let nv = max(dot(n, v), 1e-3);
  let a2 = rough * rough * rough * rough;
  let dn = nh * nh * (a2 - 1.0) + 1.0;
  let D = a2 / (PI * dn * dn);
  let k = (rough + 1.0) * (rough + 1.0) / 8.0;
  let G = nv / (nv * (1.0 - k) + k) * ndl / (ndl * (1.0 - k) + k);
  let Fr = 0.04 + 0.96 * pow(1.0 - max(dot(v, hv), 0.0), 5.0);
  c += SUN_C * D * G * Fr / (4.0 * nv * max(ndl, 1e-3) + 1e-4) * ndl * sh;
  if (mat == MAT_LEAF) {
    // translucency: thin leaf masses glow when the sun is behind them
    let fwd = pow(max(dot(d, l), 0.0), 4.0);
    c += alb * vec3f(1.2, 1.5, 0.5) * SUN_C * sh * (1.0 - li.z) * (0.15 * max(-dot(n, l), 0.0) + 0.35 * fwd);
  }
  if (mat == MAT_WOLF) {
    c += alb * SUN_C * sh * 0.3 * pow(1.0 - nv, 3.0);   // fur sheen at grazing angles
  }
  return c;
}

// ---- stylized palettes (display values) ---------------------------------------------------------------

fn base_style(mat: u32, p: vec3f, n: vec3f) -> vec3f {
  switch (mat) {
    case 1u: {
      let r = length(p.xz);
      let n1 = noise3(vec3f(p.x, 0.0, p.z) * 0.35);
      var c = mix(vec3f(0.45, 0.64, 0.27), vec3f(0.56, 0.70, 0.30), smoothstep(-0.1, 0.1, n1));
      let dirt = smoothstep(0.42, 0.48, noise3(vec3f(p.x, 3.0, p.z) * 0.15)) * (1.0 - smoothstep(6.0, 12.0, r));
      c = mix(c, vec3f(0.74, 0.62, 0.45), dirt);
      return mix(c, vec3f(0.36, 0.42, 0.22), smoothstep(16.0, 18.0, r + 3.0 * n1));
    }
    case 2u: { return vec3f(0.45, 0.34, 0.27); }
    case 3u: { return mix(vec3f(0.30, 0.56, 0.24), vec3f(0.46, 0.62, 0.22), tree_seed(p).y); }
    case 4u: {
      let q = tower_q(p);
      let m = masonry(q, wrap_a(atan2(q.z, q.x) - DOOR_A));
      var c = vec3f(0.76, 0.72, 0.66) * (0.93 + 0.1 * m.y);
      c = mix(c, vec3f(0.62, 0.58, 0.54), 1.0 - smoothstep(0.015, 0.03, m.x));
      let moss = max(smoothstep(0.6, 0.7, n.y), 1.0 - smoothstep(0.7, 0.8, q.y + 0.6 * noise3(p * 1.7)));
      return mix(c, vec3f(0.48, 0.58, 0.32), moss);
    }
    case 5u: {
      return mix(vec3f(0.66, 0.64, 0.60), vec3f(0.48, 0.58, 0.32), smoothstep(0.55, 0.65, n.y + 0.3 * noise3(p * 3.0)));
    }
    case 6u: {
      // the wolf's own albedo, brightened; cel posterizes it gently (hard steps would turn the
      // albedo's fur-streak noise into stripes), paint leaves the texture to the strokes
      let a = min(pow(wolf_albedo(wolf_local(p)), vec3f(1.0 / 2.2)) * 1.12, vec3f(1.0));
      if (STYLE != 1u) { return a; }
      let L = max(luma(a), 1e-3);
      let x = L * 3.0;
      let Lq = (floor(x) + smoothstep(0.15, 0.85, fract(x)) + 0.35) / 3.0;
      return clamp(a * (Lq / L), vec3f(0.0), vec3f(1.0));
    }
    default: { return vec3f(0.5); }
  }
}

fn line_col(mat: u32) -> vec3f {
  switch (mat) {
    case 1u: { return vec3f(0.20, 0.28, 0.13); }
    case 2u: { return vec3f(0.20, 0.13, 0.09); }
    case 3u: { return vec3f(0.10, 0.21, 0.11); }
    case 4u: { return vec3f(0.27, 0.24, 0.25); }
    case 5u: { return vec3f(0.25, 0.23, 0.22); }
    case 6u: { return vec3f(0.12, 0.10, 0.11); }
    default: { return vec3f(0.15); }
  }
}

fn style_fog(c: vec3f, d: vec3f, t: f32) -> vec3f {
  let hor = select(srgb(vec3f(0.74, 0.86, 0.97)), srgb(vec3f(0.80, 0.86, 0.92)), STYLE == 2u);
  return mix(c, hor, 0.88 * smoothstep(20.0, 280.0, t));
}

// ---- cel -------------------------------------------------------------------------------------------------

fn sky_cel(d: vec3f) -> vec3f {
  let l = F.sun.xyz;
  var c = mix(srgb(vec3f(0.74, 0.86, 0.97)), srgb(vec3f(0.24, 0.50, 0.90)), smoothstep(0.0, 0.6, d.y));
  if (d.y > 0.0) {
    let n = sky_clouds(d);
    let cov = smoothstep(0.10, 0.12, n) * smoothstep(0.02, 0.1, d.y);
    let lit = smoothstep(0.0, 0.02, sky_clouds(normalize(d + l * 0.05)) - n + 0.04);
    c = mix(c, mix(srgb(vec3f(0.78, 0.82, 0.94)), vec3f(1.0), lit), cov);
  }
  c = mix(c, vec3f(1.0, 0.97, 0.85), smoothstep(0.9993, 0.9995, dot(d, l)));
  return c;
}

fn shade_cel(mat: u32, p: vec3f, n: vec3f, d: vec3f, li: vec4f) -> vec3f {
  let l = F.sun.xyz;
  let lin = srgb(base_style(mat, p, n));
  var ndl = dot(n, l) + 0.08;
  if (mat == MAT_LEAF) { ndl += 0.45 * noise3(p * 1.4); }     // leaf-cluster shaped light patches
  let lit = smoothstep(-0.02, 0.02, ndl) * smoothstep(0.30, 0.42, li.x);
  var c = lin * mix(vec3f(0.30, 0.33, 0.55), vec3f(1.04, 1.0, 0.92), lit);
  c *= mix(0.72, 1.0, smoothstep(0.35, 0.55, li.y));
  if (mat != MAT_GROUND) {
    let rim = smoothstep(0.62, 0.68, 1.0 - max(dot(n, -d), 0.0)) * (0.35 + 0.65 * lit);
    c += (lin * 0.4 + vec3f(0.05, 0.06, 0.08)) * rim;
  }
  if (FIELD_LINES) {
    let crease = smoothstep(0.30, 0.55, li.w);
    c = mix(c, srgb(line_col(mat)), crease * 0.85);
    if (mat == MAT_STONE || mat == MAT_ROCK) { c = mix(c, c * 1.3 + 0.02, smoothstep(0.35, 0.7, -li.w) * 0.6); }
  }
  return c;
}

// ---- painterly ------------------------------------------------------------------------------------------

// Strokes: a solid texture anchored in object space. Each cell holds one stroke, an ellipsoid at a
// jittered centre, 1.05 cells long along the stroke direction, 0.55 across and 0.45 through the
// surface. A stroke exists only if its centre lies within 0.4 cells of the surface (first-order,
// from the point's distance `off` and the normal), so strokes hug surfaces, and those near a
// silhouette reach past it. Higher-priority strokes cover lower ones (a soft maximum, below).
const ST_L: f32 = 1.05;
const ST_W: f32 = 0.55;
const ST_T: f32 = 0.45;

fn stroke_layer(p: vec3f, n: vec3f, tang: vec3f, rand_angle: bool, cell: f32, off: f32, seed: u32) -> vec3f {
  let q = p / cell;
  let base = vec3i(floor(q));
  let bit = cross(n, tang);
  let oc = off / cell;
  // A soft maximum over the covering strokes' priorities, with feathered edges: it reads as the
  // top stroke over the others (a mosaic of dabs), but stays continuous, so a sub-pixel change
  // in the surface point can't flip a pixel to another stroke.
  var acc = vec3f(0.0);
  var wsum = 0.0;
  var cover = 0.0;
  for (var z = -1; z <= 1; z++) {
    for (var y = -1; y <= 1; y++) {
      for (var x = -1; x <= 1; x++) {
        let c = base + vec3i(x, y, z);
        // reject before hashing: a jitter of ±0.4 moves a centre at most 0.7 cells, so a cell whose
        // centre is over 1.1 cells off the surface slab, or 1.75 cells away, can't reach this point
        let r0 = q - (vec3f(c) + 0.5);
        if (abs(oc - dot(r0, n)) > 1.1 || dot(r0, r0) > 3.06) { continue; }
        let h = pcg4d(vec4u(bitcast<vec3u>(c), seed));
        let jit = vec3f(h.xyz & vec3u(0xffffu)) * (1.0 / 65535.0) - 0.5;
        let r = q - (vec3f(c) + 0.5 + jit * 0.8);
        let w = dot(r, n);
        if (abs(oc - w) > 0.4) { continue; }      // the stroke's centre is off the surface
        var tu = tang;
        var tv = bit;
        if (rand_angle) {
          let a = f32(h.w & 0xffu) * (PI / 255.0);
          tu = tang * cos(a) + bit * sin(a);
          tv = cross(n, tu);
        }
        let u = dot(r, tu) / ST_L;
        let v = dot(r, tv) / ST_W;
        let e = u * u + v * v + (w / ST_T) * (w / ST_T);
        if (e >= 1.0) { continue; }
        let cov = smoothstep(1.0, 0.7, e);
        let wt = cov * exp(14.0 * f32(h.w >> 16u) * (1.0 / 65535.0));
        acc += wt * vec3f(f32(h.x >> 16u) * (2.0 / 65535.0) - 1.0, f32(h.y >> 16u) * (2.0 / 65535.0) - 1.0, 1.0);
        wsum += wt;
        cover = max(cover, cov);
      }
    }
  }
  if (wsum <= 0.0) { return vec3f(0.0); }
  return vec3f(acc.xy / wsum * cover, cover);
}

/** Two octaves around the target cell size (F.prm.y px), blended so contrast holds (Bénard et al.). */
fn strokes(p: vec3f, n: vec3f, tang: vec3f, rand_angle: bool, fp: f32, off: f32, seed: u32) -> vec3f {
  let lod = log2(max(F.prm.y * fp, 1e-4) / 0.01);
  let k = floor(lod);
  let f = lod - k;
  let c0 = 0.01 * exp2(k);
  let wb = smoothstep(0.3, 0.7, f);   // most pixels need one octave; the blend stays continuous
  let wa = 1.0 - wb;
  var a = vec3f(0.0);
  var b = vec3f(0.0);
  let lv = u32(i32(k) + 64);
  if (wa > 0.0) { a = stroke_layer(p, n, tang, rand_angle, c0, off, seed * 131u + lv); }
  if (wb > 0.0) { b = stroke_layer(p, n, tang, rand_angle, c0 * 2.0, off, seed * 131u + lv + 1u); }
  let m = (a.xy * wa + b.xy * wb) * inverseSqrt(wa * wa + wb * wb);
  return vec3f(m, a.z * wa + b.z * wb);
}

struct Frame3 { p: vec3f, n: vec3f, t: vec3f, rnd: bool }

/** Where a material's strokes are anchored, and which way they run (the field's gradient, projected). */
fn stroke_frame(mat: u32, p: vec3f, n: vec3f) -> Frame3 {
  var f = Frame3(p, n, vec3f(1.0, 0.0, 0.0), false);
  var axis = vec3f(0.0, 1.0, 0.0);
  switch (mat) {
    case 1u: { axis = normalize(vec3f(1.0, 0.0, 0.35)); }
    case 2u: { axis = vec3f(0.0, 1.0, 0.0); }
    case 3u, 5u: { axis = vec3f(0.0, 1.0, 0.0); f.rnd = true; }
    case 4u: { axis = normalize(cross(vec3f(0.0, 1.0, 0.0), n) + vec3f(1e-4, 0.0, 0.0)); }
    case 6u: {
      // anchored in the wolf's frame, oriented by the fur-free body (the fur's noisy normal smears them)
      f.p = wolf_local(p);
      f.n = wolf_smooth_n(f.p);
      axis = vec3f(0.0, 0.0, 1.0);
    }
    default: {}
  }
  var t = axis - f.n * dot(f.n, axis);
  if (dot(t, t) < 1e-4) { t = vec3f(1.0, 0.0, 0.0) - f.n * f.n.x; }
  f.t = normalize(t);
  return f;
}

fn sky_paint(d: vec3f) -> vec3f {
  let l = F.sun.xyz;
  var c = mix(srgb(vec3f(0.80, 0.86, 0.92)), srgb(vec3f(0.36, 0.55, 0.84)), smoothstep(0.0, 0.7, d.y));
  if (d.y > 0.0) {
    let uv = d.xz / (d.y + 0.1) * 0.9;
    let n = fbm3(vec3f(uv.x * 0.8 + 3.0, uv.y * 1.6, 1.7), 4);
    let streak = noise3(vec3f(uv.x * 2.0, uv.y * 14.0, 4.0));
    let cov = smoothstep(0.0, 0.3, n + 0.06 * streak) * smoothstep(0.0, 0.15, d.y);
    let warm = smoothstep(-0.1, 0.3, dot(d, l));
    c = mix(c, mix(srgb(vec3f(0.86, 0.88, 0.94)), srgb(vec3f(1.0, 0.96, 0.86)), warm), cov * 0.9);
  }
  return c;
}

fn paint_value(mat: u32, p: vec3f, n: vec3f, d: vec3f, li: vec4f) -> f32 {
  let l = F.sun.xyz;
  let shv = smoothstep(0.08, 0.6, li.x);
  var v = 0.12 + 0.58 * max(dot(n, l), 0.0) * shv + 0.25 * li.y * (0.6 + 0.4 * n.y);
  if (mat == MAT_LEAF) {
    v += 0.2 * noise3(p * 1.4) * shv;
    v += 0.35 * pow(max(dot(d, l), 0.0), 4.0) * (1.0 - li.z) * shv;   // backlit glow
  }
  return v;
}

fn paint_color(mat: u32, p: vec3f, n: vec3f, v: f32, s: vec3f) -> vec3f {
  let b = base_style(mat, p, n);
  let creature = mat == MAT_WOLF;
  let x = clamp(v + s.x * select(0.13, 0.08, creature), 0.0, 1.0) * 3.0;
  let q = (floor(x) + smoothstep(0.2, 0.8, fract(x))) / 3.0;
  // cool shadows for the world, warmer and less saturated ones for the creature's pale coat
  let shadow = b * select(vec3f(0.50, 0.53, 0.68), vec3f(0.56, 0.54, 0.60), creature) + vec3f(0.01, 0.015, 0.035);
  let light = min(b * vec3f(1.18, 1.10, 0.92) + vec3f(0.05, 0.04, 0.0), vec3f(1.0));
  var c = mix(shadow, b, smoothstep(0.0, 0.5, q));
  c = mix(c, light, smoothstep(0.5, 1.0, q));
  c *= 1.0 + s.y * vec3f(0.06, 0.0, -0.06);
  return srgb(clamp(c, vec3f(0.0), vec3f(1.0)));
}

fn shade_paint(mat: u32, p: vec3f, n: vec3f, d: vec3f, li: vec4f, fp: f32) -> vec3f {
  let fr = stroke_frame(mat, p, n);
  let s = strokes(fr.p, fr.n, fr.t, fr.rnd, fp, 0.0, mat);
  var c = paint_color(mat, p, n, paint_value(mat, p, n, d, li), s);
  return c * mix(0.8, 1.0, li.y);
}

// ---- the kernel -----------------------------------------------------------------------------------------

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

@compute @workgroup_size(8, 8)
fn shade(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = vec3u(gid0.xy + F.dims.zw, 0u);         // + the tile origin (0 for a whole frame)
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let g = gb_unpack(textureLoad(gb, vec2i(gid.xy), 0));
  let d = ray_dir(gid.xy);
  let pa = F.eye.w;
  var c = vec3f(0.0);

  if (VIEW == 1u) {
    c = vec3f(1.0);
    if (FIELD_LINES) { c = vec3f(1.0 - clamp(F.prm.x + 0.5 - g.line_px, 0.0, 1.0)); }
    textureStore(out_tex, vec2i(gid.xy), vec4f(c, 1.0));
    return;
  }
  if (VIEW == 2u) {
    textureStore(out_tex, vec2i(gid.xy), vec4f(heat(f32(g.steps) / 96.0), 1.0));
    return;
  }

  let li = textureLoad(lt, vec2i(gid.xy), 0);
  if (VIEW >= 3u) {
    let x = li[(VIEW + 1u) % 4u];   // 3 → x, 4 → y, 5 → z, 6 → w
    textureStore(out_tex, vec2i(gid.xy), vec4f(select(vec3f(x * x), vec3f(0.5 + x, 0.5, 0.5 - x), VIEW == 6u), 1.0));
    return;
  }
  let p = F.eye.xyz + d * g.t;
  let fp = g.t * pa;
  if (STYLE == 0u) {
    if (g.mat == MAT_SKY) { c = sky_real(d); }
    else { c = fog_real(shade_real(g.mat, p, g.n, d, li), d, g.t); }
    c = tonemap(c);
  } else if (STYLE == 1u) {
    if (g.mat == MAT_SKY) { c = sky_cel(d); }
    else { c = style_fog(shade_cel(g.mat, p, g.n, d, li), d, g.t); }
    if (FIELD_LINES && g.line_px < 63.0) {
      let a = clamp(F.prm.x + 0.5 - g.line_px, 0.0, 1.0);
      c = mix(c, style_fog(srgb(line_col(g.line_mat)), d, g.line_t), a);
    }
  } else {
    if (g.mat == MAT_SKY) { c = sky_paint(d); }
    else { c = style_fog(shade_paint(g.mat, p, g.n, d, li, fp), d, g.t); }
    if (HALO && g.line_px < F.prm2.x && g.line_mat != MAT_GROUND && g.line_mat != MAT_SKY) {
      // Overshoot: the stroke field around the near surface, at the ray's closest approach.
      let pl = F.eye.xyz + d * g.line_t;
      let fpl = g.line_t * pa;
      let nl = scene_n(pl, fpl, max(fpl, 1e-3));
      let fr = stroke_frame(g.line_mat, pl, nl);
      let s = strokes(fr.p, fr.n, fr.t, fr.rnd, fpl, g.line_px * fpl, g.line_mat);
      if (s.z > 0.5) {
        let v = 0.12 + 0.58 * max(dot(nl, F.sun.xyz), 0.0) * 0.8 + 0.2;
        c = style_fog(paint_color(g.line_mat, pl, nl, v, s), d, g.line_t);
      }
    }
  }
  textureStore(out_tex, vec2i(gid.xy), vec4f(c, 1.0));
}
