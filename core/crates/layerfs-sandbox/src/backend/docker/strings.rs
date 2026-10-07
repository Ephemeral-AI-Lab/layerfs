//! Streaming JSON strings with exact UTF-8 and surrogate handling.
use super::json::Json;
use crate::RuntimeError;
use std::io::{Read, Write};
pub(super) struct Text {
    bytes: [u8; 128],
    len: usize,
    pub overflow: bool,
}
impl Text {
    fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
            overflow: false,
        }
    }
    fn push(&mut self, c: char, capture: bool) {
        if !capture || self.overflow {
            return;
        }
        let mut encoded = [0; 4];
        let bytes = c.encode_utf8(&mut encoded).as_bytes();
        if self.len + bytes.len() > 128 {
            self.overflow = true;
            return;
        }
        self.bytes[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
    }
    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).expect("validated UTF-8")
    }
    pub fn equals(&self, s: &str) -> bool {
        !self.overflow && self.text() == s
    }
}
impl<R: Read> Json<R> {
    pub fn string(&mut self, capture: bool) -> Result<Text, RuntimeError> {
        self.expect(b'"')?;
        let mut text = Text::new();
        loop {
            let byte = self.byte()?;
            let c = match byte {
                b'"' => return Ok(text),
                0..=31 => return Err(RuntimeError::Protocol("JSON unescaped control")),
                b'\\' => match self.byte()? {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'b' => '\x08',
                    b'f' => '\x0c',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'u' => {
                        let first = self.hex4()?;
                        let scalar = if (0xd800..=0xdbff).contains(&first) {
                            self.expect(b'\\')?;
                            self.expect(b'u')?;
                            let second = self.hex4()?;
                            if !(0xdc00..=0xdfff).contains(&second) {
                                return Err(RuntimeError::Protocol("JSON surrogate pair"));
                            }
                            0x10000
                                + ((u32::from(first) - 0xd800) << 10)
                                + (u32::from(second) - 0xdc00)
                        } else {
                            u32::from(first)
                        };
                        char::from_u32(scalar)
                            .ok_or(RuntimeError::Protocol("JSON surrogate/scalar"))?
                    }
                    _ => return Err(RuntimeError::Protocol("JSON escape")),
                },
                32..=127 => char::from(byte),
                _ => {
                    let width = match byte {
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf4 => 4,
                        _ => return Err(RuntimeError::Protocol("JSON UTF-8 prefix")),
                    };
                    let mut bytes = [0; 4];
                    bytes[0] = byte;
                    for b in &mut bytes[1..width] {
                        *b = self.byte()?;
                    }
                    std::str::from_utf8(&bytes[..width])
                        .map_err(|_| RuntimeError::Protocol("JSON UTF-8"))?
                        .chars()
                        .next()
                        .expect("one validated scalar")
                }
            };
            text.push(c, capture);
        }
    }
    fn hex4(&mut self) -> Result<u16, RuntimeError> {
        let mut n = 0;
        for _ in 0..4 {
            let b = self.byte()?;
            let v = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(RuntimeError::Protocol("JSON unicode digit")),
            };
            n = n * 16 + u16::from(v);
        }
        Ok(n)
    }
}
pub(super) fn quoted(out: &mut (impl Write + ?Sized), value: &str) -> std::io::Result<()> {
    out.write_all(b"\"")?;
    for c in value.chars() {
        match c {
            '"' => out.write_all(b"\\\"")?,
            '\\' => out.write_all(b"\\\\")?,
            '\n' => out.write_all(b"\\n")?,
            '\r' => out.write_all(b"\\r")?,
            '\t' => out.write_all(b"\\t")?,
            c if c < ' ' => {
                let v = c as u8;
                let hex = b"0123456789abcdef";
                out.write_all(&[
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    hex[(v >> 4) as usize],
                    hex[(v & 15) as usize],
                ])?
            }
            c => {
                let mut bytes = [0; 4];
                out.write_all(c.encode_utf8(&mut bytes).as_bytes())?
            }
        }
    }
    out.write_all(b"\"")
}
