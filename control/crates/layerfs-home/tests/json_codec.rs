//! The JSON text codec: base64 and quoted strings print and read back
//! exactly as the character-at-a-time codec before them did (the
//! `oracle` module keeps that codec), so worker frames are unchanged.
use layerfs_bridge::control::SdkValue;
use layerfs_home::json::{base64, parse, print, unbase64, write_typed};

/// The codec as it was before the bulk one, kept as the reference.
mod oracle {
    use layerfs_bridge::control::SdkValue;
    use std::fmt::Write as _;

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn quote(s: &str) -> String {
        let mut out = String::from('"');
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
        out
    }

    pub fn base64(bytes: &[u8]) -> String {
        let mut out = String::new();
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

    pub fn unbase64(text: &str) -> Option<Vec<u8>> {
        let text = text.trim_end_matches('=');
        let mut out = Vec::new();
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

    /// `print` of the worker frame's form: bytes always as base64.
    pub fn print_typed(value: &SdkValue) -> String {
        match value {
            SdkValue::Null => "null".to_owned(),
            SdkValue::Bool(b) => b.to_string(),
            SdkValue::Int(n) => n.to_string(),
            SdkValue::Text(s) => quote(s),
            SdkValue::Bytes(b) => format!("{{\"base64\":{}}}", quote(&base64(b))),
            SdkValue::List(items) => {
                let items: Vec<String> = items.iter().map(print_typed).collect();
                format!("[{}]", items.join(","))
            }
            SdkValue::Map(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(name, item)| format!("{}:{}", quote(name), print_typed(item)))
                    .collect();
                format!("{{{}}}", fields.join(","))
            }
        }
    }
}

/// A fixed xorshift sequence, so every run checks the same inputs.
struct Seq(u64);

impl Seq {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

#[test]
fn base64_matches_the_rfc_4648_vectors() {
    let vectors = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];
    for (plain, coded) in vectors {
        assert_eq!(base64(plain.as_bytes()), coded);
        assert_eq!(unbase64(coded).as_deref(), Some(plain.as_bytes()));
    }
}

#[test]
fn base64_round_trips_as_before_for_every_short_length_and_a_large_buffer() {
    let mut seq = Seq(0x9e37_79b9_7f4a_7c15);
    let mut buffers: Vec<Vec<u8>> = (0..=10).map(|len| seq.bytes(len)).collect();
    buffers.extend((0..=10).map(|len| vec![0xff; len]));
    // past one 3072-byte encoding block, at each remainder
    buffers.extend([3071, 3072, 3073, 3074, 6144].map(|len| seq.bytes(len)));
    buffers.push(seq.bytes((1 << 20) + 1));
    for bytes in buffers {
        let coded = base64(&bytes);
        assert_eq!(coded, oracle::base64(&bytes), "{} bytes", bytes.len());
        assert_eq!(
            unbase64(&coded),
            Some(bytes.clone()),
            "{} bytes",
            bytes.len()
        );
    }
}

#[test]
fn unbase64_accepts_and_refuses_exactly_what_it_did() {
    let cases = [
        "",
        "=",
        "==",
        "====",
        "Z",
        "Z=",
        "Zg",
        "Zg=",
        "Zg==",
        "Zg===",
        "Zh==",
        "Zm9",
        "Zm9v",
        "Zm9vY",
        "Zm9vYg",
        "Zm9vYmE",
        "@",
        "-",
        "_",
        " ",
        "Zm9v YmFy",
        "Zm9v\n",
        " Zm9v",
        "Zm=9v",
        "Zg==Zg==",
        "=Zg",
        "Zm9v\0",
        "é",
        "Zm9vé",
        "Zm9v😀",
        "+/+/",
        "-_-_",
        "AAAA@",
        "AAAAA@",
        "AAAAAA@=",
    ];
    for text in cases {
        assert_eq!(unbase64(text), oracle::unbase64(text), "{text:?}");
    }
    // random text over base64, padding and characters outside it
    let pool: Vec<char> = "ABCXYZabcxyz0189+/====@-_ \n.é😀".chars().collect();
    let mut seq = Seq(7);
    for _ in 0..20_000 {
        let len = seq.below(14);
        let text: String = (0..len).map(|_| pool[seq.below(pool.len())]).collect();
        assert_eq!(unbase64(&text), oracle::unbase64(&text), "{text:?}");
    }
}

/// Text over every ASCII character and some longer ones.
fn texts() -> Vec<String> {
    let mut pool: Vec<char> = (0u8..0x80).map(char::from).collect();
    pool.extend([
        'é',
        '\u{80}',
        '\u{7ff}',
        '€',
        '\u{2028}',
        '\u{feff}',
        '😀',
        '\u{10ffff}',
    ]);
    let mut texts: Vec<String> = pool.iter().map(char::to_string).collect();
    texts.push(pool.iter().collect());
    texts.push("plain text with no escapes at all".to_owned());
    texts.push("\"\\\"\\".to_owned());
    let mut seq = Seq(42);
    for _ in 0..5_000 {
        let len = seq.below(24);
        texts.push((0..len).map(|_| pool[seq.below(pool.len())]).collect());
    }
    texts
}

#[test]
fn strings_print_byte_for_byte_as_before_and_read_back() {
    for text in texts() {
        let printed = print(&SdkValue::text(text.clone()));
        assert_eq!(printed, oracle::quote(&text), "{text:?}");
        assert_eq!(
            parse(&printed),
            Ok(SdkValue::text(text.clone())),
            "{text:?}"
        );
        // UTF-8 bytes print as their text; other bytes as base64
        assert_eq!(print(&SdkValue::Bytes(text.clone().into_bytes())), printed);
    }
    let bytes = vec![0xff, 0, b'"', 0x80];
    let printed = print(&SdkValue::Bytes(bytes.clone()));
    assert_eq!(
        printed,
        format!("{{\"base64\":\"{}\"}}", oracle::base64(&bytes))
    );
    assert_eq!(parse(&printed), Ok(SdkValue::Bytes(bytes)));
}

#[test]
fn the_typed_form_prints_as_before_with_every_byte_value_as_base64() {
    let mut seq = Seq(3);
    let value = SdkValue::map([
        ("op", SdkValue::text("project.stage")),
        (
            "args",
            SdkValue::map([
                ("name", SdkValue::text("web \"x\"\n\u{1}é")),
                ("part", SdkValue::Bytes(seq.bytes(10_000))),
                ("utf8", SdkValue::Bytes(b"plain".to_vec())),
                ("empty", SdkValue::Bytes(Vec::new())),
                (
                    "list",
                    SdkValue::List(vec![
                        SdkValue::Null,
                        SdkValue::Bool(true),
                        SdkValue::Int(-7),
                    ]),
                ),
            ]),
        ),
    ]);
    let mut out = String::from("prefix");
    write_typed(&mut out, &value);
    assert_eq!(out, format!("prefix{}", oracle::print_typed(&value)));
    assert_eq!(parse(&out["prefix".len()..]), Ok(value));
}

#[test]
fn escapes_read_as_before_and_bad_strings_are_refused_where_they_were() {
    let read = |text: &str| parse(text).map_err(|error| error.0);
    let escaped = r#""a\"\\\/\b\f\n\r\t\u00e9\ud83d\ude00z""#;
    let expected = "a\"\\/\u{8}\u{c}\n\r\té😀z";
    assert_eq!(read(escaped), Ok(SdkValue::text(expected)));
    assert_eq!(read("\"é😀\""), Ok(SdkValue::text("é😀")));
    let refused = [
        ("\"ab", "unterminated string at byte 3"),
        ("\"é", "unterminated string at byte 3"),
        ("\"a\u{1}\"", "control character in string at byte 2"),
        ("\"a\nb\"", "control character in string at byte 2"),
        ("\"\\x\"", "bad escape at byte 3"),
        ("\"\\", "bad escape at byte 2"),
        ("\"\\u12zz\"", "bad \\u escape at byte 3"),
        ("\"\\u12\"", "short \\u escape at byte 3"),
        ("\"\\ud83d\"", "lone surrogate at byte 7"),
        ("\"\\ud83d\\u0041\"", "lone surrogate at byte 13"),
        ("\"\\udc00\"", "bad \\u escape at byte 7"),
    ];
    for (text, error) in refused {
        assert_eq!(read(text), Err(error.to_owned()), "{text:?}");
    }
}
