//! Complete independent E04 expected bytes; observer reads remain separately scoped.
use super::{
    digest::{hex, Sha256},
    fixture::{self, Fixture, BASE_BYTES, WRITES, WRITE_BYTES},
};
use layerfs_content::{filesystem::PathName, object::InodeKind};
use layerfs_sdk::client::{ClientRequest, Operation, PortFailure, ReplyView};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{OverlayRead, SourceView, Time};
use std::{
    error::Error,
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
};

#[derive(Debug)]
pub struct OracleReport {
    pub final_sha256: String,
    pub expected_sha256: String,
    pub bytes: u64,
    pub mode: u32,
    pub nlink: u64,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub old_file_content_root_unchanged: bool,
    pub namespace_membership_checked: bool,
    pub root_serial: u64,
    pub namespace_entries: u64,
    /// Exact captured Runtime binding equality, not a fresh history-head query.
    pub binding_unchanged: bool,
}
pub struct OracleFailure {
    pub cause: Box<dyn Error>,
    pub bytes_checked: u64,
    pub operation: Option<layerfs_daemon::upstream::UpstreamOperation>,
}
impl fmt::Debug for OracleFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut view = out.debug_struct("OriginalOracleFailure");
        view.field("cause", &self.cause)
            .field("bytes_checked", &self.bytes_checked)
            .field("operation", &self.operation);
        if let Some(operation) = &self.operation {
            match operation.failure() {
                Ok(original) => view.field("first_object_failure", &*original),
                Err(original) => view.field("failure_owner_refusal", &original),
            };
        }
        view.finish()
    }
}
impl fmt::Display for OracleFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for OracleFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

/// Checks the selected root+dense.bin membership, every final byte and EOF.
/// Only closed original input and frozen input cells supply expectations;
/// neither current output nor a reconstructed root is used to infer them.
/// Caller owns observed read/source release receipts. Membership is bounded
/// by this two-path fixture, not a general namespace/whole-root validation.
pub fn verify(
    fixture: &Fixture,
    view: &SourceView,
    ports: &impl OverlayRead,
    final_time: Time,
) -> Result<(OracleReport, layerfs_bridge::codec::Message), OracleFailure> {
    let mut checked = 0;
    let mut operation = None;
    let result = (|| -> Result<(OracleReport, layerfs_bridge::codec::Message), Box<dyn Error>> {
        let root_serial = view.root_serial();
        if root_serial != fixture.upstream.expected().root_serial {
            return Err("E04 original root serial differs".into());
        }
        let membership = view.list(ports, root_serial, None)?;
        if membership.entries.len() != 1
            || membership.entries[0].0.as_slice() != fixture::FILE_NAME.as_bytes()
            || membership.entries[0].1 != fixture.serial
            || membership.continuation.is_some()
        {
            return Err("E04 complete selected root membership differs".into());
        }
        let stat = view.lookup(ports, root_serial, &PathName::new(fixture::FILE_NAME)?)?;
        if stat.serial != fixture.serial
            || stat.kind != InodeKind::RegularFile
            || stat.logical_len != BASE_BYTES
            || stat.metadata.mode != fixture.stat.metadata.mode
            || stat.namespace_refs != fixture.stat.value.namespace_ref_count
            || stat.metadata.mtime_seconds != final_time.seconds
            || stat.metadata.mtime_nanoseconds != final_time.nanoseconds
        {
            return Err("E04 complete final portable facts differ".into());
        }
        let mut base = File::open(&fixture.base_input)?;
        let before = base.metadata()?;
        if before.len() != BASE_BYTES {
            return Err("E04 closed initial input changed length".into());
        }
        let mut original_hash = Sha256::new();
        let mut expected_hash = Sha256::new();
        let mut actual_hash = Sha256::new();
        let mut expected = [0; 65_536];
        let mut original = [0; 65_536];
        let mut at = 0;
        while at < BASE_BYTES {
            base.read_exact(&mut original)?;
            original_hash.update(&original);
            expected.copy_from_slice(&original);
            for cell in 0..expected.len() / WRITE_BYTES {
                let global = at / WRITE_BYTES as u64 + cell as u64;
                // 809 is the multiplicative inverse of104729 modulo4096.
                // This is the registered fixed trace, not candidate discovery.
                let index = ((global * 809) % (BASE_BYTES / WRITE_BYTES as u64)) as usize;
                if index < WRITES {
                    if fixture::write_offset(index) != global * WRITE_BYTES as u64 {
                        return Err("E04 frozen trace inverse".into());
                    }
                    expected[cell * WRITE_BYTES..(cell + 1) * WRITE_BYTES]
                        .copy_from_slice(&fixture.write_input(index)?);
                }
            }
            let mut actual = Vec::with_capacity(expected.len());
            let length = view.read(
                ports,
                fixture.serial,
                at,
                expected.len() as u32,
                &mut actual,
            )?;
            if length != expected.len() as u64 || actual.as_slice() != expected {
                return Err(format!("E04 final bytes differ at complete window{at}").into());
            }
            actual_hash.update(&actual);
            expected_hash.update(&expected);
            at += expected.len() as u64;
            checked = at;
        }
        if hex(&original_hash.finish()) != fixture.base_sha256
            || base.metadata()?.modified()? != before.modified()?
        {
            return Err("E04 independent initial input changed during oracle".into());
        }
        let mut eof = Vec::new();
        if view.read(ports, fixture.serial, BASE_BYTES, 1, &mut eof)? != 0 || !eof.is_empty() {
            return Err("E04 exact final EOF".into());
        }
        let fresh = fixture.upstream.operation().map_err(ScopeRefusal)?;
        operation = Some(fresh);
        let reader = operation
            .as_ref()
            .ok_or("E04 oracle operation owner")?
            .client();
        base.seek(SeekFrom::Start(0))?;
        let mut old = OldRoot {
            expected: base,
            hash: Sha256::new(),
            bytes: 0,
            failure: None,
        };
        let old_result = Timing::disabled("E04 old-root observer", |scope| {
            layerfs_content::read_all_bounded(
                reader,
                fixture.stat.value.content_root,
                BASE_BYTES,
                &mut old,
                scope.child("read"),
            )
        })
        .0;
        if let Err(content) = old_result {
            return Err(Box::new(OldRootFailure {
                content,
                original: old.failure.take(),
            }));
        }
        if old.bytes != BASE_BYTES || hex(&old.hash.finish()) != fixture.base_sha256 {
            return Err("E04 old immutable root changed".into());
        }
        let request = ClientRequest::control(Operation::Binding, None, 0)?;
        let message = fixture.calls.call(request).map_err(PortFailure::Call)?;
        match ReplyView::decode(message.bytes()) {
            Ok(ReplyView::Binding(binding)) if &binding == fixture.upstream.binding() => (),
            _ => return Err(Box::new(PortFailure::Remote(message))),
        }
        Ok((
            OracleReport {
                final_sha256: hex(&actual_hash.finish()),
                expected_sha256: hex(&expected_hash.finish()),
                bytes: checked,
                mode: stat.metadata.mode,
                nlink: stat.namespace_refs,
                mtime_seconds: stat.metadata.mtime_seconds,
                mtime_nanoseconds: stat.metadata.mtime_nanoseconds,
                old_file_content_root_unchanged: true,
                namespace_membership_checked: true,
                root_serial,
                namespace_entries: membership.entries.len() as u64,
                binding_unchanged: true,
            },
            message,
        ))
    })();
    match result {
        Ok(report) => Ok(report),
        Err(cause) => Err(OracleFailure {
            cause,
            bytes_checked: checked,
            operation,
        }),
    }
}
#[derive(Debug)]
struct ScopeRefusal(layerfs_daemon::upstream::OperationRefusal);
impl fmt::Display for ScopeRefusal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for ScopeRefusal {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0.error)
    }
}
struct OldRoot {
    expected: File,
    hash: Sha256,
    bytes: u64,
    failure: Option<io::Error>,
}
impl Write for OldRoot {
    fn write(&mut self, mut bytes: &[u8]) -> io::Result<usize> {
        if self.failure.is_some() {
            return Err(io::Error::other("E04 original old-root refusal retained"));
        }
        let length = bytes.len();
        let mut expected = [0; 65_536];
        while !bytes.is_empty() {
            let count = bytes.len().min(expected.len());
            if let Err(original) = self.expected.read_exact(&mut expected[..count]) {
                self.failure = Some(original);
                return Err(io::Error::other("E04 original old-root refusal retained"));
            }
            if bytes[..count] != expected[..count] {
                self.failure = Some(io::Error::other("E04 immutable old bytes differ"));
                return Err(io::Error::other("E04 original old-root refusal retained"));
            }
            self.hash.update(&bytes[..count]);
            self.bytes += count as u64;
            bytes = &bytes[count..];
        }
        Ok(length)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Content's Write adapter reports Io; retain the sink's original cause beside
/// that original typed Content result instead of replacing either one.
#[derive(Debug)]
struct OldRootFailure {
    content: layerfs_content::ContentError,
    original: Option<io::Error>,
}
impl fmt::Display for OldRootFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for OldRootFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.original {
            Some(original) => Some(original),
            None => Some(&self.content),
        }
    }
}
