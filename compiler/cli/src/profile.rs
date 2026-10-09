//! `wrela profile <package-dir> [--frames n] [--size WxH] [--input script.json] [--serial]
//! [--json]`: where a frame's time goes, on this machine. It builds the program as `wrela build`
//! does, runs its frames on the native host's GPU (in lockstep with its ticker, 60 a second;
//! 120 frames unless said), and reports:
//! - the GPU's time: each label's median a frame (a pass or a dispatch, named by
//!   `std::gpu::label`, else by its pipeline), and the median frame's span, its first start to
//!   its last end. Passes overlap on some GPUs, so labels can add up to more than the span;
//!   `--serial` runs each pass alone, so each label's time is its own and the span isn't a
//!   frame's;
//! - the CPU's: the median time of a frame's call, the host's work on its commands included,
//!   and the phases the program timed (`std::time::phase`), summed over the run;
//! - each pipeline's WGSL, largest first.
//!
//! The first frames make the GPU's pipelines, so a run of a few frames says less. Times are this
//! machine's: compare a change with a profile from before it, on the same machine.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;
use wrela_host::{GpuTiming, Host, Options, Timing};

struct Args {
    dir: PathBuf,
    frames: u32,
    size: (u32, u32),
    input: Option<PathBuf>,
    serial: bool,
    json: bool,
}

fn parse(args: &[String]) -> Option<Args> {
    let mut a = Args {
        dir: PathBuf::new(),
        frames: 120,
        size: (1280, 720),
        input: None,
        serial: false,
        json: false,
    };
    let mut dir = None;
    let mut it = args.iter();
    while let Some(x) = it.next() {
        match x.as_str() {
            "--frames" => a.frames = it.next()?.parse().ok().filter(|&n| n > 0)?,
            "--size" => a.size = crate::parse_size(it.next()?)?,
            "--input" => a.input = Some(PathBuf::from(it.next()?)),
            "--serial" => a.serial = true,
            "--json" => a.json = true,
            _ if dir.is_none() && !x.starts_with('-') => dir = Some(PathBuf::from(x)),
            _ => return None,
        }
    }
    a.dir = dir?;
    Some(a)
}

/// What a profile found.
struct Profile {
    /// Each label's time: the longest first.
    labels: Vec<Label>,
    /// The median frame's span on the GPU, ms.
    span: Option<f64>,
    /// The median frame call's time on the CPU, ms.
    frame: f64,
    phases: wrela_host::Traced,
    /// Each pipeline's name and WGSL bytes, the largest first.
    wgsl: Vec<(String, u64)>,
}

pub fn run(args: &[String]) -> ExitCode {
    let Some(a) = parse(args) else { return crate::usage() };
    match profile(&a) {
        Ok(p) => {
            if a.json {
                print!("{}", json(&a, &p));
            } else {
                print!("{}", text(&a, &p));
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("error: {why}");
            ExitCode::from(2)
        }
    }
}

fn profile(a: &Args) -> Result<Profile, String> {
    let out = wrela_driver::build(&a.dir);
    let built = a.dir.join("build").join("profile");
    crate::write_build(&out, &built)?;
    let script = match &a.input {
        Some(p) => {
            let text = std::fs::read_to_string(p)
                .map_err(|e| format!("can't read {}: {e}", p.display()))?;
            wrela_abi::input::parse_script(&text)?
        }
        None => Vec::new(),
    };
    let timing = if a.serial { Timing::Serial } else { Timing::Span };
    let options = Options { timing, quiet: true, defer_init: true, ..Options::default() };
    let mut host = Host::load_with(&built, &options).map_err(|e| e.to_string())?;
    host.init().map_err(|e| format!("in `init`: {e}"))?;
    let (w, h) = a.size;
    let mut calls = Vec::with_capacity(a.frames as usize);
    for k in 0..a.frames {
        let t = Instant::now();
        host.lockstep_frame(k, 60.0, w, h, &script).map_err(|e| format!("in frame {k}: {e}"))?;
        calls.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let timings = host.take_timings().map_err(|e| e.to_string())?;
    let phases = host.trace().take();
    drop(host);
    let spans: Vec<f64> = wrela_host::frame_spans(&timings).into_iter().map(|(_, ms)| ms).collect();
    Ok(Profile {
        labels: by_label(&timings),
        span: median(spans),
        frame: median(calls).unwrap_or(0.0),
        phases,
        wgsl: wgsl(&built)?,
    })
}

/// A label's time on the GPU: its median ms over the frames it ran in (a label run twice in a
/// frame is summed), its ms in all, and in how many frames it ran.
struct Label {
    name: String,
    median: f64,
    total: f64,
    frames: usize,
}

/// Each label's time: the longest median first.
fn by_label(timings: &[GpuTiming]) -> Vec<Label> {
    let mut per: Vec<(String, std::collections::BTreeMap<usize, f64>)> = Vec::new();
    for t in timings {
        let i = match per.iter().position(|(l, _)| *l == t.label) {
            Some(i) => i,
            None => {
                per.push((t.label.clone(), Default::default()));
                per.len() - 1
            }
        };
        *per[i].1.entry(t.frame).or_default() += t.nanos / 1e6;
    }
    let mut out: Vec<Label> = per
        .into_iter()
        .map(|(name, frames)| {
            let ms: Vec<f64> = frames.into_values().collect();
            let total = ms.iter().sum();
            let n = ms.len();
            Label { name, median: median(ms).unwrap_or(0.0), total, frames: n }
        })
        .collect();
    out.sort_by(|x, y| y.median.total_cmp(&x.median));
    out
}

fn median(mut xs: Vec<f64>) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    xs.sort_by(f64::total_cmp);
    Some(xs[xs.len() / 2])
}

/// Each pipeline's name and the bytes of its WGSL, from the build's manifest: the largest first.
fn wgsl(built: &std::path::Path) -> Result<Vec<(String, u64)>, String> {
    let manifest =
        std::fs::read_to_string(built.join("manifest.json")).map_err(|e| e.to_string())?;
    let m: serde_json::Value = serde_json::from_str(&manifest).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for p in m["pipelines"].as_array().into_iter().flatten() {
        let (Some(name), Some(shader)) = (p["name"].as_str(), p["shader"].as_str()) else {
            continue;
        };
        let bytes = std::fs::metadata(built.join(shader)).map_err(|e| e.to_string())?.len();
        out.push((name.to_string(), bytes));
    }
    out.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
    Ok(out)
}

fn kib(bytes: u64) -> String {
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

/// The report for people.
fn text(a: &Args, p: &Profile) -> String {
    let (w, h) = a.size;
    let mut s = format!("`{}`: {} frames at {w}×{h}\n", a.dir.display(), a.frames);
    s.push_str("\nGPU, each frame's median\n");
    match p.span {
        Some(span) if !a.serial => {
            s.push_str(&format!("  {:<32} {span:>9.3} ms\n", "the frame's span"))
        }
        _ => {}
    }
    let every = |l: &&Label| l.frames == a.frames as usize;
    for l in p.labels.iter().filter(every) {
        s.push_str(&format!("  {:<32} {:>9.3} ms\n", l.name, l.median));
    }
    let mut some: Vec<&Label> = p.labels.iter().filter(|l| !every(l)).collect();
    some.sort_by(|x, y| y.total.total_cmp(&x.total));
    if !some.is_empty() {
        s.push_str("in some frames only: the ms in all, and in how many\n");
        for l in some {
            s.push_str(&format!("  {:<32} {:>9.3} ms in {}\n", l.name, l.total, l.frames));
        }
    }
    if p.labels.is_empty() {
        s.push_str("  nothing: the program records no GPU work\n");
    }
    s.push_str("\nCPU\n");
    s.push_str(&format!(
        "  {:<32} {:>9.3} ms (the host's work included)\n",
        "a frame's call, median", p.frame
    ));
    if !p.phases.phases.is_empty() {
        s.push_str(&format!("  phases over the run: {}\n", p.phases.phases_line()));
    }
    let total: u64 = p.wgsl.iter().map(|(_, b)| b).sum();
    s.push_str(&format!(
        "\nWGSL: {}, {} in all\n",
        crate::count(p.wgsl.len(), "pipeline", "pipelines"),
        kib(total)
    ));
    for (name, bytes) in &p.wgsl {
        s.push_str(&format!("  {name:<32} {:>10}\n", kib(*bytes)));
    }
    s
}

/// The report for tools: times in ms (phases in seconds), WGSL in bytes.
fn json(a: &Args, p: &Profile) -> String {
    let labels: Vec<_> = p
        .labels
        .iter()
        .map(|l| {
            serde_json::json!({
                "label": l.name,
                "median_ms": l.median,
                "total_ms": l.total,
                "frames": l.frames,
            })
        })
        .collect();
    let phases: Vec<_> = p
        .phases
        .phases
        .iter()
        .map(|ph| serde_json::json!({ "name": ph.name, "seconds": ph.seconds, "times": ph.times }))
        .collect();
    let wgsl: Vec<_> =
        p.wgsl.iter().map(|(n, b)| serde_json::json!({ "pipeline": n, "bytes": b })).collect();
    let all = serde_json::json!({
        "frames": a.frames,
        "size": [a.size.0, a.size.1],
        "serial": a.serial,
        "gpu": { "span_ms": if a.serial { None } else { p.span }, "labels": labels },
        "cpu": { "frame_ms": p.frame, "phases": phases },
        "wgsl": wgsl,
    });
    format!("{all}\n")
}
