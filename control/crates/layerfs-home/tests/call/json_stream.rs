//! The streaming JSON reader reads exactly what `parse` reads, with the
//! same refusals, through any buffer size, and leaves what follows the
//! value unread.
use layerfs_home::json::{parse, print, read_value};
use std::io::{BufReader, Read};

fn streamed(text: &[u8], capacity: usize) -> (Result<String, String>, Vec<u8>) {
    let mut input = BufReader::with_capacity(capacity, text);
    let value = read_value(&mut input)
        .map(|value| print(&value))
        .map_err(|error| error.0);
    let mut rest = Vec::new();
    input.read_to_end(&mut rest).unwrap();
    (value, rest)
}

const VALID: &[&str] = &[
    "null",
    " true ",
    "false",
    "0",
    "-42",
    "9223372036854775807",
    r#""""#,
    r#""plain text""#,
    r#""esc \" \\ \/ \b \f \n \r \t é 😀""#,
    "[]",
    "[1, [2, [3]], {}]",
    r#"{"a": 1, "b": [true, null], "c": {"d": "e"}}"#,
    r#"{"op":"write","path":"/x","content":"x\n"}"#,
    r#"{"base64":"aGVsbG8="}"#,
];

const INVALID: &[&str] = &[
    "",
    "nul",
    "1.5",
    "1e3",
    "99999999999999999999",
    r#""unterminated"#,
    "\"control \u{1} char\"",
    r#""bad \x escape""#,
    r#""lone \ud83d surrogate""#,
    r#""\u12""#,
    "[1 2]",
    r#"{"a" 1}"#,
    r#"{"a": 1, "a": 2}"#,
    "{1: 2}",
    "@",
];

#[test]
fn every_value_reads_as_parse_reads_it_through_any_buffer() {
    for text in VALID {
        let expected = print(&parse(text.trim()).unwrap());
        for capacity in [1, 2, 3, 7, 64, 64 << 10] {
            let (value, rest) = streamed(text.as_bytes(), capacity);
            assert_eq!(
                value.as_deref(),
                Ok(expected.as_str()),
                "{text} at {capacity}"
            );
            // only blanks after the value stay unread
            assert!(rest.iter().all(u8::is_ascii_whitespace), "{text}");
        }
    }
}

#[test]
fn what_parse_refuses_is_refused() {
    for text in INVALID {
        assert!(parse(text).is_err(), "parse took {text}");
        for capacity in [1, 5, 64 << 10] {
            assert!(
                streamed(text.as_bytes(), capacity).0.is_err(),
                "{text} at {capacity}"
            );
        }
    }
    let deep = "[".repeat(10_000) + &"]".repeat(10_000);
    assert!(parse(&deep).is_err());
    assert!(streamed(deep.as_bytes(), 64).0.is_err());
    // invalid UTF-8 inside a string
    assert!(streamed(b"\"\xff\xfe\"", 4).0.is_err());
}

#[test]
fn the_bytes_after_a_value_stay_unread() {
    let text = br#"{"a":[1,2]} , {"b":2}"#;
    let mut input = BufReader::with_capacity(3, &text[..]);
    let first = read_value(&mut input).unwrap();
    assert_eq!(print(&first), print(&parse(r#"{"a":[1,2]}"#).unwrap()));
    let mut rest = String::new();
    input.read_to_string(&mut rest).unwrap();
    assert_eq!(rest, r#" , {"b":2}"#);
}

#[test]
fn a_large_string_reads_whole_through_a_small_buffer() {
    let body = "z".repeat(3 << 20) + "\\n" + &"é".repeat(1000);
    let text = format!(r#"{{"op":"write","path":"/big","content":"{body}"}}"#);
    let (value, _) = streamed(text.as_bytes(), 4 << 10);
    assert_eq!(value.unwrap(), print(&parse(&text).unwrap()));
}
