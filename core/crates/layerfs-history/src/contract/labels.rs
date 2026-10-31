//! Canonical JSON storage for a Commit's bounded free-form labels.
use crate::error::{HistoryError, HistoryResult};

/// Encodes labels as one JSON array, preserving order and duplicates.
pub fn encode(labels: &[String]) -> String {
    let mut out = String::from("[");
    for (index, label) in labels.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push('"');
        for c in label.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0c}' => out.push_str("\\f"),
                c if c.is_control() => {
                    use std::fmt::Write as _;
                    write!(out, "\\u{:04x}", c as u32).expect("String write");
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

/// Decodes one stored JSON array of strings. Malformed JSON is an integrity failure.
pub fn decode(text: &str) -> HistoryResult<Vec<String>> {
    let mut parser = Parser { text, at: 0 };
    let labels = parser.array()?;
    parser.space();
    if parser.at == text.len() {
        Ok(labels)
    } else {
        Err(malformed())
    }
}

fn malformed() -> HistoryError {
    HistoryError::Integrity("Commit labels JSON")
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn array(&mut self) -> HistoryResult<Vec<String>> {
        self.space();
        self.byte(b'[')?;
        self.space();
        let mut labels = Vec::new();
        if self.take(b']') {
            return Ok(labels);
        }
        loop {
            labels.push(self.string()?);
            self.space();
            if self.take(b']') {
                return Ok(labels);
            }
            self.byte(b',')?;
        }
    }

    fn string(&mut self) -> HistoryResult<String> {
        self.space();
        self.byte(b'"')?;
        let mut out = String::new();
        loop {
            let Some(rest) = self.text.get(self.at..) else {
                return Err(malformed());
            };
            let c = rest.chars().next().ok_or_else(malformed)?;
            self.at += c.len_utf8();
            match c {
                '"' => return Ok(out),
                '\\' => self.escape(&mut out)?,
                c if (c as u32) < 0x20 => return Err(malformed()),
                c => out.push(c),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> HistoryResult<()> {
        let c = self.char()?;
        match c {
            '"' | '\\' | '/' => out.push(c),
            'b' => out.push('\u{08}'),
            'f' => out.push('\u{0c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let first = self.hex()?;
                let scalar = if (0xd800..0xdc00).contains(&first) {
                    self.byte(b'\\')?;
                    self.byte(b'u')?;
                    let second = self.hex()?;
                    if !(0xdc00..0xe000).contains(&second) {
                        return Err(malformed());
                    }
                    0x10000 + (((first - 0xd800) as u32) << 10) + (second - 0xdc00) as u32
                } else if (0xdc00..0xe000).contains(&first) {
                    return Err(malformed());
                } else {
                    first as u32
                };
                out.push(char::from_u32(scalar).ok_or_else(malformed)?);
            }
            _ => return Err(malformed()),
        }
        Ok(())
    }

    fn hex(&mut self) -> HistoryResult<u16> {
        let mut value = 0u16;
        for _ in 0..4 {
            let digit = self.char()?.to_digit(16).ok_or_else(malformed)?;
            value = (value << 4) | digit as u16;
        }
        Ok(value)
    }

    fn space(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
    }

    fn char(&mut self) -> HistoryResult<char> {
        let c = self
            .text
            .get(self.at..)
            .and_then(|rest| rest.chars().next())
            .ok_or_else(malformed)?;
        self.at += c.len_utf8();
        Ok(c)
    }

    fn byte(&mut self, expected: u8) -> HistoryResult<()> {
        if self.take(expected) {
            Ok(())
        } else {
            Err(malformed())
        }
    }

    fn take(&mut self, expected: u8) -> bool {
        if self.text.as_bytes().get(self.at) == Some(&expected) {
            self.at += 1;
            true
        } else {
            false
        }
    }
}
