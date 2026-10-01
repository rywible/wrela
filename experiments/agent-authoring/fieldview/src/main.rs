//! fieldview: render a WGSL distance field to a four-view contact sheet, and report what an
//! author can't easily see in a picture (floating pieces, ground penetration, framing, gradient).
//!
//! Usage: fieldview <creature.wgsl> <out.png> [--size 512] [--center x,y,z] [--radius r]
//!
//! The field file defines `fn field(p: vec3f) -> f32` (metres, +y up, ground at y = 0, facing +z)
//! and optionally `fn albedo(p: vec3f) -> vec3f`. The helpers in lib.wgsl are available.

use std::{collections::VecDeque, env, fs, process::exit, time::Instant};

const LIB: &str = include_str!("lib.wgsl");
const RENDER: &str = include_str!("render.wgsl");
const DEFAULT_ALBEDO: &str = "fn albedo(p: vec3f) -> vec3f { return vec3f(0.6, 0.5, 0.42); }\n";
const GRID: u32 = 96;

struct Args {
    input: String,
    output: String,
    size: u32,
    center: [f32; 3],
    radius: f32,
}

fn usage() -> ! {
    eprintln!("usage: fieldview <field.wgsl> <out.png> [--size 512] [--center x,y,z] [--radius r]");
    exit(64)
}

fn parse_args() -> Args {
    let mut it = env::args().skip(1);
    let mut a = Args { input: String::new(), output: String::new(), size: 512, center: [0.0, 1.2, 0.6], radius: 2.0 };
    let mut pos = vec![];
    while let Some(s) = it.next() {
        match s.as_str() {
            "--size" => a.size = it.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()),
            "--radius" => a.radius = it.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()),
            "--center" => {
                let v: Vec<f32> = it.next().unwrap_or_else(|| usage()).split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if v.len() != 3 { usage() }
                a.center = [v[0], v[1], v[2]];
            }
            _ => pos.push(s),
        }
    }
    if pos.len() != 2 { usage() }
    a.input = pos.remove(0);
    a.output = pos.remove(0);
    a
}

fn main() {
    let a = parse_args();
    let user = fs::read_to_string(&a.input).unwrap_or_else(|e| { eprintln!("can't read {}: {e}", a.input); exit(66) });
    // The user's code comes first so compiler line numbers match their file.
    let mut src = user.clone();
    src.push('\n');
    if !user.contains("fn albedo") {
        src.push_str(DEFAULT_ALBEDO);
    }
    src.push_str(LIB);
    src.push_str(RENDER);

    let module = match naga::front::wgsl::parse_str(&src) {
        Ok(m) => m,
        Err(e) => { eprint!("{}", e.emit_to_string_with_path(&src, &a.input)); exit(2) }
    };
    if let Err(e) = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&module) {
        eprint!("{}", e.emit_to_string_with_path(&src, &a.input));
        exit(2)
    }

    let (device, queue) = pollster::block_on(gpu());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("field"), source: wgpu::ShaderSource::Wgsl(src.into()) });

    let t0 = Instant::now();
    let pixels = render(&device, &queue, &shader, &a);
    let render_ms = t0.elapsed().as_secs_f64() * 1e3;
    write_png(&a.output, a.size * 2, a.size * 2, &pixels);

    let samples = sample_grid(&device, &queue, &shader, &a);
    println!("fieldview: rendered {} in {:.0} ms", a.output, render_ms);
    println!("tiles: top-left = side (from +x), top-right = front (from +z), bottom-left = three-quarter from above, bottom-right = top-down");
    report(&samples, &a);
}

async fn gpu() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
        .await
        .unwrap_or_else(|e| { eprintln!("no GPU adapter: {e}"); exit(70) });
    adapter
        .request_device(&wgpu::DeviceDescriptor { label: None, required_limits: adapter.limits(), ..Default::default() })
        .await
        .unwrap_or_else(|e| { eprintln!("no GPU device: {e}"); exit(70) })
}

fn camera_bytes(eye: [f32; 3], target: [f32; 3], fov: f32, size: u32, x0: u32, y0: u32) -> Vec<u8> {
    let v = [eye[0], eye[1], eye[2], 0.0, target[0], target[1], target[2], 0.0, fov, size as f32, x0 as f32, y0 as f32];
    bytemuck::cast_slice(&v).to_vec()
}

fn render(device: &wgpu::Device, queue: &wgpu::Queue, shader: &wgpu::ShaderModule, a: &Args) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fieldview"),
        layout: None,
        vertex: wgpu::VertexState { module: shader, entry_point: Some("fv_vs"), compilation_options: Default::default(), buffers: &[] },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fv_fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview: None,
        cache: None,
    });
    let s = a.size;
    let full = s * 2;
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: full, height: full, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let c = a.center;
    let r = a.radius;
    let d = 2.8 * r;
    let fov = 2.0 * (1.15f32 / 2.8).atan();
    let views = [
        ([c[0] + d, c[1] + 0.05 * d, c[2]], 0, 0),
        ([c[0], c[1] + 0.05 * d, c[2] + d], s, 0),
        ([c[0] + 0.62 * d, c[1] + 0.45 * d, c[2] + 0.62 * d], 0, s),
        ([c[0] + 0.001, c[1] + d, c[2] + 0.12 * d], s, s),
    ];
    let mut enc = device.create_command_encoder(&Default::default());
    for (i, (eye, x0, y0)) in views.iter().enumerate() {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buf, 0, &camera_bytes(*eye, c, fov, s, *x0, *y0));
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if i == 0 { wgpu::LoadOp::Clear(wgpu::Color::BLACK) } else { wgpu::LoadOp::Load },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.set_viewport(*x0 as f32, *y0 as f32, s as f32, s as f32, 0.0, 1.0);
        pass.draw(0..3, 0..1);
    }
    let row = (full * 4).div_ceil(256) * 256;
    let out = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * full) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &out, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
        wgpu::Extent3d { width: full, height: full, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    let data = read(device, &out);
    let mut pixels = Vec::with_capacity((full * full * 4) as usize);
    for y in 0..full {
        let start = (y * row) as usize;
        pixels.extend_from_slice(&data[start..start + (full * 4) as usize]);
    }
    pixels
}

fn read(device: &wgpu::Device, buf: &wgpu::Buffer) -> Vec<u8> {
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map failed"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll failed");
    let v = slice.get_mapped_range().to_vec();
    buf.unmap();
    v
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    let file = fs::File::create(path).unwrap_or_else(|e| { eprintln!("can't write {path}: {e}"); exit(73) });
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(rgba)).expect("png encode failed");
}

fn sample_grid(device: &wgpu::Device, queue: &wgpu::Queue, shader: &wgpu::ShaderModule, a: &Args) -> Vec<f32> {
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("grid"),
        layout: None,
        module: shader,
        entry_point: Some("fv_grid"),
        compilation_options: Default::default(),
        cache: None,
    });
    let n = GRID;
    let cell = 2.0 * a.radius / n as f32;
    let o = [a.center[0] - a.radius, a.center[1] - a.radius, a.center[2] - a.radius];
    let mut gbytes = bytemuck::cast_slice(&[o[0], o[1], o[2], cell]).to_vec();
    gbytes.extend_from_slice(bytemuck::cast_slice(&[n, 0u32, 0, 0]));
    let gbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 32, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    queue.write_buffer(&gbuf, 0, &gbytes);
    let size = (n * n * n * 4) as u64;
    let sbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let rbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 1, resource: gbuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: sbuf.as_entire_binding() },
        ],
    });
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        let g = n.div_ceil(4);
        pass.dispatch_workgroups(g, g, g);
    }
    enc.copy_buffer_to_buffer(&sbuf, 0, &rbuf, 0, size);
    queue.submit([enc.finish()]);
    bytemuck::cast_slice(&read(device, &rbuf)).to_vec()
}

fn report(d: &[f32], a: &Args) {
    let n = GRID as usize;
    let cell = 2.0 * a.radius / n as f32;
    let o = [a.center[0] - a.radius, a.center[1] - a.radius, a.center[2] - a.radius];
    let pos = |i: usize, j: usize, k: usize| [o[0] + (i as f32 + 0.5) * cell, o[1] + (j as f32 + 0.5) * cell, o[2] + (k as f32 + 0.5) * cell];
    let idx = |i: usize, j: usize, k: usize| i + n * (j + n * k);
    let vox = (cell as f64).powi(3);

    let (mut inside, mut below, mut lo, mut hi) = (0usize, 0usize, [f32::MAX; 3], [f32::MIN; 3]);
    let mut edges: Vec<&str> = vec![];
    let names = ["-x", "+x", "-y", "+y", "-z", "+z"];
    let mut touch = [false; 6];
    let (mut nan, mut gmax, mut gover, mut gcount) = (0usize, 0f32, 0usize, 0usize);
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let v = d[idx(i, j, k)];
                if !v.is_finite() { nan += 1; continue; }
                for (axis, (ni, nj, nk)) in [(i + 1, j, k), (i, j + 1, k), (i, j, k + 1)].into_iter().enumerate() {
                    let _ = axis;
                    if ni < n && nj < n && nk < n {
                        let w = d[idx(ni, nj, nk)];
                        if w.is_finite() && (v.abs() < 0.1 || w.abs() < 0.1) {
                            let g = (w - v).abs() / cell;
                            gmax = gmax.max(g);
                            gcount += 1;
                            if g > 1.2 { gover += 1; }
                        }
                    }
                }
                if v < 0.0 {
                    inside += 1;
                    let p = pos(i, j, k);
                    if p[1] < 0.0 { below += 1; }
                    for t in 0..3 { lo[t] = lo[t].min(p[t]); hi[t] = hi[t].max(p[t]); }
                    for (t, b) in [i == 0, i == n - 1, j == 0, j == n - 1, k == 0, k == n - 1].into_iter().enumerate() {
                        if b { touch[t] = true; }
                    }
                }
            }
        }
    }
    for t in 0..6 { if touch[t] { edges.push(names[t]); } }

    // Connected pieces (6-connectivity) of the inside region.
    let mut label = vec![u32::MAX; n * n * n];
    let mut pieces: Vec<(usize, [f32; 3])> = vec![];
    for start in 0..n * n * n {
        if !(d[start] < 0.0) || label[start] != u32::MAX { continue; }
        let id = pieces.len() as u32;
        let mut q = VecDeque::from([start]);
        label[start] = id;
        let (mut count, mut sum) = (0usize, [0f64; 3]);
        while let Some(c) = q.pop_front() {
            let (i, j, k) = (c % n, (c / n) % n, c / (n * n));
            count += 1;
            let p = pos(i, j, k);
            for t in 0..3 { sum[t] += p[t] as f64; }
            let mut push = |ni: usize, nj: usize, nk: usize| {
                let e = idx(ni, nj, nk);
                if d[e] < 0.0 && label[e] == u32::MAX { label[e] = id; q.push_back(e); }
            };
            if i > 0 { push(i - 1, j, k) } if i + 1 < n { push(i + 1, j, k) }
            if j > 0 { push(i, j - 1, k) } if j + 1 < n { push(i, j + 1, k) }
            if k > 0 { push(i, j, k - 1) } if k + 1 < n { push(i, j, k + 1) }
        }
        pieces.push((count, [(sum[0] / count as f64) as f32, (sum[1] / count as f64) as f32, (sum[2] / count as f64) as f32]));
    }
    pieces.sort_by(|x, y| y.0.cmp(&x.0));

    println!("grid: {n}^3 samples, cell {:.1} cm, box centre {:?} radius {} m", cell * 100.0, a.center, a.radius);
    if inside == 0 {
        println!("volume: nothing inside the box (field >= 0 everywhere sampled)");
        return;
    }
    println!("volume: {:.3} m^3 (about {:.0} kg at 1050 kg/m^3)", inside as f64 * vox, inside as f64 * vox * 1050.0);
    println!("bounds: x [{:.2}, {:.2}]  y [{:.2}, {:.2}]  z [{:.2}, {:.2}] m", lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]);
    if pieces.len() == 1 {
        println!("pieces: 1 (everything is connected)");
    } else {
        println!("pieces: {} separate pieces. Largest first (volume, centre):", pieces.len());
        for (c, ctr) in pieces.iter().take(8) {
            println!("  {:.4} m^3 at ({:.2}, {:.2}, {:.2})", *c as f64 * vox, ctr[0], ctr[1], ctr[2]);
        }
    }
    if below > 0 {
        println!("ground: {:.4} m^3 is below y = 0 (sunk into the ground)", below as f64 * vox);
    } else {
        println!("ground: nothing below y = 0; lowest inside sample at y = {:.3} m", lo[1]);
    }
    if edges.is_empty() {
        println!("framing: fits inside the box");
    } else {
        println!("framing: cut off at the box's {} side(s); increase --radius or move --center", edges.join(", "));
    }
    println!(
        "gradient near the surface (|d| < 10 cm): max {:.2}, {:.2}% of samples above 1.2 (above ~1 risks sphere-tracing artifacts)",
        gmax, 100.0 * gover as f64 / gcount.max(1) as f64
    );
    if nan > 0 {
        println!("WARNING: {nan} samples are NaN or infinite");
    }
}
