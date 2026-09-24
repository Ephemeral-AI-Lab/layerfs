//! One operation at a time; bounded concurrent upload and response consumption.
use super::{connection::Connection, payload::COALESCE_BYTES, protocol::*};
use crate::contract::*;
use std::{
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub use crate::contract::Source;

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
        source: &mut (impl Source + ?Sized),
        output: &mut dyn Write,
    ) -> Result<Response, Failure> {
        let deadline = Instant::now() + Duration::from_millis(r.deadline_ms as u64);
        self.call_until(r, source, output, deadline)
    }

    /// Uses the caller's process-local deadline through upload and delivery.
    /// The request can shorten this deadline, never extend it.
    pub fn call_until(
        &mut self,
        r: &Request,
        source: &mut (impl Source + ?Sized),
        output: &mut dyn Write,
        deadline: Instant,
    ) -> Result<Response, Failure> {
        r.validate()?;
        if self.closed || r.id <= self.previous {
            return Err(Code::InvalidInput.into());
        }
        self.previous = r.id;
        let deadline = deadline.min(Instant::now() + Duration::from_millis(r.deadline_ms as u64));
        self.connection.send.deadline(deadline);
        self.connection.receive.deadline(deadline);
        // A duration crosses the wire, never an Instant. Metadata preparation
        // consumes the same local budget; the remote peer starts its own clock.
        let remaining_ms = deadline
            .checked_duration_since(Instant::now())
            .and_then(|remaining| u32::try_from(remaining.as_millis()).ok())
            .filter(|remaining| *remaining > 0)
            .ok_or(Code::Deadline)?;
        let metadata = encode_request_with_budget(r, remaining_ms)?;
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
                        let mut frames = 0u64;
                        let mut filled = 0;
                        loop {
                            if cancel.load(Ordering::Acquire) {
                                return Err(Code::Io.into());
                            }
                            if Instant::now() >= deadline {
                                return Err(Code::Deadline.into());
                            }
                            let n = source.read(&mut buffer[filled..], deadline, &cancel)?;
                            if n > buffer.len() - filled {
                                return Err(Code::InvalidInput.into());
                            }
                            if cancel.load(Ordering::Acquire) {
                                return Err(Code::Io.into());
                            }
                            if Instant::now() >= deadline {
                                return Err(Code::Deadline.into());
                            }
                            bytes = bytes
                                .checked_add(n as u64)
                                .filter(|v| *v <= expected)
                                .ok_or(Code::InvalidInput)?;
                            if n == 0 && bytes != expected {
                                return Err(Code::InvalidInput.into());
                            }
                            filled += n;
                            if filled >= COALESCE_BYTES || (n == 0 && filled > 0) {
                                frames += 1;
                                if frames >= frame_budget(expected) {
                                    return Err(Code::Capacity.into());
                                }
                                send.write(&Frame {
                                    kind: Kind::Body,
                                    id: r.id,
                                    bytes: buffer[..filled].to_vec(),
                                })?;
                                filled = 0;
                            }
                            if n == 0 {
                                break;
                            }
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
                let mut frames = 0u64;
                loop {
                    let frame = receive.read().map_err(|_| delivery(r))?;
                    frames += 1;
                    if frame.id != r.id || frames > frame_budget(r.response_bytes) {
                        return Err(delivery(r));
                    }
                    match frame.kind {
                        Kind::ResultData => {
                            if matches!(
                                r.operation,
                                Operation::HistoryCommand(
                                    HistoryCommand::ImportNativeDirectory { .. }
                                )
                            ) {
                                if frame.bytes != [0] {
                                    return Err(delivery(r));
                                }
                                continue;
                            }
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
                            return Err(
                                decode_request_failure(r, &frame.bytes).map_err(|_| delivery(r))?
                            )
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
/// True when one history reply answers exactly this query.
fn query_matches(query: &HistoryQuery, result: &HistoryResult) -> bool {
    match (query, result) {
        (HistoryQuery::GetStack { .. }, HistoryResult::Stack(_)) => true,
        (
            HistoryQuery::ListStacks { limit, .. },
            HistoryResult::Stacks {
                records,
                continuation,
            },
        ) => page_matches(*limit, records.len(), continuation),
        (HistoryQuery::GetBranch { .. }, HistoryResult::BranchSnapshot(snapshot)) => {
            snapshot.root_serial.is_some()
        }
        (
            HistoryQuery::ListBranches { limit, .. },
            HistoryResult::Branches {
                records,
                continuation,
            },
        ) => page_matches(*limit, records.len(), continuation),
        (HistoryQuery::GetCommit { .. }, HistoryResult::Commit(_)) => true,
        (
            HistoryQuery::CommitHistory { limit, .. },
            HistoryResult::Commits {
                records,
                continuation,
            },
        ) => page_matches(*limit, records.len(), continuation),
        (HistoryQuery::GetLayer { .. }, HistoryResult::Layer(_)) => true,
        (
            HistoryQuery::LayerHistory { limit, .. },
            HistoryResult::Layers {
                records,
                continuation,
            },
        ) => page_matches(*limit, records.len(), continuation),
        (HistoryQuery::GetStage { .. }, HistoryResult::Stage(_)) => true,
        (
            HistoryQuery::ListStages { limit, .. },
            HistoryResult::Stages {
                records,
                continuation,
            },
        ) => page_matches(*limit, records.len(), continuation),
        _ => false,
    }
}

/// True when one history reply answers exactly this command.
fn command_matches(command: &HistoryCommand, result: &HistoryResult) -> bool {
    if let (HistoryCommand::Fork { .. }, HistoryResult::BranchSnapshot(snapshot)) =
        (command, result)
    {
        return snapshot.root_serial.is_none();
    }
    matches!(
        (command, result),
        (
            HistoryCommand::InitLayerStack { .. } | HistoryCommand::ImportNativeDirectory { .. },
            HistoryResult::StackCreated(_)
        ) | (HistoryCommand::StageChanges(_), HistoryResult::Stage(_))
            | (
                HistoryCommand::CommitStaged { .. } | HistoryCommand::Commit(_),
                HistoryResult::Committed(_)
            )
            | (HistoryCommand::AddLayer { .. }, HistoryResult::Published(_))
            | (
                HistoryCommand::DiscardStage { .. },
                HistoryResult::Discarded { .. }
            )
            | (
                HistoryCommand::ReserveInodes { .. },
                HistoryResult::Reservation { .. }
            )
    )
}

/// A page is well formed when it fits the request and its continuation is short.
fn page_matches(limit: u16, records: usize, continuation: &[u8]) -> bool {
    records <= usize::from(limit) && continuation.len() <= CURSOR_BYTES
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
        history: None,
    }
}

fn matches_response(r: &Request, response: &Response, bytes: u64) -> bool {
    if let Operation::HistoryQuery(query) = &r.operation {
        return matches!(response, Response::History(result) if query_matches(query, result))
            && bytes == 0;
    }
    if let Operation::HistoryCommand(command) = &r.operation {
        return matches!(response, Response::History(result) if command_matches(command, result))
            && bytes == 0;
    }
    match (&r.operation, response) {
        (
            Operation::ConstructPortableMetadata {
                kind,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            },
            Response::MetadataConstructed {
                kind: actual_kind,
                mode: actual_mode,
                mtime_seconds: actual_seconds,
                mtime_nanoseconds: actual_nanoseconds,
                ..
            },
        ) => {
            kind == actual_kind
                && mode == actual_mode
                && mtime_seconds == actual_seconds
                && mtime_nanoseconds == actual_nanoseconds
                && response.validate_metadata_constructed().is_ok()
                && bytes == 0
        }
        (
            Operation::UpdatePortableMetadata {
                base,
                kind,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            },
            Response::MetadataSaved {
                base: actual_base,
                kind: actual_kind,
                mode: actual_mode,
                mtime_seconds: actual_seconds,
                mtime_nanoseconds: actual_nanoseconds,
                ..
            },
        ) => {
            base == actual_base
                && kind == actual_kind
                && mode == actual_mode
                && mtime_seconds == actual_seconds
                && mtime_nanoseconds == actual_nanoseconds
                && response.validate_metadata_saved().is_ok()
                && bytes == 0
        }
        (
            Operation::WorkspaceStatus {
                workspace,
                incarnation,
            },
            Response::WorkspaceStatus(status),
        ) => {
            status.workspace == *workspace
                && status.incarnation == *incarnation
                && status.validate().is_ok()
                && bytes == 0
        }
        (
            Operation::WorkspaceStatus {
                workspace,
                incarnation,
            },
            Response::WorkspaceAttachment(status),
        ) => {
            status.workspace == *workspace
                && status.incarnation == *incarnation
                && status.validate().is_ok()
                && bytes == 0
        }
        (
            Operation::WorkspaceAttach {
                workspace,
                incarnation,
            },
            Response::WorkspaceAttach(result),
        ) => {
            result.workspace == *workspace
                && result.incarnation == *incarnation
                && result.validate().is_ok()
                && bytes == 0
        }
        (Operation::SandboxHello, Response::SandboxHello(hello)) => {
            hello.validate().is_ok() && bytes == 0
        }
        (
            Operation::WorkspaceOpen {
                workspace,
                incarnation,
                ..
            },
            Response::WorkspaceAttach(result),
        ) => {
            result.workspace == *workspace
                && result.incarnation == *incarnation
                && result.validate().is_ok()
                && bytes == 0
        }
        (
            Operation::WorkspaceExec {
                workspace,
                incarnation,
                ..
            },
            Response::WorkspaceExec(result),
        ) => {
            result.workspace == *workspace
                && result.incarnation == *incarnation
                && result.validate().is_ok()
                && bytes == 0
        }
        (
            Operation::WorkspaceCommit {
                workspace,
                incarnation,
            },
            Response::WorkspaceCommit(result),
        ) => {
            result.workspace == *workspace
                && result.incarnation == *incarnation
                && result.validate().is_ok()
                && bytes == 0
        }
        (
            Operation::WorkspaceStatus {
                workspace,
                incarnation,
            },
            Response::WorkspaceWritableStatus(result),
        ) => {
            result.status.workspace == *workspace
                && result.status.incarnation == *incarnation
                && result.validate().is_ok()
                && bytes == 0
        }
        (
            Operation::WorkspaceUnmount {
                workspace,
                incarnation,
            },
            Response::WorkspaceUnmount(result),
        )
        | (
            Operation::WorkspaceCloseClean {
                workspace,
                incarnation,
            },
            Response::WorkspaceCloseClean(result),
        )
        | (
            Operation::WorkspaceMount {
                workspace,
                incarnation,
            },
            Response::WorkspaceMount(result),
        ) => {
            result.workspace == *workspace
                && result.incarnation == *incarnation
                && result.validate().is_ok()
                && bytes == 0
        }
        (Operation::ReadFile { start, end, .. }, Response::Read { length }) => {
            *length == end - start && *length == bytes
        }
        (Operation::ConstructFile { length }, Response::Saved { length: actual, .. }) => {
            length == actual && bytes == 0
        }
        (Operation::ConstructSymlink { target }, Response::Saved { length, .. }) => {
            target.len() as u64 == *length && bytes == 0
        }
        (
            Operation::EditFile {
                base_length, edits, ..
            },
            Response::Saved {
                length, metadata, ..
            },
        ) => {
            edits.iter().try_fold(*base_length, |n, e| {
                n.checked_sub(e.end - e.start)?.checked_add(e.replacement)
            }) == Some(*length)
                && metadata.is_none()
                && bytes == 0
        }
        (
            Operation::EditFileWithMetadata {
                base_length, edits, ..
            },
            Response::Saved {
                length, metadata, ..
            },
        ) => {
            // The merged save promises the portable root it produced, so a
            // reply without one is not this operation's answer.
            edits.iter().try_fold(*base_length, |n, e| {
                n.checked_sub(e.end - e.start)?.checked_add(e.replacement)
            }) == Some(*length)
                && metadata.is_some()
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
                query: Inspect::Attributes { path },
                ..
            },
            Response::Attributes { .. },
        ) => response.validate_attributes(Some(path.is_empty())).is_ok() && bytes == 0,
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
