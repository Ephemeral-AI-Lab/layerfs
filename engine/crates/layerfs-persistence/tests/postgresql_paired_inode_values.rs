//! The captured writer combines two bounded immutable ordinal reads.
//! Needs LAYERFS_TEST_POSTGRES_URL.
#[path = "support/pg_chains.rs"]
mod chains;

use layerfs_content::{inode_leaf::INODE_VALUE_BYTES, ObjectId};
use layerfs_storage::{
    location::InodeValueRow,
    policy::{POOLED_VALUE_CACHE_BYTES, READ_OBJECT_LIMIT},
    port::{PackPersistence, PersistenceError, Publication},
};

const VALUE_ROOM: usize = POOLED_VALUE_CACHE_BYTES / INODE_VALUE_BYTES;

#[test]
fn pairs_pooled_values_on_an_ordinary_read_only_session() {
    let Some(url) = chains::url() else { return };
    let store = chains::Store::new(&url, "read_only_value_pair");
    let end = READ_OBJECT_LIMIT as u32;
    let first: Vec<_> = (1..=end).collect();
    let last = [end + 1];
    store
        .handles
        .storage
        .publish(&Publication {
            inode_values: vec![value(1), value(last[0])],
            ..Publication::default()
        })
        .unwrap();
    let read = layerfs_persistence::PostgresHandles::open_read_only(
        &store.selection,
        chains::BINDING,
        chains::CURSOR,
    )
    .unwrap();
    let provider = &read.storage;
    let mut ordinary = [Vec::new(), Vec::new()];
    provider.inode_values(&first, &mut ordinary[0]).unwrap();
    provider.inode_values(&last, &mut ordinary[1]).unwrap();
    assert_eq!(ordinary, [vec![value(1)], vec![value(last[0])]]);

    let before = provider.diagnostics().unwrap();
    let mut paired = [Vec::new(), Vec::new()];
    assert!(provider
        .inode_values_pair([&first, &last], &mut paired)
        .unwrap());
    let work = provider.diagnostics().unwrap().since(&before);
    assert_eq!(paired, ordinary);
    assert_eq!(
        (work.round_trips, work.statements, work.write_transactions),
        (1, 2, 0)
    );
}

fn value(ordinal: u32) -> InodeValueRow {
    let mut value = [0; INODE_VALUE_BYTES];
    value[9..41].copy_from_slice(ObjectId::for_bytes(&ordinal.to_le_bytes()).as_bytes());
    InodeValueRow { ordinal, value }
}

#[test]
fn pairs_exact_values_in_one_fenced_flight() {
    let Some(url) = chains::url() else { return };
    let store = chains::Store::new(&url, "inode_value_pair");
    let end = VALUE_ROOM as u32;
    let split = READ_OBJECT_LIMIT as u32;
    let first: Vec<_> = (1..=split).collect();
    let second: Vec<_> = (split + 1..=end).collect();
    let stored = (1..=end)
        .filter(|ordinal| *ordinal != split && *ordinal != split + 1)
        .map(value)
        .collect();
    let provider = &store.handles.storage;
    provider
        .publish(&Publication {
            inode_values: stored,
            ..Publication::default()
        })
        .unwrap();
    let mut ordinary = [Vec::new(), Vec::new()];
    provider.inode_values(&first, &mut ordinary[0]).unwrap();
    provider.inode_values(&second, &mut ordinary[1]).unwrap();
    assert_eq!(ordinary[0].len(), first.len() - 1);
    assert_eq!(ordinary[1].len(), second.len() - 1);
    let before = provider.diagnostics().unwrap();
    let mut paired = [Vec::new(), Vec::new()];
    assert!(provider
        .inode_values_pair([&first, &second], &mut paired)
        .unwrap());
    let after = provider.diagnostics().unwrap().since(&before);
    assert_eq!(paired, ordinary);
    assert_eq!(
        (after.round_trips, after.statements, after.transactions),
        (1, 2, 0)
    );

    let before = provider.diagnostics().unwrap();
    assert!(!provider
        .inode_values_pair([&first, &[]], &mut paired)
        .unwrap());
    assert_eq!(
        provider.inode_values_pair([&first, &first[..5]], &mut paired),
        Err(PersistenceError::Malformed)
    );
    assert_eq!(
        provider.diagnostics().unwrap().since(&before).round_trips,
        0
    );

    let mut admin = store.admin();
    admin
        .execute(
            "UPDATE history_meta SET catalog_incarnation=catalog_incarnation+1",
            &[],
        )
        .unwrap();
    assert!(matches!(
        provider.inode_values_pair([&first, &second], &mut paired),
        Err(PersistenceError::Refused { status }) if status == "Stale"
    ));
}

#[test]
fn malformed_second_reply_refuses_the_flight_once() {
    let Some(url) = chains::url() else { return };
    let store = chains::Store::new(&url, "inode_value_pair_refused");
    let first: Vec<_> = (1..=READ_OBJECT_LIMIT as u32).collect();
    let last = [READ_OBJECT_LIMIT as u32 + 1_000];
    let provider = &store.handles.storage;
    provider
        .publish(&Publication {
            inode_values: vec![value(1), value(last[0])],
            ..Publication::default()
        })
        .unwrap();
    provider.inode_values(&[1], &mut Vec::new()).unwrap();
    store
        .admin()
        .execute(
            "UPDATE inode_value_block SET body=set_byte(body,0,0) WHERE first_ordinal=$1",
            &[&i64::from(last[0])],
        )
        .unwrap();
    let before = provider.diagnostics().unwrap();
    let refused = provider.inode_values_pair([&first, &last], &mut [Vec::new(), Vec::new()]);
    let after = provider.diagnostics().unwrap().since(&before);
    assert_eq!(refused, Err(PersistenceError::Malformed));
    assert_eq!(
        (after.round_trips, after.statements, after.transactions),
        (1, 2, 0)
    );
}
