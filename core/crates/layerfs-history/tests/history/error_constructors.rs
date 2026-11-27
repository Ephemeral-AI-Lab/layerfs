//! A provider outside this crate builds each refusal context through the
//! public constructors, and readers still reach the public fields.

use layerfs_history::error::{
    HolderKind, HolderState, NameHolder, NameSource, NameTakenState, RemovedPath, RemovedState,
};
use layerfs_history::HistoryError;

#[test]
fn name_taken_context_is_constructed_and_read() {
    let holder = NameHolder::new(HolderKind::Branch, "feature", HolderState::Retired);
    assert_eq!(holder.kind, HolderKind::Branch);
    assert_eq!(holder.name, "feature");
    assert_eq!(holder.state, HolderState::Retired);

    let state = NameTakenState::new(holder, NameSource::Home);
    assert_eq!(state.source, NameSource::Home);
    assert_eq!(state.holder.name, "feature");

    let error = HistoryError::NameTaken(Box::new(state));
    match error.cause() {
        HistoryError::NameTaken(state) => {
            assert_eq!(state.holder.kind, HolderKind::Branch);
            assert_eq!(state.holder.state, HolderState::Retired);
            assert_eq!(state.source, NameSource::Home);
        }
        other => panic!("expected NameTaken, got {other:?}"),
    }
}

#[test]
fn removed_context_is_constructed_and_read() {
    let path = RemovedPath::new("/gone.txt", 42);
    assert_eq!(path.path, "/gone.txt");
    assert_eq!(path.frontier_serial, 42);

    let state = RemovedState::new(vec![path, RemovedPath::new("/other.txt", 7)]);
    assert_eq!(state.paths.len(), 2);
    assert_eq!(state.paths[1].path, "/other.txt");

    let error = HistoryError::Removed(Box::new(state));
    match error.cause() {
        HistoryError::Removed(state) => {
            assert_eq!(state.paths[0].path, "/gone.txt");
            assert_eq!(state.paths[0].frontier_serial, 42);
            assert_eq!(state.paths[1].frontier_serial, 7);
        }
        other => panic!("expected Removed, got {other:?}"),
    }
}

#[test]
fn every_holder_kind_and_state_builds() {
    for kind in [HolderKind::Branch, HolderKind::Project] {
        for state in [
            HolderState::Live,
            HolderState::Retired,
            HolderState::Deleted,
        ] {
            let holder = NameHolder::new(kind, "name", state);
            assert_eq!(holder.kind, kind);
            assert_eq!(holder.state, state);
        }
    }
    for source in [NameSource::Store, NameSource::Home] {
        let holder = NameHolder::new(HolderKind::Project, "project", HolderState::Live);
        let state = NameTakenState::new(holder, source);
        assert_eq!(state.source, source);
    }
}
