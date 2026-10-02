// Sky and aerial perspective (the sky slice, timed separately). Appended to common.wgsl.
// An analytic sky, height-attenuated Rayleigh and Mie extinction with sun-tinted in-scatter, a fade
// over the last 15% of the view distance, then tone mapping.

@group(1) @binding(0) var depthS: texture_2d<f32>;
@group(1) @binding(1) var hdrS: texture_2d<f32>;
@group(1) @binding(2) var outS: texture_storage_2d<rgba8unorm, write>;

const BETA_R = vec3f(5.8e-6, 13.5e-6, 33.1e-6);
const H_R: f32 = 8000.0;
const BETA_M: f32 = 2.6e-5;
const H_M: f32 = 1100.0;
const SUN_SKY = vec3f(1.0, 0.82, 0.62);

fn sky_color(d: vec3f) -> vec3f {
  let mu = max(dot(d, F.sun.xyz), 0.0);
  let y = clamp(d.y, 0.0, 1.0);
  let horizon = vec3f(0.60, 0.68, 0.78);
  let zenith = vec3f(0.13, 0.27, 0.55);
  var c = mix(horizon, zenith, pow(y, 0.5));
  c += SUN_SKY * (0.18 * pow(mu, 6.0) + 0.55 * pow(mu, 48.0)) * (1.0 - 0.5 * y);
  c += SUN_SKY * 40.0 * smoothstep(0.99975, 0.9999, mu);
  // below the horizon: haze
  c = mix(c, horizon * 0.9, smoothstep(0.0, -0.08, d.y));
  return c;
}

/** Optical depth of an exponential atmosphere along t metres from height y0, direction dy. */
fn depth_exp(y0: f32, dy: f32, t: f32, H: f32) -> f32 {
  let base = exp(-max(y0, -200.0) / H);
  if (abs(dy) < 1e-5) { return base * t; }
  return base * H * (1.0 - exp(-dy * t / H)) / dy;
}

fn aerial(c: vec3f, t: f32, d: vec3f) -> vec3f {
  let y0 = F.cam.y - F.shift.y;
  let tr = BETA_R * depth_exp(y0, d.y, t, H_R);
  let tm = BETA_M * depth_exp(y0, d.y, t, H_M);
  let T = exp(-(tr + 1.1 * tm));
  let mu = max(dot(d, F.sun.xyz), 0.0);
  let fog = vec3f(0.56, 0.64, 0.75) + SUN_SKY * (0.25 * pow(mu, 4.0) + 0.5 * pow(mu, 24.0));
  return c * T + fog * (1.0 - T);
}

fn tonemap(x: vec3f) -> vec3f {
  let c = x * 0.85;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}

@compute @workgroup_size(8, 8)
fn sky(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let px = vec2i(gid.xy);
  let t = textureLoad(depthS, px, 0).r;
  let d = ray_dir(vec2f(gid.xy) + 0.5);
  var c: vec3f;
  if (F.dbg.x != 0u) {
    c = textureLoad(hdrS, px, 0).rgb;
    if (t >= INF) { c = vec3f(0.05); }
    textureStore(outS, px, vec4f(pow(clamp(c, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2)), 1.0));
    return;
  }
  if (t >= INF) {
    c = sky_color(d);
  } else {
    c = aerial(textureLoad(hdrS, px, 0).rgb, t, d);
    let V = F.shift.w;
    c = mix(c, sky_color(d), smoothstep(0.85 * V, V, t));
  }
  c = tonemap(c);
  textureStore(outS, px, vec4f(pow(c, vec3f(1.0 / 2.2)), 1.0));
}
