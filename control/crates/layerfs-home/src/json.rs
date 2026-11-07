//! JSON text for call values: the shell's stdout and stderr, the worker's
//! frames and the Python and Node front ends' arguments all use it.
//!
//! Numbers are whole (`SdkValue::Int`); a fraction or exponent is refused.
//! Bytes print as a string when they are UTF-8, else as
//! `{"base64": "..."}`, which parses back to bytes. A worker frame with
//! attachments prints each byte value as `{"attachment": i}` instead
//! ([`write_attached`]); the frame carries the bytes themselves.
use layerfs_bridge::control::SdkValue;
use std::fmt::Write as _;

/// Why JSON text could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError(pub String);

/// Reads one JSON value; anything after it but whitespace is refused.
pub fn parse(text: &str) -> Result<SdkValue, JsonError> {
    let mut reader = Reader {
        text,
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
    let mut out = String::with_capacity(size(value, false));
    write_value(&mut out, value, &mut Form::Print);
    out
}

/// Appends `value` to `out` as [`print`] would, except that every byte
/// value prints as `{"base64": ...}`, even UTF-8 bytes, so it keeps its
/// type (the worker frame's form).
pub fn write_typed(out: &mut String, value: &SdkValue) {
    out.reserve(size(value, false));
    write_value(out, value, &mut Form::Typed);
}

/// Appends `value` to `out` as [`write_typed`] would, except that every
/// byte value prints as `{"attachment": i}` and is added to `attachments`,
/// `i` its place there (a worker frame with attachments).
pub fn write_attached<'v>(out: &mut String, value: &'v SdkValue, attachments: &mut Vec<&'v [u8]>) {
    out.reserve(size(value, true));
    write_value(out, value, &mut Form::Attached(attachments));
}

/// How byte values print.
enum Form<'a, 'v> {
    /// As text when they are UTF-8, else as base64.
    Print,
    /// Always as base64.
    Typed,
    /// As `{"attachment": i}`, the bytes added to the list.
    Attached(&'a mut Vec<&'v [u8]>),
}

/// About how long `value` prints, to reserve once.
fn size(value: &SdkValue, attached: bool) -> usize {
    match value {
        SdkValue::Null | SdkValue::Bool(_) | SdkValue::Int(_) => 20,
        SdkValue::Text(s) => s.len() + 2,
        SdkValue::Bytes(_) if attached => 36,
        SdkValue::Bytes(b) => b.len().div_ceil(3) * 4 + 13,
        SdkValue::List(items) => {
            let each = items.iter().map(|item| size(item, attached) + 1);
            each.sum::<usize>() + 2
        }
        SdkValue::Map(fields) => {
            let each = fields
                .iter()
                .map(|(name, item)| name.len() + 4 + size(item, attached));
            each.sum::<usize>() + 2
        }
    }
}

fn write_value<'v>(out: &mut String, value: &'v SdkValue, form: &mut Form<'_, 'v>) {
    match value {
        SdkValue::Null => out.push_str("null"),
        SdkValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        SdkValue::Int(n) => {
            let _ = write!(out, "{n}");
        }
        SdkValue::Text(s) => quote(out, s),
        SdkValue::Bytes(b) => match (form, std::str::from_utf8(b)) {
            (Form::Attached(attachments), _) => {
                let _ = write!(out, "{{\"attachment\":{}}}", attachments.len());
                attachments.push(b);
            }
            (Form::Print, Ok(s)) => quote(out, s),
            _ => {
                // base64 text needs no escaping
                out.push_str("{\"base64\":\"");
                push_base64(out, b);
                out.push_str("\"}");
            }
        },
        SdkValue::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, item, form);
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
                write_value(out, item, form);
            }
            out.push('}');
        }
    }
}

/// `s` quoted: `"`, `\`, `\n`, `\r` and `\t` escaped as such, any other
/// control character as `\u00xx`, everything else as is. The runs between
/// escapes are copied whole; every escaped byte is ASCII, so each run ends
/// on a character boundary.
fn quote(out: &mut String, s: &str) {
    out.reserve(s.len() + 2);
    out.push('"');
    let mut run = 0;
    for (at, b) in s.bytes().enumerate() {
        let escape = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0..=0x1f => "",
            _ => continue,
        };
        out.push_str(&s[run..at]);
        if escape.is_empty() {
            let _ = write!(out, "\\u{b:04x}");
        } else {
            out.push_str(escape);
        }
        run = at + 1;
    }
    out.push_str(&s[run..]);
    out.push('"');
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Each byte's place in [`ALPHABET`]; `INVALID` for a byte not in it.
const SEXTET: [u8; 256] = {
    let mut table = [INVALID; 256];
    let mut i = 0;
    while i < 64 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};
const INVALID: u8 = 0xff;

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    push_base64(&mut out, bytes);
    out
}

/// Appends `bytes` to `out` as standard base64 with padding, a block of
/// 3072 bytes (4096 characters) at a time.
fn push_base64(out: &mut String, bytes: &[u8]) {
    out.reserve(bytes.len().div_ceil(3) * 4);
    let mut block = [0u8; 4096];
    let char_of = |n: u32, shift: u32| ALPHABET[((n >> shift) & 63) as usize];
    for chunk in bytes.chunks(3072) {
        let mut triples = chunk.chunks_exact(3);
        let mut len = 0;
        for (three, four) in (&mut triples).zip(block.chunks_exact_mut(4)) {
            let n = u32::from_be_bytes([0, three[0], three[1], three[2]]);
            four.copy_from_slice(&[char_of(n, 18), char_of(n, 12), char_of(n, 6), char_of(n, 0)]);
            len += 4;
        }
        // only the last chunk has a remainder: 3072 is a multiple of 3
        let rest = triples.remainder();
        if !rest.is_empty() {
            let n = rest
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
            for (i, c) in block[len..len + 4].iter_mut().enumerate() {
                *c = if i <= rest.len() {
                    char_of(n, 18 - 6 * i as u32)
                } else {
                    b'='
                };
            }
            len += 4;
        }
        // the block is all ASCII
        out.push_str(std::str::from_utf8(&block[..len]).unwrap_or_default());
    }
}

/// Bytes from standard base64; `None` when the text is not base64.
/// Trailing `=` are ignored, as are the bits left over after the last
/// whole byte.
pub fn unbase64(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=').as_bytes();
    let (quads, rest) = text.split_at(text.len() / 4 * 4);
    // a last 2 or 3 characters hold 1 or 2 bytes; a lone one holds none
    let tail = rest.len().saturating_sub(1);
    let mut out = vec![0u8; quads.len() / 4 * 3 + tail];
    let mut bad = 0u8;
    for (four, three) in quads.chunks_exact(4).zip(out.chunks_exact_mut(3)) {
        let [a, b, c, d] = [four[0], four[1], four[2], four[3]].map(|x| SEXTET[usize::from(x)]);
        bad |= a | b | c | d;
        let n = (u32::from(a) << 18) | (u32::from(b) << 12) | (u32::from(c) << 6) | u32::from(d);
        three.copy_from_slice(&n.to_be_bytes()[1..]);
    }
    let mut n = 0u32;
    for (i, x) in rest.iter().enumerate() {
        let v = SEXTET[usize::from(*x)];
        bad |= v;
        n |= u32::from(v & 63) << (18 - 6 * i);
    }
    // every place is below 64; only INVALID has the top bit
    if bad & 0x80 != 0 {
        return None;
    }
    let start = out.len() - tail;
    out[start..].copy_from_slice(&n.to_be_bytes()[1..=tail]);
    Some(out)
}

/// Nesting deeper than this is refused rather than recursed into.
const MAX_DEPTH: usize = 128;

struct Reader<'a> {
    text: &'a str,
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

    /// A string's runs between escapes are copied whole: each ends at an
    /// ASCII byte, so on a character boundary of the UTF-8 text.
    fn string(&mut self) -> Result<String, JsonError> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let run = self.at;
            let special = |b: &u8| matches!(b, b'"' | b'\\' | 0..=0x1f);
            self.at = match self.bytes[run..].iter().position(special) {
                Some(len) => run + len,
                None => self.bytes.len(),
            };
            out.push_str(&self.text[run..self.at]);
            match self.bytes.get(self.at) {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
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
                    out.push(c);
                }
                Some(_) => return Err(self.error("control character in string")),
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
