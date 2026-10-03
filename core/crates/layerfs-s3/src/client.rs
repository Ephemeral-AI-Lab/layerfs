//! Three C2 calls, conditional creation and terminal transport uncertainty.
use crate::{
    config::S3Config,
    counters::S3Diagnostics,
    http::{parse_length, Http, Reply},
    sign,
};
use layerfs_storage::port::{ByteRange, ObjectError, ObjectKey, ObjectStore, Put};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};
struct Connection {
    http: Option<Http>,
    counts: S3Diagnostics,
    completed_close: bool,
    prior: S3Diagnostics,
}
/// MinIO client with an explicitly selected bounded connection window. Failed requests are terminal.
/// A new operation may connect once after a prior acknowledged normal close.
pub struct S3Objects {
    config: S3Config,
    connections: Vec<Mutex<Connection>>,
    next: AtomicUsize,
    failed: AtomicBool,
}
impl S3Objects {
    /// Connects once to the existing service and namespace. No bucket/bootstrap I/O.
    pub fn connect(config: S3Config) -> Result<Self, ObjectError> {
        Self::connect_count(config, 1)
    }
    /// Opens exactly four independent one-attempt connections for namespace Init.
    /// Use with C2's bounded parallel save; ordinary operations use connect.
    pub fn connect_parallel(config: S3Config) -> Result<Self, ObjectError> {
        Self::connect_count(config, 4)
    }
    fn connect_count(config: S3Config, count: usize) -> Result<Self, ObjectError> {
        config.validate()?;
        let mut connections = Vec::with_capacity(count);
        for _ in 0..count {
            let http = Http::connect(&config)?;
            let counts = http.diagnostics();
            connections.push(Mutex::new(Connection {
                http: Some(http),
                counts,
                completed_close: false,
                prior: S3Diagnostics::default(),
            }));
        }
        Ok(Self {
            config,
            connections,
            next: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
        })
    }
    /// Actual aggregate request and byte counts across this declared window.
    pub fn diagnostics(&self) -> Result<S3Diagnostics, ObjectError> {
        let mut counts = S3Diagnostics::default();
        for connection in &self.connections {
            counts.accumulate(
                connection
                    .lock()
                    .map_err(|_| ObjectError::Uncertain)?
                    .counts,
            );
        }
        Ok(counts)
    }
    fn request(
        &self,
        method: &str,
        key: ObjectKey,
        body: &[u8],
        extra: Option<(&str, String)>,
        validation_ns: u64,
    ) -> Result<Reply, ObjectError> {
        let signing_started = std::time::Instant::now();
        let (_, _, host) = self.config.address()?;
        let path = self.config.path(key);
        let payload = if method == "PUT" {
            *key.as_bytes()
        } else {
            *ObjectKey::for_bytes(&[]).as_bytes()
        };
        let headers = sign::headers(&self.config, method, &path, &host, payload, extra)?;
        let signing_ns = signing_started.elapsed().as_nanos() as u64;
        if self.failed.load(Ordering::Acquire) {
            return Err(ObjectError::Uncertain);
        }
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.connections.len();
        let mut connection = self.connections[index]
            .lock()
            .map_err(|_| ObjectError::Uncertain)?;
        if self.failed.load(Ordering::Acquire) {
            return Err(ObjectError::Uncertain);
        }
        if connection.http.is_none() {
            if !connection.completed_close {
                return Err(ObjectError::Uncertain);
            }
            // This is the first connection of a new operation after a successful
            // response explicitly closed the prior one. Failure never enters here.
            connection.completed_close = false;
            connection.http = Some(Http::connect(&self.config).inspect_err(|_| {
                self.failed.store(true, Ordering::Release);
            })?);
        }
        let http = connection.http.as_mut().ok_or(ObjectError::Uncertain)?;
        http.note_local(method, signing_ns, validation_ns);
        let result = http.request(&self.config, method, &path, &headers, body);
        let counts = http.diagnostics();
        connection.counts = connection.prior;
        connection.counts.accumulate(counts);
        if matches!(result, Err(ObjectError::Uncertain | ObjectError::Malformed)) {
            self.failed.store(true, Ordering::Release);
            connection.http = None;
            connection.completed_close = false;
        } else if let Ok(reply) = &result {
            if reply.close {
                connection.http = None;
                connection.completed_close =
                    (200..300).contains(&reply.status) || reply.status == 412;
                connection.prior = connection.counts;
            }
        }
        result
    }
}
impl ObjectStore for S3Objects {
    fn put_if_absent(&self, key: ObjectKey, body: &[u8]) -> Result<Put, ObjectError> {
        let started = std::time::Instant::now();
        if body.len() > self.config.max_body_bytes || ObjectKey::for_bytes(body) != key {
            return Err(ObjectError::Malformed);
        }
        let reply = self.request(
            "PUT",
            key,
            body,
            Some(("if-none-match", "*".to_owned())),
            started.elapsed().as_nanos() as u64,
        )?;
        match reply.status {
            200 | 201 => Ok(Put::Created),
            412 => Ok(Put::AlreadyPresent),
            status => Err(ObjectError::Refused { status }),
        }
    }
    fn read(
        &self,
        key: ObjectKey,
        range: Option<ByteRange>,
        out: &mut Vec<u8>,
    ) -> Result<(), ObjectError> {
        out.clear();
        let extra = range
            .map(|range| {
                let end = range
                    .start
                    .checked_add(range.length)
                    .and_then(|n| n.checked_sub(1))
                    .filter(|_| {
                        range.length > 0 && range.length <= self.config.max_body_bytes as u64
                    })
                    .ok_or(ObjectError::Malformed)?;
                Ok::<_, ObjectError>(("range", format!("bytes={}-{}", range.start, end)))
            })
            .transpose()?;
        let reply = self.request("GET", key, &[], extra, 0)?;
        match reply.status {
            404 => return Err(ObjectError::Missing),
            200 if range.is_none() => {}
            206 if range.is_some() => {}
            status => return Err(ObjectError::Refused { status }),
        }
        let length = reply
            .headers
            .get("content-length")
            .map(|value| parse_length(value))
            .transpose()?;
        if length.is_some_and(|length| length != reply.body.len()) {
            return Err(ObjectError::Malformed);
        }
        if let Some(range) = range {
            if reply.body.len() as u64 != range.length {
                return Err(ObjectError::Malformed);
            }
            let expected = format!("bytes {}-{}/", range.start, range.start + range.length - 1);
            let total = reply
                .headers
                .get("content-range")
                .and_then(|value| value.strip_prefix(&expected))
                .ok_or(ObjectError::Malformed)?;
            let total = parse_length(total)?;
            if (total as u64) < range.start + range.length {
                return Err(ObjectError::Malformed);
            }
        }
        *out = reply.body;
        Ok(())
    }
    fn head(&self, key: ObjectKey) -> Result<Option<u64>, ObjectError> {
        let reply = self.request("HEAD", key, &[], None, 0)?;
        match reply.status {
            404 => Ok(None),
            200 => {
                let length = parse_length(
                    reply
                        .headers
                        .get("content-length")
                        .ok_or(ObjectError::Malformed)?,
                )?;
                if length > self.config.max_body_bytes {
                    return Err(ObjectError::Malformed);
                }
                Ok(Some(length as u64))
            }
            status => Err(ObjectError::Refused { status }),
        }
    }
}
