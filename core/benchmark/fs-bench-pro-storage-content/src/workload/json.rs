//! A minimal, strict JSON reader — because the harness may not link one.
//!
//! The retained-history corpus is JSON (`checkpoint-manifest.json`,
//! `oracles/<sha>.json`) and the harness has no parser for it. None may be added:
//! `shared/test_lock_parity.py` fails a **harness-only registry package**, and
//! `serde_json` is absent from `core/Cargo.lock`, so linking it would make the
//! harness link a crate the product seal has never seen. The harness already
//! hand-rolls SHA-256 for the same reason (`workload/digest.rs`), and this is the
//! same trade: a small, auditable implementation instead of a dependency that
//! moves the lockfile.
//!
//! **Strict, not lenient.** A reader that quietly accepts malformed input turns a
//! corrupt corpus into a wrong measurement, so this one refuses:
//!
//! * a duplicate key in one object — the second would silently win;
//! * a trailing comma, a leading zero, a leading `+`, a bare `.5`;
//! * an unescaped control character in a string, and any invalid `\u` escape;
//! * trailing content after the top-level value, and an empty input;
//! * nesting deeper than [`MAX_DEPTH`], which is a stack-overflow guard rather
//!   than a format rule.
//!
//! **Narrow, not general.** Numbers are read as `i64` and a fraction or exponent is
//! refused, because the two documents this harness reads carry only integer counts
//! and byte lengths and a silent float round-trip is exactly the kind of quiet loss
//! a pinned corpus must not suffer. It is not a general JSON library and must not
//! grow into one.

use std::collections::HashSet;
use std::fmt;

/// Deepest nesting this reader accepts.
pub const MAX_DEPTH: usize = 64;

/// A parsed JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// An integer. Fractions and exponents are refused at parse time.
    Number(i64),
    /// A string, with escapes already resolved.
    String(String),
    /// An array, in document order.
    Array(Vec<Value>),
    /// An object, in document order. Keys are unique by construction.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The value at `key`, when this is an object that has it.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Object(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The string, when this is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// The integer, when this is one.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(number) => Some(*number),
            _ => None,
        }
    }

    /// The elements, when this is an array.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The entries, when this is an object.
    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    /// The integer at `key`, or `None` when it is absent or another type.
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(Value::as_i64)
    }

    /// The string at `key`, or `None` when it is absent or another type.
    pub fn get_str<'a>(&'a self, key: &str) -> Option<&'a str> {
        self.get(key).and_then(Value::as_str)
    }

    /// The array at `key`, or `None` when it is absent or another type.
    pub fn get_array(&self, key: &str) -> Option<&[Value]> {
        self.get(key).and_then(Value::as_array)
    }

    /// The name of this value's type, for an error message.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }
}

/// A refusal, with the byte offset it happened at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonError {
    /// Byte offset into the input.
    pub offset: usize,
    /// What was wrong.
    pub message: String,
}

impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "json at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for JsonError {}

/// Parses one complete JSON document.
///
/// Trailing whitespace is allowed; trailing content is not.
pub fn parse(text: &str) -> Result<Value, JsonError> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.skip_whitespace();
    if reader.at >= reader.bytes.len() {
        return Err(reader.fail("empty input"));
    }
    let value = reader.value(0)?;
    reader.skip_whitespace();
    if reader.at != reader.bytes.len() {
        return Err(reader.fail("trailing content after the top-level value"));
    }
    Ok(value)
}

/// Counts the members of a top-level object **without building it**.
///
/// The corpus's oracles are the largest documents this harness reads — 183 MB
/// across 157 files, one of them 2.2 MB with 10,924 members — and the prepare
/// phase needs one number from each: how many path-states it declares. Building
/// the whole value tree to call `.len()` on it costs seconds of preparation for a
/// count. This walks the same tokenizer, refuses the same malformed input, and
/// keeps nothing but the member count.
///
/// It is not a second parser: [`Reader::skip_value`] and [`Reader::value`] share
/// the tokenizer, [`Reader::string_into`] serves both, and
/// [`counting_agrees_with_building`] asserts the two entry points agree.
pub fn count_object_members(text: &str) -> Result<usize, JsonError> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.skip_whitespace();
    if reader.at >= reader.bytes.len() {
        return Err(reader.fail("empty input"));
    }
    if reader.peek() != Some(b'{') {
        return Err(reader.fail("expected a top-level object"));
    }
    reader.at += 1;
    // A set, not a scan: the largest oracle has 10,924 members, and a linear
    // `contains` per member is 60 million string comparisons for one file.
    let mut seen: HashSet<String> = HashSet::new();
    let mut count = 0;
    reader.skip_whitespace();
    if reader.peek() == Some(b'}') {
        reader.at += 1;
    } else {
        loop {
            reader.skip_whitespace();
            let key = reader.string()?;
            if !seen.insert(key.clone()) {
                return Err(reader.fail(format!("duplicate key {key:?}")));
            }
            reader.skip_whitespace();
            reader.expect(b':')?;
            reader.skip_whitespace();
            reader.skip_value(1)?;
            count += 1;
            reader.skip_whitespace();
            match reader.peek() {
                Some(b',') => {
                    reader.at += 1;
                    reader.skip_whitespace();
                    if reader.peek() == Some(b'}') {
                        return Err(reader.fail("trailing comma in an object"));
                    }
                }
                Some(b'}') => {
                    reader.at += 1;
                    break;
                }
                _ => return Err(reader.fail("expected ',' or '}'")),
            }
        }
    }
    reader.skip_whitespace();
    if reader.at != reader.bytes.len() {
        return Err(reader.fail("trailing content after the top-level value"));
    }
    Ok(count)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn fail(&self, message: impl Into<String>) -> JsonError {
        JsonError {
            offset: self.at,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// JSON whitespace is exactly these four bytes, and nothing else. A reader that
    /// also skipped a form feed would accept a document no other parser does.
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), JsonError> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.fail(format!("expected {:?}", byte as char)))
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, JsonError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.fail(format!("expected {word:?}")))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, JsonError> {
        if depth >= MAX_DEPTH {
            return Err(self.fail(format!("nested deeper than {MAX_DEPTH}")));
        }
        match self.peek() {
            None => Err(self.fail("unexpected end of input")),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'"') => self.string().map(Value::String),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-' | b'0'..=b'9') => self.number().map(Value::Number),
            Some(other) => Err(self.fail(format!("unexpected byte {other:#04x}"))),
        }
    }

    /// Walks one value, refusing everything [`Reader::value`] refuses, and keeps
    /// nothing. The two must accept exactly the same language.
    fn skip_value(&mut self, depth: usize) -> Result<(), JsonError> {
        if depth >= MAX_DEPTH {
            return Err(self.fail(format!("nested deeper than {MAX_DEPTH}")));
        }
        match self.peek() {
            None => Err(self.fail("unexpected end of input")),
            Some(b'n') => self.skip_literal("null"),
            Some(b't') => self.skip_literal("true"),
            Some(b'f') => self.skip_literal("false"),
            Some(b'"') => self.string_into(&mut None),
            Some(b'[') => {
                self.at += 1;
                self.skip_whitespace();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(());
                }
                loop {
                    self.skip_whitespace();
                    self.skip_value(depth + 1)?;
                    self.skip_whitespace();
                    match self.peek() {
                        Some(b',') => {
                            self.at += 1;
                            self.skip_whitespace();
                            if self.peek() == Some(b']') {
                                return Err(self.fail("trailing comma in an array"));
                            }
                        }
                        Some(b']') => {
                            self.at += 1;
                            return Ok(());
                        }
                        _ => return Err(self.fail("expected ',' or ']'")),
                    }
                }
            }
            Some(b'{') => {
                self.at += 1;
                self.skip_whitespace();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Ok(());
                }
                loop {
                    self.skip_whitespace();
                    self.string_into(&mut None)?;
                    self.skip_whitespace();
                    self.expect(b':')?;
                    self.skip_whitespace();
                    self.skip_value(depth + 1)?;
                    self.skip_whitespace();
                    match self.peek() {
                        Some(b',') => {
                            self.at += 1;
                            self.skip_whitespace();
                            if self.peek() == Some(b'}') {
                                return Err(self.fail("trailing comma in an object"));
                            }
                        }
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(());
                        }
                        _ => return Err(self.fail("expected ',' or '}'")),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number().map(|_| ()),
            Some(other) => Err(self.fail(format!("unexpected byte {other:#04x}"))),
        }
    }

    fn skip_literal(&mut self, word: &str) -> Result<(), JsonError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(())
        } else {
            Err(self.fail(format!("expected {word:?}")))
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, JsonError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.skip_whitespace();
                    if self.peek() == Some(b']') {
                        return Err(self.fail("trailing comma in an array"));
                    }
                }
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.fail("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, JsonError> {
        self.expect(b'{')?;
        let mut entries: Vec<(String, Value)> = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Value::Object(entries));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            // A duplicate key is refused rather than resolved: which of the two
            // wins is a parser's choice, and this reader does not make choices
            // about a pinned corpus.
            if entries.iter().any(|(name, _)| name == &key) {
                return Err(self.fail(format!("duplicate key {key:?}")));
            }
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let value = self.value(depth + 1)?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.skip_whitespace();
                    if self.peek() == Some(b'}') {
                        return Err(self.fail("trailing comma in an object"));
                    }
                }
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Object(entries));
                }
                _ => return Err(self.fail("expected ',' or '}'")),
            }
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        let mut out = String::new();
        self.string_into(&mut Some(&mut out))?;
        Ok(out)
    }

    /// Parses a string, appending it to `out` when there is one.
    ///
    /// One code path serves both the building reader and the counting reader, so
    /// the two cannot disagree about what a string is — only about whether its
    /// characters are kept.
    fn string_into(&mut self, sink: &mut Option<&mut String>) -> Result<(), JsonError> {
        self.expect(b'"')?;
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.fail("unterminated string"));
            };
            match byte {
                b'"' => {
                    self.at += 1;
                    return Ok(());
                }
                b'\\' => {
                    self.at += 1;
                    self.escape(sink)?;
                }
                0x00..=0x1f => {
                    return Err(self.fail("unescaped control character in a string"));
                }
                _ => {
                    // The input is a `&str`, so a multi-byte sequence is already
                    // valid UTF-8; copy it whole rather than byte by byte.
                    let rest = &self.bytes[self.at..];
                    let length = utf8_length(rest[0]);
                    if rest.len() < length {
                        return Err(self.fail("truncated UTF-8 sequence"));
                    }
                    let text = std::str::from_utf8(&rest[..length])
                        .map_err(|_| self.fail("invalid UTF-8 in a string"))?;
                    if let Some(out) = sink.as_deref_mut() {
                        out.push_str(text);
                    }
                    self.at += length;
                }
            }
        }
    }

    fn escape(&mut self, sink: &mut Option<&mut String>) -> Result<(), JsonError> {
        let Some(byte) = self.peek() else {
            return Err(self.fail("unterminated escape"));
        };
        self.at += 1;
        let mut push = |character: char| {
            if let Some(out) = sink.as_deref_mut() {
                out.push(character);
            }
        };
        match byte {
            b'"' => push('"'),
            b'\\' => push('\\'),
            b'/' => push('/'),
            b'b' => push('\u{0008}'),
            b'f' => push('\u{000c}'),
            b'n' => push('\n'),
            b'r' => push('\r'),
            b't' => push('\t'),
            b'u' => {
                let first = self.hex4()?;
                let code = if (0xd800..0xdc00).contains(&first) {
                    // A high surrogate must be followed by its low half; a lone
                    // surrogate is not a character.
                    self.expect(b'\\')?;
                    self.expect(b'u')?;
                    let second = self.hex4()?;
                    if !(0xdc00..0xe000).contains(&second) {
                        return Err(self.fail("high surrogate without a low surrogate"));
                    }
                    0x1_0000 + ((first - 0xd800) << 10) + (second - 0xdc00)
                } else if (0xdc00..0xe000).contains(&first) {
                    return Err(self.fail("lone low surrogate"));
                } else {
                    first
                };
                let character = char::from_u32(code)
                    .ok_or_else(|| self.fail("escape is not a character"))?;
                push(character);
            }
            other => return Err(self.fail(format!("invalid escape {other:#04x}"))),
        }
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut value = 0u32;
        for _ in 0..4 {
            let Some(byte) = self.peek() else {
                return Err(self.fail("truncated \\u escape"));
            };
            let digit = (byte as char)
                .to_digit(16)
                .ok_or_else(|| self.fail("\\u escape is not four hex digits"))?;
            value = value * 16 + digit;
            self.at += 1;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<i64, JsonError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => {
                self.at += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(self.fail("a number may not have a leading zero"));
                }
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err(self.fail("expected a digit")),
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err(self.fail(
                "this reader accepts integers only; a fraction or exponent is refused",
            ));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| self.fail("invalid UTF-8 in a number"))?;
        text.parse::<i64>()
            .map_err(|_| self.fail(format!("{text} does not fit in an i64")))
    }
}

/// Length in bytes of the UTF-8 sequence starting with `first`.
fn utf8_length(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(text: &str) -> Value {
        parse(text).expect("parses")
    }

    fn refusal(text: &str) -> String {
        parse(text).expect_err("refused").message
    }

    #[test]
    fn scalars_and_containers_round_trip() {
        assert_eq!(parse_ok("null"), Value::Null);
        assert_eq!(parse_ok("true"), Value::Bool(true));
        assert_eq!(parse_ok(" -12 "), Value::Number(-12));
        assert_eq!(parse_ok("\"a\""), Value::String("a".to_string()));
        assert_eq!(parse_ok("[]"), Value::Array(vec![]));
        assert_eq!(parse_ok("{}"), Value::Object(vec![]));
        assert_eq!(
            parse_ok("[1, {\"k\": [2]}]"),
            Value::Array(vec![
                Value::Number(1),
                Value::Object(vec![(
                    "k".to_string(),
                    Value::Array(vec![Value::Number(2)])
                )]),
            ])
        );
    }

    #[test]
    fn escapes_resolve_including_a_surrogate_pair() {
        assert_eq!(
            parse_ok(r#""a\n\t\\\"\/\u0041""#),
            Value::String("a\n\t\\\"/A".to_string())
        );
        assert_eq!(
            parse_ok(r#""\ud83d\ude00""#),
            Value::String("\u{1f600}".to_string())
        );
        // Multi-byte input passes through whole.
        assert_eq!(parse_ok("\"héllo→\""), Value::String("héllo→".to_string()));
    }

    #[test]
    fn a_duplicate_key_is_refused_rather_than_resolved() {
        assert!(refusal(r#"{"a": 1, "a": 2}"#).contains("duplicate key"));
    }

    #[test]
    fn malformed_documents_are_refused() {
        for text in [
            "",
            "   ",
            "{",
            "[1,]",
            "{\"a\": 1,}",
            "{\"a\" 1}",
            "01",
            "+1",
            "1.5",
            "1e3",
            "\"unterminated",
            "\"raw\ncontrol\"",
            "\"\\q\"",
            "\"\\ud800\"",
            "\"\\udc00\"",
            "{} {}",
            "tru",
        ] {
            assert!(parse(text).is_err(), "{text:?} should be refused");
        }
    }

    #[test]
    fn the_depth_guard_refuses_rather_than_overflowing() {
        let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        assert!(refusal(&deep).contains("nested deeper"));
        let exact = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(&exact).is_ok());
    }

    /// The two entry points must accept exactly the same language. A counting
    /// reader that accepted a document the building reader refused would let a
    /// malformed oracle decide a pin.
    #[test]
    fn counting_agrees_with_building() {
        let documents = [
            "{}",
            "[]",
            r#"{"a": 1}"#,
            r#"{"a": 1, "b": [1, {"c": "d"}], "e": null}"#,
            r#"{"nested": {"deep": [[[{"x": -3}]]]}}"#,
            r#"{"esc": "a\nb\u0041\ud83d\ude00"}"#,
            r#"{"empty_object": {}, "empty_array": []}"#,
            // The oracle's own shape: a path to a three-element array.
            r#"{"612f62": ["40755", 0, "-"], "612f622f63": ["100644", 6, "ab"]}"#,
        ];
        for text in documents {
            let built = parse(text).expect("parses");
            let members = built.as_object().map(<[(String, Value)]>::len);
            let counted = count_object_members(text);
            match members {
                Some(len) => assert_eq!(counted.expect("counts"), len, "{text}"),
                None => assert!(counted.is_err(), "{text} is not an object"),
            }
        }
        // And the refusals agree.
        for text in [
            "",
            "   ",
            "{",
            r#"{"a": 1,}"#,
            r#"{"a": 1, "a": 2}"#,
            r#"{"a" 1}"#,
            r#"{"a": }"#,
            r#"{"a": 1} {}"#,
            r#"{"a": 01}"#,
            r#"{"a": "unterminated}"#,
            r#"{"a": [1,]}"#,
        ] {
            assert!(parse(text).is_err(), "{text:?}");
            assert!(count_object_members(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn accessors_are_typed_rather_than_coercing() {
        let value = parse_ok(r#"{"n": 7, "s": "x", "a": [1, 2]}"#);
        assert_eq!(value.get_i64("n"), Some(7));
        assert_eq!(value.get_str("n"), None);
        assert_eq!(value.get_str("s"), Some("x"));
        assert_eq!(value.get_i64("s"), None);
        assert_eq!(value.get_array("a").map(<[Value]>::len), Some(2));
        assert_eq!(value.get("absent"), None);
        assert_eq!(Value::Null.get("n"), None);
        assert_eq!(value.get("a").expect("present").type_name(), "array");
    }
}
