//! The contract between compiled wrela programs and the hosts that run them: layer 0's half of
//! `runtime/command-stream.md`, in executable form.
//!
//! The compiler uses it to write manifests and to know the bytes its WASM must submit; the native
//! host uses it to decode and check what a program submits. The browser runtime mirrors this crate
//! in TypeScript, and the tests here are the cases both must agree on.
//!
//! - [`stream`]: the command buffer format, its [`Encoder`] and [`decode`], and the
//!   [`FrameCheck`] that enforces the frame rules across buffers.
//! - [`manifest`]: the build manifest's types, JSON form and validation.
//! - [`StateHash`]: the FNV-1a hash of everything a program submits (AC7).

mod hash;
pub mod manifest;
pub mod stream;

pub use hash::StateHash;
pub use manifest::{CommandError, Manifest, ManifestError};
pub use stream::{
    Command, DecodeError, EncodeError, Encoder, FrameCheck, MAX_FRAME_VERTICES, SequenceError,
    decode,
};

/// The contract's version. It's in every command buffer's header and in the manifest; a host
/// rejects any version it wasn't built for.
pub const VERSION: u16 = 0;

/// The module a program imports host functions from.
pub const IMPORT_MODULE: &str = "wrela";

/// `submit(ptr: u32, len: u32)`: hands the host one complete command buffer.
pub const SUBMIT: &str = "submit";

/// The program's exported linear memory.
pub const MEMORY: &str = "memory";

/// `frame(time: f32, width: u32, height: u32)`: called once per frame.
pub const FRAME: &str = "frame";

/// The fixed inputs of test mode, so every host renders the same frames.
pub mod test_mode {
    /// The screen's width in pixels.
    pub const WIDTH: u32 = 1920;
    /// The screen's height in pixels.
    pub const HEIGHT: u32 = 1080;
    /// Frames run before the capture.
    pub const FRAMES: u32 = 60;
    /// A frame whose GPU work takes longer than this, in milliseconds, ends the run with an
    /// error: four times the ~100 ms a submission may take (CLAUDE.md's GPU safety rules). A
    /// frame between the two gets a warning. Stopping at the first such frame keeps a slow
    /// program from holding the GPU for all its frames.
    pub const FRAME_LIMIT_MS: u32 = 400;

    /// The time passed to frame `index` (from 0): `index / 60` seconds, computed in f64 and
    /// rounded once to the nearest f32, which is what JavaScript's `Math.fround(i / 60)` gives.
    pub fn time(index: u32) -> f32 {
        (f64::from(index) / 60.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::test_mode;

    #[test]
    fn test_mode_times_round_once_from_f64() {
        assert_eq!(test_mode::time(0), 0.0);
        assert_eq!(test_mode::time(30), 0.5);
        assert_eq!(test_mode::time(60), 1.0);
        // 59/60 = 0.98333…; one rounding from f64 gives this f32. A host that divides in f32
        // happens to agree here, but the rule is f64 then round, as in JavaScript.
        assert_eq!(test_mode::time(59).to_bits(), 0x3f7b_bbbc);
        for i in 0..test_mode::FRAMES {
            let exact = f64::from(i) / 60.0;
            let t = f64::from(test_mode::time(i));
            assert!((t - exact).abs() <= exact * f64::from(f32::EPSILON), "{i}");
        }
    }
}
