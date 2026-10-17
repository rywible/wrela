//! A service request's bytes and its answer's: little-endian words, and runs of bytes as a `u32`
//! length then the bytes. Each service's layout is its own (the module that carries it out says
//! it); this is how they're read and written.

/// A request's bytes, read from the front.
pub struct Body<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Body<'a> {
    pub fn new(bytes: &'a [u8]) -> Body<'a> {
        Body { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let Some(end) = end else {
            return Err(format!("the request ends before byte {}", self.at + n));
        };
        let part = &self.bytes[self.at..end];
        self.at = end;
        Ok(part)
    }

    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    pub fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }

    /// A run of bytes: its length, then them.
    pub fn bytes(&mut self) -> Result<&'a [u8], String> {
        let n = self.u32()? as usize;
        self.take(n)
    }

    /// A run of UTF-8.
    pub fn str(&mut self) -> Result<&'a str, String> {
        std::str::from_utf8(self.bytes()?).map_err(|_| "a string that isn't UTF-8".to_string())
    }

    /// What's left.
    pub fn rest(&mut self) -> &'a [u8] {
        let part = &self.bytes[self.at..];
        self.at = self.bytes.len();
        part
    }

    pub fn is_empty(&self) -> bool {
        self.at == self.bytes.len()
    }

    /// Fails unless every byte was read.
    pub fn end(&self) -> Result<(), String> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(format!("{} bytes past the request's end", self.bytes.len() - self.at))
        }
    }
}

/// An answer's bytes, written at the back.
#[derive(Default)]
pub struct Out(pub Vec<u8>);

impl Out {
    pub fn u8(mut self, x: u8) -> Out {
        self.0.push(x);
        self
    }

    pub fn u32(mut self, x: u32) -> Out {
        self.0.extend_from_slice(&x.to_le_bytes());
        self
    }

    pub fn i32(mut self, x: i32) -> Out {
        self.0.extend_from_slice(&x.to_le_bytes());
        self
    }

    pub fn u64(mut self, x: u64) -> Out {
        self.0.extend_from_slice(&x.to_le_bytes());
        self
    }

    /// A run of bytes: its length, then them.
    pub fn bytes(mut self, b: &[u8]) -> Out {
        self = self.u32(b.len() as u32);
        self.0.extend_from_slice(b);
        self
    }

    pub fn str(self, s: &str) -> Out {
        self.bytes(s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::{Body, Out};

    #[test]
    fn a_body_reads_back_what_was_written() {
        let out = Out::default().u8(7).u32(70_000).u64(1 << 40).str("héllo").bytes(&[1, 2]);
        let mut b = Body::new(&out.0);
        assert_eq!(b.u8(), Ok(7));
        assert_eq!(b.u32(), Ok(70_000));
        assert_eq!(b.u64(), Ok(1 << 40));
        assert_eq!(b.str(), Ok("héllo"));
        assert_eq!(b.bytes(), Ok(&[1u8, 2][..]));
        assert!(b.end().is_ok());
    }

    #[test]
    fn a_short_body_says_where_it_ends() {
        let mut b = Body::new(&[3, 0, 0, 0, b'a']);
        assert_eq!(b.bytes(), Err("the request ends before byte 7".to_string()));
        let b = Body::new(&[1]);
        assert_eq!(b.end(), Err("1 bytes past the request's end".to_string()));
    }
}
