use super::contract::{EvalResult, Selector};
use super::error::{display_error, io_error};
use std::fs;
use std::path::Path;

pub fn read_selector(store: &Path) -> EvalResult<Selector> {
    let bytes = fs::read(store.join("CURRENT")).map_err(io_error)?;
    if bytes.len() != 154
        || &bytes[..8] != b"LFSCUR1\0"
        || u16::from_be_bytes(bytes[8..10].try_into().unwrap()) != 1
        || u16::from_be_bytes(bytes[18..20].try_into().unwrap()) != 34
    {
        return Err("invalid Store selector framing".to_owned());
    }
    let generation = u64::from_be_bytes(bytes[10..18].try_into().unwrap());
    if std::str::from_utf8(&bytes[20..54]).map_err(display_error)?
        != format!("generation-{generation:016x}.sqlite")
    {
        return Err("Store selector filename mismatch".to_owned());
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/store-current/v1\0");
    hasher.update(&bytes[..122]);
    if hasher.finalize().as_bytes() != &bytes[122..] {
        return Err("Store selector checksum mismatch".to_owned());
    }
    Ok(Selector {
        generation,
        store_id: hex(&bytes[58..90]),
        profile_id: hex(&bytes[90..122]),
    })
}

pub fn selected_database_bytes(store: &Path, generation: u64) -> EvalResult<u64> {
    fs::metadata(store.join(format!("generation-{generation:016x}.sqlite")))
        .map(|metadata| metadata.len())
        .map_err(io_error)
}

pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}
