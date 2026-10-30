//! The S3 API driver: single PUT up to the part size, multipart above it,
//! ranged GET, HEAD, DELETE and ListObjectsV2 paging. One attempt per
//! request, on a connection the client kept alive from an earlier reply or
//! a new one.
use crate::driver::{
    check_metadata, open_span, Driver, Listed, Metadata, Span, Stat, MAX_LIST_PAGES,
};
use crate::error::{local, RemoteError};
use crate::limits::Limits;
use crate::s3_api::config::S3Config;
use crate::s3_api::connection::Kept;
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
    /// Idle connections for the next requests.
    kept: Kept,
}

impl S3Client {
    /// Checks `config`; sends nothing.
    pub fn new(config: S3Config) -> Result<Self, RemoteError> {
        config.validate()?;
        Ok(Self {
            config,
            kept: Kept::default(),
        })
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
        exchange(
            &self.config,
            &self.kept,
            method,
            &target,
            &headers,
            body,
            sink,
        )
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
                let modified = reply
                    .headers
                    .get("last-modified")
                    .and_then(|text| crate::time::http_date(text));
                Ok(Some(Stat {
                    bytes,
                    metadata,
                    modified,
                }))
            }
            _ => Err(refused(&reply)),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<Listed>, RemoteError> {
        if !prefix.is_empty() {
            self.config.object_path(prefix.trim_end_matches('/'))?;
        }
        let key = self.config.key(prefix);
        // the namespace's own prefix is cut from each key listed
        let namespace = self.config.key("");
        let mut found = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..MAX_LIST_PAGES {
            let mut query = vec![("list-type", "2".to_owned()), ("prefix", key.clone())];
            if let Some(token) = &token {
                query.push(("continuation-token", token.clone()));
            }
            let reply = self.send(
                "GET",
                &format!("/{}", self.config.bucket),
                &query,
                BTreeMap::new(),
                Body::Empty,
                sign::hash(&[]),
                None,
            )?;
            if reply.status != 200 {
                return Err(refused(&reply));
            }
            let text = String::from_utf8_lossy(&reply.body);
            for entry in text.split("<Contents>").skip(1) {
                let entry = entry.split("</Contents>").next().unwrap_or_default();
                let name = xml_text(entry, "Key")
                    .and_then(|key| key.strip_prefix(namespace.as_str()).map(str::to_owned))
                    .ok_or_else(|| {
                        RemoteError::Uncertain("listing: a key outside the prefix".to_owned())
                    })?;
                let bytes = xml_text(entry, "Size")
                    .and_then(|size| parse_length(&size).ok())
                    .ok_or_else(|| {
                        RemoteError::Uncertain(format!("listing: {name} has no size"))
                    })?;
                found.push(Listed {
                    modified: xml_text(entry, "LastModified")
                        .and_then(|t| crate::time::iso8601(&t)),
                    metadata: None,
                    bytes,
                    name,
                });
            }
            if xml_text(&text, "IsTruncated").as_deref() != Some("true") {
                return Ok(found);
            }
            token = Some(
                xml_text(&text, "NextContinuationToken")
                    .filter(|token| !token.is_empty())
                    .ok_or_else(|| {
                        RemoteError::Uncertain(
                            "listing: truncated with no continuation token".to_owned(),
                        )
                    })?,
            );
        }
        Err(RemoteError::Invalid(format!(
            "the listing of {prefix:?} has more than {MAX_LIST_PAGES} pages"
        )))
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
