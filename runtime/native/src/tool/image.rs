//! `std::tool::image` (language.md §15.5): images decoded, resized and encoded on the CPU, beside
//! the tool. PNG, JPEG and WebP decode to RGBA8 (rows top to bottom); `resize` is Lanczos3, as
//! Pillow's `Image.Resampling.LANCZOS` computes its weights, over premultiplied alpha as Pillow's
//! RGBA resize is; PNG encodes losslessly, and WebP lossily through libwebp, with the quality and
//! method the request gives. The same inputs give the same bytes: no codec here uses threads or
//! SIMD paths that differ by machine, except libwebp's, which are bit-exact by its design.
//!
//! **Bytes are reserved before they're held.** The host reads each request's sizes first (an
//! image's header, a buffer's dimensions), then reserves the answer's blob and the work's own
//! buffers in the blob budget in one step, on the worker, before anything is decoded or
//! allocated: an image that doesn't fit fails then, unread. The output is in that blob; what the
//! work didn't use of the reservation is let go. The blob is the request's until the tool reads
//! the answer, so a failed request gives it back.
//!
//! **The work stops with its request.** It runs on a thread of its own, and the heavy worker that
//! waits for it looks for the request's deadline (from when the host took it) and the tool's end
//! every few milliseconds. Then it cancels the work, which a resize sees between rows and a codec
//! between its phases. If the work hasn't stopped within the grace after that (a codec's own call
//! doesn't stop), the run is the host's failure (status 71), as an HTTP client that won't return
//! is.

use super::services::{Answer, Body, Data, Services, Task, Work};
use super::toolchain::Out;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// A request's deadline unless it asks for another.
const DEADLINE: Duration = Duration::from_secs(60);

/// How often the worker looks at the work it waits for.
const POLL: Duration = Duration::from_millis(10);

/// The most pixels an image has each way (libwebp's own most is one fewer).
pub(crate) const MOST_SIDE: u32 = 16384;

/// What the work is cancelled with: past its deadline, or the tool ended.
const CANCELLED: &str = "the image's work was cancelled";

/// An image request, read.
enum Op {
    Decode(Data),
    Resize { rgba: Data, size: (u32, u32), to: (u32, u32) },
    Png { rgba: Data, size: (u32, u32) },
    Webp { rgba: Data, size: (u32, u32), quality: f32, method: u8 },
}

impl Op {
    fn name(&self) -> &'static str {
        match self {
            Op::Decode(_) => "decode",
            Op::Resize { .. } => "resize",
            Op::Png { .. } => "encode_png",
            Op::Webp { .. } => "encode_webp",
        }
    }
}

/// Whether `w`×`h` is a size an image may have.
fn side(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 || w > MOST_SIDE || h > MOST_SIDE {
        return Err(format!("an image of {w}×{h} isn't 1 to {MOST_SIDE} pixels each way"));
    }
    Ok(())
}

/// How request `name` of the image service, with `b`, is carried out.
pub(crate) fn route(name: &str, b: &mut Body) -> Work {
    let entered = Instant::now();
    let parsed = (|| {
        let op = match name {
            "decode" => Op::Decode(b.data()?),
            "resize" => {
                let rgba = b.data()?;
                let size = (b.u32()?, b.u32()?);
                let to = (b.u32()?, b.u32()?);
                side(to.0, to.1)?;
                Op::Resize { rgba, size, to }
            }
            "encode_png" => {
                let rgba = b.data()?;
                Op::Png { rgba, size: (b.u32()?, b.u32()?) }
            }
            "encode_webp" => {
                let rgba = b.data()?;
                let size = (b.u32()?, b.u32()?);
                let quality = f64::from_bits(b.u64()?);
                let method = b.u8()?;
                if !(0.0..=100.0).contains(&quality) {
                    return Err(format!("a WebP's quality is 0 to 100, not {quality}"));
                }
                if method > 6 {
                    return Err(format!("a WebP's method is 0 to 6, not {method}"));
                }
                if size.0 >= MOST_SIDE || size.1 >= MOST_SIDE {
                    return Err(format!("a WebP is at most {} pixels each way", MOST_SIDE - 1));
                }
                Op::Webp { rgba, size, quality: quality as f32, method }
            }
            _ => return Err(format!("the image service has no `{name}`")),
        };
        let deadline = super::http::deadline(f64::from_bits(b.u64()?), DEADLINE)?;
        b.end()?;
        Ok((op, deadline))
    })();
    match parsed {
        Ok((op, deadline)) => {
            let task: Task = Box::new(move |s, request| {
                super::services::answer(carry_out(s, request, op, entered + deadline, deadline))
            });
            Work::Heavy(task)
        }
        Err(why) => Work::Now(Answer::Bytes([&[1u8][..], why.as_bytes()].concat())),
    }
}

/// The bytes `data` names: its own, or its blob's, shared (while they're held, the blob can't be
/// appended to or released, as while a file is written from it).
fn bytes(s: &Services, data: Data) -> Result<Arc<Vec<u8>>, String> {
    match data {
        Data::Bytes(b) => Ok(Arc::new(b)),
        Data::Blob(id) => s.blobs().get(id),
    }
}

/// `rgba`'s length checked against `w`×`h` pixels of RGBA8.
fn pixels(rgba: &[u8], (w, h): (u32, u32)) -> Result<(), String> {
    side(w, h)?;
    let want = u64::from(w) * u64::from(h) * 4;
    if rgba.len() as u64 != want {
        return Err(format!("{w}×{h} RGBA8 is {want} bytes, not {}", rgba.len()));
    }
    Ok(())
}

/// The most bytes a PNG of `w`×`h` RGBA8 takes: its rows, each with its filter's byte, stored
/// (deflate's worst, 5 bytes a block of 65535), and its chunks' framing.
fn png_most(w: u32, h: u32) -> u64 {
    let raw = u64::from(h) * (1 + u64::from(w) * 4);
    raw + 5 * (raw / 16383 + 1) + 4096
}

/// The work the request does, on its own thread: what it gives (a size and bytes), or why not.
type Job = Box<dyn FnOnce(&AtomicBool) -> Result<(u32, u32, Vec<u8>), String> + Send>;

/// Carries out `op` for `request` by `due`: the answer's payload, or why it failed.
fn carry_out(
    s: &Arc<Services>,
    request: u32,
    op: Op,
    due: Instant,
    deadline: Duration,
) -> Result<Vec<u8>, String> {
    let what = op.name();
    if Instant::now() >= due {
        return Err(format!(
            "`{what}` didn't start within its deadline of {} s",
            deadline.as_secs_f64()
        ));
    }
    // The sizes first, then the reservation, then the work: nothing is decoded or allocated
    // that the budget doesn't hold.
    let (reserve, sized, job): (u64, bool, Job) = match op {
        Op::Decode(data) => {
            let input = bytes(s, data)?;
            let (_, w, h) = header(&input)?;
            side(w, h)?;
            // The RGBA, and the decoder's own buffer of the image's pixels.
            let rgba = u64::from(w) * u64::from(h) * 4;
            (2 * rgba, true, Box::new(move |cancel| decode(&input, cancel)))
        }
        Op::Resize { rgba, size, to } => {
            let input = bytes(s, rgba)?;
            pixels(&input, size)?;
            let out = u64::from(to.0) * u64::from(to.1) * 4;
            // The horizontal pass's rows, four `f32`s a pixel.
            let between = if to.0 == size.0 { 0 } else { u64::from(to.0) * u64::from(size.1) * 16 };
            let job: Job = Box::new(move |cancel| {
                let out = lanczos(&input, size, to, cancel)?;
                Ok((to.0, to.1, out))
            });
            (out + between, true, job)
        }
        Op::Png { rgba, size } => {
            let input = bytes(s, rgba)?;
            pixels(&input, size)?;
            let most = png_most(size.0, size.1);
            let job: Job = Box::new(move |cancel| {
                Ok((size.0, size.1, encode_png(&input, size, most, cancel)?))
            });
            (most, false, job)
        }
        Op::Webp { rgba, size, quality, method } => {
            let input = bytes(s, rgba)?;
            pixels(&input, size)?;
            let n = input.len() as u64;
            // libwebp's copy of the pixels, its YUV planes, and the encoded bytes (at most as many
            // as the pixels': libwebp's own, then the answer's).
            let job: Job = Box::new(move |cancel| {
                Ok((size.0, size.1, encode_webp(&input, size, quality, method, n, cancel)?))
            });
            (3 * n, false, job)
        }
    };
    let id = s.blobs().create_reserved(request, reserve).map_err(|why| {
        format!("`{what}` needs {reserve} bytes of the blob budget for its pixels: {why}")
    })?;
    let (w, h, out) = apart(s, request, what, due, deadline, job)?;
    s.blobs().fill(id, out)?;
    let mut answer = Out::default();
    if sized {
        answer.u32(w).u32(h);
    }
    answer.u8(1).u32(id);
    Ok(answer.0)
}

/// Runs `job` on a thread of its own, waiting for it until `due`: what it gave, or why it didn't.
/// Past `due`, or when the tool ends, the job is cancelled, and has the grace; if it hasn't
/// returned then, the run is the host's failure.
fn apart(
    s: &Services,
    request: u32,
    what: &str,
    due: Instant,
    deadline: Duration,
    job: Job,
) -> Result<(u32, u32, Vec<u8>), String> {
    let cancel = Arc::new(AtomicBool::new(false));
    let (sent, got) = std::sync::mpsc::channel();
    let flag = cancel.clone();
    let spawned =
        std::thread::Builder::new().name(format!("wrela-image-{request}")).spawn(move || {
            let run = std::panic::AssertUnwindSafe(|| job(&flag));
            let result = std::panic::catch_unwind(run)
                .unwrap_or_else(|_| Err("the image's work failed (it panicked)".into()));
            let _ = sent.send(result);
        });
    let thread = spawned.map_err(|e| format!("can't start the image's thread: {e}"))?;
    let (why, grace) = loop {
        match got.recv_timeout(POLL) {
            Ok(result) => {
                let _ = thread.join();
                return result;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
                return Err("the image's thread ended without an answer".into());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if s.closing.load(Ordering::Acquire) {
            // Half the grace: the pool waits for this worker the whole grace.
            break (format!("the tool ended during `{what}`"), s.grace / 2);
        }
        if Instant::now() >= due {
            let why = format!(
                "`{what}` ran past its deadline of {} s, and the host stopped it",
                deadline.as_secs_f64()
            );
            break (why, s.grace);
        }
    };
    cancel.store(true, Ordering::Release);
    if got.recv_timeout(grace).is_ok() {
        let _ = thread.join();
        return Err(why);
    }
    let _ = s.stuck.set(format!(
        "`{what}` for request {request} didn't stop within {grace:?} of being cancelled"
    ));
    Err(why)
}

fn stopped(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { Err(CANCELLED.into()) } else { Ok(()) }
}

/// An encoded image's format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Png,
    Jpeg,
    Webp,
}

/// `bytes`' format, by its first bytes, and its size, from its header alone.
fn header(bytes: &[u8]) -> Result<(Format, u32, u32), String> {
    let format = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Format::Png
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Format::Jpeg
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Format::Webp
    } else {
        return Err("the bytes aren't a PNG, a JPEG or a WebP".into());
    };
    let (w, h) = match format {
        // Its first chunk, `IHDR`, read here: nothing of the rest is.
        Format::Png => match bytes.get(8..24) {
            Some(h) if h[..8] == [0, 0, 0, 13, b'I', b'H', b'D', b'R'] => {
                let be = |at: usize| u32::from_be_bytes(h[at..at + 4].try_into().expect("4"));
                (be(8), be(12))
            }
            _ => return Err("the PNG doesn't start with its header (`IHDR`)".into()),
        },
        Format::Jpeg => {
            let mut d = jpeg_decoder(bytes);
            d.decode_headers().map_err(|e| format!("the JPEG: {e:?}"))?;
            let info = d.info().ok_or("the JPEG has no size")?;
            (u32::from(info.width), u32::from(info.height))
        }
        Format::Webp => {
            let d = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes))
                .map_err(|e| format!("the WebP: {e}"))?;
            d.dimensions()
        }
    };
    Ok((format, w, h))
}

fn png_reader(bytes: &[u8]) -> Result<png::Reader<std::io::Cursor<&[u8]>>, String> {
    // Its text and colour profile in at most 4 MiB: the header is read before any reservation.
    let limits = png::Limits { bytes: 4 << 20 };
    let mut decoder = png::Decoder::new_with_limits(std::io::Cursor::new(bytes), limits);
    // Indexed, grey and 16-bit images come out as 8-bit grey, grey and alpha, RGB or RGBA.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    decoder.read_info().map_err(|e| format!("the PNG: {e}"))
}

fn jpeg_decoder(bytes: &[u8]) -> zune_jpeg::JpegDecoder<std::io::Cursor<&[u8]>> {
    let options = zune_core::options::DecoderOptions::default()
        .jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGBA)
        .set_max_width(MOST_SIDE as usize)
        .set_max_height(MOST_SIDE as usize);
    zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(bytes), options)
}

/// `channels` bytes a pixel, as RGBA8: grey, grey and alpha, or RGB, made opaque where they have
/// no alpha.
fn to_rgba(pixels: Vec<u8>, channels: usize) -> Vec<u8> {
    if channels == 4 {
        return pixels;
    }
    let mut out = Vec::with_capacity(pixels.len() / channels * 4);
    for p in pixels.chunks_exact(channels) {
        match channels {
            1 => out.extend_from_slice(&[p[0], p[0], p[0], 255]),
            2 => out.extend_from_slice(&[p[0], p[0], p[0], p[1]]),
            _ => out.extend_from_slice(&[p[0], p[1], p[2], 255]),
        }
    }
    out
}

/// Decodes `bytes` (a PNG, a JPEG or a WebP: its first frame, if it has several) as RGBA8.
fn decode(bytes: &[u8], cancel: &AtomicBool) -> Result<(u32, u32, Vec<u8>), String> {
    let (format, w, h) = header(bytes)?;
    stopped(cancel)?;
    let rgba = match format {
        Format::Png => {
            let mut reader = png_reader(bytes)?;
            let size = reader.output_buffer_size().ok_or("the PNG is too large")?;
            let mut out = vec![0; size];
            let frame = reader.next_frame(&mut out).map_err(|e| format!("the PNG: {e}"))?;
            out.truncate(frame.buffer_size());
            stopped(cancel)?;
            let channels = match frame.color_type {
                png::ColorType::Grayscale => 1,
                png::ColorType::GrayscaleAlpha => 2,
                png::ColorType::Rgb => 3,
                png::ColorType::Rgba => 4,
                other => return Err(format!("the PNG's pixels are {other:?}")),
            };
            to_rgba(out, channels)
        }
        Format::Jpeg => {
            let mut d = jpeg_decoder(bytes);
            d.decode().map_err(|e| format!("the JPEG: {e:?}"))?
        }
        Format::Webp => {
            let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes))
                .map_err(|e| format!("the WebP: {e}"))?;
            let size = d.output_buffer_size().ok_or("the WebP is too large")?;
            let mut out = vec![0; size];
            d.read_image(&mut out).map_err(|e| format!("the WebP: {e}"))?;
            stopped(cancel)?;
            to_rgba(out, if d.has_alpha() { 4 } else { 3 })
        }
    };
    if rgba.len() as u64 != u64::from(w) * u64::from(h) * 4 {
        return Err(format!("the image decoded to {} bytes, not {w}×{h} RGBA8", rgba.len()));
    }
    Ok((w, h, rgba))
}

/// Lanczos3, as Pillow's: `sinc(x) sinc(x / 3)` for `|x| < 3`.
pub(crate) fn lanczos3(x: f64) -> f64 {
    fn sinc(x: f64) -> f64 {
        if x == 0.0 {
            return 1.0;
        }
        let x = x * std::f64::consts::PI;
        x.sin() / x
    }
    if (-3.0..3.0).contains(&x) { sinc(x) * sinc(x / 3.0) } else { 0.0 }
}

/// Each output pixel's first input pixel and weights, resampling `from` pixels to `to`, as
/// Pillow's `precompute_coeffs` computes them: a pixel's centre is at `(x + 0.5) × scale`, the
/// filter is widened by the scale when it shrinks, and the weights sum to 1.
pub(crate) fn weights(from: u32, to: u32) -> Vec<(usize, Vec<f32>)> {
    let scale = f64::from(from) / f64::from(to);
    let filterscale = scale.max(1.0);
    let support = 3.0 * filterscale;
    (0..to)
        .map(|x| {
            let center = (f64::from(x) + 0.5) * scale;
            // C's `(int)`: towards zero.
            let lo = ((center - support + 0.5) as i64).max(0) as usize;
            let hi = ((center + support + 0.5) as i64).min(i64::from(from)) as usize;
            let mut w: Vec<f64> =
                (lo..hi).map(|i| lanczos3((i as f64 - center + 0.5) / filterscale)).collect();
            let sum: f64 = w.iter().sum();
            if sum != 0.0 {
                for k in &mut w {
                    *k /= sum;
                }
            }
            (lo, w.into_iter().map(|k| k as f32).collect())
        })
        .collect()
}

/// `rgba` (`w`×`h`) resized to `to` with Lanczos3: horizontally, then vertically, each pass
/// skipped where the size stays, over premultiplied alpha, rounded to 8 bits once at the end.
fn lanczos(
    rgba: &[u8],
    (w, h): (u32, u32),
    to: (u32, u32),
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let (w, h) = (w as usize, h as usize);
    let (tw, th) = (to.0 as usize, to.1 as usize);
    // Premultiplied, as Pillow's RGBA resize is (through "RGBa").
    let premultiplied = |p: &[u8]| -> [f32; 4] {
        let a = f32::from(p[3]) / 255.0;
        [f32::from(p[0]) * a, f32::from(p[1]) * a, f32::from(p[2]) * a, f32::from(p[3])]
    };
    let mut rows: Vec<[f32; 4]> = Vec::with_capacity(tw * h);
    if tw == w {
        rows.extend(rgba.chunks_exact(4).map(premultiplied));
    } else {
        let across = weights(w as u32, to.0);
        for y in 0..h {
            stopped(cancel)?;
            let row = &rgba[y * w * 4..(y + 1) * w * 4];
            for (lo, ks) in &across {
                let mut sum = [0.0f32; 4];
                for (k, wt) in ks.iter().enumerate() {
                    let p = premultiplied(&row[(lo + k) * 4..(lo + k) * 4 + 4]);
                    for c in 0..4 {
                        sum[c] += p[c] * wt;
                    }
                }
                rows.push(sum);
            }
        }
    }
    let down = (th != h).then(|| weights(h as u32, to.1));
    let mut out = Vec::with_capacity(tw * th * 4);
    for y in 0..th {
        stopped(cancel)?;
        for x in 0..tw {
            let p = match &down {
                None => rows[y * tw + x],
                Some(down) => {
                    let (lo, ks) = &down[y];
                    let mut sum = [0.0f32; 4];
                    for (k, wt) in ks.iter().enumerate() {
                        let q = rows[(lo + k) * tw + x];
                        for c in 0..4 {
                            sum[c] += q[c] * wt;
                        }
                    }
                    sum
                }
            };
            let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
            let a = p[3];
            if a <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let un = 255.0 / a;
            out.extend_from_slice(&[byte(p[0] * un), byte(p[1] * un), byte(p[2] * un), byte(a)]);
        }
    }
    Ok(out)
}

/// A `Vec` written to no further than its capacity: past it, writing fails, so an encoder can't
/// take more than was reserved.
struct Capped(Vec<u8>);

impl std::io::Write for Capped {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.0.len() + buf.len() > self.0.capacity() {
            return Err(std::io::Error::other("more bytes than were reserved"));
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `rgba` (`w`×`h`) as a PNG of RGBA8, in at most `most` bytes.
fn encode_png(
    rgba: &[u8],
    (w, h): (u32, u32),
    most: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let fail = |e: png::EncodingError| format!("the PNG: {e}");
    let mut out = Capped(Vec::with_capacity(most as usize));
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(fail)?;
        let mut stream = writer.stream_writer().map_err(fail)?;
        // A band of rows at a time, so a cancel is seen between them.
        let band = (w as usize) * 4 * 64;
        for chunk in rgba.chunks(band) {
            stopped(cancel)?;
            std::io::Write::write_all(&mut stream, chunk).map_err(|e| format!("the PNG: {e}"))?;
        }
        stream.finish().map_err(fail)?;
    }
    Ok(out.0)
}

/// `rgba` (`w`×`h`) as a lossy WebP at `quality` (0 to 100) and `method` (0, fastest, to 6,
/// smallest), in at most `most` bytes: libwebp's defaults otherwise, its alpha at quality 100, on
/// one thread.
fn encode_webp(
    rgba: &[u8],
    (w, h): (u32, u32),
    quality: f32,
    method: u8,
    most: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let mut config =
        webp::WebPConfig::new().map_err(|()| "libwebp's configuration didn't start".to_string())?;
    config.lossless = 0;
    config.quality = quality;
    config.method = i32::from(method);
    config.alpha_quality = 100;
    config.thread_level = 0;
    stopped(cancel)?;
    let encoded = webp::Encoder::from_rgba(rgba, w, h)
        .encode_advanced(&config)
        .map_err(|e| format!("libwebp: {e:?}"))?;
    stopped(cancel)?;
    if encoded.len() as u64 > most {
        return Err(format!("the WebP is {} bytes, more than the {most} reserved", encoded.len()));
    }
    Ok(encoded.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_sum_to_one_and_reach_as_far_as_pillows() {
        // Halving: each output pixel reaches 6 input pixels each way, its centre between two.
        let half = weights(8, 4);
        assert_eq!(half.len(), 4);
        for (lo, ks) in &half {
            let sum: f32 = ks.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "{sum}");
            assert!(lo + ks.len() <= 8);
        }
        assert_eq!(half[0].0, 0, "clamped at the first pixel");
        // The same size: the identity.
        for (x, (lo, ks)) in weights(5, 5).iter().enumerate() {
            let at = ks.iter().position(|&k| (k - 1.0).abs() < 1e-6).expect("one weight of 1");
            assert_eq!(lo + at, x);
        }
        assert_eq!(lanczos3(3.0), 0.0);
        assert_eq!(lanczos3(0.0), 1.0);
    }

    #[test]
    fn a_header_gives_the_format_and_size_and_a_cancel_stops_a_decode() {
        let mut png = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png, 3, 2);
            e.set_color(png::ColorType::Rgb);
            e.write_header().expect("a header").write_image_data(&[7; 18]).expect("pixels");
        }
        assert_eq!(header(&png), Ok((Format::Png, 3, 2)));
        assert!(header(b"GIF89a").unwrap_err().contains("aren't"));
        let (w, h, rgba) = decode(&png, &AtomicBool::new(false)).expect("decoded");
        assert_eq!((w, h), (3, 2));
        assert_eq!(&rgba[..4], &[7, 7, 7, 255]);
        assert!(decode(&png, &AtomicBool::new(true)).unwrap_err().contains("cancelled"));
    }
}
