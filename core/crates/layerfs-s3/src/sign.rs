//! Header-based SigV4 with a SHA-256 payload and signed operation preconditions.
use crate::config::S3Config;
use layerfs_storage::port::ObjectError;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}
fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn hmac(key: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut block = [0_u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&hash(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = [0_u8; 64];
    let mut outer = [0_u8; 64];
    for i in 0..64 {
        inner[i] = block[i] ^ 0x36;
        outer[i] = block[i] ^ 0x5c;
    }
    let mut digest = Sha256::new();
    digest.update(inner);
    digest.update(bytes);
    let result = digest.finalize();
    let mut digest = Sha256::new();
    digest.update(outer);
    digest.update(result);
    digest.finalize().into()
}
fn timestamp() -> Result<String, ObjectError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ObjectError::Malformed)?
        .as_secs();
    let mut days = seconds / 86400;
    for year in 1970..=9999 {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let count = 365 + u64::from(leap);
        if days >= count {
            days -= count;
            continue;
        }
        let months = [
            31,
            28 + u64::from(leap),
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        for (index, count) in months.into_iter().enumerate() {
            if days >= count {
                days -= count;
                continue;
            }
            return Ok(format!(
                "{year:04}{:02}{:02}T{:02}{:02}{:02}Z",
                index + 1,
                days + 1,
                seconds % 86400 / 3600,
                seconds % 3600 / 60,
                seconds % 60
            ));
        }
    }
    Err(ObjectError::Malformed)
}
pub(crate) fn headers(
    config: &S3Config,
    method: &str,
    path: &str,
    host: &str,
    payload: [u8; 32],
    extra: Option<(&str, String)>,
) -> Result<String, ObjectError> {
    let stamp = timestamp()?;
    let date = &stamp[..8];
    let mut values = BTreeMap::from([
        ("host", host.to_owned()),
        ("x-amz-content-sha256", hex(&payload)),
        ("x-amz-date", stamp.clone()),
    ]);
    if let Some((name, value)) = extra {
        values.insert(name, value);
    }
    let mut canonical = String::new();
    for (name, value) in &values {
        canonical.push_str(name);
        canonical.push(':');
        canonical.push_str(value);
        canonical.push('\n');
    }
    let names = values.keys().copied().collect::<Vec<_>>().join(";");
    let scope = format!("{date}/{}/s3/aws4_request", config.region);
    let request = format!(
        "{method}\n{path}\n\n{canonical}\n{names}\n{}",
        hex(&payload)
    );
    let message = format!(
        "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
        hex(&hash(request.as_bytes()))
    );
    let mut key = hmac(
        format!("AWS4{}", config.secret_key).as_bytes(),
        date.as_bytes(),
    );
    for part in [config.region.as_str(), "s3", "aws4_request"] {
        key = hmac(&key, part.as_bytes());
    }
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={names}, Signature={}",
        config.access_key,
        hex(&hmac(&key, message.as_bytes()))
    );
    let mut output = String::new();
    for (name, value) in &values {
        output.push_str(name);
        output.push_str(": ");
        output.push_str(value);
        output.push_str("\r\n");
    }
    output.push_str(&format!("Authorization: {authorization}\r\n"));
    Ok(output)
}
