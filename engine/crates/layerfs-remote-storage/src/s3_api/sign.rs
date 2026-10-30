//! Header-based SigV4 with a SHA-256 payload. Revived from the retired
//! `layerfs-s3` signer, with a canonical query string added for multipart.
use crate::error::RemoteError;
use crate::pieces::hex;
use crate::s3_api::config::S3Config;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn hmac(key: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut block = [0_u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&hash(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(bytes);
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// `YYYYMMDDTHHMMSSZ` for now, in UTC.
fn timestamp() -> Result<String, RemoteError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RemoteError::Invalid("clock before 1970".to_owned()))?
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
    Err(RemoteError::Invalid("clock beyond 9999".to_owned()))
}

/// RFC 3986 encoding of everything but unreserved characters.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The canonical query string, also used verbatim on the request line.
pub(crate) fn query_string(query: &[(&str, String)]) -> String {
    let mut pairs: Vec<(String, String)> =
        query.iter().map(|(k, v)| (encode(k), encode(v))).collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Adds `host`, `x-amz-content-sha256`, `x-amz-date` and `authorization`
/// to `headers` (lowercase names), signing every header in it. `path` is
/// already made of unreserved characters and slashes.
pub(crate) fn authorize(
    config: &S3Config,
    method: &str,
    path: &str,
    query: &str,
    headers: &mut BTreeMap<String, String>,
    payload: &[u8; 32],
) -> Result<(), RemoteError> {
    let (_, _, host, _) = config.address()?;
    let stamp = timestamp()?;
    let date = &stamp[..8];
    headers.insert("host".to_owned(), host);
    headers.insert("x-amz-content-sha256".to_owned(), hex(payload));
    headers.insert("x-amz-date".to_owned(), stamp.clone());
    let canonical = headers
        .iter()
        .fold(String::new(), |mut text, (name, value)| {
            text.push_str(&format!("{name}:{}\n", value.trim()));
            text
        });
    let names = headers.keys().cloned().collect::<Vec<_>>().join(";");
    let scope = format!("{date}/{}/s3/aws4_request", config.region);
    let request = format!(
        "{method}\n{path}\n{query}\n{canonical}\n{names}\n{}",
        hex(payload)
    );
    let message = format!(
        "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
        hex(&hash(request.as_bytes()))
    );
    let secret = &config.credentials.secret_key;
    let mut key = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    for part in [config.region.as_str(), "s3", "aws4_request"] {
        key = hmac(&key, part.as_bytes());
    }
    headers.insert(
        "authorization".to_owned(),
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={names}, Signature={}",
            config.credentials.access_key,
            hex(&hmac(&key, message.as_bytes()))
        ),
    );
    Ok(())
}
