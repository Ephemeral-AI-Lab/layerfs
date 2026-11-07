//! Worker frames carry byte values as raw attachments (protocol 3): a
//! frame with none is the plain frame it always was, one with some sets
//! the header's top bit and carries the bytes after its JSON. Malformed or
//! oversized frames are refused from what they claim, before anything of
//! that size is allocated or read.
use layerfs_bridge::control::SdkValue;
use layerfs_home::json;
use layerfs_machine::worker::{
    frame_bytes, read_frame, read_frame_within, ATTACHED, MAX_FRAME, MAX_HELLO,
};
use std::io::Cursor;

fn read(bytes: Vec<u8>) -> Result<Option<SdkValue>, (String, String)> {
    read_frame(&mut Cursor::new(bytes)).map_err(|r| (r.code, r.message))
}

fn round_trip(value: &SdkValue) -> Vec<u8> {
    let bytes = frame_bytes(value).unwrap();
    assert_eq!(read(bytes.clone()).unwrap().as_ref(), Some(value));
    bytes
}

/// An attached frame from its parts: `json`, then each attachment.
fn attached(json: &str, attachments: &[&[u8]]) -> Vec<u8> {
    let mut body = (json.len() as u32).to_be_bytes().to_vec();
    body.extend_from_slice(json.as_bytes());
    body.extend_from_slice(&(attachments.len() as u32).to_be_bytes());
    for attachment in attachments {
        body.extend_from_slice(&(attachment.len() as u64).to_be_bytes());
        body.extend_from_slice(attachment);
    }
    let mut frame = (body.len() as u32 | ATTACHED).to_be_bytes().to_vec();
    frame.extend_from_slice(&body);
    frame
}

fn header(frame: &[u8]) -> u32 {
    u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]])
}

#[test]
fn a_frame_without_bytes_is_the_plain_frame_it_was() {
    let value = SdkValue::map([
        ("secret", SdkValue::text("k")),
        ("n", SdkValue::Int(-7)),
        (
            "list",
            SdkValue::List(vec![SdkValue::Null, SdkValue::Bool(true)]),
        ),
    ]);
    let frame = round_trip(&value);
    let mut text = String::new();
    json::write_typed(&mut text, &value);
    let mut expected = (text.len() as u32).to_be_bytes().to_vec();
    expected.extend_from_slice(text.as_bytes());
    assert_eq!(frame, expected);
    assert_eq!(header(&frame) & ATTACHED, 0);
}

#[test]
fn a_plain_frame_with_base64_bytes_still_reads_as_bytes() {
    let text = r#"{"part":{"base64":"AAEC/w=="},"step":"write"}"#;
    let mut frame = (text.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(text.as_bytes());
    let value = read(frame).unwrap().unwrap();
    assert_eq!(
        value.get("part"),
        Some(&SdkValue::Bytes(vec![0, 1, 2, 255]))
    );
    assert_eq!(value.get_text("step"), Some("write"));
}

#[test]
fn byte_values_travel_as_attachments_in_order() {
    let value = SdkValue::map([
        ("op", SdkValue::text("project.stage")),
        ("part", SdkValue::Bytes(vec![0, 1, 2, 255])),
        // UTF-8 bytes stay bytes
        ("text", SdkValue::Bytes(b"plain".to_vec())),
    ]);
    let frame = round_trip(&value);
    let expected = attached(
        r#"{"op":"project.stage","part":{"attachment":0},"text":{"attachment":1}}"#,
        &[&[0, 1, 2, 255], b"plain"],
    );
    assert_eq!(frame, expected);
    assert_ne!(header(&frame) & ATTACHED, 0);
}

#[test]
fn several_empty_nested_and_large_attachments_round_trip() {
    let large: Vec<u8> = (0..(9u32 << 20) + 5).map(|i| (i * 7 % 256) as u8).collect();
    let value = SdkValue::map([
        ("empty", SdkValue::Bytes(Vec::new())),
        (
            "nested",
            SdkValue::List(vec![
                SdkValue::Bytes(vec![1]),
                SdkValue::map([("deep", SdkValue::Bytes(vec![2, 3]))]),
                SdkValue::text("between"),
            ]),
        ),
        ("large", SdkValue::Bytes(large.clone())),
    ]);
    let frame = round_trip(&value);
    // no base64: the frame is the bytes and a little framing
    assert!(frame.len() < large.len() + 256, "{}", frame.len());
}

#[test]
fn an_attached_frame_with_no_attachments_reads() {
    let value = read(attached(r#"{"value":1}"#, &[])).unwrap().unwrap();
    assert_eq!(value.get_int("value"), Some(1));
}

#[test]
fn an_attachment_must_be_named_exactly_once() {
    let twice = attached(r#"[{"attachment":0},{"attachment":0}]"#, &[b"x"]);
    let unknown = attached(r#"{"a":{"attachment":1}}"#, &[b"x"]);
    let negative = attached(r#"{"a":{"attachment":-1}}"#, &[b"x"]);
    let unnamed = attached(r#"{"a":{"attachment":0}}"#, &[b"x", b"y"]);
    for frame in [twice, unknown, negative, unnamed] {
        let refusal = read(frame).unwrap_err();
        assert_eq!(refusal.0, "Failed", "{}", refusal.1);
    }
}

// Each frame below holds exactly the bytes it claims to: a read past them
// would be `Unavailable`, and an allocation of what a part claims would
// abort, so `Failed` or `TooLarge` shows the refusal came first.
#[test]
fn malformed_attached_frames_are_refused_before_what_they_claim_is_read() {
    let with_body = |body: &[u8]| {
        let mut frame = (body.len() as u32 | ATTACHED).to_be_bytes().to_vec();
        frame.extend_from_slice(body);
        frame
    };
    // a JSON length past the body
    let refusal = read(with_body(&u32::MAX.to_be_bytes())).unwrap_err();
    assert_eq!(refusal.0, "Failed", "{}", refusal.1);
    // more attachments than the body could hold
    let mut body = 2u32.to_be_bytes().to_vec();
    body.extend_from_slice(b"{}");
    body.extend_from_slice(&u32::MAX.to_be_bytes());
    let refusal = read(with_body(&body)).unwrap_err();
    assert_eq!(refusal.0, "Failed", "{}", refusal.1);
    // an attachment claiming far more than the body holds: refused, not
    // allocated
    let mut body = 2u32.to_be_bytes().to_vec();
    body.extend_from_slice(b"{}");
    body.extend_from_slice(&1u32.to_be_bytes());
    body.extend_from_slice(&u64::MAX.to_be_bytes());
    let refusal = read(with_body(&body)).unwrap_err();
    assert_eq!(refusal.0, "Failed", "{}", refusal.1);
    // bytes after the last attachment
    let mut frame = attached("{}", &[]);
    frame.push(0);
    let length = header(&frame) + 1;
    frame[..4].copy_from_slice(&length.to_be_bytes());
    let refusal = read(frame).unwrap_err();
    assert_eq!(refusal.0, "Failed", "{}", refusal.1);
}

#[test]
fn an_attached_frame_longer_than_allowed_is_refused_from_its_header() {
    for claimed in [MAX_FRAME as u32 + 1, !ATTACHED] {
        let header = (claimed | ATTACHED).to_be_bytes().to_vec();
        let refusal = read(header).unwrap_err();
        assert_eq!(refusal.0, "TooLarge", "{}", refusal.1);
    }
    // a first frame is bounded by MAX_HELLO, attachments included
    let frame = attached("{}", &[&vec![0u8; MAX_HELLO]]);
    let refusal = read_frame_within(&mut Cursor::new(frame), MAX_HELLO).unwrap_err();
    assert_eq!(refusal.code, "TooLarge");
}

#[test]
fn a_frame_cut_short_is_unavailable() {
    let frame = attached(r#"{"a":{"attachment":0}}"#, &[b"0123456789"]);
    let cut = frame[..frame.len() - 3].to_vec();
    let refusal = read(cut).unwrap_err();
    assert_eq!(refusal.0, "Unavailable", "{}", refusal.1);
    // a clean end before any byte is no frame
    assert_eq!(read(Vec::new()).unwrap(), None);
}
