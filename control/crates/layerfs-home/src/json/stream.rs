//! One JSON value read from a byte stream, built as it is read: no copy of
//! its text is held, only the value (a streamed `file apply` element, on
//! the CLI and on the host worker). It reads exactly what [`super::parse`]
//! reads, with the same refusals, and leaves what follows the value unread.
use super::{base64_object, JsonError, MAX_DEPTH};
use layerfs_bridge::control::SdkValue;
use std::io::BufRead;

/// Reads one JSON value from `input`; the bytes after it stay unread.
pub fn read_value(input: &mut dyn BufRead) -> Result<SdkValue, JsonError> {
    Stream {
        input,
        at: 0,
        depth: 0,
    }
    .value()
}

struct Stream<'a> {
    input: &'a mut dyn BufRead,
    at: u64,
    depth: usize,
}

impl Stream<'_> {
    fn error(&self, what: &str) -> JsonError {
        JsonError(format!("{what} at byte {}", self.at))
    }

    fn peek(&mut self) -> Result<Option<u8>, JsonError> {
        loop {
            match self.input.fill_buf() {
                Ok(buffer) => return Ok(buffer.first().copied()),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(JsonError(format!("cannot read the input: {error}"))),
            }
        }
    }

    fn bump(&mut self) {
        self.input.consume(1);
        self.at += 1;
    }

    fn next(&mut self) -> Result<Option<u8>, JsonError> {
        let byte = self.peek()?;
        if byte.is_some() {
            self.bump();
        }
        Ok(byte)
    }

    fn space(&mut self) -> Result<(), JsonError> {
        while matches!(self.peek()?, Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.bump();
        }
        Ok(())
    }

    fn literal(&mut self, word: &[u8], value: SdkValue) -> Result<SdkValue, JsonError> {
        for expected in word {
            if self.next()? != Some(*expected) {
                return Err(self.error("unknown literal"));
            }
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<SdkValue, JsonError> {
        self.space()?;
        match self.peek()? {
            None => Err(self.error("unexpected end")),
            Some(b'n') => self.literal(b"null", SdkValue::Null),
            Some(b't') => self.literal(b"true", SdkValue::Bool(true)),
            Some(b'f') => self.literal(b"false", SdkValue::Bool(false)),
            Some(b'"') => Ok(SdkValue::Text(self.string()?)),
            Some(b'[') => self.nested(Self::list),
            Some(b'{') => self.nested(Self::map),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
        }
    }

    fn nested(
        &mut self,
        read: fn(&mut Self) -> Result<SdkValue, JsonError>,
    ) -> Result<SdkValue, JsonError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("JSON nested too deeply"));
        }
        let value = read(self);
        self.depth -= 1;
        value
    }

    fn list(&mut self) -> Result<SdkValue, JsonError> {
        self.bump();
        let mut items = Vec::new();
        self.space()?;
        if self.peek()? == Some(b']') {
            self.bump();
            return Ok(SdkValue::List(items));
        }
        loop {
            items.push(self.value()?);
            self.space()?;
            match self.next()? {
                Some(b',') => {}
                Some(b']') => return Ok(SdkValue::List(items)),
                _ => return Err(self.error("expected , or ]")),
            }
        }
    }

    fn map(&mut self) -> Result<SdkValue, JsonError> {
        self.bump();
        let mut fields: Vec<(String, SdkValue)> = Vec::new();
        self.space()?;
        if self.peek()? == Some(b'}') {
            self.bump();
            return Ok(SdkValue::Map(fields));
        }
        loop {
            self.space()?;
            if self.peek()? != Some(b'"') {
                return Err(self.error("expected a field name"));
            }
            let name = self.string()?;
            self.space()?;
            if self.next()? != Some(b':') {
                return Err(self.error("expected :"));
            }
            let value = self.value()?;
            if fields.iter().any(|(n, _)| *n == name) {
                return Err(self.error("duplicate field name"));
            }
            fields.push((name, value));
            self.space()?;
            match self.next()? {
                Some(b',') => {}
                Some(b'}') => return Ok(base64_object(fields)),
                _ => return Err(self.error("expected , or }")),
            }
        }
    }

    fn number(&mut self) -> Result<SdkValue, JsonError> {
        let mut text = String::new();
        if self.peek()? == Some(b'-') {
            self.bump();
            text.push('-');
        }
        while let Some(digit @ b'0'..=b'9') = self.peek()? {
            self.bump();
            text.push(char::from(digit));
        }
        if matches!(self.peek()?, Some(b'.' | b'e' | b'E')) {
            return Err(self.error("only whole numbers are accepted"));
        }
        text.parse::<i64>()
            .map(SdkValue::Int)
            .map_err(|_| self.error("number out of range"))
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut n = 0;
        for _ in 0..4 {
            let digit = self
                .next()?
                .and_then(|byte| char::from(byte).to_digit(16))
                .ok_or_else(|| self.error("bad \\u escape"))?;
            n = n * 16 + digit;
        }
        Ok(n)
    }

    /// The runs between escapes are copied from the input's buffer whole;
    /// the string is checked as UTF-8 once, at its end.
    fn string(&mut self) -> Result<String, JsonError> {
        self.bump();
        let mut out = Vec::new();
        loop {
            let (run, special) = {
                let buffer = loop {
                    match self.input.fill_buf() {
                        Ok(buffer) => break buffer,
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => {
                            return Err(JsonError(format!("cannot read the input: {error}")))
                        }
                    }
                };
                if buffer.is_empty() {
                    (0, Some(0xff))
                } else {
                    let run = buffer
                        .iter()
                        .position(|b| matches!(b, b'"' | b'\\' | 0..=0x1f))
                        .unwrap_or(buffer.len());
                    out.extend_from_slice(&buffer[..run]);
                    (run, buffer.get(run).copied())
                }
            };
            self.input.consume(run);
            self.at += run as u64;
            match special {
                None => {}
                // the input ended inside the string
                Some(0xff) if run == 0 && self.peek()?.is_none() => {
                    return Err(self.error("unterminated string"))
                }
                Some(b'"') => {
                    self.bump();
                    return String::from_utf8(out).map_err(|_| self.error("invalid UTF-8"));
                }
                Some(b'\\') => {
                    self.bump();
                    let c = match self.next()? {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => self.unicode()?,
                        _ => return Err(self.error("bad escape")),
                    };
                    let mut utf8 = [0; 4];
                    out.extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
                }
                Some(_) => return Err(self.error("control character in string")),
            }
        }
    }

    fn unicode(&mut self) -> Result<char, JsonError> {
        let high = self.hex4()?;
        let code = if (0xd800..0xdc00).contains(&high) {
            if self.next()? != Some(b'\\') || self.next()? != Some(b'u') {
                return Err(self.error("lone surrogate"));
            }
            let low = self.hex4()?;
            if !(0xdc00..0xe000).contains(&low) {
                return Err(self.error("lone surrogate"));
            }
            0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00)
        } else {
            high
        };
        char::from_u32(code).ok_or_else(|| self.error("bad \\u escape"))
    }
}
