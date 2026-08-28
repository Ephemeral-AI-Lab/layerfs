use layerfs_core::ObjectId;
use layerfs_workspace::{
    DirectDriver, LeaseKind, OperationId, OperationState, OperationWorkspace, Presentation,
    WorkspaceDriver, WorkspaceError, WorkspacePaths, WorkspaceTicket,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn ticket(presentation: Presentation) -> WorkspaceTicket {
    WorkspaceTicket {
        operation_id: OperationId([0x11; 32]),
        working_storage_id: [0x22; 32],
        expected_branch_generation: 7,
        base_root: ObjectId::for_bytes(b"base"),
        nonce: [0x33; 16],
        presentation,
    }
}

#[test]
fn direct_lifecycle_quiesces_once_and_binds_the_exact_base() {
    let first_ticket = ticket(Presentation::Direct);
    let base = first_ticket.base_root;
    let (mut workspace, begin) =
        OperationWorkspace::start(first_ticket, DirectDriver::default(), None).unwrap();
    assert_eq!(begin.state, OperationState::Active);
    let writer = workspace.leases().acquire(LeaseKind::Writer).unwrap();
    assert!(workspace.freeze(Duration::from_millis(1)).is_err());
    drop(writer);
    assert_eq!(workspace.state(), OperationState::Incomplete);

    let ticket = ticket(Presentation::Direct);
    let (mut workspace, _) =
        OperationWorkspace::start(ticket, DirectDriver::default(), None).unwrap();
    workspace.freeze(Duration::from_secs(1)).unwrap();
    assert!(workspace
        .finalize_candidate(ObjectId::for_bytes(b"wrong"), base, Vec::new())
        .is_err());
    let candidate = workspace
        .finalize_candidate(base, ObjectId::for_bytes(b"candidate"), b"delta".to_vec())
        .unwrap();
    assert_eq!(candidate.base_root, base);
    assert_eq!(candidate.normalized_transition, b"delta");
    let end = workspace.cleanup().unwrap();
    assert!(end.cleanup_complete);
    assert_eq!(end.candidate_root, Some(candidate.candidate_root));
}

struct TimedDirectDriver;

impl WorkspaceDriver for TimedDirectDriver {
    fn presentation(&self) -> Presentation {
        Presentation::Direct
    }

    fn view_path(&self) -> Option<&Path> {
        None
    }

    fn freeze(&mut self) -> layerfs_workspace::Result<()> {
        std::thread::sleep(Duration::from_millis(20));
        Ok(())
    }

    fn cleanup(&mut self) -> layerfs_workspace::Result<()> {
        Ok(())
    }
}

#[test]
fn freeze_observation_does_not_charge_driver_work_to_quiescence() {
    let (mut workspace, _) =
        OperationWorkspace::start(ticket(Presentation::Direct), TimedDirectDriver, None).unwrap();
    let observation = workspace.freeze_observed(Duration::from_secs(1)).unwrap();
    assert!(observation.driver_freeze_ns >= Duration::from_millis(20).as_nanos());
    assert!(observation.quiescence_ns < observation.driver_freeze_ns);
}

#[test]
fn custody_requires_the_exact_marker_before_removal() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let mut paths = WorkspacePaths::create(&root, &ticket(Presentation::Mount)).unwrap();
    assert!(paths.validate().is_ok());
    std::fs::write(paths.root().join("owner"), b"substituted").unwrap();
    assert!(paths.remove_owned().is_err());
    assert!(root.join("workspaces").exists());
    std::fs::remove_dir_all(root).unwrap();
}

struct TestDriver {
    view: PathBuf,
    cleanups: Arc<AtomicUsize>,
}

impl WorkspaceDriver for TestDriver {
    fn presentation(&self) -> Presentation {
        Presentation::Mount
    }

    fn view_path(&self) -> Option<&Path> {
        Some(&self.view)
    }

    fn freeze(&mut self) -> layerfs_workspace::Result<()> {
        Ok(())
    }

    fn cleanup(&mut self) -> layerfs_workspace::Result<()> {
        self.cleanups.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn failed_quiescence_cannot_clean_driver_or_remove_custody() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-quiescence-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let ticket = ticket(Presentation::Mount);
    let paths = WorkspacePaths::create(&root, &ticket).unwrap();
    let custody = paths.root().to_owned();
    let cleanups = Arc::new(AtomicUsize::new(0));
    let driver = TestDriver {
        view: paths.view().to_owned(),
        cleanups: cleanups.clone(),
    };
    let (mut workspace, _) = OperationWorkspace::start(ticket, driver, Some(paths)).unwrap();
    let writer = workspace.leases().acquire(LeaseKind::Writer).unwrap();
    assert!(matches!(
        workspace.freeze(Duration::from_millis(1)),
        Err(WorkspaceError::Timeout)
    ));
    assert!(matches!(workspace.cleanup(), Err(WorkspaceError::Busy)));
    assert_eq!(cleanups.load(Ordering::Relaxed), 0);
    assert!(custody.exists());
    drop(writer);
    assert!(workspace.cleanup().unwrap().cleanup_complete);
    assert_eq!(cleanups.load(Ordering::Relaxed), 1);
    assert!(!custody.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn displaced_custody_is_preserved_instead_of_deleting_the_substitute() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-displacement-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let mut paths = WorkspacePaths::create(&root, &ticket(Presentation::Mount)).unwrap();
    let original = paths.root().to_owned();
    let displaced = root.join("displaced");
    std::fs::rename(&original, &displaced).unwrap();
    std::fs::create_dir(&original).unwrap();
    std::fs::create_dir(original.join("view")).unwrap();
    std::fs::create_dir(original.join("spool")).unwrap();
    std::fs::copy(displaced.join("owner"), original.join("owner")).unwrap();
    std::fs::copy(displaced.join("recovery"), original.join("recovery")).unwrap();

    assert!(matches!(
        paths.remove_owned(),
        Err(WorkspaceError::OwnershipMismatch)
    ));
    assert!(original.exists());
    assert!(displaced.exists());
    std::fs::remove_dir_all(root).unwrap();
}
