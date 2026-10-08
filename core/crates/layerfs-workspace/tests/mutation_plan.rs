//! Public resumable mutation rounds over the same real Content/Overlay fixture.
mod common;
mod harness;
use harness::{create, Bench, T1, T2};
use layerfs_workspace::{JobOutcome, MutationStage, Operation, Outcome, Refusal, WorkspaceError};
use std::{fmt, sync::Arc};

#[test]
fn preparation_and_owner_rounds_do_no_provider_work_and_publish_once() {
    let b = Bench::new("mutation-plan");
    b.window(|view| {
        let serial = b.workspace.next_serial(&b.allocator).unwrap();
        let before = b.demand();
        let mut plan = b.workspace.prepare_mutation(view, create(1, "planned"), T1, Some(serial)).unwrap();
        assert_eq!(plan.stage(), MutationStage::Owner);
        assert_eq!(b.demand(), before);
        let initial = b.overlay.state(b.route()).unwrap().revision;
        let first = plan.job().unwrap().perform(&b.overlay);
        assert_eq!(b.demand(), before);
        assert!(plan.accept(first).unwrap().is_none());
        assert_eq!(plan.stage(), MutationStage::Base);
        assert!(plan.job().is_none());
        assert_eq!(b.overlay.state(b.route()).unwrap().revision, initial);
        plan.supply(view).unwrap();
        assert!(b.demand() > before);
        assert_eq!(plan.stage(), MutationStage::Owner);
        let before_owner = b.demand();
        let original = plan.job().unwrap().perform(&b.overlay);
        assert_eq!(b.demand(), before_owner);
        let Some(Outcome::Applied { publication, stat }) = plan.accept(original).unwrap() else { panic!("missing original publication") };
        assert_eq!(stat.unwrap().serial, serial);
        assert_eq!(plan.stage(), MutationStage::Finished);
        assert!(plan.job().is_none());
        assert!(matches!(plan.operation(), Operation::Create { name, .. } if name.as_bytes()==b"planned"));
        assert_eq!(b.overlay.state(b.route()).unwrap().revision, initial + 1);
        assert!(plan.accept(Ok(JobOutcome::Unchanged { inode: None })).is_err());
        assert!(plan.supply(view).is_err());
        assert_eq!(b.overlay.state(b.route()).unwrap().revision, initial + 1);
        b.overlay.reply_attempted(publication).unwrap();
    });
    assert!(b.lookup(1, "planned").is_some());
}

#[test]
fn later_owner_round_rechecks_current_namespace_after_a_park() {
    let b = Bench::new("mutation-plan-interleave");
    b.window(|view| {
        let serial = b.workspace.next_serial(&b.allocator).unwrap();
        let mut plan = b
            .workspace
            .prepare_mutation(view, create(1, "raced"), T1, Some(serial))
            .unwrap();
        let first = plan.job().unwrap().perform(&b.overlay);
        assert!(plan.accept(first).unwrap().is_none());
        let winner = b.applied(create(1, "raced"), T2).unwrap();
        assert_ne!(winner.serial, serial);
        let revision = b.overlay.state(b.route()).unwrap().revision;
        plan.supply(view).unwrap();
        let decision = plan.job().unwrap().perform(&b.overlay);
        assert!(matches!(
            plan.accept(decision),
            Err(WorkspaceError::Refused(Refusal::Exists))
        ));
        assert_eq!(plan.stage(), MutationStage::Finished);
        assert!(plan.job().is_none());
        assert_eq!(b.overlay.state(b.route()).unwrap().revision, revision);
        assert_eq!(b.lookup(1, "raced").unwrap(), winner);
    });
}

#[test]
fn preparation_refusal_returns_original_input_before_provider_or_sql_effects() {
    let b = Bench::new("mutation-plan-input");
    b.window(|view| {
        let before = b.demand();
        let revision = b.overlay.state(b.route()).unwrap().revision;
        for serial in [None, Some(0), Some(u64::MAX)] {
            let failure = match b.workspace.prepare_mutation(view, create(1, "original"), T1, serial) {
                Err(failure) => failure,
                Ok(_) => panic!("invalid serial admitted"),
            };
            assert!(matches!(failure.operation, Operation::Create { name, .. } if name.as_bytes()==b"original"));
        }
        assert_eq!(b.demand(), before);
        assert_eq!(b.overlay.state(b.route()).unwrap().revision, revision);
    });
}

#[derive(Debug)]
struct Original(Arc<()>);
impl fmt::Display for Original {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "original failure")
    }
}
impl std::error::Error for Original {}

#[test]
fn original_owner_error_is_returned_and_cannot_reenter_an_owner_round() {
    let b = Bench::new("mutation-plan-error");
    b.window(|view| {
        let serial = b.workspace.next_serial(&b.allocator).unwrap();
        let mut plan = b
            .workspace
            .prepare_mutation(view, create(1, "failed"), T1, Some(serial))
            .unwrap();
        let marker = Arc::new(());
        let error = WorkspaceError::Service(Box::new(Original(marker.clone())));
        let Err(WorkspaceError::Service(original)) = plan.accept(Err(error)) else {
            panic!("lost original error")
        };
        assert!(Arc::ptr_eq(
            &original.downcast_ref::<Original>().unwrap().0,
            &marker
        ));
        assert_eq!(plan.stage(), MutationStage::Finished);
        assert!(plan.job().is_none());
        assert!(
            matches!(plan.operation(), Operation::Create { name, .. } if name.as_bytes()==b"failed")
        );
        assert_eq!(b.overlay.state(b.route()).unwrap().revision, 0);
    });
}

#[test]
fn different_source_and_failed_base_demand_finish_without_discarding_input() {
    let b = Bench::with_cache("mutation-plan-base-error", 0);
    b.window(|view| {
        let serial = b.workspace.next_serial(&b.allocator).unwrap();
        let mut wrong = b.workspace.prepare_mutation(view, create(1, "wrong-source"), T1, Some(serial)).unwrap();
        let first = wrong.job().unwrap().perform(&b.overlay);
        assert!(wrong.accept(first).unwrap().is_none());
        b.window(|other| {
            let before = b.demand();
            assert!(matches!(wrong.supply(other), Err(WorkspaceError::Overlay(layerfs_overlay::OverlayError::Stale))));
            assert_eq!(b.demand(), before);
        });
        assert!(wrong.job().is_none());
        let next = b.workspace.next_serial(&b.allocator).unwrap();
        let mut missing = b.workspace.prepare_mutation(view, create(1, "missing-base"), T1, Some(next)).unwrap();
        let first = missing.job().unwrap().perform(&b.overlay);
        assert!(missing.accept(first).unwrap().is_none());
        b.fixture.store.objects.lock().unwrap().clear();
        let charge = missing.charge();
        assert!(missing.supply(view).is_err());
        assert_eq!(missing.stage(), MutationStage::Finished);
        assert!(missing.job().is_none());
        assert!(missing.charge() >= charge);
        assert!(matches!(missing.operation(), Operation::Create { name, .. } if name.as_bytes()==b"missing-base"));
        assert_eq!(b.overlay.state(b.route()).unwrap().revision, 0);
    });
}
