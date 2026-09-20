//! One operation at a time; bounded concurrent upload and response consumption.
use super::{connection::Connection, protocol::*};
use crate::contract::*;
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// A stable, cooperative input capability. Implementations must stop on the
/// absolute deadline/cancellation; the native pipe source enforces this with poll.
pub trait Source: Send {
    fn read(
        &mut self,
        buffer: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize>;
}
impl Source for &[u8] {
    fn read(&mut self, b: &mut [u8], _: Instant, cancel: &AtomicBool) -> io::Result<usize> {
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        io::Read::read(self, b)
    }
}
pub struct Client {
    connection: Connection,
    previous: u64,
    closed: bool,
}
impl Client {
    pub fn new(mut connection: Connection) -> Result<Self, Failure> {
        let hello = Frame {
            kind: Kind::Hello,
            id: 0,
            bytes: 1u16.to_be_bytes().to_vec(),
        };
        connection.send.write(&hello)?;
        let response = connection.receive.read()?;
        if response.kind != Kind::Hello || response.id != 0 || response.bytes != hello.bytes {
            return Err(Code::Unsupported.into());
        }
        Ok(Self {
            connection,
            previous: 0,
            closed: false,
        })
    }
    pub fn call(
        &mut self,
        r: &Request,
        source: &mut impl Source,
        output: &mut dyn Write,
    ) -> Result<Response, Failure> {
        r.validate()?;
        if self.closed || r.id <= self.previous {
            return Err(Code::InvalidInput.into());
        }
        self.previous = r.id;
        let deadline = Instant::now() + Duration::from_millis(r.deadline_ms as u64);
        self.connection.send.deadline(deadline);
        self.connection.receive.deadline(deadline);
        let metadata = encode_request(r)?;
        // Once BEGIN is attempted, a transport failure cannot establish mutation abort.
        if self
            .connection
            .send
            .write(&Frame {
                kind: Kind::Begin,
                id: r.id,
                bytes: metadata,
            })
            .is_err()
        {
            self.closed = true;
            return Err(delivery(r));
        }
        let cancel = AtomicBool::new(false);
        let expected = r.operation.input_length()?;
        let send = &mut self.connection.send;
        let receive = &mut self.connection.receive;
        let result = std::thread::scope(|threads| {
            let upload = std::thread::Builder::new()
                .name("layerfs-upload".into())
                .stack_size(2 * 1024 * 1024)
                .spawn_scoped(threads, || -> Result<(), Failure> {
                    let result = (|| {
                        let mut buffer = [0u8; FRAME_BYTES];
                        let mut bytes = 0u64;
                        let mut frames = 0;
                        loop {
                            let n = source.read(&mut buffer, deadline, &cancel)?;
                            if n > buffer.len() {
                                return Err(Code::InvalidInput.into());
                            }
                            if n == 0 {
                                if bytes != expected {
                                    return Err(Code::InvalidInput.into());
                                }
                                break;
                            }
                            bytes = bytes
                                .checked_add(n as u64)
                                .filter(|v| *v <= expected)
                                .ok_or(Code::InvalidInput)?;
                            frames += 1;
                            if frames >= MAX_FRAMES {
                                return Err(Code::Capacity.into());
                            }
                            send.write(&Frame {
                                kind: Kind::Body,
                                id: r.id,
                                bytes: buffer[..n].to_vec(),
                            })?;
                        }
                        send.write(&Frame {
                            kind: Kind::EndInput,
                            id: r.id,
                            bytes: bytes.to_be_bytes().to_vec(),
                        })
                    })();
                    if result.is_err() {
                        send.end_upload();
                    }
                    result
                })
                .map_err(|_| delivery(r))?;
            let response = (|| {
                let mut bytes = 0u64;
                let mut frames = 0;
                loop {
                    let frame = receive.read().map_err(|_| delivery(r))?;
                    frames += 1;
                    if frame.id != r.id || frames > MAX_FRAMES {
                        return Err(delivery(r));
                    }
                    match frame.kind {
                        Kind::ResultData => {
                            if !matches!(r.operation, Operation::ReadFile { .. }) {
                                return Err(delivery(r));
                            }
                            bytes = bytes
                                .checked_add(frame.bytes.len() as u64)
                                .filter(|v| *v <= r.response_bytes)
                                .ok_or_else(|| delivery(r))?;
                            output.write_all(&frame.bytes).map_err(|_| delivery(r))?;
                        }
                        Kind::Success => {
                            let response =
                                decode_response(&frame.bytes).map_err(|_| delivery(r))?;
                            if let Response::Read { length } = &response {
                                if *length != bytes {
                                    return Err(delivery(r));
                                }
                            }
                            if !matches_response(r, &response, bytes) {
                                return Err(delivery(r));
                            }
                            return Ok(response);
                        }
                        Kind::Failure => {
                            return Err(decode_failure(&frame.bytes).map_err(|_| delivery(r))?)
                        }
                        _ => return Err(delivery(r)),
                    }
                }
            })();
            cancel.store(true, Ordering::Release);
            if response.is_err() {
                receive.close();
            }
            let upload = upload.join().map_err(|_| delivery(r))?;
            match response {
                Err(e) => Err(e),
                Ok(value) => {
                    upload.map_err(|_| delivery(r))?;
                    Ok(value)
                }
            }
        });
        if result.is_err() {
            self.closed = true;
            self.connection.receive.close();
        }
        result
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.connection.receive.close();
    }
}
fn delivery(r: &Request) -> Failure {
    Failure {
        code: if r.operation.mutation() {
            Code::Unknown
        } else {
            Code::Io
        },
        unknown: r.operation.mutation(),
        cleanup: None,
    }
}

fn matches_response(r: &Request, response: &Response, bytes: u64) -> bool {
    match (&r.operation, response) {
        (Operation::ReadFile { start, end, .. }, Response::Read { length }) => {
            *length == end - start && *length == bytes
        }
        (Operation::ConstructFile { length }, Response::Saved { length: actual, .. }) => {
            length == actual && bytes == 0
        }
        (
            Operation::EditFile {
                base_length, edits, ..
            },
            Response::Saved { length, .. },
        ) => {
            edits.iter().try_fold(*base_length, |n, e| {
                n.checked_sub(e.end - e.start)?.checked_add(e.replacement)
            }) == Some(*length)
                && bytes == 0
        }
        (Operation::UpdatePreparedFilesystem { .. }, Response::FilesystemSaved { .. }) => {
            bytes == 0
        }
        (
            Operation::Inspect {
                query: Inspect::File,
                ..
            },
            Response::File {
                length,
                representation,
            },
        ) => *length <= MAX_FILE && [1, 2].contains(representation) && bytes == 0,
        (
            Operation::Inspect {
                query: Inspect::Stat { .. },
                ..
            },
            Response::Stat { nanoseconds, .. },
        ) => *nanoseconds < 1_000_000_000 && bytes == 0,
        (
            Operation::Inspect {
                query: Inspect::List { entries, .. },
                ..
            },
            Response::List {
                entries: actual, ..
            },
        ) => actual.len() <= *entries as usize && bytes == 0,
        (
            Operation::Inspect {
                query: Inspect::Readlink { .. },
                ..
            },
            Response::Link(link),
        ) => link.len() <= 4096 && bytes == 0,
        _ => false,
    }
}
