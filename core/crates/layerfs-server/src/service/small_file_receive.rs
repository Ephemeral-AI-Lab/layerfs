//! Actual ordinary v1 EOF/kind/table proof before small authority confirmation.
use super::{error::content, save::validation::validate_inode_role};
use layerfs_bridge::contract::*;
use layerfs_content::{
    filesystem::{
        rows::{PendingSmallFiles, SmallFileBuilder, VerifiedSmallFileRows},
        state::VerifiedSmallFileState,
        InodeUpdate,
    },
    object::inode_leaf::{InodeKind, InodeValue},
    AuthenticatedObjects, ObjectId,
};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{io::Read, time::Instant};
struct Sink<'a, 'r> {
    builder: &'a mut SmallFileBuilder,
    reader: &'a dyn AuthenticatedObjects,
    deadline: Instant,
    scope: &'a TimingScope<'r, Active>,
}
impl PreparedBindingSink for Sink<'_, '_> {
    fn begin_directory(&mut self, _parent: u64, _bindings: u32) -> Result<(), Failure> {
        Err(Code::InvalidInput.into())
    }
    fn binding(&mut self, _name: &[u8], _child: Option<u64>) -> Result<(), Failure> {
        Err(Code::InvalidInput.into())
    }
    fn end_directory(&mut self, _completion: PreparedDirectoryCompletion) -> Result<(), Failure> {
        Err(Code::InvalidInput.into())
    }
    fn identity(&mut self, row: PreparedIdentity) -> Result<(), Failure> {
        if Instant::now() >= self.deadline {
            return Err(Code::Deadline.into());
        }
        let PreparedIdentity::Rooted {
            serial,
            kind: 1,
            content: root,
            metadata,
            fresh: false,
        } = row
        else {
            return Err(Code::InvalidInput.into());
        };
        let root = ObjectId::from_bytes(&root).map_err(content)?;
        let metadata = ObjectId::from_bytes(&metadata).map_err(content)?;
        validate_inode_role(
            self.reader,
            InodeKind::RegularFile,
            root,
            metadata,
            self.scope,
        )?;
        self.builder
            .push(
                self.reader,
                InodeUpdate {
                    serial,
                    value: InodeValue {
                        kind: InodeKind::RegularFile,
                        namespace_ref_count: 0,
                        content_root: root,
                        metadata_root: metadata,
                    },
                },
            )
            .map_err(content)
    }
}
/// The selected class validates actual base/file kinds and exact source/body EOF.
pub(crate) fn receive(
    changes: &PreparedChanges,
    input: &mut dyn Read,
    pending: PendingSmallFiles,
    reader: &dyn AuthenticatedObjects,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<(VerifiedSmallFileRows, VerifiedSmallFileState), Failure> {
    let subject = pending.subject();
    if !super::small_file_admission::selected(changes)?
        || pending.selector() != Some(super::small_file_admission::selector(changes))
        || subject.base().map(|r| *r.0.as_bytes()) != Some(changes.base)
        || subject.namespace().object().as_bytes() != &changes.scope
        || subject.root_serial() != changes.root_serial
    {
        return Err(Code::InvalidInput.into());
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let mut builder = SmallFileBuilder::from_pending(pending, reader).map_err(content)?;
    read_prepared_bindings(
        &changes.totals,
        changes.root_serial,
        input,
        &mut Sink {
            builder: &mut builder,
            reader,
            deadline,
            scope,
        },
    )?;
    let rows = builder.finish(input).map_err(content)?;
    if rows.len() as u64 != changes.totals.identities {
        return Err(Code::InvalidInput.into());
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let state = VerifiedSmallFileState::from_rows(&rows).map_err(content)?;
    Ok((rows, state))
}
