//! S3 multipart upload: create, equal-sized parts, complete; abort on any
//! failure. Each request is sent once. An abort is a new request that
//! discards the upload, not a resend of the failed one.
use crate::driver::{Metadata, Span};
use crate::error::RemoteError;
use crate::s3_api::client::{hash_span, metadata_headers, refused, xml_text, S3Client};
use crate::s3_api::http::Body;
use crate::s3_api::sign;
use std::collections::BTreeMap;
use std::fs::File;

const MIB: u64 = 1024 * 1024;
/// Smallest part (all but the last).
pub const MIN_PART: u64 = 5 * MIB;
/// Largest part.
pub const MAX_PART: u64 = 5 * 1024 * MIB;
/// Most parts in one upload.
pub const MAX_PARTS: u64 = 10_000;

pub(crate) fn check_part(size: u64) -> Result<(), RemoteError> {
    if !(MIN_PART..=MAX_PART).contains(&size) {
        return Err(RemoteError::Invalid(format!(
            "multipart part size {size} outside {MIN_PART}..={MAX_PART}"
        )));
    }
    Ok(())
}

/// The part size for `length` bytes: `configured`, raised to whole MiB
/// when `length` would otherwise need more than `MAX_PARTS` parts.
pub fn part_size(length: u64, configured: u64) -> Result<u64, RemoteError> {
    let needed = length.div_ceil(MAX_PARTS).div_ceil(MIB) * MIB;
    let size = configured.max(needed);
    check_part(size)?;
    Ok(size)
}

pub(crate) fn upload(
    client: &S3Client,
    object: &str,
    file: &mut File,
    span: Span,
    metadata: &Metadata,
) -> Result<(), RemoteError> {
    let size = part_size(span.length, client.config().part_size)?;
    let upload = create(client, object, metadata)?;
    let result = (|| {
        let mut etags = Vec::new();
        let mut offset = 0;
        while offset < span.length {
            let part = Span {
                offset: span.offset + offset,
                length: size.min(span.length - offset),
            };
            let number = etags.len() as u64 + 1;
            etags.push(send_part(client, object, &upload, number, file, part)?);
            offset += part.length;
        }
        complete(client, object, &upload, &etags)
    })();
    match result {
        Ok(()) => Ok(()),
        Err(cause) => match abort(client, object, &upload) {
            Ok(()) => Err(cause),
            Err(abort) => Err(RemoteError::PartsLeft {
                cause: Box::new(cause),
                upload,
                abort: Box::new(abort),
            }),
        },
    }
}

fn create(client: &S3Client, object: &str, metadata: &Metadata) -> Result<String, RemoteError> {
    let mut headers = metadata_headers(metadata);
    headers.insert(
        "content-type".to_owned(),
        "application/octet-stream".to_owned(),
    );
    let reply = client.send(
        "POST",
        object,
        &[("uploads", String::new())],
        headers,
        Body::Empty,
        sign::hash(&[]),
        None,
    )?;
    if reply.status != 200 {
        return Err(refused(&reply));
    }
    xml_text(&String::from_utf8_lossy(&reply.body), "UploadId")
        .filter(|id| !id.is_empty())
        .ok_or_else(|| RemoteError::Uncertain(format!("POST {object}?uploads: no UploadId")))
}

fn send_part(
    client: &S3Client,
    object: &str,
    upload: &str,
    number: u64,
    file: &mut File,
    span: Span,
) -> Result<String, RemoteError> {
    let payload = hash_span(file, span)?;
    let reply = client.send(
        "PUT",
        object,
        &[
            ("partNumber", number.to_string()),
            ("uploadId", upload.to_owned()),
        ],
        BTreeMap::new(),
        Body::File { file, span },
        payload,
        None,
    )?;
    if reply.status != 200 {
        return Err(refused(&reply));
    }
    reply
        .headers
        .get("etag")
        .filter(|etag| !etag.is_empty())
        .cloned()
        .ok_or_else(|| RemoteError::Uncertain(format!("part {number} of {object}: no ETag")))
}

fn complete(
    client: &S3Client,
    object: &str,
    upload: &str,
    etags: &[String],
) -> Result<(), RemoteError> {
    let mut body = String::from("<CompleteMultipartUpload>");
    for (index, etag) in etags.iter().enumerate() {
        let etag = etag.replace('&', "&amp;").replace('<', "&lt;");
        body.push_str(&format!(
            "<Part><PartNumber>{}</PartNumber><ETag>{etag}</ETag></Part>",
            index + 1
        ));
    }
    body.push_str("</CompleteMultipartUpload>");
    let reply = client.send(
        "POST",
        object,
        &[("uploadId", upload.to_owned())],
        BTreeMap::from([("content-type".to_owned(), "application/xml".to_owned())]),
        Body::Bytes(body.as_bytes()),
        sign::hash(body.as_bytes()),
        None,
    )?;
    let text = String::from_utf8_lossy(&reply.body);
    if reply.status != 200 || text.contains("<Error>") {
        return Err(refused(&reply));
    }
    if !text.contains("<CompleteMultipartUploadResult") {
        return Err(RemoteError::Uncertain(format!(
            "complete {object}: no result in the reply"
        )));
    }
    Ok(())
}

/// Discards upload `upload` and its parts. An absent upload is not an error.
pub(crate) fn abort(client: &S3Client, object: &str, upload: &str) -> Result<(), RemoteError> {
    let reply = client.send(
        "DELETE",
        object,
        &[("uploadId", upload.to_owned())],
        BTreeMap::new(),
        Body::Empty,
        sign::hash(&[]),
        None,
    )?;
    match reply.status {
        200 | 204 | 404 => Ok(()),
        _ => Err(refused(&reply)),
    }
}
