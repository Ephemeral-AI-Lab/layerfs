//! Differential corpus: candidate decoder vs the reference decoder.
use layerfs_content::object::codec::{decode_bytes_object, encode_bytes_object, HEADER_LEN};

const MIB: usize = 1024 * 1024;

/// Builds a canonical envelope by hand, bypassing the encoder.
fn build(kind: u8, payload_len: u32, value_len: u32, body_len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(HEADER_LEN + 4 + body_len);
    v.extend_from_slice(b"LFSO");
    v.push(kind);
    v.extend_from_slice(&payload_len.to_be_bytes());
    v.extend_from_slice(&value_len.to_be_bytes());
    v.extend(std::iter::repeat_n(0x5au8, body_len));
    v
}

fn category(r: Result<&[u8], layerfs_content::ContentError>) -> String {
    match r {
        Ok(v) => format!("ok:{}", v.len()),
        Err(layerfs_content::ContentError::ObjectLimitExceeded { limit, actual }) => {
            format!("limit(limit={limit},actual={actual})")
        }
        Err(layerfs_content::ContentError::UnexpectedEof) => "eof".into(),
        Err(layerfs_content::ContentError::UnsupportedFraming) => "framing".into(),
        Err(layerfs_content::ContentError::WrongLogicalRole) => "role".into(),
        Err(layerfs_content::ContentError::TrailingBytes) => "trailing".into(),
        Err(layerfs_content::ContentError::LengthOverflow) => "overflow".into(),
        Err(other) => format!("other:{other}"),
    }
}

fn probe(label: &str, bytes: &[u8]) {
    println!("{label:44} {}", category(decode_bytes_object(bytes)));
}

fn main() {
    println!("--- encoder (candidate only; reference refuses here) ---");
    for n in [0usize, 1, 4096, 8 * MIB, 8 * MIB + 1, 12 * MIB] {
        let v = vec![0x5au8; n];
        let line = match encode_bytes_object(&v) {
            Ok(c) => format!("ok:{}", c.len()),
            Err(e) => format!("{e}"),
        };
        println!("{:44} {line}", format!("encode value_len={n}"));
    }

    println!("--- decoder corpus ---");
    // Valid objects, encoded by the candidate encoder.
    for n in [0usize, 1, 4096, 8 * MIB] {
        let v = vec![0x5au8; n];
        let c = encode_bytes_object(&v).unwrap();
        probe(&format!("valid value_len={n}"), &c);
        let mut short = c.clone();
        short.pop();
        probe(&format!("valid value_len={n}, truncated 1"), &short);
        let mut long = c.clone();
        long.push(0);
        probe(&format!("valid value_len={n}, trailing 1"), &long);
        let mut bad_magic = c.clone();
        bad_magic[0] = b'X';
        probe(&format!("value_len={n}, wrong magic"), &bad_magic);
        let mut bad_kind = c.clone();
        bad_kind[4] = 9;
        probe(&format!("value_len={n}, kind=9"), &bad_kind);
    }
    // Hand-built length edge cases.
    probe("hand: payload=0 value=0 body=0", &build(1, 0, 0, 0));
    probe("hand: payload=3 value=0 body=0", &build(1, 3, 0, 0));
    probe("hand: payload=4 value=0 body=0", &build(1, 4, 0, 0));
    probe("hand: payload=4 value=1 body=1", &build(1, 4, 1, 1));
    probe("hand: payload=5 value=1 body=1", &build(1, 5, 1, 1));
    probe("hand: value_len=8MiB+1, payload legal", &build(1, (8 * MIB + 1 + 4) as u32, (8 * MIB + 1) as u32, 0));
    probe("hand: value_len=envelope max 16777203", &build(1, 16_777_207, 16_777_203, 0));
    probe("hand: value_len=16777204 (envelope illegal)", &build(1, 16_777_208, 16_777_204, 0));
    probe("hand: payload=u32::MAX", &build(1, u32::MAX, 0, 0));
    probe("hand: value_len=u32::MAX", &build(1, u32::MAX, u32::MAX, 0));
    probe("hand: payload=8 value_len=8 (mismatch)", &build(1, 8, 8, 0));
}
