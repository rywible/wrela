// The world as fields, plus cloth and hair lookups, shadows and shading. Included by trace.wgsl and
// raster.wgsl, which declare the bindings (F, M, prims, chars, pos, attr, segs, sheetb, boff, blist,
// hvol, hlight, hskip, lsamp, smap, csamp) and the overrides (CLOTH, HAIRSH, SMAP, REF, P,
// SHADOW_STEPS, STEP_SCALE).

struct Mats { m: array<vec4f, 256> }   // per sheet: [front, pattern] [back, rough] [accent, kind] [patch0, patches, nu, wrap]

struct Counters {
  c_steps: u32, c_marches: u32, c_hits: u32, c_misses: u32, c_caps: u32, c_cands: u32, c_entered: u32, c_tris: u32,
  h0: u32, h1: u32, h2: u32, h3: u32, h4: u32, minstep: f32,
  b_steps: u32, b_marches: u32, b_caps: u32,
  s_cands: u32, s_layers: u32,
  v_samples: u32,
  sh_steps: u32, sh_caps: u32, sh_rays: u32, sh_hair: u32,
}
var<private> cnt: Counters;

const SOFT: f32 = 18.0;           // soft-shadow sharpness (penumbra ∝ distance / SOFT)

// ---- characters: rigid round cones, smooth-unioned (spike 02's rule) ----------------------------------

fn cp(c: u32, k: u32) -> vec4f { return chars[c * CS + C_PARTS + k]; }

fn char_d(c: u32, mask: u32, p: vec3f) -> f32 {
  let kb = chars[c * CS + C_LO].w;
  var d = BIG + select(0.0, 1.0, kb == SALT);
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let b = firstTrailingBit(m);
    m &= m - 1u;
    let a = cp(c, 2u * b);
    let e = cp(c, 2u * b + 1u);
    d = smin(d, round_cone_d(p, a.xyz, e.xyz, a.w, e.w), kb);
  }
  return d;
}

struct Iv { m: u32, t0: f32, t1: f32 }

/** Per-ray PartMask: parts whose capsule, inflated by the blend radius, the ray enters before tmax. */
fn char_mask(c: u32, o: vec3f, d: vec3f, tmax: f32) -> Iv {
  let kb = chars[c * CS + C_LO].w;
  var r = Iv(0u, INF, 0.0);
  for (var b = 0u; b < NPARTS; b++) {
    let a = cp(c, 2u * b);
    let e = cp(c, 2u * b + 1u);
    let iv = ray_capsule(o, d, a.xyz, e.xyz, max(a.w, e.w) + kb);
    if (iv.y > 0.0 && iv.x <= iv.y && iv.x < tmax) {
      r.m |= 1u << b;
      r.t0 = min(r.t0, iv.x);
      r.t1 = max(r.t1, iv.y);
    }
  }
  return r;
}

fn char_normal(c: u32, mask: u32, p: vec3f) -> vec3f {
  let h = 0.0005;
  let k = vec2f(1.0, -1.0);
  return normalize(k.xyy * char_d(c, mask, p + k.xyy * h) + k.yyx * char_d(c, mask, p + k.yyx * h)
                 + k.yxy * char_d(c, mask, p + k.yxy * h) + k.xxx * char_d(c, mask, p + k.xxx * h));
}

/** Albedo and roughness of a body at p: by nearest part, with a scalp and hands. */
fn char_material(c: u32, mask: u32, p: vec3f) -> vec4f {
  var best = BIG;
  var bp = 0u;
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let b = firstTrailingBit(m);
    m &= m - 1u;
    let a = cp(c, 2u * b);
    let e = cp(c, 2u * b + 1u);
    let dd = round_cone_d(p, a.xyz, e.xyz, a.w, e.w);
    if (dd < best) { best = dd; bp = b; }
  }
  let base = c * CS;
  let garment = chars[base + C_GARMENT];
  let trousers = chars[base + C_TROUSERS];
  let skin = chars[base + C_SKIN];
  let hair = chars[base + C_HAIR];
  if (bp == 2u) { return vec4f(skin.rgb, 0.5); }
  if (bp == P_HEAD) {
    let a = cp(c, 6u);
    let e = cp(c, 7u);
    let dir = normalize(p - (a.xyz + e.xyz) * 0.5);
    let fwd = dot(dir, chars[base + C_FWD].xyz);
    let scalp = dir.y > 0.33 - 0.62 * max(-fwd, 0.0) && fwd < 0.62;
    if (scalp && hair.w > 0.5) { return vec4f(hair.rgb * 0.6, 0.6); }
    return vec4f(skin.rgb, 0.5);
  }
  if (bp == 5u || bp == 7u) {
    let a = cp(c, 2u * bp);
    let e = cp(c, 2u * bp + 1u);
    let s = dot(p - a.xyz, e.xyz - a.xyz) / dot(e.xyz - a.xyz, e.xyz - a.xyz);
    if (s > 0.8) { return vec4f(skin.rgb, 0.5); }
    return vec4f(garment.rgb, garment.w);
  }
  if (bp >= 12u) { return vec4f(0.045, 0.03, 0.02, 0.55); }
  if (bp >= 8u) { return vec4f(trousers.rgb, trousers.w); }
  return vec4f(garment.rgb, garment.w);
}

// ---- the tower: a shaft, a corbel band, a crenellated parapet, flag poles and banner rods -------------

const N_CRENEL: f32 = 16.0;

fn tower_d(p: vec3f) -> f32 {
  let q = vec3f(p.x - TOWER_C.x, p.y, p.z - TOWER_C.y);
  let r = length(q.xz);
  var d = max(r - TOWER_R, p.y - TOWER_TOP);
  d = min(d, max(r - (TOWER_R + 0.28), abs(p.y - 12.55) - 0.45));
  let ring = max(max(r - (TOWER_R + 0.28), (TOWER_R - 0.14) - r), abs(p.y - 13.55) - 0.55);
  let sec = 2.0 * PI / N_CRENEL;
  let ang = atan2(q.z, q.x);
  let a2 = ang - sec * floor(ang / sec) - 0.5 * sec;
  let lt = r * sin(a2);
  let cut = max(abs(lt) - 0.36, 13.62 - p.y);
  d = min(d, max(ring, -cut));
  // Flag poles above the parapet; a lower bound below it.
  if (p.y > 12.6) {
    for (var f = 0; f < 4; f++) {
      let phi = radians(-60.0 + 38.0 * f32(f));
      let c = vec2f(TOWER_C.x + sin(phi) * (TOWER_R - 0.15), TOWER_C.y - cos(phi) * (TOWER_R - 0.15));
      let h = vec2f(length(p.xz - c) - 0.032, max(13.0 - p.y, p.y - 17.3));
      d = min(d, min(max(h.x, h.y), 0.0) + length(max(h, vec2f(0.0))));
    }
  } else {
    d = min(d, 12.96 - p.y);
  }
  // Banner rods.
  if (abs(p.y - 10.6) < 0.3) {
    for (var b = 0; b < 6; b++) {
      let phi = radians(-52.0 + 20.8 * f32(b));
      let n = vec3f(sin(phi), 0.0, -cos(phi));
      let tg = vec3f(cos(phi), 0.0, sin(phi));
      let c = vec3f(TOWER_C.x, 10.6, TOWER_C.y) + n * (TOWER_R + 0.3);
      let s = clamp(dot(p - c, tg), -0.52, 0.52);
      d = min(d, length(p - c - tg * s) - 0.024);
      // Two brackets back to the wall.
      for (var k = -1; k <= 1; k += 2) {
        let bc = c + tg * (0.42 * f32(k));
        let bs = clamp(dot(p - bc, n), -0.32, 0.0);
        d = min(d, length(p - bc - n * bs) - 0.018);
      }
    }
  } else {
    d = min(d, abs(p.y - 10.6) - 0.03);
  }
  return d;
}

fn tower_normal(p: vec3f) -> vec3f {
  let h = 0.002;
  let k = vec2f(1.0, -1.0);
  return normalize(k.xyy * tower_d(p + k.xyy * h) + k.yyx * tower_d(p + k.yyx * h)
                 + k.yxy * tower_d(p + k.yxy * h) + k.xxx * tower_d(p + k.xxx * h));
}

fn tower_bound(o: vec3f, d: vec3f) -> vec2f {
  let lo = vec3f(TOWER_C.x - TOWER_R - 0.6, 0.0, TOWER_C.y - TOWER_R - 0.6);
  let hi = vec3f(TOWER_C.x + TOWER_R + 0.6, 17.4, TOWER_C.y + TOWER_R + 0.6);
  return ray_box(o, vec3f(safe_inv(d.x), safe_inv(d.y), safe_inv(d.z)), lo, hi);
}

fn tower_trace(o: vec3f, d: vec3f, tmax: f32, eps_scale: f32, steps: u32) -> f32 {
  let iv = tower_bound(o, d);
  if (iv.x > iv.y) { return INF; }
  var t = max(iv.x, 0.0);
  let t1 = min(iv.y, tmax);
  for (var i = 0u; i < steps; i++) {
    let h = tower_d(o + d * t);
    if (h < max(eps_scale * t * F.eye.w, 1e-4)) { return t + h; }
    t += h * STEP_SCALE;
    if (t > t1) { return INF; }
  }
  return INF;
}

fn tower_shadow(o: vec3f, l: vec3f) -> f32 {
  let iv = tower_bound(o, l);
  if (iv.x > iv.y) { return 1.0; }
  var t = max(iv.x, 0.0);
  var res = 1.0;
  var ph = 1e10;
  for (var i = 0u; i < SHADOW_STEPS; i++) {
    let h = tower_d(o + l * t);
    if (h < 1e-4) { return 0.0; }
    let y = h * h / (2.0 * ph);
    let dd = sqrt(max(h * h - y * y, 0.0));
    res = min(res, SOFT * dd / max(t - y, 1e-3));
    if (res < 0.004) { return 0.0; }
    ph = h;
    t += max(h * STEP_SCALE, 0.004);
    if (t > iv.y) { return res; }
  }
  return res;
}

/** Stone courses, mortar, a door and slit windows, in the tower's cylindrical coordinates. */
fn tower_albedo(p: vec3f, n: vec3f, fp: f32) -> vec4f {
  let q = vec2f(p.x - TOWER_C.x, p.z - TOWER_C.y);
  let r = length(q);
  let ang = atan2(q.y, q.x);
  let u = (ang + PI) * TOWER_R;
  let course = 0.36;
  let row = floor(p.y / course);
  let len = 0.62 + 0.25 * hash2(vec2i(i32(row), 7));
  let off = hash2(vec2i(i32(row), 3)) * len;
  let bx = floor((u + off) / len);
  let fu = fract((u + off) / len) * len;
  let fy = fract(p.y / course) * course;
  let mortar_w = 0.018 + fp;
  let m = 1.0 - smoothstep(0.0, mortar_w, min(min(fu, len - fu), min(fy, course - fy)));
  let tint = 0.82 + 0.3 * hash2(vec2i(i32(bx), i32(row)));
  var stone = vec3f(0.42, 0.39, 0.34) * tint * (0.85 + 0.3 * noise3(p * 3.1));
  stone = mix(stone, stone * vec3f(0.75, 0.8, 0.7), smoothstep(2.5, 0.0, p.y) * 0.6);
  var col = mix(stone, vec3f(0.17, 0.16, 0.14), m * step(r, TOWER_R + 0.02) * step(0.3, abs(n.y) * -1.0 + 1.0));
  // Door: facing the courtyard (−z), an arch of dark planks.
  let s = r * atan2(q.x, -q.y);
  if (r < TOWER_R + 0.05 && abs(s) < 0.8 && p.y < 2.1 + 0.8 * sqrt(max(1.0 - (s / 0.8) * (s / 0.8), 0.0))) {
    let plank = fract(s * 5.0);
    col = vec3f(0.09, 0.055, 0.03) * (0.7 + 0.5 * smoothstep(0.0, 0.1, plank) * smoothstep(1.0, 0.9, plank));
  }
  // Slit windows.
  for (var w = 0; w < 3; w++) {
    let wa = radians(-38.0 + 38.0 * f32(w));
    let ws = r * (atan2(q.x, -q.y) - wa);
    for (var k = 0; k < 2; k++) {
      let wy = 5.2 + 3.4 * f32(k) + f32(w) * 0.4;
      if (r < TOWER_R + 0.05 && abs(ws) < 0.09 && abs(p.y - wy) < 0.55) { col = vec3f(0.02, 0.018, 0.016); }
    }
  }
  if (r < TOWER_R + 0.4 && p.y > 10.3 && p.y < 10.9 && r > TOWER_R + 0.1) { col = vec3f(0.05, 0.04, 0.035); }  // iron rods
  if (p.y > 13.0 && r < TOWER_R - 0.05) { col = vec3f(0.3, 0.27, 0.22); }
  if (r < TOWER_R - 0.05 && p.y > 13.05) { col = vec3f(0.09, 0.07, 0.05); }  // poles
  return vec4f(col, 0.85);
}

// ---- the ground: flagstones in the square, grass beyond ------------------------------------------------

fn ground_albedo(p: vec3f, fp: f32) -> vec4f {
  let x = p.x;
  let z = p.z;
  let sq = length((p.xz - vec2f(1.4, 6.0)) / vec2f(1.0, 1.15));
  let rowh = 0.55;
  let row = floor(z / rowh);
  let len = 0.55 + 0.35 * hash2(vec2i(i32(row), 11));
  let off = hash2(vec2i(i32(row), 5)) * len;
  let bx = floor((x + off) / len);
  let fu = fract((x + off) / len) * len;
  let fz = fract(z / rowh) * rowh;
  let gap = min(min(fu, len - fu), min(fz, rowh - fz));
  let jag = 0.012 * noise2(p.xz * 9.0);
  let m = 1.0 - smoothstep(0.0, 0.016 + fp + jag, gap - 0.004);
  let tint = 0.78 + 0.4 * hash2(vec2i(i32(bx), i32(row)));
  var stone = vec3f(0.25, 0.23, 0.20) * tint * (0.72 + 0.55 * fbm2(p.xz * 2.3)) * (0.85 + 0.3 * fbm2(p.xz * 0.31 + 4.0));
  let dirt = vec3f(0.12, 0.10, 0.07);
  var col = mix(stone, dirt, m);
  let grass = mix(vec3f(0.07, 0.10, 0.03), vec3f(0.16, 0.16, 0.06), fbm2(p.xz * 0.7)) * (0.8 + 0.4 * noise2(p.xz * 6.0));
  let edge = smoothstep(11.0, 12.5, sq + 1.2 * fbm2(p.xz * 0.4));
  col = mix(col, grass, edge);
  // Darker round the tower foot.
  let rt = length(p.xz - TOWER_C) - TOWER_R;
  col *= 0.65 + 0.35 * smoothstep(0.0, 1.6, rt);
  return vec4f(col, mix(0.75, 0.95, edge));
}

// ---- cloth: patches of the simulated sheet --------------------------------------------------------------

fn patch_info(pi: u32) -> vec4u { return bitcast<vec4u>(prims[pr_pinfo(pi)]); }

/** Local vertex triple of triangle k of a P×P patch (two per quad: a c b, b c e). */
fn tri_local(k: u32) -> vec3u {
  let q = k >> 1u;
  let i = q % P;
  let j = q / P;
  let a = j * (P + 1u) + i;
  let b = a + 1u;
  let c = a + P + 1u;
  let e = c + 1u;
  return select(vec3u(b, c, e), vec3u(a, c, b), (k & 1u) == 0u);
}

struct ClothHit { t: f32, pid: u32, tri: u32, bary: vec3f }

/** Normal (outward side) and uv/sheet at a point of patch pi, from barycentrics on triangle k. */
fn cloth_attr(pi: u32, k: u32, bary: vec3f) -> array<vec4f, 2> {
  let inf = patch_info(pi);
  let tl = tri_local(k);
  let ia = patch_vertex(inf, tl.x % (P + 1u), tl.x / (P + 1u));
  let ib = patch_vertex(inf, tl.y % (P + 1u), tl.y / (P + 1u));
  let ic = patch_vertex(inf, tl.z % (P + 1u), tl.z / (P + 1u));
  let n = attr[2u * ia].xyz * bary.x + attr[2u * ib].xyz * bary.y + attr[2u * ic].xyz * bary.z;
  let uv = attr[2u * ia + 1u] * bary.x + attr[2u * ib + 1u] * bary.y + attr[2u * ic + 1u] * bary.z;
  return array<vec4f, 2>(vec4f(normalize(n), 0.0), vec4f(uv.xy, f32(patch_sheet(inf)), 0.0));
}

/** Cloth albedo, roughness and a sheen weight, by sheet pattern and side. */
fn cloth_material(sheet: u32, uv: vec2f, front: bool, p: vec3f, fp: f32) -> vec4f {
  let m0 = M.m[sheet * 4u];
  let m1 = M.m[sheet * 4u + 1u];
  let m2 = M.m[sheet * 4u + 2u];
  let pattern = u32(m0.w);
  var col = select(m1.rgb, m0.rgb, front);
  let acc = m2.rgb;
  let aa = max(fp * 1.5, 0.002);    // a pixel's footprint, roughly in uv (sheets are ~1 m)
  if (pattern == 1u) {                       // cape: gold trim on the outside, ochre lining
    let edge = min(min(uv.x, 1.0 - uv.x), 1.0 - uv.y);
    if (front) { col = mix(col, acc, 1.0 - smoothstep(0.022, 0.022 + aa, edge)); }
  } else if (pattern == 2u || pattern == 3u) { // robes: a hem band; tunics also a centre stripe
    let hem = smoothstep(0.86 - aa, 0.86, uv.y) * (1.0 - smoothstep(0.93, 0.93 + aa, uv.y));
    col = mix(col, acc * 0.7, hem);
    if (pattern == 3u) {
      let band2 = smoothstep(0.78 - aa, 0.78, uv.y) * (1.0 - smoothstep(0.81, 0.81 + aa, uv.y));
      let yoke = 1.0 - smoothstep(0.1, 0.1 + aa, uv.y);
      col = mix(col, acc * 0.6, max(band2, yoke * 0.8));
    }
  } else if (pattern == 4u || pattern == 5u) { // banners: a border and a roundel, or chevrons
    let b = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y) * 0.33);
    var mark = 1.0 - smoothstep(0.06, 0.06 + aa, b);
    if (pattern == 4u) {
      let c = length((uv - vec2f(0.5, 0.4)) * vec2f(1.0, 3.0));
      mark = max(mark, 1.0 - smoothstep(0.28, 0.28 + aa, abs(c - 0.22) + 0.2));
      mark = max(mark, 1.0 - smoothstep(0.1, 0.1 + aa, c));
    } else {
      let ch = fract(uv.y * 4.0 - abs(uv.x - 0.5) * 1.6);
      mark = max(mark, step(0.62, ch) * step(0.12, uv.y) * step(uv.y, 0.88));
    }
    // Swallow-tail at the bottom is cut by the tether; leave the shape rectangular.
    col = mix(col, acc, mark);
  } else if (pattern == 6u) {                // flags: split diagonally
    col = mix(col, acc, step(uv.x * 0.6, uv.y));
  }
  // Weave: fine noise, filtered by the footprint.
  let w = 1.0 - smoothstep(0.0015, 0.004, fp);
  col *= 1.0 + 0.12 * w * (noise3(p * 900.0) - 0.0) + 0.08 * (noise3(p * 37.0));
  return vec4f(col, m1.w);
}

// ---- hair density grid ------------------------------------------------------------------------------------

fn hair_grid() -> vec4f { return prims[PR_HAIR]; }          // xyz min, w voxel size

fn hair_trans(o: vec3f, l: vec3f, fine: bool) -> f32 {
  let g = hair_grid();
  let v = g.w;
  let hi = g.xyz + vec3f(hd()) * v;
  let iv = ray_box(o, vec3f(safe_inv(l.x), safe_inv(l.y), safe_inv(l.z)), g.xyz, hi);
  if (iv.x > iv.y || v <= 0.0) { return 1.0; }
  var t = max(iv.x, 0.0);
  var tau = 0.0;
  let dt = select(1.5, 0.25, fine) * v;
  for (var i = 0u; i < select(320u, 1600u, fine); i++) {
    if (t > iv.y) { break; }
    let x = (o + l * t - g.xyz) / v;
    if (!fine) {
      let sk = textureLoad(hskip, vec3i(x / f32(HCOARSE)), 0).x;
      if (sk > 0.0) { t += sk; continue; }
    }
    cnt.sh_hair += 1u;
    tau += textureSampleLevel(hvol, lsamp, x / vec3f(hd()), 0.0).x * dt;
    if (tau > 7.0) { return 0.0; }
    t += dt;
  }
  return exp(-tau);
}

/** The hair light grid's occluders, evaluated exactly (hair.wgsl hair_occlusion, for the reference). */
fn hair_occlusion(p: vec3f, l: vec3f) -> f32 {
  for (var b = 0u; b < 4u; b++) {
    let a = chars[C_PARTS + 2u * b];
    let e = chars[C_PARTS + 2u * b + 1u];
    let iv = ray_capsule(p + l * 0.004, l, a.xyz, e.xyz, max(a.w, e.w));
    // Only if the ray enters the body ahead of the sample: a grid cell centred inside the scalp
    // isn't shaded by the head it straddles.
    if (iv.x > 0.0 && iv.x <= iv.y) { return 0.06; }
  }
  return tower_occlusion(p, l);
}

/**
 * Sun reaching a hair sample: the light grid (self-shadowing and occluders). REF_LIGHT marches the
 * density exactly per sample instead; it measures the light grid's own error, kept apart from the
 * visibility techniques' errors (README).
 */
fn hair_light_at(q: vec3f) -> f32 {
  if (REF_LIGHT) { return hair_trans(q, F.sun.xyz, true) * hair_occlusion(q, F.sun.xyz); }
  let g = hair_grid();
  // Biased one light cell toward the sun (deep-shadow-map practice): transmittance falls from 1 to
  // ~0 within a cell in dense hair, and an unbiased lookup bands along the cell layers.
  let q2 = q + F.sun.xyz * g.w * f32(HLIGHT_DIV);
  return textureSampleLevel(hlight, lsamp, (q2 - g.xyz) / g.w / vec3f(hd()), 0.0).x;
}

/** Longitudinal Gaussian. */
fn lobe(x: f32, b: f32) -> f32 { return exp(-x * x / (2.0 * b * b)) / (b * 2.5066); }

/** Hair shading: Kajiya–Kay diffuse and a Marschner-style R / TT / TRT approximation along tangent tg. */
fn hair_shade(tg: vec3f, v: vec3f, base: vec3f, lit: f32, amb: f32) -> vec3f {
  let l = F.sun.xyz;
  let sl = clamp(dot(tg, l), -1.0, 1.0);
  let sv = clamp(dot(tg, v), -1.0, 1.0);
  let th = 0.5 * (asin(sl) + asin(sv));
  let lp = l - tg * sl;
  let vp = v - tg * sv;
  let cphi = dot(lp, vp) * inverseSqrt(max(dot(lp, lp) * dot(vp, vp), 1e-8));
  let nR = 0.25 * sqrt(max(0.5 + 0.5 * cphi, 0.0));
  let nTT = exp(-3.5 * (cphi + 1.0) * (cphi + 1.0)) * 1.6;
  let r = lobe(th + 0.07, 0.13) * nR;
  let tt = lobe(th - 0.035, 0.07) * nTT * base * 2.5;
  let trt = lobe(th - 0.11, 0.2) * base * 0.55;
  let diff = base * (0.25 + 0.75 * sqrt(max(1.0 - sl * sl, 0.0))) * 0.55;
  let sky_amb = vec3f(0.30, 0.36, 0.46);
  return SUN_COL * lit * (diff + (vec3f(r) + tt + trt) * 0.32) + base * sky_amb * amb;
}

// ---- shadows -------------------------------------------------------------------------------------------------

fn light_cell(o: vec3f) -> i32 {
  let rel = o - F.lc.xyz;
  let e = F.lc.w;
  let g = f32(F.grid.x);
  let l = (vec2f(dot(rel, F.lr.xyz), dot(rel, F.lu.xyz)) + e) / (2.0 * e) * g;
  if (any(l < vec2f(0.0)) || any(l >= vec2f(g))) { return -1; }
  return i32(u32(l.y) * F.grid.x + u32(l.x));
}

fn lrange(cell: u32, cat: u32) -> vec2u {
  let i = (F.bins.x + cell) * NCAT + cat;
  return vec2u(boff[i], boff[i + 1u]);
}

fn char_shadow(c: u32, o: vec3f, l: vec3f, res_in: f32) -> f32 {
  let iv = char_mask(c, o, l, INF);
  if (iv.m == 0u) { return res_in; }
  var res = res_in;
  var t = max(iv.t0, 0.0);
  var ph = 1e10;
  for (var i = 0u; i < SHADOW_STEPS; i++) {
    cnt.sh_steps += 1u;
    let h = char_d(c, iv.m, o + l * t);
    if (h < 1e-4) { return 0.0; }
    let y = h * h / (2.0 * ph);
    let dd = sqrt(max(h * h - y * y, 0.0));
    res = min(res, SOFT * dd / max(t - y, 1e-3));
    if (res < 0.004) { return 0.0; }
    ph = h;
    t += max(h * STEP_SCALE, 0.002);
    if (t > iv.t1) { return res; }
  }
  cnt.sh_caps += 1u;
  return res;
}

/** Patch bound interval: its box intersected with its slab. */
fn patch_interval(pi: u32, o: vec3f, d: vec3f, inv: vec3f) -> vec2f {
  let a = prims[PR_PATCH + PATCH_STRIDE * pi];
  let b = prims[PR_PATCH + PATCH_STRIDE * pi + 1u];
  let n = prims[PR_PATCH + PATCH_STRIDE * pi + 2u];
  let box = ray_box(o, inv, a.xyz, b.xyz);
  if (box.x > box.y) { return EMPTY; }
  let slab = ray_slab(o, d, n.xyz, a.w, b.w);
  let r = vec2f(max(box.x, slab.x), min(box.y, slab.y));
  return select(EMPTY, r, r.x <= r.y && r.y > 0.0);
}

/** Distance from p to patch pi's thickened surface: exact (min over its triangles) minus CLOTH_H. */
fn patch_d(inf: vec4u, p: vec3f) -> f32 {
  var dmin = BIG;
  for (var k = 0u; k < 2u * P * P; k++) {
    let tl = tri_local(k);
    let a = pos[patch_vertex(inf, tl.x % (P + 1u), tl.x / (P + 1u))].xyz;
    let b = pos[patch_vertex(inf, tl.y % (P + 1u), tl.y / (P + 1u))].xyz;
    let c = pos[patch_vertex(inf, tl.z % (P + 1u), tl.z / (P + 1u))].xyz;
    dmin = min(dmin, ud_tri(p, a, b, c));
  }
  return dmin - CLOTH_H;
}

fn patch_shadow(pi: u32, o: vec3f, l: vec3f, inv: vec3f, res_in: f32) -> f32 {
  let iv = patch_interval(pi, o, l, inv);
  if (iv.x > iv.y) { return res_in; }
  let inf = patch_info(pi);
  if (CLOTH == 2u) {
    for (var k = 0u; k < 2u * P * P; k++) {
      let tl = tri_local(k);
      let a = pos[patch_vertex(inf, tl.x % (P + 1u), tl.x / (P + 1u))].xyz;
      let b = pos[patch_vertex(inf, tl.y % (P + 1u), tl.y / (P + 1u))].xyz;
      let c = pos[patch_vertex(inf, tl.z % (P + 1u), tl.z / (P + 1u))].xyz;
      if (ray_tri(o, l, a, b, c).x < INF) { return 0.0; }
    }
    return res_in;
  }
  // Hard shadows by default: march the patch's exact field to a hit, as the triangles are tested.
  // FIELD_SOFT uses the field's classic soft penumbra estimate (min SOFT·h/t) instead. Measured: with
  // single-quad patches hard is cheaper (the slab clips each march to a sliver); with 2×2 patches a
  // ray leaving a robe at a grazing angle creeps along the sheet and hard costs more (README).
  if (!FIELD_SOFT) {
    var th = max(iv.x, 0.0);
    for (var i = 0u; i < SHADOW_STEPS; i++) {
      cnt.sh_steps += 1u;
      let h = patch_d(inf, o + l * th);
      if (h < 2e-4 + 0.25 * th * F.eye.w * EPS_SCALE) { return 0.0; }
      th += h * STEP_SCALE;
      if (th > iv.y) { return res_in; }
    }
    cnt.sh_caps += 1u;
    return res_in;
  }
  var res = res_in;
  var t = max(iv.x, 0.0);
  var ph = 1e10;
  for (var i = 0u; i < SHADOW_STEPS; i++) {
    cnt.sh_steps += 1u;
    let h = patch_d(inf, o + l * t);
    if (h < 1e-4) { return 0.0; }
    let y = h * h / (2.0 * ph);
    let dd = sqrt(max(h * h - y * y, 0.0));
    res = min(res, SOFT * dd / max(t - y, 1e-3));
    if (res < 0.004) { return 0.0; }
    ph = h;
    t += max(h * STEP_SCALE, 0.001);
    if (t > iv.y) { return res; }
  }
  cnt.sh_caps += 1u;
  return res;
}

fn sheet_box(s: u32) -> array<vec3f, 2> {
  var r: array<vec3f, 2>;
  for (var k = 0u; k < 6u; k++) {
    let key = sheetb[s * 6u + k];
    let bits = select(~key, key & 0x7fffffffu, (key & 0x80000000u) != 0u);
    r[k / 3u][k % 3u] = bitcast<f32>(bits);
  }
  return r;
}

fn smap_vis(p: vec3f) -> f32 {
  let c = F.lightvp * vec4f(p, 1.0);
  let uv = vec2f(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
  if (any(uv < vec2f(0.0)) || any(uv > vec2f(1.0)) || c.z > 1.0) { return 1.0; }
  let z = c.z - 0.00025;           // ~2 cm over the 80 m light depth range; the origin is already normal-offset
  let px = 1.0 / 2048.0;
  var s = 0.0;
  for (var j = -1; j <= 1; j += 2) {
    for (var i = -1; i <= 1; i += 2) {
      s += textureSampleCompareLevel(smap, csamp, uv + vec2f(f32(i), f32(j)) * 0.75 * px, z);
    }
  }
  return s * 0.25;
}

/** Sun visibility at p (normal n): the tower, bodies and cloth (fields, or the shadow map), and hair. */
fn sun_vis(p: vec3f, n: vec3f, eps: f32, hair: bool) -> f32 {
  let l = F.sun.xyz;
  let o = p + n * (0.006 + 2.0 * eps);
  cnt.sh_rays += 1u;
  var v = tower_shadow(o, l);
  if (v <= 0.0) { return 0.0; }
  let inv = vec3f(safe_inv(l.x), safe_inv(l.y), safe_inv(l.z));
  if (REF) {
    for (var c = 0u; c < F.grid.y; c++) {
      v = char_shadow(c, o, l, v);
      if (v <= 0.0) { return 0.0; }
    }
    if (CLOTH > 0u && !SMAP && CLOTHSH) {
      for (var s = 0u; s < F.misc.z; s++) {
        let bx = sheet_box(s);
        let iv = ray_box(o, inv, bx[0] - CLOTH_H, bx[1] + CLOTH_H);
        if (iv.x > iv.y) { continue; }
        let p0 = u32(M.m[s * 4u + 3u].x);
        let pn = u32(M.m[s * 4u + 3u].y);
        for (var pi = p0; pi < p0 + pn; pi++) {
          v = patch_shadow(pi, o, l, inv, v);
          if (v <= 0.0) { return 0.0; }
        }
      }
    }
  } else {
    let cell = light_cell(o);
    if (cell >= 0) {
      let rc = lrange(u32(cell), CAT_CHAR);
      for (var e = rc.x; e < min(rc.y, rc.x + 1024u); e++) {
        v = char_shadow(blist[e], o, l, v);
        if (v <= 0.0) { return 0.0; }
      }
      if (CLOTH > 0u && !SMAP && CLOTHSH) {
        let rp = lrange(u32(cell), CAT_PATCH);
        for (var e = rp.x; e < min(rp.y, rp.x + 1024u); e++) {
          v = patch_shadow(blist[e], o, l, inv, v);
          if (v <= 0.0) { return 0.0; }
        }
      }
    }
  }
  if (SMAP) { v *= smap_vis(o); }
  if (hair && HAIRSH) { v *= hair_trans(o, l, REF); }
  return v;
}

// ---- surface shading --------------------------------------------------------------------------------------

fn shade(albedo: vec3f, rough: f32, n: vec3f, v: vec3f, vis: f32, ao: f32, sheen: f32) -> vec3f {
  let l = F.sun.xyz;
  let ndl = max(dot(n, l), 0.0);
  let h = normalize(l + v);
  let a = max(rough * rough, 0.03);
  let ndh = max(dot(n, h), 0.0);
  let dd = ndh * ndh * (a * a - 1.0) + 1.0;
  let D = a * a / (PI * dd * dd);
  let fres = 0.04 + 0.96 * pow(1.0 - max(dot(h, v), 0.0), 5.0);
  let spec = D * fres * 0.25 / max(dot(n, v) * 0.5 + 0.5, 0.2);
  let skyc = mix(vec3f(0.16, 0.13, 0.10), vec3f(0.27, 0.34, 0.48), 0.5 + 0.5 * n.y);
  let rim = sheen * pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.6;
  return albedo * (skyc * ao + SUN_COL * ndl * vis * (1.0 + rim)) + SUN_COL * spec * ndl * vis;
}

/** Shades a cloth point: the side facing the viewer, its pattern, and light through thin banners. */
fn shade_cloth(p: vec3f, d: vec3f, nrm: vec3f, uv: vec2f, sheet: u32, fp: f32, vis_front: f32, vis_back: f32) -> vec3f {
  let front = dot(nrm, d) < 0.0;
  let n = select(-nrm, nrm, front);
  let mat = cloth_material(sheet, uv, front, p, fp);
  let kind = u32(M.m[sheet * 4u + 2u].w);
  var c = shade(mat.rgb, mat.w, n, -d, vis_front, 1.0, 0.6);
  // Thin cloth transmits some sunlight when lit from behind.
  let back = max(dot(-n, F.sun.xyz), 0.0);
  let thin = select(0.12, 0.35, kind == KIND_BANNER || kind == KIND_FLAG);
  c += mat.rgb * SUN_COL * back * vis_back * thin;
  return c;
}
