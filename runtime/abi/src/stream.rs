//! Command buffers: what a program passes to `submit`.
//!
//! A buffer is a 4-byte header (`WR` and the version) followed by commands, each an opcode, a
//! payload length and the payload. Everything is little-endian. [`decode`] checks one buffer;
//! [`FrameCheck`] checks the order of commands across all the buffers of one `frame` call.

use std::error::Error;
use std::fmt;

use crate::VERSION;

/// The first two bytes of every command buffer.
pub const MAGIC: [u8; 2] = *b"WR";

/// The header's length in bytes: the magic, then the version as a `u16`.
pub const HEADER_LEN: usize = 4;

/// The bytes before each command's payload: its opcode and the payload's length, both `u32`.
pub const COMMAND_HEADER_LEN: usize = 8;

/// The header of a buffer in `version`. For version 0 it reads as the `u32` `0x0000_5257`.
pub const fn header(version: u16) -> [u8; HEADER_LEN] {
    let v = version.to_le_bytes();
    [MAGIC[0], MAGIC[1], v[0], v[1]]
}

/// Command opcodes. 0 is never valid, so zeroed memory fails loudly; later versions take
/// opcodes from 4 up.
pub mod opcode {
    /// Begin a render pass on the screen target, cleared to a colour.
    pub const BEGIN_SCREEN_PASS: u32 = 1;
    /// Draw with a render pipeline in the open screen pass.
    pub const DRAW: u32 = 2;
    /// End the screen pass and show it; ends the frame.
    pub const PRESENT: u32 = 3;
}

/// One decoded command.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Begin a render pass on the screen target, cleared to `clear` (r, g, b, a). Every
    /// component is finite.
    BeginScreenPass { clear: [f32; 4] },
    /// Draw `vertex_count` vertices for each of `instance_count` instances with render pipeline
    /// `pipeline` (a manifest id). `uniforms` is the whole content of the pipeline's uniform at
    /// group 0, binding 0; its length is a multiple of 4.
    Draw {
        pipeline: u32,
        vertex_count: u32,
        instance_count: u32,
        uniforms: Vec<u8>,
    },
    /// End the screen pass and show it. Ends the frame.
    Present,
}

impl Command {
    pub fn opcode(&self) -> u32 {
        match self {
            Command::BeginScreenPass { .. } => opcode::BEGIN_SCREEN_PASS,
            Command::Draw { .. } => opcode::DRAW,
            Command::Present => opcode::PRESENT,
        }
    }

    /// The command's name as `runtime/command-stream.md` writes it.
    pub fn name(&self) -> &'static str {
        opcode_name(self.opcode()).unwrap_or("unknown")
    }
}

/// The name of a known opcode, as `runtime/command-stream.md` writes it.
pub fn opcode_name(opcode: u32) -> Option<&'static str> {
    match opcode {
        opcode::BEGIN_SCREEN_PASS => Some("BEGIN_SCREEN_PASS"),
        opcode::DRAW => Some("DRAW"),
        opcode::PRESENT => Some("PRESENT"),
        _ => None,
    }
}

/// Builds one command buffer. It only produces buffers that [`decode`] accepts.
#[derive(Clone, Debug)]
pub struct Encoder {
    bytes: Vec<u8>,
}

impl Default for Encoder {
    fn default() -> Self {
        Encoder::new()
    }
}

impl Encoder {
    /// A buffer holding just the header for [`VERSION`].
    pub fn new() -> Self {
        Encoder {
            bytes: header(VERSION).to_vec(),
        }
    }

    /// Appends a command, or refuses one the decoder would reject.
    pub fn push(&mut self, command: &Command) -> Result<&mut Self, EncodeError> {
        let start = self.bytes.len();
        self.put(command.opcode());
        self.put(0); // the payload length, patched below
        match command {
            Command::BeginScreenPass { clear } => {
                if let Some(index) = clear.iter().position(|c| !c.is_finite()) {
                    self.bytes.truncate(start);
                    return Err(EncodeError::NonFiniteClear {
                        component: index,
                        value: clear[index],
                    });
                }
                for c in clear {
                    self.put(c.to_bits());
                }
            }
            Command::Draw {
                pipeline,
                vertex_count,
                instance_count,
                uniforms,
            } => {
                if !uniforms.len().is_multiple_of(4) {
                    self.bytes.truncate(start);
                    return Err(EncodeError::UnalignedUniforms {
                        len: uniforms.len(),
                    });
                }
                self.put(*pipeline);
                self.put(*vertex_count);
                self.put(*instance_count);
                self.bytes.extend_from_slice(uniforms);
            }
            Command::Present => {}
        }
        let payload = self.bytes.len() - start - COMMAND_HEADER_LEN;
        let Ok(length) = u32::try_from(payload) else {
            self.bytes.truncate(start);
            return Err(EncodeError::TooLong { len: payload });
        };
        self.bytes[start + 4..start + 8].copy_from_slice(&length.to_le_bytes());
        Ok(self)
    }

    /// The bytes so far: a complete buffer.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn put(&mut self, word: u32) {
        self.bytes.extend_from_slice(&word.to_le_bytes());
    }
}

/// Encodes `commands` as one buffer.
pub fn encode(commands: &[Command]) -> Result<Vec<u8>, EncodeError> {
    let mut encoder = Encoder::new();
    for command in commands {
        encoder.push(command)?;
    }
    Ok(encoder.finish())
}

/// Why a command can't be encoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EncodeError {
    /// A clear colour component is NaN or infinite.
    NonFiniteClear { component: usize, value: f32 },
    /// Uniform bytes whose length isn't a multiple of 4.
    UnalignedUniforms { len: usize },
    /// A payload longer than `u32::MAX` bytes.
    TooLong { len: usize },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::NonFiniteClear { component, value } => write!(
                f,
                "clear colour component {component} is {value}, which isn't finite"
            ),
            EncodeError::UnalignedUniforms { len } => {
                write!(f, "{len} bytes of uniforms isn't a multiple of 4")
            }
            EncodeError::TooLong { len } => {
                write!(f, "a {len}-byte payload doesn't fit a u32 length")
            }
        }
    }
}

impl Error for EncodeError {}

/// Decodes one command buffer: the bytes of one `submit` call.
pub fn decode(bytes: &[u8]) -> Result<Vec<Command>, DecodeError> {
    let Some(head) = bytes.first_chunk::<HEADER_LEN>() else {
        return Err(DecodeError::MissingHeader { len: bytes.len() });
    };
    if head[..2] != MAGIC {
        return Err(DecodeError::BadMagic {
            found: [head[0], head[1]],
        });
    }
    let version = u16::from_le_bytes([head[2], head[3]]);
    if version != VERSION {
        return Err(DecodeError::UnsupportedVersion { found: version });
    }

    let mut commands = Vec::new();
    let mut offset = HEADER_LEN;
    while offset < bytes.len() {
        let rest = &bytes[offset..];
        let Some(head) = rest.first_chunk::<COMMAND_HEADER_LEN>() else {
            return Err(DecodeError::TruncatedCommand {
                offset,
                remaining: rest.len(),
            });
        };
        let opcode = word(head, 0);
        let length = word(head, 4);
        if !length.is_multiple_of(4) {
            return Err(DecodeError::UnalignedLength {
                offset,
                opcode,
                length,
            });
        }
        let available = rest.len() - COMMAND_HEADER_LEN;
        let payload = match usize::try_from(length) {
            Ok(len) if len <= available => &rest[COMMAND_HEADER_LEN..COMMAND_HEADER_LEN + len],
            _ => {
                return Err(DecodeError::PayloadPastEnd {
                    offset,
                    opcode,
                    length,
                    available,
                });
            }
        };
        let wrong_length = |expected| DecodeError::WrongLength {
            offset,
            opcode,
            length,
            expected,
        };
        let command = match opcode {
            opcode::BEGIN_SCREEN_PASS => {
                if payload.len() != 16 {
                    return Err(wrong_length("exactly 16"));
                }
                let clear = [
                    f32::from_bits(word(payload, 0)),
                    f32::from_bits(word(payload, 4)),
                    f32::from_bits(word(payload, 8)),
                    f32::from_bits(word(payload, 12)),
                ];
                if let Some(component) = clear.iter().position(|c| !c.is_finite()) {
                    return Err(DecodeError::NonFiniteClear { offset, component });
                }
                Command::BeginScreenPass { clear }
            }
            opcode::DRAW => {
                if payload.len() < 12 {
                    return Err(wrong_length("at least 12"));
                }
                Command::Draw {
                    pipeline: word(payload, 0),
                    vertex_count: word(payload, 4),
                    instance_count: word(payload, 8),
                    uniforms: payload[12..].to_vec(),
                }
            }
            opcode::PRESENT => {
                if !payload.is_empty() {
                    return Err(wrong_length("exactly 0"));
                }
                Command::Present
            }
            _ => return Err(DecodeError::UnknownOpcode { offset, opcode }),
        };
        commands.push(command);
        offset += COMMAND_HEADER_LEN + payload.len();
    }
    Ok(commands)
}

/// The little-endian `u32` at `at`. Callers have checked that four bytes are there.
fn word(bytes: &[u8], at: usize) -> u32 {
    let mut word = [0; 4];
    word.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(word)
}

/// Why a command buffer was rejected. `offset` is where the offending command starts, in bytes
/// from the start of the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Fewer than 4 bytes: no room for the header.
    MissingHeader { len: usize },
    /// The buffer doesn't start with `WR`.
    BadMagic { found: [u8; 2] },
    /// A version this host wasn't built for.
    UnsupportedVersion { found: u16 },
    /// Fewer than 8 bytes where a command starts.
    TruncatedCommand { offset: usize, remaining: usize },
    /// A payload length that isn't a multiple of 4.
    UnalignedLength {
        offset: usize,
        opcode: u32,
        length: u32,
    },
    /// A payload that runs past the end of the buffer.
    PayloadPastEnd {
        offset: usize,
        opcode: u32,
        length: u32,
        available: usize,
    },
    /// Opcode 0, or one this version doesn't define.
    UnknownOpcode { offset: usize, opcode: u32 },
    /// A payload length the opcode doesn't allow. `expected` says what it allows, in bytes.
    WrongLength {
        offset: usize,
        opcode: u32,
        length: u32,
        expected: &'static str,
    },
    /// A clear colour with a NaN or infinite component.
    NonFiniteClear { offset: usize, component: usize },
}

impl DecodeError {
    /// Where in the buffer the problem is, in bytes.
    pub fn offset(&self) -> usize {
        match *self {
            DecodeError::MissingHeader { .. } | DecodeError::BadMagic { .. } => 0,
            DecodeError::UnsupportedVersion { .. } => 2,
            DecodeError::TruncatedCommand { offset, .. }
            | DecodeError::UnalignedLength { offset, .. }
            | DecodeError::PayloadPastEnd { offset, .. }
            | DecodeError::UnknownOpcode { offset, .. }
            | DecodeError::WrongLength { offset, .. }
            | DecodeError::NonFiniteClear { offset, .. } => offset,
        }
    }
}

/// An opcode in a message: its name when it has one.
fn describe(opcode: u32) -> String {
    match opcode_name(opcode) {
        Some(name) => name.to_string(),
        None => format!("opcode {opcode}"),
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DecodeError::MissingHeader { len } => write!(
                f,
                "a command buffer of {len} bytes has no room for the {HEADER_LEN}-byte header"
            ),
            DecodeError::BadMagic { found } => write!(
                f,
                "a command buffer starts with {:#04x} {:#04x}, not `WR`",
                found[0], found[1]
            ),
            DecodeError::UnsupportedVersion { found } => write!(
                f,
                "command buffer version {found} isn't supported; this host reads version {VERSION}"
            ),
            DecodeError::TruncatedCommand { offset, remaining } => write!(
                f,
                "at byte {offset}: {remaining} bytes left, too few for a command's \
                 {COMMAND_HEADER_LEN}-byte header"
            ),
            DecodeError::UnalignedLength {
                offset,
                opcode,
                length,
            } => write!(
                f,
                "at byte {offset}: {} has a {length}-byte payload, not a multiple of 4",
                describe(opcode)
            ),
            DecodeError::PayloadPastEnd {
                offset,
                opcode,
                length,
                available,
            } => write!(
                f,
                "at byte {offset}: {} has a {length}-byte payload but only {available} bytes follow",
                describe(opcode)
            ),
            DecodeError::UnknownOpcode { offset, opcode } => {
                write!(f, "at byte {offset}: unknown opcode {opcode}")
            }
            DecodeError::WrongLength {
                offset,
                opcode,
                length,
                expected,
            } => write!(
                f,
                "at byte {offset}: {} has a {length}-byte payload; it takes {expected} bytes",
                describe(opcode)
            ),
            DecodeError::NonFiniteClear { offset, component } => write!(
                f,
                "at byte {offset}: BEGIN_SCREEN_PASS clear colour component {component} isn't finite"
            ),
        }
    }
}

impl Error for DecodeError {}

/// Checks the frame rules: the commands of one `frame` call, across all its buffers, are one
/// `BEGIN_SCREEN_PASS`, any number of `DRAW`s, then one `PRESENT`. Feed it every command in order,
/// then call [`FrameCheck::finish`] when `frame` returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameCheck {
    state: FrameState,
    /// The vertices the frame's draws have drawn so far: each `DRAW`'s `vertex_count` times its
    /// `instance_count`.
    vertices: u64,
}

/// The most vertices one frame's `DRAW`s may draw in all: each `DRAW`'s `vertex_count` times
/// its `instance_count`, summed. Version 0 programs draw a few full-screen triangles; the bound
/// stops a garbage count (`0xFFFF_FFFF` vertices) before it reaches the GPU, where one
/// submission could run for minutes. It doesn't bound GPU time: a few thousand full-screen
/// triangles are within it and still slow, which test mode's frame limit catches.
pub const MAX_FRAME_VERTICES: u64 = 1 << 20;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FrameState {
    #[default]
    BeforePass,
    InPass,
    Presented,
}

impl FrameCheck {
    pub fn new() -> Self {
        FrameCheck::default()
    }

    /// Accepts the next command, or says which rule it breaks. After an error the check is
    /// unchanged; the host stops anyway.
    pub fn command(&mut self, command: &Command) -> Result<(), SequenceError> {
        use FrameState::{BeforePass, InPass, Presented};
        self.state = match (self.state, command) {
            (Presented, command) => {
                return Err(SequenceError::AfterPresent {
                    command: command.name(),
                });
            }
            (BeforePass, Command::BeginScreenPass { .. }) => InPass,
            (BeforePass, Command::Draw { .. }) => return Err(SequenceError::DrawOutsidePass),
            (BeforePass, Command::Present) => return Err(SequenceError::PresentWithoutPass),
            (InPass, Command::BeginScreenPass { .. }) => {
                return Err(SequenceError::SecondScreenPass);
            }
            (
                InPass,
                Command::Draw {
                    vertex_count,
                    instance_count,
                    ..
                },
            ) => {
                // At most (2^32 - 1)^2 plus the budget: it can't overflow.
                let vertices =
                    self.vertices + u64::from(*vertex_count) * u64::from(*instance_count);
                if vertices > MAX_FRAME_VERTICES {
                    return Err(SequenceError::TooManyVertices { vertices });
                }
                self.vertices = vertices;
                InPass
            }
            (InPass, Command::Present) => Presented,
        };
        Ok(())
    }

    /// Whether the frame has been presented.
    pub fn is_presented(&self) -> bool {
        self.state == FrameState::Presented
    }

    /// Call when `frame` returns: the frame must have been presented.
    pub fn finish(&self) -> Result<(), SequenceError> {
        if self.is_presented() {
            Ok(())
        } else {
            Err(SequenceError::NotPresented)
        }
    }
}

/// A break of the frame rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SequenceError {
    /// A `DRAW` before `BEGIN_SCREEN_PASS`.
    DrawOutsidePass,
    /// A second `BEGIN_SCREEN_PASS` in one frame.
    SecondScreenPass,
    /// `PRESENT` before `BEGIN_SCREEN_PASS`.
    PresentWithoutPass,
    /// A command after `PRESENT`.
    AfterPresent { command: &'static str },
    /// `frame` returned without `PRESENT`.
    NotPresented,
    /// The frame's `DRAW`s would draw more than [`MAX_FRAME_VERTICES`] vertices, counting up to
    /// and including this one.
    TooManyVertices { vertices: u64 },
}

impl fmt::Display for SequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SequenceError::DrawOutsidePass => {
                f.write_str("a DRAW before BEGIN_SCREEN_PASS: draws go inside the screen pass")
            }
            SequenceError::SecondScreenPass => {
                f.write_str("a second BEGIN_SCREEN_PASS: a frame has one screen pass")
            }
            SequenceError::PresentWithoutPass => {
                f.write_str("a PRESENT before BEGIN_SCREEN_PASS: there's nothing to show")
            }
            SequenceError::AfterPresent { command } => {
                write!(f, "a {command} after PRESENT: PRESENT ends the frame")
            }
            SequenceError::NotPresented => {
                f.write_str("`frame` returned without PRESENT: every frame ends with one")
            }
            SequenceError::TooManyVertices { vertices } => write!(
                f,
                "this frame's DRAWs draw {vertices} vertices (vertex_count × instance_count, \
                 summed), over the {MAX_FRAME_VERTICES} a frame may draw"
            ),
        }
    }
}

impl Error for SequenceError {}

#[cfg(test)]
mod tests;
