//! Issued live operation identity and exact owner/phase/table association.
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use crate::error::{ContentError, ContentResult};

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct Issued {
    selector: [u8; 32],
    token: u64,
    binding: Option<[u8; 32]>,
    shared: AtomicBool,
}

/// A live issued identity. Raw tokens/headers cannot construct this capability.
#[derive(Debug)]
pub struct StateSelection(Arc<Issued>);

impl Clone for StateSelection {
    fn clone(&self) -> Self {
        self.0.shared.store(true, Ordering::Release);
        Self(Arc::clone(&self.0))
    }
}

impl PartialEq for StateSelection {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) && self.0.binding == other.0.binding
    }
}
impl Eq for StateSelection {}

impl StateSelection {
    /// Burns one checked token from the live process issuer.
    pub fn issue(selector: [u8; 32]) -> ContentResult<Self> {
        if selector == [0; 32] {
            return Err(ContentError::InvalidOrderingRecord("state selector"));
        }
        let token = NEXT_TOKEN
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |next| {
                if next == 0 {
                    None
                } else {
                    Some(next.checked_add(1).unwrap_or(0))
                }
            })
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "indexed_state.tokens",
            })?;
        Ok(Self(Arc::new(Issued {
            selector,
            token,
            binding: None,
            shared: AtomicBool::new(false),
        })))
    }

    /// Associates the exact native or logical owner once, before sharing.
    /// The supplying owner proves its binding; C1 performs no native I/O.
    pub fn bind_owner(&mut self, binding: [u8; 32]) -> ContentResult<()> {
        if binding == [0; 32] {
            return Err(ContentError::InvalidOrderingRecord("state owner binding"));
        }
        let issued = Arc::get_mut(&mut self.0).ok_or(ContentError::InvalidOrderingRecord(
            "state selection shared",
        ))?;
        if issued.shared.load(Ordering::Acquire) {
            return Err(ContentError::InvalidOrderingRecord(
                "state selection shared",
            ));
        }
        if issued.binding.is_some() {
            return Err(ContentError::InvalidOrderingRecord("state selection bound"));
        }
        issued.binding = Some(binding);
        Ok(())
    }

    /// Complete operation selector, without prefix or digest truncation.
    pub fn selector(&self) -> &[u8; 32] {
        &self.0.selector
    }

    /// Checked compact token issued for this exact live attempt.
    pub fn token(&self) -> u64 {
        self.0.token
    }

    /// Exact owner association; an unbound selection has no usable scope.
    pub fn owner_binding(&self) -> ContentResult<&[u8; 32]> {
        self.0
            .binding
            .as_ref()
            .ok_or(ContentError::InvalidOrderingRecord(
                "state selection unbound",
            ))
    }
}

/// Closed tables with implemented producers and distinct private codecs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StateTable {
    /// Ordered serial-to-directory-root facts produced by filesystem construction.
    DirectoryRoots = 1,
    /// Exclusive directory/symlink binding membership during validation.
    BindingClaims = 2,
    /// Exclusive sites and monotone base facts for one selected row source.
    BindingSites = 3,
    /// Closed effective directory vertices and external solver state.
    GraphNodes = 4,
    /// Closed effective directory arcs and exact multiplicity.
    GraphEdges = 5,
}

impl StateTable {
    /// Private codec tag for the implemented table.
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// Encoded full selector, token, owner binding, phase and table width.
pub const STATE_SCOPE_BYTES: usize = 81;

/// Exact bound operation/owner and immutable phase/table context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateScope {
    selection: StateSelection,
    binding: [u8; 32],
    phase: u64,
    table: StateTable,
}

impl StateScope {
    /// Selects one nonzero phase/table of an already bound live owner.
    pub fn new(selection: StateSelection, phase: u64, table: StateTable) -> ContentResult<Self> {
        if phase == 0 {
            return Err(ContentError::InvalidOrderingRecord("state phase"));
        }
        let binding = *selection.owner_binding()?;
        Ok(Self {
            selection,
            binding,
            phase,
            table,
        })
    }

    /// The issued identity and its exact owner association.
    pub fn selection(&self) -> &StateSelection {
        &self.selection
    }

    /// Immutable selected phase number.
    pub const fn phase(&self) -> u64 {
        self.phase
    }

    /// Implemented selected table.
    pub const fn table(&self) -> StateTable {
        self.table
    }

    /// selector32/token8/owner-binding32/phase8/table1, with no truncation.
    pub fn as_bytes(&self) -> [u8; STATE_SCOPE_BYTES] {
        let mut bytes = [0; STATE_SCOPE_BYTES];
        bytes[..32].copy_from_slice(self.selection.selector());
        bytes[32..40].copy_from_slice(&self.selection.token().to_be_bytes());
        bytes[40..72].copy_from_slice(&self.binding);
        bytes[72..80].copy_from_slice(&self.phase.to_be_bytes());
        bytes[80] = self.table.code();
        bytes
    }
}
