use super::error::VfsError;
use super::lease::{CaptureLease, LeaseState, WriterLease};
use super::managed_edit::SpoolPart;
use super::managed_lifecycle::{ManagedState, ManagedWorkspace};
use super::managed_state::refresh_error_state;
use crate::legacy_full::OperationCounters;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};

struct EndSeekFailure;

struct LaterEndSeekFailure {
    end_seeks: u8,
    len: u64,
    position: u64,
}

impl Read for EndSeekFailure {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl Write for EndSeekFailure {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for EndSeekFailure {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        match position {
            SeekFrom::End(_) => Err(std::io::Error::other("injected end-seek failure")),
            _ => Ok(0),
        }
    }
}

impl layerfs_materialization::driver::OwnedTempHandle for EndSeekFailure {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_len(&mut self, _len: u64) -> layerfs_materialization::driver::Result<()> {
        Err(layerfs_materialization::driver::DriverError::Unsupported)
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

impl Read for LaterEndSeekFailure {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl Write for LaterEndSeekFailure {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = u64::try_from(buffer.len()).unwrap();
        self.position += written;
        self.len = self.len.max(self.position);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for LaterEndSeekFailure {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        match position {
            SeekFrom::Start(position) => self.position = position,
            SeekFrom::Current(0) => {}
            SeekFrom::End(0) => {
                self.end_seeks += 1;
                if self.end_seeks > 1 {
                    return Err(std::io::Error::other("injected observation failure"));
                }
                self.position = self.len;
            }
            _ => return Err(std::io::Error::other("unsupported test seek")),
        }
        Ok(self.position)
    }
}

impl layerfs_materialization::driver::OwnedTempHandle for LaterEndSeekFailure {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_len(&mut self, len: u64) -> layerfs_materialization::driver::Result<()> {
        self.len = len;
        self.position = self.position.min(len);
        Ok(())
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

#[test]
fn initial_spool_seek_failure_is_fail_closed() {
    let mut workspace = ManagedWorkspace {
        external: None,
        edits: Vec::new(),
        state: ManagedState::Live,
        spool: Some(Box::new(EndSeekFailure)),
    };

    assert!(workspace
        .append_spool_parts(&[SpoolPart::Bytes(b"edit")])
        .is_err());
    assert_eq!(workspace.state, ManagedState::Indeterminate);
}

#[test]
fn post_append_spool_observation_failure_is_fail_closed() {
    let mut workspace = ManagedWorkspace {
        external: None,
        edits: Vec::new(),
        state: ManagedState::Live,
        spool: Some(Box::new(LaterEndSeekFailure {
            end_seeks: 0,
            len: 0,
            position: 0,
        })),
    };

    workspace
        .append_spool_parts(&[SpoolPart::Bytes(b"edit")])
        .unwrap();
    workspace.state = ManagedState::Dirty;
    assert!(workspace
        .observe_spool(&mut OperationCounters::default())
        .is_err());
    assert_eq!(workspace.state, ManagedState::Indeterminate);
}

#[test]
fn writer_and_capture_admission_are_one_atomic_state_transition() {
    let state = Arc::new(Mutex::new(LeaseState::default()));
    let writer = WriterLease::begin(state.clone()).unwrap();
    assert!(matches!(
        CaptureLease::begin(state.clone()),
        Err(VfsError::WorkspaceBusy)
    ));
    drop(writer);
    let mut capture = CaptureLease::begin(state.clone()).unwrap();
    assert!(matches!(
        WriterLease::begin(state.clone()),
        Err(VfsError::WorkspaceBusy)
    ));
    capture.finish().unwrap();
    assert!(matches!(
        WriterLease::begin(state),
        Err(VfsError::WorkspaceBusy)
    ));
}

#[test]
fn post_visibility_refresh_failure_requires_discard_or_rebuild() {
    assert_eq!(refresh_error_state(false), ManagedState::Live);
    assert_eq!(refresh_error_state(true), ManagedState::IncompleteDerived);
    let workspace = ManagedWorkspace {
        external: None,
        edits: Vec::new(),
        state: refresh_error_state(true),
        spool: None,
    };
    assert!(matches!(
        workspace.require_live(),
        Err(VfsError::IncompleteDerived)
    ));
}
