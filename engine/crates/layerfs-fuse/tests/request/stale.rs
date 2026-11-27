//! Which failed steps end their request alone as a stale serial: only the
//! engine's own Stale answer, never a wrapper that merely carries one.
use layerfs_content::ContentError;
use layerfs_vfs::ports::mount::{stale, ServiceError};
use layerfs_overlay::{KernelObservation, OverlayError};
use layerfs_vfs::{KernelReadDecision, KernelReadFailure, WorkspaceError};
use std::sync::Arc;

/// A reading visit whose job answered `result` with no decision.
fn visit(result: Result<(), OverlayError>) -> ServiceError {
    Box::new(KernelReadFailure {
        reason: "kernel read visit did not produce a usable original answer",
        original: Arc::new(KernelObservation::<KernelReadDecision> {
            decision: None,
            result: result.map(|()| None),
            open_candidate: None,
            directory_candidate: None,
        }),
    })
}

#[test]
fn only_the_engines_own_stale_answer_is_a_stale_serial() {
    // The visit's job answered Stale and rolled back.
    assert!(stale(&visit(Err(OverlayError::Stale))));
    // The window finished outside the owner found the serial gone.
    assert!(stale(
        &(Box::new(WorkspaceError::Overlay(OverlayError::Stale)) as ServiceError)
    ));

    // An uncertain outcome is never classified from its cause.
    assert!(!stale(&visit(Err(OverlayError::Uncertain {
        cause: Box::new(OverlayError::Stale),
        completion: None,
    }))));
    // Corruption stays what it is.
    assert!(!stale(
        &(Box::new(WorkspaceError::Content(ContentError::InvalidRecord(
            "directory entry count"
        ))) as ServiceError)
    ));
    assert!(!stale(&visit(Err(OverlayError::Invalid(
        "kernel entry final"
    )))));
    // A bare overlay Stale, as an owner wrapper's cause would show it, is not
    // one of the two shapes.
    assert!(!stale(&(Box::new(OverlayError::Stale) as ServiceError)));
    // A decided visit carries no stale answer.
    assert!(!stale(&visit(Ok(()))));
}
