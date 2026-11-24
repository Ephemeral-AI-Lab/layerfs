//! The typed, read-only view of one durable mount admission.
//!
//! An operator preview and the doctor's worker-side read report the same
//! facts about a holder: its operation identity, the names and place it
//! recorded, and the authority state the Store keeps. This module names that
//! view once, built from a [`GcIntent`]; it never changes an intent.
use super::records::GcIntent;

/// One mount admission as an operator reads it, including the place
/// authority the Store recorded. The view is read-only: building or reading
/// it changes no intent and releases nothing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountHolderView {
    /// The 16-byte operation identity (`gc_intent.op_id`).
    pub op: [u8; 16],
    /// The owning project name, when the Store knows one.
    pub project: Option<String>,
    /// The owning Branch name, when the Store knows one.
    pub branch: Option<String>,
    /// The mounting Workspace identity, when the intent names one.
    pub workspace: Option<[u8; 32]>,
    /// The Sandbox name, when the intent names one.
    pub sandbox: Option<String>,
    /// The machine name, when the intent names one.
    pub machine: Option<String>,
    /// The owning runtime token.
    pub owner_runtime: Option<String>,
    /// The owning boot token.
    pub owner_boot: Option<String>,
    /// The machine or worker generation that placed the intent, when known.
    pub machine_incarnation: Option<String>,
    /// The Sandbox generation that placed the intent, when known.
    pub sandbox_generation: Option<String>,
    /// True when the owner is uncertain and only `Unresolved` may report it.
    pub uncertain: bool,
}

impl MountHolderView {
    /// The view of one mount intent.
    #[must_use]
    pub fn of(intent: &GcIntent) -> Self {
        Self {
            op: intent.op_id,
            project: intent.project.clone(),
            branch: intent.branch.clone(),
            workspace: intent.workspace,
            sandbox: intent.sandbox.clone(),
            machine: intent.machine.clone(),
            owner_runtime: intent.owner_runtime.clone(),
            owner_boot: intent.owner_boot.clone(),
            machine_incarnation: intent.machine_incarnation.clone(),
            sandbox_generation: intent.sandbox_generation.clone(),
            uncertain: intent.uncertain,
        }
    }

    /// The state an operator report shows: `live` or `uncertain`.
    #[must_use]
    pub fn state(&self) -> &'static str {
        if self.uncertain {
            "uncertain"
        } else {
            "live"
        }
    }

    /// True when the row has neither owner nor place evidence: the only rows
    /// `settle_legacy` may mark. A row with owner tokens but no place, or a
    /// place but no owner, is legacy too; a row with both is not.
    #[must_use]
    pub fn legacy(&self) -> bool {
        self.owner_runtime.is_none()
            || self.owner_boot.is_none()
            || (self.sandbox.is_none() && self.machine.is_none())
    }
}
