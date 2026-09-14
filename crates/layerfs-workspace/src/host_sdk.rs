//! One bounded host SDK attempt. Control uncertainty retains the exact scope;
//! after installation only its owned file reader is retried, never the edit.
use crate::host_operations::{HostOperations, ReadLease, ReadLeaseReservation};
use crate::overlay_budget::{Charge, Operation, Permit};
use crate::snapshot::Snapshot;
use crate::{WorkspaceFileRangeEdit, WorkspaceFileReplacement};
use layerfs_content::object::ContentDigestWriter;
use layerfs_fuse::{host_wire as wire, live_wire, NodeId};
use layerfs_layerstack_store::{Result, StoreError};
use std::io::{self, Write};
use std::ops::Range;
use std::sync::{Arc, Mutex};

// The existing control frame determines mask capacity. Inputs remain borrowed;
// no operation vector or repeated payload history is retained for recovery.
const MAX_RANGES: usize = (live_wire::MAX_FRAME - 38) / 16;
const MEMORY: u64 = (2 * live_wire::MAX_FRAME + 1024) as u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Recovery {
    Unchanged,
    Applied,
}

pub(crate) struct HostSdk {
    operations: Arc<HostOperations>,
    state: Mutex<State>,
}
struct State {
    next_scope: u64,
    pending: Option<Pending>,
    detached: bool,
}
struct Pending {
    scope: u64,
    node: NodeId,
    intent: [u8; 32],
    ranges: Vec<Range<u64>>,
    reservation: Option<ReadLeaseReservation>,
    phase: Phase,
    failure: Option<StoreError>,
    _memory: Permit,
}
enum Phase {
    Reserve,
    Begin,
    Install,
    Activate(Snapshot),
    Apply {
        lease: ReadLease,
        frame: Option<Vec<u8>>,
    },
    Cancel,
    Cleanup(Recovery),
}

impl HostSdk {
    pub(crate) fn new(operations: Arc<HostOperations>) -> Self {
        Self {
            operations,
            state: Mutex::new(State {
                next_scope: 1,
                pending: None,
                detached: false,
            }),
        }
    }

    /// A retry while pending must carry the same input. A different edit cannot
    /// substitute for the unresolved operation or change its target by path.
    pub(crate) fn edit(
        &self,
        path: &str,
        edits: &[WorkspaceFileRangeEdit],
        mut control: impl FnMut(&[u8]) -> io::Result<Vec<u8>>,
    ) -> Result<()> {
        let intent = fingerprint(path, edits)?;
        let mut state = self.state.lock().map_err(|_| StoreError::StoreBusy)?;
        if state.detached {
            return Err(StoreError::StoreBusy);
        }
        if let Some(pending) = &state.pending {
            if pending.intent != intent {
                return Err(StoreError::StoreBusy);
            }
        } else {
            let memory = self.operations.host.budget.enter(
                Operation::Request,
                Charge {
                    memory_bytes: MEMORY,
                    ..Charge::default()
                },
            )?;
            let ranges = masks(edits)?;
            let scope = state.next_scope;
            state.next_scope = scope
                .checked_add(1)
                .ok_or(StoreError::InvalidInput("SDK scope exhausted"))?;
            let node = self.operations.host.pin_path(path)?;
            // From here every failure retains the pin until known cleanup.
            state.pending = Some(Pending {
                scope,
                node,
                intent,
                ranges,
                reservation: None,
                phase: Phase::Reserve,
                failure: None,
                _memory: memory,
            });
        }
        self.drive(&mut state, Some(edits), &mut control)
            .map(|_| ())
    }

    /// Before installation, recovery cancels BEGIN whether or not its reply was
    /// received. After installation it finishes the original APPLY. An original
    /// preparation error is returned only after its cancellation and pin cleanup
    /// are known; callers can then start another independent SDK operation.
    pub(crate) fn recover(
        &self,
        mut control: impl FnMut(&[u8]) -> io::Result<Vec<u8>>,
    ) -> Result<Recovery> {
        let mut state = self.state.lock().map_err(|_| StoreError::StoreBusy)?;
        if state.detached {
            drop(state);
            return self.after_detach();
        }
        let Some(pending) = &mut state.pending else {
            return Ok(Recovery::Unchanged);
        };
        match pending.phase {
            Phase::Reserve => pending.phase = Phase::Cleanup(Recovery::Unchanged),
            Phase::Begin | Phase::Install => pending.phase = Phase::Cancel,
            _ => {}
        }
        self.drive(&mut state, None, &mut control)
    }

    /// Only after the caller has verified that the mounted consumer and its old
    /// control worker have exited. With no possible kernel writeback remaining,
    /// an installed but undelivered APPLY needs ownership retirement, not replay.
    /// This permanently retires SDK use of this runtime and is never a Commit
    /// operation. Any release/unpin failure leaves the exact pending owner here.
    pub(crate) fn after_detach(&self) -> Result<Recovery> {
        let mut state = self.state.lock().map_err(|_| StoreError::StoreBusy)?;
        state.detached = true;
        let Some(pending) = &mut state.pending else {
            return Ok(Recovery::Unchanged);
        };
        let outcome = match &pending.phase {
            Phase::Activate(_) | Phase::Apply { .. } => Recovery::Applied,
            Phase::Cleanup(outcome) => *outcome,
            _ => Recovery::Unchanged,
        };
        if let Phase::Apply { lease, .. } = &pending.phase {
            // The daemon may already have released it. This host operation is
            // idempotent and in-flight readers retain their own lease Arcs.
            self.operations.release_read_lease(lease.id)?;
        }
        // Remember release completion before unpin: a failed unpin retries only
        // the remaining ownership work and cannot resurrect an SDK control call.
        pending.phase = Phase::Cleanup(outcome);
        self.operations.host.unpin(pending.node)?;
        // The SDK caller already received a failed operation/control result.
        // Teardown reports the known installation outcome, not a new edit pass.
        state.pending.take();
        Ok(outcome)
    }

    pub(crate) fn is_pending(&self) -> Result<bool> {
        Ok(self
            .state
            .lock()
            .map_err(|_| StoreError::StoreBusy)?
            .pending
            .is_some())
    }

    fn drive(
        &self,
        state: &mut State,
        edits: Option<&[WorkspaceFileRangeEdit]>,
        control: &mut impl FnMut(&[u8]) -> io::Result<Vec<u8>>,
    ) -> Result<Recovery> {
        loop {
            let pending = state
                .pending
                .as_mut()
                .ok_or(StoreError::Integrity("SDK attempt"))?;
            match &mut pending.phase {
                Phase::Reserve => {
                    match self.operations.reserve_read_lease(pending.node) {
                        Ok(reservation) => pending.reservation = Some(reservation),
                        Err(error) => {
                            pending.failure = Some(error);
                            pending.phase = Phase::Cleanup(Recovery::Unchanged);
                            continue;
                        }
                    }
                    pending.phase = Phase::Begin;
                }
                Phase::Begin => {
                    request(
                        control,
                        &wire::coherence_begin(pending.scope, pending.node)?,
                    )?;
                    pending.phase = Phase::Install;
                }
                Phase::Install => {
                    let edits = edits.ok_or(StoreError::Integrity("SDK retry input"))?;
                    match self.operations.host.edit_file_snapshot(pending.node, edits) {
                        Ok(snapshot) => pending.phase = Phase::Activate(snapshot),
                        Err(error) => {
                            pending.failure = Some(error);
                            pending.phase = Phase::Cancel;
                        }
                    }
                }
                Phase::Activate(snapshot) => {
                    // install_read_lease consumes nothing on failure. Keeping
                    // this exact root makes retry independent of later writes.
                    let lease = self
                        .operations
                        .install_read_lease(&mut pending.reservation, snapshot)?;
                    pending.phase = Phase::Apply { lease, frame: None };
                }
                Phase::Apply { lease, frame } => {
                    if frame.is_none() {
                        *frame = Some(wire::coherence_apply(
                            pending.scope,
                            lease.id,
                            lease.node,
                            lease.size,
                            &pending.ranges,
                        )?);
                        // Frame now owns the exact mask, so only one mask copy
                        // survives while a control response is unresolved.
                        pending.ranges = Vec::new();
                    }
                    request(control, frame.as_ref().unwrap())?;
                    pending.phase = Phase::Cleanup(Recovery::Applied);
                }
                Phase::Cancel => {
                    request(control, &wire::coherence_cancel(pending.scope)?)?;
                    pending.phase = Phase::Cleanup(Recovery::Unchanged);
                }
                Phase::Cleanup(result) => {
                    let result = *result;
                    self.operations.host.unpin(pending.node)?;
                    let pending = state.pending.take().unwrap();
                    return match pending.failure {
                        Some(error) => Err(error),
                        None => Ok(result),
                    };
                }
            }
        }
    }
}

fn request(control: &mut impl FnMut(&[u8]) -> io::Result<Vec<u8>>, frame: &[u8]) -> Result<()> {
    if !control(frame)?.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "SDK control response").into());
    }
    Ok(())
}

fn fingerprint(path: &str, edits: &[WorkspaceFileRangeEdit]) -> Result<[u8; 32]> {
    // Preserve the existing remote EDIT_BEGIN path contract before any pin,
    // reservation or control scope can become externally visible.
    layerfs_content::CanonicalPath::new(path)?;
    let first = edits
        .first()
        .ok_or(StoreError::InvalidInput("workspace edit batch"))?;
    let mut digest = ContentDigestWriter::new();
    digest.write_all(b"layerfs/host-sdk/v1\0")?;
    digest.write_all(&(path.len() as u64).to_be_bytes())?;
    digest.write_all(path.as_bytes())?;
    digest.write_all(&first.workspace_id.bytes())?;
    digest.write_all(&(edits.len() as u64).to_be_bytes())?;
    for edit in edits {
        if edit.path != path || edit.workspace_id != first.workspace_id {
            return Err(StoreError::InvalidInput("workspace edit target"));
        }
        edit.start
            .checked_add(edit.delete_len)
            .ok_or(StoreError::InvalidInput("workspace edit range"))?;
        digest.write_all(&edit.start.to_be_bytes())?;
        digest.write_all(&edit.delete_len.to_be_bytes())?;
        match &edit.replacement {
            WorkspaceFileReplacement::Inline(bytes) => {
                if bytes.len() > layerfs_workspace_core::file_edit::MAX_INLINE_PER_EDIT {
                    return Err(StoreError::InvalidInput("workspace inline edit limit"));
                }
                digest.write_all(&[0])?;
                digest.write_all(&(bytes.len() as u64).to_be_bytes())?;
                digest.write_all(bytes)?;
            }
            WorkspaceFileReplacement::Zero(length) => {
                digest.write_all(&[1])?;
                digest.write_all(&length.to_be_bytes())?;
            }
        }
    }
    Ok(digest.finish())
}

fn masks(edits: &[WorkspaceFileRangeEdit]) -> Result<Vec<Range<u64>>> {
    let mut ranges: Vec<Range<u64>> = Vec::new();
    // Capacity is admitted before pinning or BEGIN, and never grows past the
    // existing control frame even for arbitrarily many overlapping edits.
    ranges
        .try_reserve_exact(MAX_RANGES)
        .map_err(|_| StoreError::StoreBusy)?;
    for edit in edits {
        let len = match &edit.replacement {
            WorkspaceFileReplacement::Inline(bytes) => bytes.len() as u64,
            WorkspaceFileReplacement::Zero(len) => *len,
        };
        let end = if len == edit.delete_len {
            edit.start
                .checked_add(len)
                .ok_or(StoreError::InvalidInput("workspace edit range"))?
        } else {
            u64::MAX
        };
        if end == edit.start {
            continue;
        }
        let mut range = edit.start..end;
        let first = ranges.partition_point(|old| old.end < range.start);
        let mut last = first;
        while last < ranges.len() && ranges[last].start <= range.end {
            range.start = range.start.min(ranges[last].start);
            range.end = range.end.max(ranges[last].end);
            last += 1;
        }
        if first == last {
            if ranges.len() == MAX_RANGES {
                return Err(StoreError::InvalidInput("SDK coherence mask capacity"));
            }
            ranges.insert(first, range);
        } else {
            ranges[first] = range;
            ranges.drain(first + 1..last);
        }
    }
    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_overlay::tests::Fixture;
    use layerfs_workspace_core::{ResourcePolicy, ROOT};
    use std::fs;

    fn edit(path: &str, start: u64, delete_len: u64, bytes: &[u8]) -> Vec<WorkspaceFileRangeEdit> {
        vec![WorkspaceFileRangeEdit {
            workspace_id: crate::WorkspaceId::new(),
            path: path.into(),
            start,
            delete_len,
            replacement: WorkspaceFileReplacement::Inline(bytes.to_vec()),
        }]
    }
    fn bytes(operations: &HostOperations, node: NodeId) -> Vec<u8> {
        let mut bytes = vec![0; operations.host.attr(node).unwrap().size as usize];
        let count = operations.host.read_into(node, 0, &mut bytes).unwrap();
        bytes.truncate(count);
        bytes
    }
    fn lost() -> io::Error {
        io::Error::new(io::ErrorKind::ConnectionReset, "injected lost reply")
    }
    fn pins(operations: &HostOperations, node: NodeId) -> u32 {
        operations
            .host
            .snapshot()
            .unwrap()
            .inode_record(node)
            .unwrap()
            .unwrap()
            .0
            .pins
    }

    #[test]
    fn unknown_control_retries_pin_exact_target_and_never_reapply_installed_edit() {
        let fixture = Fixture::new(
            |path| fs::write(path.join("file"), b"abcdef").unwrap(),
            ResourcePolicy::default(),
        );
        let operations = Arc::new(HostOperations::new(fixture.host.clone()));
        let sdk = HostSdk::new(operations.clone());
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        let edits = edit("file", 2, 0, b"X");
        let mut begin_frame = Vec::new();
        assert!(sdk
            .edit("file", &edits, |frame| {
                begin_frame = frame.to_vec();
                Err(lost())
            })
            .is_err());
        assert!(sdk.is_pending().unwrap());
        assert_eq!(bytes(&operations, node), b"abcdef");
        assert_eq!(pins(&operations, node), 1);
        // A changed path binding must not substitute another inode on retry.
        fixture
            .host
            .rename(ROOT, b"file", ROOT, b"moved", false)
            .unwrap();
        let other = fixture.host.create_file(ROOT, b"file", 0o644).unwrap().node;
        fixture.host.write(other, 0, b"other").unwrap();
        let different = edit("file", 2, 0, b"Y");
        assert_eq!(
            sdk.edit("file", &different, |_| panic!(
                "different intent dispatched"
            )),
            Err(StoreError::StoreBusy)
        );
        let mut apply_frame = Vec::new();
        assert!(sdk
            .edit("file", &edits, |frame| {
                match wire::decode_coherence(frame).unwrap() {
                    wire::Coherence::Begin { .. } => {
                        assert_eq!(frame, begin_frame);
                        Ok(Vec::new())
                    }
                    wire::Coherence::Apply {
                        lease,
                        node: leased,
                        ranges,
                        ..
                    } => {
                        assert_eq!(leased, node);
                        assert_eq!(
                            wire::coherence_ranges(ranges).collect::<Vec<_>>(),
                            vec![2..u64::MAX]
                        );
                        let request = wire::encode_request(wire::Request {
                            session: [42; 16],
                            sequence: 0,
                            acknowledged: 0,
                            operation: wire::Operation::ReadLease {
                                lease,
                                offset: 0,
                                size: 64,
                            },
                        })
                        .unwrap();
                        let response = operations.request(&request).unwrap();
                        match wire::decode_reply(&response, 0).unwrap() {
                            wire::Reply::Known(Ok(content)) => assert_eq!(content, b"abXcdef"),
                            _ => panic!("owned SDK read failed"),
                        }
                        // Subsequent live writes do not change this attempt's input.
                        fixture.host.write(node, 0, b"Q").unwrap();
                        operations.release_read_lease(lease).unwrap();
                        apply_frame = frame.to_vec();
                        Err(lost())
                    }
                    _ => panic!("unexpected cancellation"),
                }
            })
            .is_err());
        assert_eq!(bytes(&operations, node), b"QbXcdef");
        assert_eq!(bytes(&operations, other), b"other");
        sdk.edit("file", &edits, |frame| {
            assert_eq!(frame, apply_frame);
            Ok(Vec::new())
        })
        .unwrap();
        assert_eq!(bytes(&operations, node), b"QbXcdef");
        assert_eq!(pins(&operations, node), 0);
        assert!(!sdk.is_pending().unwrap());
        println!("SDK retry PASS: lost BEGIN, pinned rename target, same-body retry, exact reader, lost APPLY after later write, no duplicate edit");
    }

    #[test]
    fn preinstall_cancel_failure_retains_pin_and_failed_edit_changes_no_content() {
        let fixture = Fixture::new(
            |path| fs::write(path.join("file"), b"abc").unwrap(),
            ResourcePolicy::default(),
        );
        let operations = Arc::new(HostOperations::new(fixture.host.clone()));
        let sdk = HostSdk::new(operations.clone());
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        let valid = edit("file", 0, 1, b"X");
        let mut scope = 0;
        assert!(sdk
            .edit("file", &valid, |frame| {
                if let wire::Coherence::Begin { scope: id, .. } =
                    wire::decode_coherence(frame).unwrap()
                {
                    scope = id;
                }
                Err(lost())
            })
            .is_err());
        assert_eq!(sdk.recover(|frame| {
            assert!(matches!(wire::decode_coherence(frame).unwrap(), wire::Coherence::Cancel { scope: id } if id == scope));
            Ok(Vec::new())
        }).unwrap(), Recovery::Unchanged);
        assert_eq!(pins(&operations, node), 0);
        let invalid = edit("file", 50, 1, b"X");
        let mut cancel = Vec::new();
        assert!(sdk
            .edit("file", &invalid, |frame| {
                match wire::decode_coherence(frame).unwrap() {
                    wire::Coherence::Begin { .. } => Ok(Vec::new()),
                    wire::Coherence::Cancel { .. } => {
                        cancel = frame.to_vec();
                        Err(lost())
                    }
                    _ => panic!("failed edit reached APPLY"),
                }
            })
            .is_err());
        assert_eq!(pins(&operations, node), 1);
        assert_eq!(bytes(&operations, node), b"abc");
        assert!(sdk
            .recover(|frame| {
                assert_eq!(frame, cancel);
                Ok(Vec::new())
            })
            .is_err());
        assert!(!sdk.is_pending().unwrap());
        assert_eq!(pins(&operations, node), 0);
        assert_eq!(bytes(&operations, node), b"abc");
        println!("SDK cancellation PASS: unknown BEGIN can cancel; failed edit keeps old bytes; unknown CANCEL retains pin until known result");
    }

    #[test]
    fn sdk_reader_capacity_is_reserved_before_begin_or_content_installation() {
        let mut policy = ResourcePolicy::default();
        policy.overlay.max_readers = 2;
        let fixture = Fixture::new(|path| fs::write(path.join("file"), b"abc").unwrap(), policy);
        let operations = Arc::new(HostOperations::new(fixture.host.clone()));
        let sdk = HostSdk::new(operations.clone());
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        let mut reservations = Vec::new();
        for _ in 0..fixture.host.policy.overlay.max_readers {
            // The independent request admission may reach its existing cap
            // first; either admission outcome must precede BEGIN and mutation.
            match operations.reserve_read_lease(node) {
                Ok(slot) => reservations.push(slot),
                Err(_) => break,
            }
        }
        assert!(sdk
            .edit("file", &edit("file", 0, 1, b"X"), |_| panic!(
                "unadmitted SDK sent BEGIN"
            ))
            .is_err());
        assert_eq!(pins(&operations, node), 0);
        assert_eq!(bytes(&operations, node), b"abc");
        assert!(!sdk.is_pending().unwrap());
        println!("SDK admission PASS: existing reader/request capacity enforced before BEGIN, no payload change or leaked pin");
    }
    #[test]
    fn verified_detach_retires_undelivered_sdk_owners_but_preserves_other_snapshots() {
        let fixture = Fixture::new(
            |path| fs::write(path.join("file"), b"abc").unwrap(),
            ResourcePolicy::default(),
        );
        let operations = Arc::new(HostOperations::new(fixture.host.clone()));
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        let edits = edit("file", 0, 1, b"X");
        let before_begin = HostSdk::new(operations.clone());
        assert!(before_begin.edit("file", &edits, |_| Err(lost())).is_err());
        assert_eq!(pins(&operations, node), 1);
        assert_eq!(before_begin.after_detach().unwrap(), Recovery::Unchanged);
        assert_eq!(pins(&operations, node), 0);
        assert_eq!(bytes(&operations, node), b"abc");
        assert!(!before_begin.is_pending().unwrap());
        assert!(before_begin
            .edit("file", &edits, |_| panic!("retired SDK dispatched"))
            .is_err());

        let installed = HostSdk::new(operations.clone());
        let mut lease_id = 0;
        assert!(installed
            .edit("file", &edits, |frame| {
                match wire::decode_coherence(frame).unwrap() {
                    wire::Coherence::Begin { .. } => Ok(Vec::new()),
                    wire::Coherence::Apply { lease, .. } => {
                        lease_id = lease;
                        // The control consumer never sees this APPLY. Its detach
                        // therefore cannot know this host-side lease identity.
                        Err(lost())
                    }
                    _ => panic!("unexpected control message"),
                }
            })
            .is_err());
        assert_ne!(lease_id, 0);
        let independent = fixture.host.snapshot().unwrap();
        fixture.host.unlink(ROOT, b"file", false).unwrap();
        assert_eq!(pins(&operations, node), 1);
        assert_eq!(installed.after_detach().unwrap(), Recovery::Applied);
        assert!(fixture.host.attr(node).is_err());
        assert!(!installed.is_pending().unwrap());
        let request = wire::encode_request(wire::Request {
            session: [17; 16],
            sequence: 0,
            acknowledged: 0,
            operation: wire::Operation::ReadLease {
                lease: lease_id,
                offset: 0,
                size: 3,
            },
        })
        .unwrap();
        let response = operations.request(&request).unwrap();
        assert!(matches!(
            wire::decode_reply(&response, 0).unwrap(),
            wire::Reply::Known(Err(_))
        ));
        let mut content = [0; 3];
        assert_eq!(
            fixture
                .host
                .read_snapshot(&independent, node, 0, &mut content)
                .unwrap(),
            3
        );
        assert_eq!(&content, b"Xbc");
        assert_eq!(installed.after_detach().unwrap(), Recovery::Unchanged);
        assert_eq!(
            installed
                .recover(|_| panic!("retired recovery dispatched"))
                .unwrap(),
            Recovery::Unchanged
        );
        println!("SDK verified-detach PASS: unresolved BEGIN and undelivered APPLY owners retire; last pin releases orphan; independent snapshot retains exact bytes");
    }
    #[test]
    fn invalid_sdk_path_is_rejected_before_control_or_target_pin() {
        let fixture = Fixture::new(
            |path| fs::write(path.join("file"), b"abc").unwrap(),
            ResourcePolicy::default(),
        );
        let operations = Arc::new(HostOperations::new(fixture.host.clone()));
        let sdk = HostSdk::new(operations.clone());
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        for path in [
            "/file".to_owned(),
            "file/".to_owned(),
            "./file".to_owned(),
            "x".repeat(4097),
        ] {
            let mut controls = 0;
            let result = sdk.edit(&path, &edit(&path, 0, 1, b"X"), |_| {
                controls += 1;
                Err(lost())
            });
            assert_eq!(controls, 0, "invalid SDK path reached control: {path}");
            assert!(
                matches!(result, Err(StoreError::Core(_))),
                "expected existing canonical path validation: {result:?}"
            );
            assert_eq!(pins(&operations, node), 0);
            assert!(!sdk.is_pending().unwrap());
            assert_eq!(bytes(&operations, node), b"abc");
        }
    }
}
