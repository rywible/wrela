#[derive(Debug, Clone)]
pub struct BoundedCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> BoundedCursor<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    pub fn take_u8(&mut self) -> Option<u8> {
        let out = self.bytes.get(self.pos).copied()?;
        self.pos += 1;
        Some(out)
    }

    pub fn take_u16(&mut self) -> Option<u16> {
        let lo = self.take_u8()?;
        let hi = self.take_u8()?;
        Some(u16::from_be_bytes([lo, hi]))
    }

    pub fn take_u64(&mut self) -> Option<u64> {
        let mut out = [0u8; 8];
        for byte in &mut out {
            *byte = self.take_u8()?;
        }
        Some(u64::from_be_bytes(out))
    }

    pub fn take_vec_bounded(&mut self, max_len: usize) -> Vec<u8> {
        let wire_len = self.take_u16().unwrap_or(0) as usize;
        let capped_len = wire_len.min(max_len).min(self.remaining());
        if capped_len == 0 {
            return Vec::new();
        }
        let start = self.pos;
        self.pos += capped_len;
        self.bytes[start..start + capped_len].to_vec()
    }
}

pub fn cap_input(bytes: &[u8], max_len: usize) -> &[u8] {
    let keep = bytes.len().min(max_len);
    &bytes[..keep]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_vec_bounded_caps_wire_len_and_remaining_bytes() {
        let bytes = [0x00, 0x0a, b'a', b'b', b'c'];
        let mut cursor = BoundedCursor::new(&bytes);
        let out = cursor.take_vec_bounded(2);
        assert_eq!(out, b"ab".to_vec());
        assert_eq!(cursor.remaining(), 1);
    }

    #[test]
    fn cap_input_never_returns_more_than_limit() {
        let bytes = [1u8, 2, 3, 4];
        assert_eq!(cap_input(&bytes, 2), &[1u8, 2]);
        assert_eq!(cap_input(&bytes, 9), &bytes);
    }
}
