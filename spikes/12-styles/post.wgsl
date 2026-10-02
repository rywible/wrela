// Screen-space passes: the baselines the field-native techniques are compared with, the flicker
// measure, and the blit. Appended to common.wgsl only (no scene).

@group(0) @binding(2) var gb: texture_2d<u32>;          // edges: G-buffer; flicker: frame A's
@group(0) @binding(3) var src: texture_2d<f32>;         // colour in; flicker: frame A's colour
@group(0) @binding(4) var dst: texture_storage_2d<rgba16float, write>;
@group(0) @binding(5) var stt: texture_2d<f32>;         // kuwahara: smoothed structure tensor
@group(0) @binding(6) var gb_b: texture_2d<u32>;        // flicker: frame B's G-buffer
@group(0) @binding(7) var col_b: texture_2d<f32>;       // flicker: frame B's colour
@group(0) @binding(8) var<storage, read_write> fl: array<atomic<u32>, 8>;

override EDGE_MODE: u32 = 0u;       // 0: Sobel-style at offset W/2 (constant cost); 1: disk of radius W
override EDGE_VIEW: u32 = 0u;       // 1: lines only (black over the input)
override DIR: u32 = 0u;             // blur direction

fn clamp_px(p: vec2i) -> vec2i { return clamp(p, vec2i(0), vec2i(F.dims.xy) - 1); }
fn depth_at(p: vec2i) -> f32 {
  let v = textureLoad(gb, clamp_px(p), 0);
  return min(bitcast<f32>(v.x), 1e5);
}
fn normal_at(p: vec2i) -> vec3f { return oct_dec(textureLoad(gb, clamp_px(p), 0).y); }
fn mat_at(p: vec2i) -> u32 { return textureLoad(gb, clamp_px(p), 0).z & 15u; }

fn edge_col(mat: u32) -> vec3f {
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

@compute @workgroup_size(8, 8)
fn edges(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = vec3u(gid0.xy + F.dims.zw, 0u);         // + the tile origin (0 for a whole frame)
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let c = vec2i(gid.xy);
  let col = textureLoad(src, c, 0).rgb;
  let W = F.prm.x;
  let tc = depth_at(c);
  var a = 0.0;
  var lm = mat_at(c);
  if (EDGE_MODE == 0u) {
    // Laplacian of inverse depth (zero on planes, so sloped ground stays clean) and normal
    // differences, sampled W/2 px apart: lines straddle the edge, about W wide.
    let s = max(1, i32(round(W * 0.5)));
    let zc = 1.0 / tc;
    var lap = -8.0 * zc;
    var nd = 0.0;
    let nc = normal_at(c);
    var nearest = tc;
    for (var j = -1; j <= 1; j++) {
      for (var i = -1; i <= 1; i++) {
        if (i == 0 && j == 0) { continue; }
        let q = c + vec2i(i, j) * s;
        let tq = depth_at(q);
        lap += 1.0 / tq;
        if (i == 0 || j == 0) { nd = max(nd, 1.0 - dot(nc, normal_at(q))); }
        if (tq < nearest) { nearest = tq; lm = mat_at(q); }
      }
    }
    a = smoothstep(0.25, 0.5, abs(lap) / zc);
    if (EDGE_VIEW == 0u) { a = max(a, smoothstep(0.35, 0.6, nd)); }   // line-only views compare silhouettes
  } else {
    // The exact-width analogue of the field-native lines: a pixel is a line if a much nearer pixel
    // lies within W px. Cost grows with W².
    // "Much nearer" is judged against the depth this pixel's own plane predicts along the neighbour's
    // ray (exact for planes), so ground receding at a grazing angle isn't mistaken for an edge.
    let R = i32(ceil(W));
    var best = 1e9;
    let pa = F.eye.w;
    let nc = normal_at(c);
    let pc = ray_dir(vec2u(c)) * tc;                 // relative to the eye
    let sky = tc >= 1e5;
    for (var j = -R; j <= R; j++) {
      for (var i = -R; i <= R; i++) {
        let r2 = f32(i * i + j * j);
        if (r2 > (W + 0.5) * (W + 0.5) || r2 >= best * best) { continue; }
        let q = clamp_px(c + vec2i(i, j));
        let tq = depth_at(q);
        var te = tc;
        if (!sky) {
          let dn = dot(nc, ray_dir(vec2u(q)));
          te = select(tc, dot(nc, pc) / dn, dn < -1e-3);
        }
        if (te - tq > max(0.02 * tq, (2.0 * W + 2.0) * pa * tq)) { best = sqrt(r2); lm = mat_at(q); }
      }
    }
    // the silhouette lies between this pixel and the nearer one, half a pixel closer on average
    a = clamp(W + 1.0 - best, 0.0, 1.0);
    // inner creases from 1 px normal differences
    var nd = 0.0;
    nd = max(nd, 1.0 - dot(nc, normal_at(c + vec2i(1, 0))));
    nd = max(nd, 1.0 - dot(nc, normal_at(c + vec2i(0, 1))));
    if (EDGE_VIEW == 0u) { a = max(a, smoothstep(0.35, 0.6, nd)); }
  }
  var out = mix(col, srgb(edge_col(lm)), a);
  if (EDGE_VIEW == 1u) { out = col * (1.0 - a); }
  textureStore(dst, c, vec4f(out, 1.0));
}

// ---- anisotropic Kuwahara (Kyprianidis et al. 2009) with polynomial sector weights (2010) ----------

@compute @workgroup_size(8, 8)
fn structure(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = vec3u(gid0.xy + F.dims.zw, 0u);         // + the tile origin (0 for a whole frame)
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let c = vec2i(gid.xy);
  let s = array<vec3f, 9>(
    textureLoad(src, clamp_px(c + vec2i(-1, -1)), 0).rgb, textureLoad(src, clamp_px(c + vec2i(0, -1)), 0).rgb, textureLoad(src, clamp_px(c + vec2i(1, -1)), 0).rgb,
    textureLoad(src, clamp_px(c + vec2i(-1, 0)), 0).rgb, vec3f(0.0), textureLoad(src, clamp_px(c + vec2i(1, 0)), 0).rgb,
    textureLoad(src, clamp_px(c + vec2i(-1, 1)), 0).rgb, textureLoad(src, clamp_px(c + vec2i(0, 1)), 0).rgb, textureLoad(src, clamp_px(c + vec2i(1, 1)), 0).rgb);
  let gx = (-s[0] - 2.0 * s[3] - s[6] + s[2] + 2.0 * s[5] + s[8]) * 0.25;
  let gy = (-s[0] - 2.0 * s[1] - s[2] + s[6] + 2.0 * s[7] + s[8]) * 0.25;
  textureStore(dst, c, vec4f(dot(gx, gx), dot(gy, gy), dot(gx, gy), 1.0));
}

@compute @workgroup_size(8, 8)
fn blur(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = vec3u(gid0.xy + F.dims.zw, 0u);         // + the tile origin (0 for a whole frame)
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let c = vec2i(gid.xy);
  let sigma = 2.0 * f32(F.dims.y) / 1080.0;
  let r = i32(ceil(3.0 * sigma));
  let step = select(vec2i(0, 1), vec2i(1, 0), DIR == 0u);
  var acc = vec4f(0.0);
  var wsum = 0.0;
  for (var i = -r; i <= r; i++) {
    let w = exp(-f32(i * i) / (2.0 * sigma * sigma));
    acc += w * textureLoad(src, clamp_px(c + step * i), 0);
    wsum += w;
  }
  textureStore(dst, c, acc / wsum);
}

// The filter reads up to (2 × 12 + 1)² texels per pixel, so each 16×16 workgroup first stages
// its tile plus a 12 px apron in workgroup memory (colour as three halves: 12.8 KB), once.
const KR: i32 = 12;                       // the apron: the ellipse's longest half-axis, 2 × radius at 1080p
const KT: i32 = 16 + 2 * KR;
var<workgroup> ktile: array<vec2u, 1600>; // KT × KT

@compute @workgroup_size(16, 16)
fn kuwahara(@builtin(global_invocation_id) gid0: vec3u, @builtin(local_invocation_index) li: u32,
            @builtin(workgroup_id) wg: vec3u) {
  let org = vec2i(wg.xy * 16u + F.dims.zw) - KR;
  for (var k = li; k < u32(KT * KT); k += 256u) {
    let col = textureLoad(src, clamp_px(org + vec2i(i32(k % u32(KT)), i32(k / u32(KT)))), 0).rgb;
    ktile[k] = vec2u(pack2x16float(col.rg), pack2x16float(vec2f(col.b, 0.0)));
  }
  workgroupBarrier();
  let gid = vec3u(gid0.xy + F.dims.zw, 0u);         // + the tile origin (0 for a whole frame)
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let c = vec2i(gid.xy);
  let st = textureLoad(stt, c, 0).xyz;
  let E = st.x;
  let G = st.y;
  let Fv = st.z;
  let disc = sqrt(max((E - G) * (E - G) + 4.0 * Fv * Fv, 0.0));
  let l1 = 0.5 * (E + G + disc);
  let l2 = 0.5 * (E + G - disc);
  var tv = vec2f(l1 - E, -Fv);
  tv = select(vec2f(0.0, 1.0), normalize(tv), length(tv) > 1e-8);
  let phi = atan2(tv.y, tv.x);
  let A = select(0.0, (l1 - l2) / (l1 + l2), l1 + l2 > 1e-8);

  let radius = F.prm.z;
  let alpha = 1.0;
  let a = radius * clamp((alpha + A) / alpha, 0.1, 2.0);
  let b = radius * clamp(alpha / (alpha + A), 0.1, 2.0);
  let cp = cos(phi);
  let sp = sin(phi);
  // v = (S R)(i, j): the major axis a runs along the edge tangent (cos φ, sin φ)
  let SR = mat2x2f(0.5 / a * cp, -0.5 / b * sp, 0.5 / a * sp, 0.5 / b * cp);
  let max_x = min(i32(sqrt(a * a * cp * cp + b * b * sp * sp)), KR);
  let max_y = min(i32(sqrt(a * a * sp * sp + b * b * cp * cp)), KR);
  let lc = c - org;
  let zeta = 1.0 / radius;
  let zc = 0.58;
  let eta = (zeta + cos(zc)) / (sin(zc) * sin(zc));
  // The eight sectors as two vec4 lanes (0, 2, 4, 6 and 1, 3, 5, 7): weighted sums of colour,
  // colour squared and weight per sector, all in registers.
  var ra = vec4f(0.0); var ga = vec4f(0.0); var ba = vec4f(0.0); var wa = vec4f(0.0);
  var r2a = vec4f(0.0); var g2a = vec4f(0.0); var b2a = vec4f(0.0);
  var rb = vec4f(0.0); var gb2 = vec4f(0.0); var bb = vec4f(0.0); var wb = vec4f(0.0);
  var r2b = vec4f(0.0); var g2b = vec4f(0.0); var b2b = vec4f(0.0);
  // |SR (i, j)|² = al i² + 2 be i j + gm j² ≤ 1/4: each row's span of i inside the ellipse, solved
  // directly, so the loop visits only the ~π a b texels it uses
  let sx2 = 0.25 / (a * a);
  let sy2 = 0.25 / (b * b);
  let al = sx2 * cp * cp + sy2 * sp * sp;
  let be = (sx2 - sy2) * cp * sp;
  let gm = sx2 * sp * sp + sy2 * cp * cp;
  for (var j = -max_y; j <= max_y; j++) {
    let fj = f32(j);
    let disc = be * be * fj * fj - al * (gm * fj * fj - 0.25);
    if (disc < 0.0) { continue; }
    let sq = sqrt(disc);
    let i0 = max(i32(ceil((-be * fj - sq) / al)), -KR);
    let i1 = min(i32(floor((-be * fj + sq) / al)), KR);
    for (var i = i0; i <= i1; i++) {
      let v = SR * vec2f(f32(i), fj);
      let vv = min(dot(v, v), 0.25);
      let e = ktile[(lc.y + j) * KT + lc.x + i];
      let col = vec3f(unpack2x16float(e.x), unpack2x16float(e.y).x);
      let v2 = 0.70710678 * vec2f(v.x - v.y, v.x + v.y);
      let xa = zeta - eta * v.x * v.x;
      let ya = zeta - eta * v.y * v.y;
      let xb = zeta - eta * v2.x * v2.x;
      let yb = zeta - eta * v2.y * v2.y;
      let za = max(vec4f(v.y + xa, -v.x + ya, -v.y + xa, v.x + ya), vec4f(0.0));
      let zb = max(vec4f(v2.y + xb, -v2.x + yb, -v2.y + xb, v2.x + yb), vec4f(0.0));
      var qa = za * za;
      var qb = zb * zb;
      let g = exp(-3.125 * vv) / max(dot(qa, vec4f(1.0)) + dot(qb, vec4f(1.0)), 1e-6);
      qa *= g;
      qb *= g;
      ra += qa * col.r; ga += qa * col.g; ba += qa * col.b; wa += qa;
      r2a += qa * col.r * col.r; g2a += qa * col.g * col.g; b2a += qa * col.b * col.b;
      rb += qb * col.r; gb2 += qb * col.g; bb += qb * col.b; wb += qb;
      r2b += qb * col.r * col.r; g2b += qb * col.g * col.g; b2b += qb * col.b * col.b;
    }
  }
  // per sector: mean, variance, and a weight that favours the flattest sector
  let ia = 1.0 / max(wa, vec4f(1e-12));
  let ib = 1.0 / max(wb, vec4f(1e-12));
  let mra = ra * ia; let mga = ga * ia; let mba = ba * ia;
  let mrb = rb * ib; let mgb = gb2 * ib; let mbb = bb * ib;
  let sa = abs(r2a * ia - mra * mra) + abs(g2a * ia - mga * mga) + abs(b2a * ia - mba * mba);
  let sb = abs(r2b * ib - mrb * mrb) + abs(g2b * ib - mgb * mgb) + abs(b2b * ib - mbb * mbb);
  let ka = select(vec4f(0.0), 1.0 / (1.0 + pow(8000.0 * sa, vec4f(4.0))) + 1e-30, wa > vec4f(0.0));
  let kb = select(vec4f(0.0), 1.0 / (1.0 + pow(8000.0 * sb, vec4f(4.0))) + 1e-30, wb > vec4f(0.0));
  let ow = dot(ka, vec4f(1.0)) + dot(kb, vec4f(1.0));
  let o = vec4f(dot(ka, mra) + dot(kb, mrb), dot(ka, mga) + dot(kb, mgb), dot(ka, mba) + dot(kb, mbb), ow);
  // every sector's weight is positive, so o.w > 0; guard only against an empty kernel
  textureStore(dst, c, vec4f(select(textureLoad(src, c, 0).rgb, o.rgb / o.w, o.w > 0.0), 1.0));
}

// ---- flicker: frame B warped into frame A ---------------------------------------------------------------

fn to_display(c: vec3f) -> vec3f { return pow(clamp(c, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2)); }

@compute @workgroup_size(8, 8)
fn flicker(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = vec3u(gid0.xy + F.dims.zw, 0u);         // + the tile origin (0 for a whole frame)
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let gbv = textureLoad(gb_b, vec2i(gid.xy), 0);
  let tb = bitcast<f32>(gbv.x);
  if ((gbv.z & 15u) == 0u || tb > 1e5) { return; }               // sky
  let P = F.eye.xyz + ray_dir(gid.xy) * tb;
  let rel = P - F.aeye.xyz;
  let z = dot(rel, F.acf.xyz);
  if (z <= 0.0) { return; }
  let u = dot(rel, F.acr.xyz) / (z * dot(F.acr.xyz, F.acr.xyz));
  let v = dot(rel, F.acu.xyz) / (z * dot(F.acu.xyz, F.acu.xyz));
  let xy = vec2f((u + 1.0) * 0.5 * f32(F.dims.x) - 0.5, (1.0 - v) * 0.5 * f32(F.dims.y) - 0.5);
  if (any(xy < vec2f(0.0)) || any(xy > vec2f(F.dims.xy) - 1.001)) { return; }
  // A must see the same surface there
  let ta = bitcast<f32>(textureLoad(gb, vec2i(round(xy)), 0).x);
  let expect = length(rel);
  if (abs(ta - expect) > 0.02 * expect + 2.0 * F.eye.w * expect) { return; }
  let i0 = vec2i(floor(xy));
  let f = xy - floor(xy);
  let ca = mix(mix(to_display(textureLoad(src, i0, 0).rgb), to_display(textureLoad(src, i0 + vec2i(1, 0), 0).rgb), f.x),
               mix(to_display(textureLoad(src, i0 + vec2i(0, 1), 0).rgb), to_display(textureLoad(src, i0 + vec2i(1, 1), 0).rgb), f.x), f.y);
  let cb = to_display(textureLoad(col_b, vec2i(gid.xy), 0).rgb);
  let dm = max(abs(ca.r - cb.r), max(abs(ca.g - cb.g), abs(ca.b - cb.b))) * 255.0;
  atomicAdd(&fl[0], 1u);
  atomicAdd(&fl[1], u32(dm * 16.0));
  if (dm > 8.0) { atomicAdd(&fl[2], 1u); }
}

// ---- blit -------------------------------------------------------------------------------------------------

@group(0) @binding(9) var blit_tex: texture_2d<f32>;
@group(0) @binding(10) var blit_samp: sampler;

struct BOut { @builtin(position) clip: vec4f, @location(0) uv: vec2f }

@vertex
fn blit_vs(@builtin(vertex_index) vi: u32) -> BOut {
  let p = vec2f(f32((vi << 1u) & 2u), f32(vi & 2u));
  var o: BOut;
  o.clip = vec4f(p * 2.0 - 1.0, 0.0, 1.0);
  o.uv = vec2f(p.x, 1.0 - p.y);
  return o;
}

@fragment
fn blit_fs(i: BOut) -> @location(0) vec4f {
  return vec4f(pow(clamp(textureSample(blit_tex, blit_samp, i.uv).rgb, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2)), 1.0);
}
