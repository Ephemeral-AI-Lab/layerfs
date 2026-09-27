use super::page::{Kind, PageRef, BODY_BYTES};
use super::pages::PageStore;
use crate::{backing::budget::Charge, WorkspaceError};
use std::sync::{Arc, Mutex};

const RECORD_HEADER: usize = 48;
pub const TINY_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedSlot {
    pub logical_page: u64,
    pub ordinal: u16,
    pub inode: u64,
    pub generation: u64,
    pub revision: u64,
    pub offset: u64,
    pub length: u16,
}

struct Pending {
    candidate: PageRef,
    rewritten: Option<PageRef>,
}

struct State {
    tail: Option<PageRef>,
    logical_page: u64,
    next_logical: u64,
    sealed: bool,
    body: Vec<u8>,
    records: u16,
    pending: Option<Pending>,
    stopped: bool,
}

/// One tail shared by all regular files in a Workspace incarnation.
pub struct TinyPack {
    store: Arc<PageStore>,
    state: Mutex<State>,
    _body_charge: Charge,
}

/// A complete new page that has not changed the active view. The caller must
/// publish its matching index changes, or explicitly abort this candidate.
pub struct PreparedSlot {
    pack: Arc<TinyPack>,
    candidate: PageRef,
    rewritten: Option<PageRef>,
    slot: PackedSlot,
    logical_page: u64,
    body: Vec<u8>,
    _body_charge: Charge,
    finished: bool,
}

impl Drop for PreparedSlot {
    fn drop(&mut self) {
        if !self.finished {
            if let Ok(mut state) = self.pack.state.lock() {
                state.stopped = true;
            }
        }
    }
}

impl TinyPack {
    pub fn new(store: Arc<PageStore>) -> Result<Arc<Self>, WorkspaceError> {
        let body_charge = store.budget().reserve(BODY_BYTES)?;
        Ok(Arc::new(Self {
            store,
            state: Mutex::new(State {
                tail: None,
                logical_page: 0,
                next_logical: 1,
                sealed: false,
                body: Vec::with_capacity(BODY_BYTES),
                records: 0,
                pending: None,
                stopped: false,
            }),
            _body_charge: body_charge,
        }))
    }

    pub fn prepare(
        self: &Arc<Self>,
        inode: u64,
        generation: u64,
        revision: u64,
        offset: u64,
        data: &[u8],
    ) -> Result<PreparedSlot, WorkspaceError> {
        if inode == 0
            || generation == 0
            || revision == 0
            || data.is_empty()
            || data.len() > TINY_LIMIT
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.pending.is_some() {
            return Err(WorkspaceError::Busy);
        }
        let fits = !state.sealed && state.body.len() + RECORD_HEADER + data.len() <= BODY_BYTES;
        let rewritten = if fits { state.tail } else { None };
        let logical_page = if rewritten.is_some() {
            state.logical_page
        } else {
            let selected = state.next_logical;
            state.next_logical = state
                .next_logical
                .checked_add(1)
                .ok_or(WorkspaceError::Capacity)?;
            selected
        };
        let mut body = Vec::with_capacity(BODY_BYTES);
        if fits {
            body.extend_from_slice(&state.body);
        }
        let ordinal = if fits { state.records } else { 0 };
        body.extend_from_slice(&inode.to_be_bytes());
        body.extend_from_slice(&generation.to_be_bytes());
        body.extend_from_slice(&revision.to_be_bytes());
        body.extend_from_slice(&offset.to_be_bytes());
        body.extend_from_slice(&logical_page.to_be_bytes());
        body.extend_from_slice(&(data.len() as u16).to_be_bytes());
        body.extend_from_slice(&ordinal.to_be_bytes());
        body.extend_from_slice(&[0; 4]);
        body.extend_from_slice(data);
        let charge = self.store.budget().reserve(BODY_BYTES)?;
        let candidate = self
            .store
            .create(Kind::Pack, generation, revision, ordinal + 1, &body)?;
        state.pending = Some(Pending {
            candidate,
            rewritten,
        });
        Ok(PreparedSlot {
            pack: self.clone(),
            candidate,
            rewritten,
            slot: PackedSlot {
                logical_page,
                ordinal,
                inode,
                generation,
                revision,
                offset,
                length: data.len() as u16,
            },
            logical_page,
            body,
            _body_charge: charge,
            finished: false,
        })
    }

    pub fn read(&self, slot: PackedSlot, physical: PageRef) -> Result<Vec<u8>, WorkspaceError> {
        let page = self.store.read(physical, Kind::Pack)?;
        let body = page.verify(Kind::Pack, self.store.incarnation(), physical)?;
        if slot.generation > page.generation() || slot.revision > page.revision() {
            return Err(WorkspaceError::Io);
        }
        let mut at = 0;
        for ordinal in 0..page.records() {
            if at + RECORD_HEADER > body.len() {
                return Err(WorkspaceError::Io);
            }
            let length = u16::from_be_bytes([body[at + 40], body[at + 41]]) as usize;
            let end = at + RECORD_HEADER + length;
            if length == 0
                || length > TINY_LIMIT
                || end > body.len()
                || body[at + 42..at + 44] != ordinal.to_be_bytes()
                || body[at + 44..at + 48] != [0; 4]
            {
                return Err(WorkspaceError::Io);
            }
            if ordinal == slot.ordinal {
                let get = |start: usize| -> Result<u64, WorkspaceError> {
                    Ok(u64::from_be_bytes(
                        body[at + start..at + start + 8]
                            .try_into()
                            .map_err(|_| WorkspaceError::Io)?,
                    ))
                };
                if get(0)? != slot.inode
                    || get(8)? != slot.generation
                    || get(16)? != slot.revision
                    || get(24)? != slot.offset
                    || get(32)? != slot.logical_page
                    || length != slot.length as usize
                {
                    return Err(WorkspaceError::Io);
                }
                return Ok(body[at + RECORD_HEADER..end].to_vec());
            }
            at = end;
        }
        Err(WorkspaceError::NotFound)
    }

    pub fn tail(&self) -> Result<Option<(u64, PageRef)>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(state.tail.map(|physical| (state.logical_page, physical)))
    }

    pub fn seal(&self) -> Result<Option<(u64, PageRef)>, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.pending.is_some() || state.stopped {
            return Err(WorkspaceError::Busy);
        }
        state.sealed = true;
        Ok(state.tail.map(|physical| (state.logical_page, physical)))
    }
}

impl PreparedSlot {
    pub fn slot(&self) -> PackedSlot {
        self.slot
    }
    /// One pooled locator changes when this candidate copies the current tail.
    pub fn locator(&self) -> (u64, PageRef) {
        (self.logical_page, self.candidate)
    }
    /// The previous physical tail can be released after the indexed view is
    /// published if no captured generation still pins it.
    pub fn retired_physical(&self) -> Option<PageRef> {
        self.rewritten
    }
    pub fn publish(mut self) -> Result<PackedSlot, WorkspaceError> {
        let mut state = self.pack.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.pending.as_ref().is_none_or(|pending| {
            pending.candidate != self.candidate || pending.rewritten != self.rewritten
        }) {
            return Err(WorkspaceError::Io);
        }
        state.tail = Some(self.candidate);
        state.logical_page = self.logical_page;
        state.sealed = false;
        state.records = self.slot.ordinal + 1;
        state.body = std::mem::take(&mut self.body);
        state.pending = None;
        self.finished = true;
        Ok(self.slot)
    }
    pub fn abort(mut self) -> Result<(), WorkspaceError> {
        self.pack.store.release(self.candidate)?;
        let mut state = self.pack.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state
            .pending
            .as_ref()
            .is_none_or(|pending| pending.candidate != self.candidate)
        {
            return Err(WorkspaceError::Io);
        }
        state.pending = None;
        self.finished = true;
        Ok(())
    }
}
