//! Complete fixed old/proposed rebind custody until SQL and native acknowledgement.
use super::{header::Header, plan::Plan, session::Resource};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{GraphMemoryLease, GraphSubject, StateSelection};
#[derive(Clone, Copy, Debug)]
pub(crate) enum Phase {
    Prepared,
    CommitKnown,
    NativeKnown,
}
pub(crate) struct HeaderSnapshot {
    bytes: [u8; 386],
    length: usize,
}
impl HeaderSnapshot {
    pub(crate) fn new(header: &Header) -> StorageResult<Self> {
        let b = header.as_bytes();
        if b.len() > 386 {
            return Err(StorageError::Integrity("scratch rebind header width"));
        }
        let mut bytes = [0; 386];
        bytes[..b.len()].copy_from_slice(b);
        Ok(Self {
            bytes,
            length: b.len(),
        })
    }
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}
pub(crate) struct Attempt {
    pub(crate) before_header: HeaderSnapshot,
    pub(crate) before_selection: StateSelection,
    pub(crate) before_plan: Plan,
    pub(crate) before_subject: Option<GraphSubject>,
    pub(crate) proposed: Box<super::rebind_context::Context>,
    pub(crate) phase: Phase,
    // One same existing controller pays actual Box/temporary vector/fixed controls.
    pub(crate) memory: Option<GraphMemoryLease>,
}
impl Attempt {
    pub(crate) fn prepare(
        r: &Resource,
        selection: StateSelection,
        plan: Plan,
        subject: Option<GraphSubject>,
    ) -> StorageResult<Self> {
        if r.plan.version() != plan.version() || r.native.reserved_bytes != plan.scratch_bytes() {
            return Err(StorageError::Integrity("scratch rebind exact native class"));
        }
        let bytes = std::mem::size_of::<Self>()
            + std::mem::size_of::<super::rebind_context::Context>()
            + 194
            + std::mem::size_of::<blake3::Hasher>()
            + 386;
        let memory = r
            .graph_memory
            .as_ref()
            .map(|m| m.reserve(bytes))
            .transpose()?;
        if memory.is_none() {
            let fixed = bytes + 2 * std::mem::size_of::<super::draft_state::Drafts>();
            if plan.version() != 6 || fixed > super::draft_state::WORKING_BYTES {
                return Err(StorageError::Integrity("draft rebind fixed working class"));
            }
        }
        let before_header = HeaderSnapshot::new(
            r.header
                .as_ref()
                .ok_or(StorageError::Integrity("scratch rebind missing header"))?,
        )?;
        let context = super::rebind_context::Context::prepare(r, selection, plan, subject)?;
        Ok(Self {
            before_header,
            before_selection: r.selection.clone(),
            before_plan: r.plan,
            before_subject: r.graph_subject.clone(),
            proposed: Box::new(context),
            phase: Phase::Prepared,
            memory,
        })
    }
    pub(crate) fn description(&self) -> String {
        format!("rebind {:?}; before token={} selector={:?} version={} S={} subject={:?} header={:?}; proposed token={} selector={:?} version={} S={} subject={:?} header={:?}",self.phase,self.before_selection.token(),self.before_selection.selector(),self.before_plan.version(),self.before_plan.scratch_bytes(),self.before_subject,self.before_header.as_bytes(),self.proposed.selection.token(),self.proposed.selection.selector(),self.proposed.plan.version(),self.proposed.plan.scratch_bytes(),self.proposed.subject,self.proposed.header.as_bytes())
    }
}
