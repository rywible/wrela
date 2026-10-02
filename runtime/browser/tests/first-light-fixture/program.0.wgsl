// A hand-written stand-in for the compiler's WGSL for examples/first-light/main.wrela: the same
// two entry points and uniform, line for line the same arithmetic.

struct Scene {
    resolution: vec2<f32>,
    time: f32,
}

@group(0) @binding(0) var<uniform> scene: Scene;

// One triangle that covers the screen: vertices 0, 1 and 2 land at (-1, -1), (3, -1) and (-1, 3).
@vertex
fn cover(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(index % 2u) * 4.0 - 1.0;
    let y = f32(index / 2u) * 4.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn shade(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (position.xy * 2.0 - scene.resolution) / scene.resolution.y;
    let t = scene.time;
    let p = vec3<f32>(uv.x, -uv.y, 0.25 * sin(t * 0.7));
    let centre = vec3<f32>(0.35 * cos(t), 0.2 * sin(t * 1.3), 0.0);
    let radius = 0.45 + 0.05 * sin(t * 2.0);
    let d = length(p - centre) - radius;

    let base = mix(vec3<f32>(1.0, 0.55, 0.25), vec3<f32>(0.25, 0.45, 0.85), step(0.0, d));
    let bands = 0.5 + 0.5 * cos(d * 62.831853);
    let fade = clamp(1.0 - abs(d) * 1.5, 0.0, 1.0);
    let shaded = base * (0.75 + 0.25 * bands * fade);
    let rim = 1.0 - smoothstep(0.0, 0.015, abs(d));
    return vec4<f32>(mix(shaded, vec3<f32>(1.0), rim), 1.0);
}
