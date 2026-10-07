//! Standard Docker duplex streams; transport completion never implies process exit.
use super::{body::Body, http::Response, socket::Socket, ExecCreated};
use crate::{ExecInspection, RuntimeError, WireFailure};
use std::{
    fmt,
    io::{self, BufReader, Read, Write},
    net::Shutdown,
};
const WINDOW: usize = 8192;
/// Actual delivered bytes, not queued bytes or a lifetime output limit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutputProgress {
    pub stdout: u64,
    pub stderr: u64,
    pub wire_payload: u64,
    pub saturated: bool,
}
impl OutputProgress {
    fn add(value: &mut u64, n: usize, saturated: &mut bool) {
        match value.checked_add(n as u64) {
            Some(n) => *value = n,
            None => {
                *value = u64::MAX;
                *saturated = true;
            }
        }
    }
}
/// Actual standard attached stream owners, independent of an Exec result.
pub struct Attached {
    created: ExecCreated,
    response: Response,
}
impl fmt::Debug for Attached {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attached")
            .field("created", &self.created)
            .finish()
    }
}
/// Original attachment retained if stream ownership splitting fails.
#[derive(Debug)]
pub struct SplitFailure {
    pub attached: Attached,
    pub cause: io::Error,
}
/// Original acknowledged runtime identity; inspection/cancellation owns no filesystem lifetime.
#[derive(Debug)]
pub struct ExecHandle {
    created: ExecCreated,
}
impl ExecHandle {
    pub fn inspect(&self) -> Result<ExecInspection, Box<WireFailure>> {
        self.created
            .docker
            .inspect_exec(self.created.container, self.created.exec)
    }
    /// Engine1.54 exposes no process-specific signal API. This refuses before any effect.
    pub fn cancel(&self) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported(
            "Engine per-Exec cancellation; use a qualified ordinary external runtime",
        ))
    }
    pub fn identity(&self) -> (crate::ContainerId, crate::ExecId) {
        (self.created.container, self.created.exec)
    }
}
/// Original Start request/transfer failure; creation custody stays owned.
#[derive(Debug)]
pub struct StartFailure {
    pub created: ExecCreated,
    pub transfer: Box<WireFailure>,
}
impl ExecCreated {
    /// Starts this original Exec once and takes standard raw/multiplexed streams.
    pub fn start(mut self) -> Result<Attached, Box<StartFailure>> {
        if self.start_attempted {
            let selected = self.exec;
            let container = self.container;
            return Err(Box::new(StartFailure {
                created: self,
                transfer: Box::new(WireFailure {
                    attempted: false,
                    sent_bytes: 0,
                    status: None,
                    cause: RuntimeError::Protocol("original Start already attempted; no replay"),
                    fence_error: None,
                    observed_exec: None,
                    observed_container: None,
                    requested_container: Some(container),
                    requested_exec: Some(selected),
                    error_body: None,
                }),
            }));
        }
        self.start_attempted = true;
        let result = self.docker.exchange(
            "POST",
            &format!("/v1.54/exec/{}/start", self.exec),
            true,
            |w| w.write_all(b"{\"Detach\":false,\"Tty\":false}"),
        );
        match result {
            Ok(response) if response.status == 101 => Ok(Attached {
                created: self,
                response,
            }),
            Ok(mut response) => {
                let mut transfer = response.fail(RuntimeError::Http(response.status), None);
                transfer.requested_container = Some(self.container);
                transfer.requested_exec = Some(self.exec);
                Err(Box::new(StartFailure {
                    created: self,
                    transfer,
                }))
            }
            Err(mut transfer) => {
                transfer.requested_container = Some(self.container);
                transfer.requested_exec = Some(self.exec);
                Err(Box::new(StartFailure {
                    created: self,
                    transfer,
                }))
            }
        }
    }
}
/// Ordinary stdin writer. Explicit close half-closes only this direction, never cancels a process.
/// Dropping this owner does not establish EOF while another socket owner remains.
pub struct RuntimeInput {
    stream: Socket,
    pub sent: u64,
    pub saturated: bool,
    enabled: bool,
    failed: bool,
    close_attempted: bool,
}
impl RuntimeInput {
    pub fn close(mut self) -> Result<InputEnd, Box<InputCloseFailure>> {
        if self.close_attempted {
            return Err(Box::new(InputCloseFailure {
                input: self,
                attempted: false,
                cause: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "stdin close already attempted; no replay",
                ),
            }));
        }
        self.close_attempted = true;
        match self.stream.stream.shutdown(Shutdown::Write) {
            Ok(()) => Ok(InputEnd {
                sent: self.sent,
                saturated: self.saturated,
                input_failed: self.failed,
            }),
            Err(cause) => Err(Box::new(InputCloseFailure {
                input: self,
                cause,
                attempted: true,
            })),
        }
    }
}
/// Original terminal stdin receipt; it establishes no remote process exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEnd {
    pub sent: u64,
    pub saturated: bool,
    pub input_failed: bool,
}
impl fmt::Debug for RuntimeInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeInput")
            .field("sent", &self.sent)
            .field("saturated", &self.saturated)
            .field("failed", &self.failed)
            .field("close_attempted", &self.close_attempted)
            .finish()
    }
}
/// Original failed half-close with its exact input owner and counters retained.
#[derive(Debug)]
pub struct InputCloseFailure {
    pub input: RuntimeInput,
    pub cause: io::Error,
    pub attempted: bool,
}
impl Write for RuntimeInput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.enabled || self.failed || self.close_attempted {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "stdin unavailable or failed; no replay",
            ));
        }
        let n = match self.stream.write(bytes) {
            Ok(n) => n,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        if n == 0 && !bytes.is_empty() {
            self.failed = true;
        }
        OutputProgress::add(&mut self.sent, n, &mut self.saturated);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
    fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let n = self.write(bytes)?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::WriteZero));
            }
            bytes = &bytes[n..];
        }
        Ok(())
    }
}
/// Output reader with its exact pending undelivered window retained on failure.
pub struct RuntimeOutput {
    body: Body<BufReader<Socket>>,
    writer: Socket,
    progress: OutputProgress,
    pending: [u8; WINDOW],
    used: usize,
    delivered: usize,
    remaining: u32,
    tag: u8,
    header: [u8; 8],
    header_used: usize,
    fenced: bool,
}
impl fmt::Debug for RuntimeOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeOutput")
            .field("progress", &self.progress)
            .field("pending_bytes", &(self.used - self.delivered))
            .field("remaining_frame", &self.remaining)
            .finish()
    }
}
/// Original output failure, including its remaining reader and undelivered window.
#[derive(Debug)]
pub struct OutputFailure {
    pub output: RuntimeOutput,
    pub cause: RuntimeError,
    pub fence_error: Option<io::Error>,
}
impl Attached {
    /// Separates ordinary stdin/output/runtime identity so each can progress independently.
    pub fn into_parts(
        self,
    ) -> Result<(RuntimeInput, RuntimeOutput, ExecHandle), Box<SplitFailure>> {
        let output_writer = match self.response.writer.try_clone() {
            Ok(stream) => stream,
            Err(cause) => {
                return Err(Box::new(SplitFailure {
                    attached: self,
                    cause,
                }))
            }
        };
        let enabled = self.created.request.stdin;
        let writer = self.response.writer;
        Ok((
            RuntimeInput {
                stream: writer,
                sent: 0,
                saturated: false,
                enabled,
                failed: false,
                close_attempted: false,
            },
            RuntimeOutput {
                body: self.response.body,
                writer: output_writer,
                progress: OutputProgress::default(),
                pending: [0; WINDOW],
                used: 0,
                delivered: 0,
                remaining: 0,
                tag: 0,
                header: [0; 8],
                header_used: 0,
                fenced: false,
            },
            ExecHandle {
                created: self.created,
            },
        ))
    }
}
impl RuntimeOutput {
    /// Original received payload which the caller sink has not acknowledged.
    /// Original acknowledged output counts, including partial delivery before failure.
    pub fn progress(&self) -> OutputProgress {
        self.progress
    }
    pub fn pending_payload(&self) -> &[u8] {
        &self.pending[self.delivered..self.used]
    }
    /// Original partial/current standard framing bytes, independent of payload delivery.
    pub fn framing_bytes(&self) -> &[u8] {
        &self.header[..self.header_used]
    }
    /// Streams through caller sinks. EOF establishes transport completion only.
    pub fn copy_to(
        mut self,
        stdout: &mut impl Write,
        stderr: &mut impl Write,
    ) -> Result<OutputProgress, Box<OutputFailure>> {
        let result = self.copy(stdout, stderr);
        match result {
            Ok(()) => Ok(self.progress),
            Err(cause) => {
                let fence_error = if self.fenced {
                    None
                } else {
                    self.fenced = true;
                    self.writer.stream.shutdown(Shutdown::Both).err()
                };
                Err(Box::new(OutputFailure {
                    output: self,
                    cause,
                    fence_error,
                }))
            }
        }
    }
    fn copy(
        &mut self,
        stdout: &mut impl Write,
        stderr: &mut impl Write,
    ) -> Result<(), RuntimeError> {
        if self.fenced {
            return Err(RuntimeError::Protocol("terminal output owner; no replay"));
        }
        loop {
            self.header_used = 0;
            while self.header_used < 8 {
                let n = self.body.read(&mut self.header[self.header_used..])?;
                if n == 0 {
                    if self.header_used == 0 {
                        return Ok(());
                    }
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                }
                self.header_used += n;
            }
            self.tag = self.header[0];
            if self.header[1..4] != [0; 3] || self.tag > 3 {
                return Err(RuntimeError::Protocol("standard runtime frame header"));
            }
            self.remaining = u32::from_be_bytes(self.header[4..8].try_into().expect("four bytes"));
            if self.tag == 3 {
                return Err(RuntimeError::Protocol(
                    "runtime system-error stream; original unread payload retained",
                ));
            }
            while self.remaining != 0 {
                let target = (self.remaining as usize).min(WINDOW);
                self.used = 0;
                self.delivered = 0;
                while self.used < target {
                    let n = self.body.read(&mut self.pending[self.used..target])?;
                    if n == 0 {
                        return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                    }
                    self.used += n;
                    self.remaining -= n as u32;
                    OutputProgress::add(
                        &mut self.progress.wire_payload,
                        n,
                        &mut self.progress.saturated,
                    );
                }
                while self.delivered < self.used {
                    let sink: &mut dyn Write = if self.tag == 2 { stderr } else { stdout };
                    let n = sink.write(&self.pending[self.delivered..self.used])?;
                    if n == 0 {
                        return Err(io::Error::from(io::ErrorKind::WriteZero).into());
                    }
                    self.delivered += n;
                    let count = if self.tag == 2 {
                        &mut self.progress.stderr
                    } else {
                        &mut self.progress.stdout
                    };
                    OutputProgress::add(count, n, &mut self.progress.saturated);
                }
                self.used = 0;
                self.delivered = 0;
            }
        }
    }
}
