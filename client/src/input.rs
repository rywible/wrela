use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputFrame {
    pub seq: u64,
    pub tick: u64,
    pub buttons: u8,
    pub axis_x: f32,
    pub axis_y: f32,
}

impl InputFrame {
    pub const BYTE_LEN: usize = 26;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputButtons {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

impl Default for InputButtons {
    fn default() -> Self {
        Self {
            up: false,
            down: false,
            left: false,
            right: false,
        }
    }
}

impl InputButtons {
    pub const BUTTON_UP: u8 = 1 << 0;
    pub const BUTTON_DOWN: u8 = 1 << 1;
    pub const BUTTON_LEFT: u8 = 1 << 2;
    pub const BUTTON_RIGHT: u8 = 1 << 3;

    pub fn set_key(&mut self, key: &str, pressed: bool) -> bool {
        match key {
            "ArrowUp" | "w" | "W" => {
                self.up = pressed;
                true
            }
            "ArrowDown" | "s" | "S" => {
                self.down = pressed;
                true
            }
            "ArrowLeft" | "a" | "A" => {
                self.left = pressed;
                true
            }
            "ArrowRight" | "d" | "D" => {
                self.right = pressed;
                true
            }
            _ => false,
        }
    }

    pub fn bitmask(&self) -> u8 {
        let mut mask = 0;
        if self.up {
            mask |= Self::BUTTON_UP;
        }
        if self.down {
            mask |= Self::BUTTON_DOWN;
        }
        if self.left {
            mask |= Self::BUTTON_LEFT;
        }
        if self.right {
            mask |= Self::BUTTON_RIGHT;
        }
        mask
    }

    pub fn axes(&self) -> (f32, f32) {
        let x = (self.right as i8 - self.left as i8) as f32;
        let y = (self.down as i8 - self.up as i8) as f32;

        if x == 0.0 && y == 0.0 {
            return (0.0, 0.0);
        }

        let length = (x * x + y * y).sqrt();
        (x / length, y / length)
    }
}

#[derive(Debug, Clone)]
pub struct InputSequencer {
    next_seq: u64,
}

impl Default for InputSequencer {
    fn default() -> Self {
        Self::new(1)
    }
}

impl InputSequencer {
    pub fn new(start_seq: u64) -> Self {
        Self {
            next_seq: start_seq.max(1),
        }
    }

    pub fn next_frame(&mut self, tick: u64, buttons: &InputButtons) -> InputFrame {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        let (axis_x, axis_y) = buttons.axes();

        InputFrame {
            seq,
            tick,
            buttons: buttons.bitmask(),
            axis_x,
            axis_y,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputBatchError {
    TooManyFrames(usize),
    PayloadTooShort { actual: usize, minimum: usize },
    LengthMismatch { expected: usize, actual: usize },
}

impl fmt::Display for InputBatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyFrames(count) => {
                write!(f, "input batch contains too many frames: {count}")
            }
            Self::PayloadTooShort { actual, minimum } => {
                write!(
                    f,
                    "input batch payload too short: got {actual}, need at least {minimum}"
                )
            }
            Self::LengthMismatch { expected, actual } => {
                write!(
                    f,
                    "input batch payload length mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for InputBatchError {}

pub fn encode_input_batch(frames: &[InputFrame]) -> Result<Vec<u8>, InputBatchError> {
    let frame_count =
        u16::try_from(frames.len()).map_err(|_| InputBatchError::TooManyFrames(frames.len()))?;

    let mut out = Vec::with_capacity(2 + frames.len() * InputFrame::BYTE_LEN);
    out.extend_from_slice(&frame_count.to_le_bytes());

    for frame in frames {
        out.extend_from_slice(&frame.seq.to_le_bytes());
        out.extend_from_slice(&frame.tick.to_le_bytes());
        out.push(frame.buttons);
        out.push(0);
        out.extend_from_slice(&frame.axis_x.to_le_bytes());
        out.extend_from_slice(&frame.axis_y.to_le_bytes());
    }

    Ok(out)
}

pub fn decode_input_batch(payload: &[u8]) -> Result<Vec<InputFrame>, InputBatchError> {
    if payload.len() < 2 {
        return Err(InputBatchError::PayloadTooShort {
            actual: payload.len(),
            minimum: 2,
        });
    }

    let count = u16::from_le_bytes([payload[0], payload[1]]) as usize;
    let expected_len = 2 + count * InputFrame::BYTE_LEN;
    if payload.len() != expected_len {
        return Err(InputBatchError::LengthMismatch {
            expected: expected_len,
            actual: payload.len(),
        });
    }

    let mut out = Vec::with_capacity(count);
    let mut offset = 2;
    for _ in 0..count {
        let seq = u64::from_le_bytes(payload[offset..offset + 8].try_into().expect("seq slice"));
        offset += 8;
        let tick = u64::from_le_bytes(payload[offset..offset + 8].try_into().expect("tick slice"));
        offset += 8;
        let buttons = payload[offset];
        offset += 2;
        let axis_x = f32::from_le_bytes(
            payload[offset..offset + 4]
                .try_into()
                .expect("axis x slice"),
        );
        offset += 4;
        let axis_y = f32::from_le_bytes(
            payload[offset..offset + 4]
                .try_into()
                .expect("axis y slice"),
        );
        offset += 4;

        out.push(InputFrame {
            seq,
            tick,
            buttons,
            axis_x,
            axis_y,
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{
        InputBatchError, InputButtons, InputFrame, InputSequencer, decode_input_batch,
        encode_input_batch,
    };

    #[test]
    fn buttons_map_to_normalized_axes() {
        let mut buttons = InputButtons::default();
        assert_eq!(buttons.axes(), (0.0, 0.0));

        buttons.set_key("w", true);
        assert_eq!(buttons.axes(), (0.0, -1.0));

        buttons.set_key("d", true);
        let (x, y) = buttons.axes();
        assert!((x - 0.70710677).abs() < 1e-5);
        assert!((y + 0.70710677).abs() < 1e-5);
    }

    #[test]
    fn sequencer_increments_sequence() {
        let mut buttons = InputButtons::default();
        buttons.set_key("ArrowLeft", true);

        let mut seq = InputSequencer::new(10);
        let frame_a = seq.next_frame(100, &buttons);
        let frame_b = seq.next_frame(101, &buttons);

        assert_eq!(frame_a.seq, 10);
        assert_eq!(frame_b.seq, 11);
        assert_eq!(
            frame_a.buttons & InputButtons::BUTTON_LEFT,
            InputButtons::BUTTON_LEFT
        );
    }

    #[test]
    fn input_batch_round_trip() {
        let frames = vec![
            InputFrame {
                seq: 1,
                tick: 10,
                buttons: InputButtons::BUTTON_UP,
                axis_x: 0.0,
                axis_y: -1.0,
            },
            InputFrame {
                seq: 2,
                tick: 11,
                buttons: InputButtons::BUTTON_RIGHT,
                axis_x: 1.0,
                axis_y: 0.0,
            },
        ];

        let bytes = encode_input_batch(&frames).expect("encode");
        let decoded = decode_input_batch(&bytes).expect("decode");

        assert_eq!(decoded, frames);
    }

    #[test]
    fn decode_rejects_bad_length() {
        let err = decode_input_batch(&[1, 0, 0]).expect_err("should fail");
        assert!(matches!(err, InputBatchError::LengthMismatch { .. }));
    }
}
