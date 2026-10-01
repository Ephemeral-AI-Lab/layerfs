//! Real scoped graph owner layouts and System allocator result lifetimes.
//! Requested layouts are separate from SQLite heap, allocator metadata and RSS.

#[path = "support/allocation_observer.rs"]
mod allocation_observer;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_oracle.rs"]
#[allow(dead_code)]
mod graph_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_oracle.rs"]
#[allow(dead_code)]
mod roots_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::filesystem::state::{GraphMemory, GRAPH_WORKING_BYTES};

#[global_allocator]
static ALLOCATOR: allocation_observer::ObservedSystem = allocation_observer::ObservedSystem;

#[test]
fn exact_scoped_credit_refuses_before_allocation_and_follows_last_owner() {
    let memory = GraphMemory::new();
    let initial = memory.reserved_bytes();
    assert_eq!(initial, GraphMemory::control_bytes());
    let first = memory.reserve(4096).unwrap();
    let second = memory
        .reserve(GRAPH_WORKING_BYTES - initial - 4096)
        .unwrap();
    assert_eq!(memory.reserved_bytes(), GRAPH_WORKING_BYTES);
    assert!(memory.reserve(1).is_err());
    let shared = memory.clone();
    drop(memory);
    assert_eq!(shared.reserved_bytes(), GRAPH_WORKING_BYTES);
    drop(first);
    assert_eq!(shared.reserved_bytes(), GRAPH_WORKING_BYTES - 4096);
    drop(second);
    assert_eq!(shared.reserved_bytes(), initial);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn maximum_real_attempt_and_held_result_are_charged_before_sql_and_actual_drop() {
    use allocation_observer::{Observation, Pause};
    use fixture::*;
    use layerfs_content::filesystem::state::{GraphBuildAck, GraphNodeChange, GraphNodeKey};
    use layerfs_content::ContentError;
    use layerfs_storage::construction_state::{ScratchAuthority, ScratchSession};
    use layerfs_storage::StorageError;
    use std::mem::size_of;
    use support::TempDir;

    with_source(128, true, |source, _| {
        let temp = TempDir::new("graph_scoped_layout");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, 128);
        empty_sites(&mut session, &scopes);
        let owner = status(&authority, session.selection().token());
        let layout = owner.graph_working.clone().unwrap();
        assert_eq!(layout.limit_bytes, GRAPH_WORKING_BYTES);
        assert_eq!(layout.session_bytes, size_of::<ScratchSession>());
        assert!(layout.resource_bytes < layout.maximum_attempt_bytes);
        assert!(layout.attempt_owner_bytes < layout.attempt_bytes);
        assert!(
            layout.control_bytes
                + layout.graph_bytes
                + layout.maximum_attempt_bytes
                + layout.maximum_ack_bytes
                <= GRAPH_WORKING_BYTES
        );
        let baseline = layout.reserved_bytes();
        assert_eq!(baseline, layout.control_bytes + layout.graph_bytes);
        let keys: Vec<_> = (1..=128)
            .map(|serial| GraphNodeKey::new(scopes.graph(), serial).unwrap())
            .collect();

        let mut held_acks = Vec::with_capacity(16);
        let node_window_bytes =
            128 * size_of::<Option<layerfs_content::filesystem::state::GraphNode>>();
        let observation = Observation::start(
            [layout.attempt_bytes, layout.graph_bytes],
            [128 * size_of::<GraphNodeChange>(), node_window_bytes],
        );
        let ack = session.graph_seed_batch(scopes.graph(), &keys).unwrap();
        let observed = observation.stop();
        assert!(!observed.overflow);
        assert_eq!(observed.peak[0], layout.attempt_bytes);
        assert_eq!(
            observed.live[0], 0,
            "acknowledged attempt Box actually released"
        );
        assert_eq!(observed.live[1], 128 * size_of::<GraphNodeChange>());
        let charged = size_of::<GraphBuildAck>() + observed.live[1];
        assert_eq!(ack.reserved_memory_bytes(), Some(charged));
        assert_eq!(layout.reserved_bytes(), baseline + charged);
        assert_eq!(ack.nodes().len(), 128);

        // Hold every full result that its actual closed Seed class permits.
        while layout.reserved_bytes() + layout.seed_attempt_bytes + charged <= GRAPH_WORKING_BYTES {
            let result = session.graph_seed_batch(scopes.graph(), &keys).unwrap();
            let _pause = Pause::new();
            held_acks.push(result);
        }
        assert!(held_acks.len() < 16, "finite actual shared working class");
        let held_credit = charged * (1 + held_acks.len());
        let connection = external(&owner.path);
        let before = scalar(&connection, "SELECT nodes FROM graph_owner");
        let result = session.graph_seed_batch(scopes.graph(), &keys);
        assert!(matches!(
            result,
            Err(StorageError::Content(ContentError::ResourceUnavailable {
                what: "graph.working_memory"
            }))
        ));
        assert_eq!(scalar(&connection, "SELECT nodes FROM graph_owner"), before);
        assert_eq!(
            scalar(&connection, "SELECT COUNT(*) FROM graph_nodes"),
            before
        );
        assert_eq!(layout.reserved_bytes(), baseline + held_credit);
        drop(connection);
        // Explicit known failed-owner release closes/removes its native resource;
        // it cannot refund the independent consumer still holding these bytes.
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
        assert_eq!(layout.reserved_bytes(), layout.control_bytes + held_credit);
        drop(ack);
        drop(held_acks);
        assert_eq!(layout.reserved_bytes(), layout.control_bytes);
        let released = observation.released();
        assert_eq!(
            released.live_total(),
            0,
            "all captured Rust owners actually freed"
        );
        let _pause = Pause::new();
        eprintln!("graph scoped layout: {layout:?}; maximum ACK={charged}; held ACK bytes={held_credit}; actual Seed attempt={} Mutation maximum={} largest={} ; System requested={observed:?}; released={released:?}; independent native16MiB refunded only after explicit release; native/allocator metadata/RSS/aggregate provider containment unavailable",layout.seed_attempt_bytes,layout.mutation_attempt_bytes,layout.maximum_attempt_bytes);
    });
}
