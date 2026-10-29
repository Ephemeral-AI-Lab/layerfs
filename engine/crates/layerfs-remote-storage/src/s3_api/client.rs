//! The S3 API driver: single PUT up to the part size, multipart above it,
//! ranged GET, HEAD and DELETE. One connection and one attempt per request.
use crate::driver::{check_metadata, open_span, Driver, Metadata, Span, Stat};
use crate::error::{local, RemoteError};
use crate::limits::Limits;
use crate::s3_api::config::S3Config;
use crate::s3_api::http::{exchange, parse_length, Body, Reply, Sink};
use crate::s3_api::{multipart, sign};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const META: &str = "x-amz-meta-";

/// One S3-compatible remote.
#[derive(Debug)]
pub struct S3Client {
    config: S3Config,
}

impl S3Client {
    /// Checks `config`; sends nothing.
    pub fn new(config: S3Config) -> Result<Self, RemoteError> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &S3Config {
        &self.config
    }

    /// Signs and sends one request to `path` (an object or bucket path).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn send(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        mut headers: BTreeMap<String, String>,
        body: Body<'_>,
        payload: [u8; 32],
        sink: Option<Sink<'_>>,
    ) -> Result<Reply, RemoteError> {
        let query = sign::query_string(query);
        sign::authorize(&self.config, method, path, &query, &mut headers, &payload)?;
        let target = if query.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{query}")
        };
        exchange(&self.config, method, &target, &headers, body, sink)
    }

    /// Upload ids of unfinished multipart uploads of `name`. An operator
    /// aborts these when a failed upload's abort itself failed.
    pub fn pending_uploads(&self, name: &str) -> Result<Vec<String>, RemoteError> {
        self.config.object_path(name)?;
        let key = self.config.key(name);
        let reply = self.send(
            "GET",
            &format!("/{}", self.config.bucket),
            &[("uploads", String::new()), ("prefix", key.clone())],
            BTreeMap::new(),
            Body::Empty,
            sign::hash(&[]),
            None,
        )?;
        if reply.status != 200 {
            return Err(refused(&reply));
        }
        let text = String::from_utf8_lossy(&reply.body);
        Ok(text
            .split("<Upload>")
            .skip(1)
            .filter(|upload| xml_text(upload, "Key").as_deref() == Some(key.as_str()))
            .filter_map(|upload| xml_text(upload, "UploadId"))
            .collect())
    }

    /// Aborts the multipart upload `upload` of `name`.
    pub fn abort_upload(&self, name: &str, upload: &str) -> Result<(), RemoteError> {
        multipart::abort(self, &self.config.object_path(name)?, upload)
    }
}

/// A failure reply's status and S3 error code.
pub(crate) fn refused(reply: &Reply) -> RemoteError {
    RemoteError::Refused {
        status: reply.status,
        code: xml_text(&String::from_utf8_lossy(&reply.body), "Code").unwrap_or_default(),
    }
}

/// The unescaped text of the first `<tag>…</tag>` in `text`.
pub(crate) fn xml_text(text: &str, tag: &str) -> Option<String> {
    let start = text.find(&format!("<{tag}>"))? + tag.len() + 2;
    let end = start + text[start..].find(&format!("</{tag}>"))?;
    Some(
        text[start..end]
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&"),
    )
}

/// The sha256 of `span` of `file`, read once before the request that sends it.
pub(crate) fn hash_span(file: &mut File, span: Span) -> Result<[u8; 32], RemoteError> {
    file.seek(SeekFrom::Start(span.offset))
        .map_err(|e| local("seek", e))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 256 * 1024];
    let mut left = span.length;
    while left > 0 {
        let want = left.min(buffer.len() as u64) as usize;
        let n = file
            .read(&mut buffer[..want])
            .map_err(|e| local("read", e))?;
        if n == 0 {
            return Err(RemoteError::Local("upload source shrank".to_owned()));
        }
        digest.update(&buffer[..n]);
        left -= n as u64;
    }
    Ok(digest.finalize().into())
}

/// `x-amz-meta-*` headers for `metadata`.
pub(crate) fn metadata_headers(metadata: &Metadata) -> BTreeMap<String, String> {
    metadata
        .iter()
        .map(|(key, value)| (format!("{META}{key}"), value.clone()))
        .collect()
}

impl Driver for S3Client {
    fn limits(&self) -> Limits {
        self.config.limits
    }

    fn upload_file(
        &self,
        name: &str,
        path: &Path,
        span: Option<Span>,
        metadata: &Metadata,
    ) -> Result<(), RemoteError> {
        let object = self.config.object_path(name)?;
        check_metadata(metadata)?;
        let (mut file, span) = open_span(path, span)?;
        let limits = self.config.limits;
        if span.length > limits.max_file {
            return Err(RemoteError::TooLarge {
                bytes: span.length,
                max: limits.max_file,
            });
        }
        if span.length > self.config.part_size || span.length > limits.max_request {
            return multipart::upload(self, &object, &mut file, span, metadata);
        }
        let payload = hash_span(&mut file, span)?;
        let mut headers = metadata_headers(metadata);
        headers.insert(
            "content-type".to_owned(),
            "application/octet-stream".to_owned(),
        );
        let body = Body::File {
            file: &mut file,
            span,
        };
        let reply = self.send("PUT", &object, &[], headers, body, payload, None)?;
        match reply.status {
            200 => Ok(()),
            _ => Err(refused(&reply)),
        }
    }

    fn download_file(
        &self,
        name: &str,
        span: Option<Span>,
        out: &mut dyn Write,
    ) -> Result<u64, RemoteError> {
        let object = self.config.object_path(name)?;
        let mut headers = BTreeMap::new();
        if let Some(span) = span {
            let end = span
                .offset
                .checked_add(span.length)
                .filter(|_| span.length > 0)
                .ok_or_else(|| RemoteError::Invalid(format!("span {span:?}")))?;
            headers.insert(
                "range".to_owned(),
                format!("bytes={}-{}", span.offset, end - 1),
            );
        }
        let expected = if span.is_some() { 206 } else { 200 };
        let sink = Sink {
            status: expected,
            out,
        };
        let reply = self.send(
            "GET",
            &object,
            &[],
            headers,
            Body::Empty,
            sign::hash(&[]),
            Some(sink),
        )?;
        if reply.status == 404 {
            return Err(RemoteError::Missing);
        }
        if reply.status != expected {
            return Err(refused(&reply));
        }
        let integrity = |why: &str| RemoteError::Integrity(format!("GET {name}: {why}"));
        if let Some(span) = span {
            let prefix = format!("bytes {}-{}/", span.offset, span.offset + span.length - 1);
            reply
                .headers
                .get("content-range")
                .filter(|range| range.starts_with(&prefix))
                .ok_or_else(|| integrity("content-range"))?;
            if reply.streamed != span.length {
                return Err(integrity("range length"));
            }
        }
        Ok(reply.streamed)
    }

    fn stat(&self, name: &str) -> Result<Option<Stat>, RemoteError> {
        let object = self.config.object_path(name)?;
        let reply = self.send(
            "HEAD",
            &object,
            &[],
            BTreeMap::new(),
            Body::Empty,
            sign::hash(&[]),
            None,
        )?;
        match reply.status {
            404 => Ok(None),
            200 => {
                let bytes = reply
                    .headers
                    .get("content-length")
                    .ok_or_else(|| RemoteError::Uncertain(format!("HEAD {name}: no length")))
                    .and_then(|v| parse_length(v).map_err(RemoteError::Uncertain))?;
                let metadata = reply
                    .headers
                    .iter()
                    .filter_map(|(k, v)| Some((k.strip_prefix(META)?.to_owned(), v.clone())))
                    .collect();
                Ok(Some(Stat { bytes, metadata }))
            }
            _ => Err(refused(&reply)),
        }
    }

    fn delete(&self, name: &str) -> Result<(), RemoteError> {
        let object = self.config.object_path(name)?;
        let reply = self.send(
            "DELETE",
            &object,
            &[],
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
}
