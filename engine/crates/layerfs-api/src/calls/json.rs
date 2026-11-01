//! JSON text for call values: the shell's stdout and stderr, the worker's
//! frames and the Python and Node front ends' arguments all use it.
//!
//! Numbers are whole (`SdkValue::Int`); a fraction or exponent is refused.
//! Bytes print as a string when they are UTF-8, else as
//! `{"base64": "..."}`, which parses back to bytes.
use layerfs_bridge::control::SdkValue;
use std::fmt::Write as _;

/// Why JSON text could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError(pub String);

/// Reads one JSON value; anything after it but whitespace is refused.
pub fn parse(text: &str) -> Result<SdkValue, JsonError> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
        depth: 0,
    };
    let value = reader.value()?;
    reader.space();
    if reader.at != reader.bytes.len() {
        return Err(reader.error("trailing text after the JSON value"));
    }
    Ok(value)
}

/// One JSON line for `value`, with no newline.
pub fn print(value: &SdkValue) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

fn write_value(out: &mut String, value: &SdkValue) {
    match value {
        SdkValue::Null => out.push_str("null"),
        SdkValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        SdkValue::Int(n) => {
            let _ = write!(out, "{n}");
        }
        SdkValue::Text(s) => quote(out, s),
        SdkValue::Bytes(b) => match std::str::from_utf8(b) {
            Ok(s) => quote(out, s),
            Err(_) => {
                out.push_str("{\"base64\":");
                quote(out, &base64(b));
                out.push('}');
            }
        },
        SdkValue::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
        SdkValue::Map(fields) => {
            out.push('{');
            for (i, (name, item)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                quote(out, name);
                out.push(':');
                write_value(out, item);
            }
            out.push('}');
        }
    }
}

fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Bytes from standard base64; `None` when the text is not base64.
pub fn unbase64(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut n, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = ALPHABET.iter().position(|a| *a == c)? as u32;
        n = (n << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Nesting deeper than this is refused rather than recursed into.
const MAX_DEPTH: usize = 128;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    depth: usize,
}

impl Reader<'_> {
    fn error(&self, what: &str) -> JsonError {
        JsonError(format!("{what} at byte {}", self.at))
    }

    fn space(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1;
        }
    }

    fn literal(&mut self, word: &str, value: SdkValue) -> Result<SdkValue, JsonError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("unknown literal"))
        }
    }

    fn value(&mut self) -> Result<SdkValue, JsonError> {
        self.space();
        match self.bytes.get(self.at) {
            None => Err(self.error("unexpected end")),
            Some(b'n') => self.literal("null", SdkValue::Null),
            Some(b't') => self.literal("true", SdkValue::Bool(true)),
            Some(b'f') => self.literal("false", SdkValue::Bool(false)),
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
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b']') {
            self.at += 1;
            return Ok(SdkValue::List(items));
        }
        loop {
            items.push(self.value()?);
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(SdkValue::List(items));
                }
                _ => return Err(self.error("expected , or ]")),
            }
        }
    }

    fn map(&mut self) -> Result<SdkValue, JsonError> {
        self.at += 1;
        let mut fields: Vec<(String, SdkValue)> = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(SdkValue::Map(fields));
        }
        loop {
            self.space();
            if self.bytes.get(self.at) != Some(&b'"') {
                return Err(self.error("expected a field name"));
            }
            let name = self.string()?;
            self.space();
            if self.bytes.get(self.at) != Some(&b':') {
                return Err(self.error("expected :"));
            }
            self.at += 1;
            let value = self.value()?;
            if fields.iter().any(|(n, _)| *n == name) {
                return Err(self.error("duplicate field name"));
            }
            fields.push((name, value));
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(base64_object(fields));
                }
                _ => return Err(self.error("expected , or }")),
            }
        }
    }

    fn number(&mut self) -> Result<SdkValue, JsonError> {
        let start = self.at;
        if self.bytes[self.at] == b'-' {
            self.at += 1;
        }
        while matches!(self.bytes.get(self.at), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        if matches!(self.bytes.get(self.at), Some(b'.' | b'e' | b'E')) {
            return Err(self.error("only whole numbers are accepted"));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or("");
        text.parse::<i64>()
            .map(SdkValue::Int)
            .map_err(|_| self.error("number out of range"))
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or_else(|| self.error("short \\u escape"))?;
        let text = std::str::from_utf8(digits).map_err(|_| self.error("bad \\u escape"))?;
        let n = u32::from_str_radix(text, 16).map_err(|_| self.error("bad \\u escape"))?;
        self.at += 4;
        Ok(n)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.at += 1;
        let mut out = Vec::new();
        loop {
            match self.bytes.get(self.at) {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.at += 1;
                    return String::from_utf8(out).map_err(|_| self.error("string is not UTF-8"));
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escaped = *self
                        .bytes
                        .get(self.at)
                        .ok_or_else(|| self.error("bad escape"))?;
                    self.at += 1;
                    let c = match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode()?,
                        _ => return Err(self.error("bad escape")),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                Some(b) if *b < 0x20 => return Err(self.error("control character in string")),
                Some(b) => {
                    out.push(*b);
                    self.at += 1;
                }
            }
        }
    }

    fn unicode(&mut self) -> Result<char, JsonError> {
        let high = self.hex4()?;
        let code = if (0xd800..0xdc00).contains(&high) {
            if self.bytes.get(self.at..self.at + 2) != Some(b"\\u") {
                return Err(self.error("lone surrogate"));
            }
            self.at += 2;
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

/// `{"base64": "..."}`, the printed form of bytes that are not UTF-8,
/// reads back as bytes.
fn base64_object(fields: Vec<(String, SdkValue)>) -> SdkValue {
    if let [(name, SdkValue::Text(text))] = fields.as_slice() {
        if name == "base64" {
            if let Some(bytes) = unbase64(text) {
                return SdkValue::Bytes(bytes);
            }
        }
    }
    SdkValue::Map(fields)
}
