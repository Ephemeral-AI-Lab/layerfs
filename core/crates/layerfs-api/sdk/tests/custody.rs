//! Typed failure classification and actual host serving-scope completion custody.
use layerfs_sdk::runtime::custody::{failure_knowledge, FailureKnowledge};
use layerfs_sdk::RuntimeError;
use layerfs_storage::StorageError;
use std::sync::Arc;

#[test]
fn typed_unknown_and_known_cleanup_failures_have_distinct_knowledge() {
    // These inspect public owning error values only. No provider/runtime operation
    // is forced to manufacture an unknown publication or cleanup failure.
    let known = RuntimeError::Storage(Arc::new(StorageError::CleanupFailed {
        original: Box::new(StorageError::Integrity("original known failure")),
        cleanup: Box::new(StorageError::Io(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        ))),
    }));
    assert_eq!(failure_knowledge(&known), FailureKnowledge::RetainedFailure);
    let unknown = RuntimeError::Storage(Arc::new(StorageError::UnknownOutcome {
        original: Box::new(StorageError::Integrity(
            "original unacknowledged publication",
        )),
    }));
    assert_eq!(
        failure_knowledge(&unknown),
        FailureKnowledge::TerminalUnknown
    );
    for error in [
        StorageError::CleanupFailed {
            original: Box::new(StorageError::UnknownOutcome {
                original: Box::new(StorageError::Integrity("publication")),
            }),
            cleanup: Box::new(StorageError::Integrity("known cleanup failure")),
        },
        StorageError::CleanupFailed {
            original: Box::new(StorageError::Integrity("known publication failure")),
            cleanup: Box::new(StorageError::UnknownOutcome {
                original: Box::new(StorageError::Integrity("cleanup")),
            }),
        },
    ] {
        assert_eq!(
            failure_knowledge(&RuntimeError::Storage(Arc::new(error))),
            FailureKnowledge::TerminalUnknown
        );
    }
    let workspace = layerfs_history::WorkspaceId::from_authority([9; 32]).unwrap();
    let history =
        layerfs_history::HistoryError::UnknownOutcome.with_observed_stage(workspace, Some(None));
    assert!(history.unknown());
    assert_eq!(
        failure_knowledge(&RuntimeError::History(history)),
        FailureKnowledge::TerminalUnknown
    );
    assert_eq!(
        failure_knowledge(&RuntimeError::Denied),
        FailureKnowledge::Known
    );
}

#[cfg(target_os = "macos")]
#[path = "support/supervisor.rs"]
mod fixture;
#[cfg(target_os = "macos")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod sockets;

#[cfg(target_os = "macos")]
mod host {
    use super::{fixture::Fixture, sockets, FailureKnowledge};
    use layerfs_content::{encode_whole_file_payload, ObjectId, ObjectRole};
    use layerfs_history::WorkspaceId;
    use layerfs_sdk::{
        runtime::{
            custody::{CustodyDisposition, ScopeFenceError, ScopeFenceRefusal},
            service::*,
        },
        CompletionPhase, ObjectReply, RuntimeError, RuntimeResult,
    };
    use layerfs_telemetry::timer::Timing;

    fn workspace(value: u8) -> WorkspaceId {
        WorkspaceId::from_authority([value; 32]).unwrap()
    }
    struct Objects(Vec<(ObjectId, Vec<u8>)>);
    impl ObjectReply for Objects {
        fn object(&mut self, id: ObjectId, canonical: &[u8]) -> RuntimeResult<()> {
            self.0.push((id, canonical.to_vec()));
            Ok(())
        }
    }
    #[test]
    fn one_consuming_scope_fence_moves_fixed_slots_active_producer_and_known_stage() {
        let mut fixture = Fixture::new(1);
        let branch = fixture.branch;
        let root = fixture.root;
        let (_client, server) = sockets::pair();
        let mut sessions = fixture.runtime.sessions();
        let binding = sessions.bind(&server.peer, workspace(1), branch).unwrap();
        let active = sessions.begin(&binding).unwrap();
        let finished = sessions.begin(&binding).unwrap();
        assert!(sessions
            .finish(&binding, finished)
            .unwrap()
            .outcome()
            .is_ok());
        let stage = sessions
            .stage_saved(&binding, finished, root, 1)
            .unwrap()
            .as_ref()
            .unwrap()
            .clone();
        let aborted = sessions.begin(&binding).unwrap();
        assert_eq!(
            sessions.abort(&binding, aborted).unwrap().phase(),
            CompletionPhase::Abort
        );
        let custody = sessions.fence().unwrap();
        assert_eq!(custody.incarnation(), [7; 32]);
        assert_eq!(custody.slots().len(), 4);
        assert!(custody.slots()[3].is_none());
        let active_record = custody.slots()[0].as_ref().unwrap();
        assert_eq!(active_record.save().token(), active.token());
        assert_eq!(active_record.binding(), &binding);
        assert_eq!(
            active_record.disposition(),
            CustodyDisposition::ActiveProducerEnded
        );
        assert!(active_record.producer_ended());
        assert!(active_record.completion().is_none());
        assert!(!active_record.history_unknown());
        let finished_record = custody.slots()[1].as_ref().unwrap();
        assert_eq!(finished_record.save(), finished);
        assert_eq!(
            finished_record.disposition(),
            CustodyDisposition::KnownTerminal
        );
        assert!(!finished_record.producer_ended());
        assert!(finished_record.retains_custody());
        assert!(!finished_record.history_unknown());
        assert_eq!(
            finished_record.history().stage().unwrap().as_ref().unwrap(),
            &stage
        );
        assert!(finished_record.history().commit().is_none());
        assert!(finished_record.history().discard().is_none());
        let aborted_record = custody.slots()[2].as_ref().unwrap();
        assert_eq!(aborted_record.save(), aborted);
        assert_eq!(
            aborted_record.disposition(),
            CustodyDisposition::KnownTerminal
        );
        assert_eq!(
            aborted_record.completion().unwrap().phase(),
            CompletionPhase::Abort
        );
        assert!(!aborted_record.retains_custody());
        let mut next = fixture.runtime.sessions();
        assert!(matches!(
            next.completion(&binding, active),
            Err(RuntimeError::StaleCapability)
        ));
        let fresh = next.begin(&binding).unwrap();
        assert_ne!(fresh.token(), active.token());
        assert!(matches!(
            next.completion(&binding, finished),
            Err(RuntimeError::StaleCapability)
        ));
        drop(next.fence().unwrap());
        // Original acknowledged stage/token remains with application custody;
        // the scope fence did not discard it or create a history transition.
        let mut records = custody.into_slots();
        let (_, exact_save, completion, history) = records[1].take().unwrap().into_parts();
        assert_eq!(exact_save, finished);
        assert_eq!(completion.unwrap().phase(), CompletionPhase::Finish);
        assert_eq!(history.stage().unwrap().as_ref().unwrap(), &stage);
    }
    #[test]
    fn held_cancelled_and_dispatched_completions_refuse_fence_before_any_slot_effect() {
        let mut fixture = Fixture::new(1);
        let branch = fixture.branch;
        let (_client, server) = sockets::pair();
        let mut sessions = fixture.runtime.sessions();
        let binding = sessions.bind(&server.peer, workspace(2), branch).unwrap();
        let active = sessions.begin(&binding).unwrap();
        let mut service = Service::new(&mut sessions, ServiceConfig::default()).unwrap();
        let queued_connection = service.connect(&server.peer, binding.clone()).unwrap();
        let queued = service
            .try_submit(queued_connection, Request::Policy)
            .map_err(|(e, _)| e)
            .unwrap();
        let fence = service.disconnect(queued_connection).unwrap();
        assert_eq!(fence.cancelled, 1);
        let cancelled = service.take_completion(queued).unwrap().unwrap();
        assert!(matches!(
            cancelled.outcome(),
            ServiceOutcome::Unattempted(Request::Policy)
        ));
        let known_connection = service.connect(&server.peer, binding.clone()).unwrap();
        let known = service
            .try_submit(known_connection, Request::Policy)
            .map_err(|(e, _)| e)
            .unwrap();
        assert_eq!(service.step(), Some(known));
        let completed = service.take_completion(known).unwrap().unwrap();
        assert!(matches!(
            completed.outcome(),
            ServiceOutcome::Dispatched(Ok(Response::Policy(_)))
        ));
        drop(service);
        let refusal = sessions
            .fence()
            .expect_err("caller-held original credits retain this scope");
        assert!(matches!(
            refusal.error,
            ScopeFenceError::RetainedCustody { owners: 2 }
        ));
        let ScopeFenceRefusal { sessions, .. } = refusal;
        assert!(matches!(
            sessions.completion(&binding, active),
            Err(RuntimeError::Invalid("Save not completed"))
        ));
        assert_eq!(sessions.bound_snapshot(&binding).unwrap(), binding);
        assert!(matches!(
            cancelled.outcome(),
            ServiceOutcome::Unattempted(Request::Policy)
        ));
        assert!(matches!(
            completed.outcome(),
            ServiceOutcome::Dispatched(Ok(Response::Policy(_)))
        ));
        drop((cancelled, completed));
        // A new explicit fence after exact owner release; there is no automatic
        // retry loop or replay of the cancelled/provider operations.
        let custody = sessions.fence().unwrap();
        assert_eq!(custody.slots()[0].as_ref().unwrap().save(), active);
        assert_eq!(
            custody.slots()[0].as_ref().unwrap().disposition(),
            CustodyDisposition::ActiveProducerEnded
        );
    }
    #[test]
    fn fresh_runtime_epoch_refuses_old_binding_and_save_with_equal_captured_context() {
        let mut old = Fixture::new(1);
        let mut fresh = Fixture::new_with_incarnation(1, [8; 32]);
        assert_eq!(old.runtime.catalog(), fresh.runtime.catalog());
        assert_eq!(old.payload, fresh.payload);
        assert!(!old.denied.get());
        assert!(!fresh.denied.get());
        let (_client, server) = sockets::pair();
        let mut sessions = old.runtime.sessions();
        let binding = sessions
            .bind(&server.peer, workspace(3), old.branch)
            .unwrap();
        let save = sessions.begin(&binding).unwrap();
        let old_custody = sessions.fence().unwrap();
        let mut next = fresh.runtime.sessions();
        let next_binding = next.bind(&server.peer, workspace(3), fresh.branch).unwrap();
        assert_eq!(binding.snapshot(), next_binding.snapshot());
        assert_eq!(binding.peer(), next_binding.peer());
        assert!(matches!(
            next.policy(&binding),
            Err(RuntimeError::StaleCapability)
        ));
        assert!(matches!(
            next.begin(&binding),
            Err(RuntimeError::StaleCapability)
        ));
        let new_save = next.begin(&next_binding).unwrap();
        assert_ne!(save.token(), new_save.token());
        assert!(matches!(
            next.completion(&next_binding, save),
            Err(RuntimeError::StaleCapability)
        ));
        assert_eq!(old_custody.slots()[0].as_ref().unwrap().save(), save);
        assert_eq!(
            old_custody.slots()[0].as_ref().unwrap().save_knowledge(),
            FailureKnowledge::Known
        );
        drop(next.fence().unwrap());
        // These are initialized Stores with equal catalog/root facts and fresh
        // runtime epochs; this is no process-crash or persisted-session recovery.
    }
    #[test]
    fn ending_active_producer_preserves_an_already_acknowledged_publication_wave() {
        let mut fixture = Fixture::new(1);
        let branch = fixture.branch;
        let (_client, server) = sockets::pair();
        let mut sessions = fixture.runtime.sessions();
        let binding = sessions.bind(&server.peer, workspace(4), branch).unwrap();
        let save = sessions.begin(&binding).unwrap();
        let mut first = None;
        // Valid WholeFile objects cross the existing 4MiB preparation window.
        // Deterministic incompressible inputs force real earlier pack publication,
        // while the final accepted object remains in the active producer.
        for index in 0..35u64 {
            let mut state = index + 1;
            let bytes: Vec<u8> = (0..120 * 1024)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state as u8
                })
                .collect();
            let canonical = encode_whole_file_payload(&bytes).unwrap();
            let id = ObjectId::for_bytes(&canonical);
            if index == 0 {
                first = Some((id, canonical.clone()));
            }
            Timing::disabled("acknowledged wave", |scope| {
                sessions.accept(
                    &binding,
                    save,
                    id,
                    ObjectRole::WholeFile,
                    canonical,
                    scope.child("accept"),
                )
            })
            .0
            .unwrap();
        }
        let (id, canonical) = first.unwrap();
        let mut before = Objects(Vec::new());
        sessions
            .read_objects(&binding, None, &[id], &mut before)
            .unwrap();
        assert_eq!(before.0, vec![(id, canonical.clone())]);
        let custody = sessions.fence().unwrap();
        let record = custody.slots()[0].as_ref().unwrap();
        assert_eq!(
            record.disposition(),
            CustodyDisposition::ActiveProducerEnded
        );
        assert!(record.completion().is_none());
        let next = fixture.runtime.sessions();
        let mut after = Objects(Vec::new());
        next.read_objects(&binding, None, &[id], &mut after)
            .unwrap();
        assert_eq!(after.0, vec![(id, canonical)]);
        assert!(record.history().stage().is_none());
        assert!(!record.history_unknown());
        drop(next.fence().unwrap());
    }
}
