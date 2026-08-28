use super::super::encoding::hex_digit;
use layerfs_core::CanonicalPath;
use layerfs_mount::workspace::MAX_REQUEST_BYTES;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

pub(super) struct SpliceRequest {
    pub(super) path: CanonicalPath,
    pub(super) path_text: String,
    pub(super) start: u64,
    pub(super) delete_len: u64,
    pub(super) replacement: Vec<u8>,
}

pub(super) const CONTROL_DECODE_Q_BYTES: usize = 2 * MAX_REQUEST_BYTES;
const CONTROL_HEX_CHUNK_BYTES: usize = 64 * 1024;
const CONTROL_PATH_BYTES: usize = 4096;
pub(super) fn read_splice_request(path: &Path) -> Result<SpliceRequest, String> {
    let mut reader = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
    let path_text = read_control_field(&mut reader, "path", CONTROL_PATH_BYTES)?;
    let path = CanonicalPath::from_bytes(path_text.as_bytes())
        .map_err(|_| "invalid canonical control path".to_owned())?;
    let start = read_control_field(&mut reader, "start", 20)?
        .parse()
        .map_err(|_| "invalid control request start".to_owned())?;
    let delete_len = read_control_field(&mut reader, "delete", 20)?
        .parse()
        .map_err(|_| "invalid control request delete".to_owned())?;
    let replacement = read_control_hex(&mut reader)?;
    Ok(SpliceRequest {
        path,
        path_text,
        start,
        delete_len,
        replacement,
    })
}

fn read_control_field(
    reader: &mut impl BufRead,
    name: &str,
    max_value_bytes: usize,
) -> Result<String, String> {
    let prefix = format!("{name}=");
    let limit = prefix.len() + max_value_bytes + 1;
    let mut line = Vec::with_capacity(limit);
    reader
        .by_ref()
        .take(limit as u64)
        .read_until(b'\n', &mut line)
        .map_err(|error| error.to_string())?;
    if line.last() != Some(&b'\n') {
        return Err(format!("control request {name} is missing or too large"));
    }
    line.pop();
    let value = line
        .strip_prefix(prefix.as_bytes())
        .ok_or_else(|| format!("expected control request field {name}"))?;
    String::from_utf8(value.to_vec()).map_err(|_| format!("control request {name} is not UTF-8"))
}

fn read_control_hex(reader: &mut impl Read) -> Result<Vec<u8>, String> {
    const PREFIX: &[u8] = b"replacement_hex=";
    let mut prefix = [0_u8; PREFIX.len()];
    reader
        .read_exact(&mut prefix)
        .map_err(|_| "missing control request replacement_hex".to_owned())?;
    if prefix != PREFIX {
        return Err("expected control request field replacement_hex".to_owned());
    }
    let mut replacement = Vec::new();
    let mut encoded = [0_u8; CONTROL_HEX_CHUNK_BYTES];
    let mut high = None;
    loop {
        let read = reader
            .read(&mut encoded)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        for (index, byte) in encoded[..read].iter().copied().enumerate() {
            if byte == b'\n' {
                if high.is_some() {
                    return Err("replacement_hex must have even length".to_owned());
                }
                if index + 1 != read {
                    return Err("replacement_hex must be the final control field".to_owned());
                }
                let mut trailing = [0_u8; 1];
                if reader
                    .read(&mut trailing)
                    .map_err(|error| error.to_string())?
                    != 0
                {
                    return Err("replacement_hex must be the final control field".to_owned());
                }
                return Ok(replacement);
            }
            let digit = hex_digit(byte)?;
            match high.take() {
                Some(high) => {
                    if replacement.len() == MAX_REQUEST_BYTES {
                        return Err("control replacement exceeds the request limit".to_owned());
                    }
                    replacement.push((high << 4) | digit);
                }
                None => high = Some(digit),
            }
        }
    }
    if high.is_some() {
        return Err("replacement_hex must have even length".to_owned());
    }
    Ok(replacement)
}
