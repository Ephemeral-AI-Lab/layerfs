//! Prepared immutable binding and one known install attempt, preserving input.
use crate::{BaseView, Workspace, WorkspaceError, WorkspaceResult};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_overlay::{Capture, Overlay, OverlayError};

/// Scope-checked next immutable root retained before the SQL install attempt.
/// This is not proof of Save/history publication: that remains the caller's
/// exact prerequisite. Refusal/uncertainty returns this input without replay.
pub struct PreparedBase {
    capture: Capture,
    expected: FilesystemRootId,
    next: BaseView,
}
impl PreparedBase {
    pub const fn capture(&self) -> Capture {
        self.capture
    }
    pub fn root(&self) -> FilesystemRootId {
        self.next.identity()
    }
}
impl Workspace {
    /// Demand/validate one new canonical root before acquiring install state.
    /// No whole-tree validation/import/materialization is hidden in this step.
    pub fn prepare_base_install(
        &self,
        capture: Capture,
        root: FilesystemRootId,
    ) -> WorkspaceResult<PreparedBase> {
        if capture.route() != self.route() {
            return Err(OverlayError::Stale.into());
        }
        let base = self.base()?;
        let expected = base.identity();
        if capture.base_root != expected.0.to_bytes() {
            return Err(WorkspaceError::BaseChanged {
                expected: capture.base_root,
                actual: expected.0.to_bytes(),
            });
        }
        let next = base.rebind(root)?;
        Ok(PreparedBase {
            capture,
            expected,
            next,
        })
    }
    /// One local install after exact known upstream publication/UpToDate.
    /// Only the bounded SQL transition holds the binding lock; canonical IO was
    /// prepared outside it. On definite/uncertain failure both original error
    /// and prepared root/capture custody return. Never infer or resend history.
    /// Native actor composition must preserve this paired binding transition.
    pub fn install_prepared_base(
        &self,
        overlay: &Overlay,
        prepared: PreparedBase,
    ) -> Result<(), (WorkspaceError, PreparedBase)> {
        if prepared.capture.route() != self.route() {
            return Err((OverlayError::Stale.into(), prepared));
        }
        let mut base = match self.base.write() {
            Ok(base) => base,
            Err(_) => return Err((WorkspaceError::BindingPoisoned, prepared)),
        };
        if base.identity() != prepared.expected {
            return Err((
                WorkspaceError::BaseChanged {
                    expected: prepared.expected.0.to_bytes(),
                    actual: base.identity().0.to_bytes(),
                },
                prepared,
            ));
        }
        if let Err(error) = overlay.install(prepared.capture, prepared.next.identity().0.to_bytes())
        {
            return Err((error.into(), prepared));
        }
        // No fallible work follows known SQL success; old immutable plans own
        // their separate client/root and do not observe this pointer change.
        *base = prepared.next;
        Ok(())
    }
}

impl std::fmt::Debug for PreparedBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedBase")
            .field("capture", &self.capture)
            .field("expected", &self.expected)
            .field("next", &self.next.identity())
            .finish()
    }
}
