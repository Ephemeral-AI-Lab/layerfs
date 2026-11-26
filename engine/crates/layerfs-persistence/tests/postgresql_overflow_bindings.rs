//! Overflow binding transactions preserve the ordinary append blocks and sequence numbers.
#[path = "support/pg_chains.rs"]
mod chains;

#[test]
fn overflow_binding_appends_keep_three_ordered_acknowledgements_in_four_trips() {
    use layerfs_history::{BindingHint, HistoryCatalog};
    let url = chains::url().expect("private PostgreSQL fixture");
    let ordinary = chains::Store::new(&url, "ordinary_binding_batches");
    let overflow = chains::Store::new(&url, "overflow_binding_batches");
    let blocks = |store: &chains::Store| -> Vec<(i64, i64, Vec<u8>)> {
        store
            .admin()
            .query(
                "SELECT grp,seq,body FROM binding_hint_block ORDER BY grp,seq",
                &[],
            )
            .unwrap()
            .iter()
            .map(|row| (row.get(0), row.get(1), row.get(2)))
            .collect()
    };

    let rows: Vec<_> = (0..10_000)
        .map(|index| BindingHint {
            child: 64 + (index % 1024) as u64,
            parent: 1 + (index / 1024) as u64,
            name: format!("entry-{index:05}").into_bytes(),
        })
        .collect();
    let provider = &overflow.handles.storage;
    let before = provider.diagnostics().unwrap();
    for (index, rows) in rows.chunks(4096).enumerate() {
        ordinary.handles.history.record_bindings(rows).unwrap();
        let previous = provider.diagnostics().unwrap();
        overflow
            .handles
            .history
            .record_overflow_bindings(rows)
            .unwrap();
        let work = provider.diagnostics().unwrap().since(&previous);
        assert_eq!(work.round_trips, if index == 0 { 2 } else { 1 });
        assert_eq!(work.prepares, u64::from(index == 0));
        assert_eq!(work.write_commits, 1);
        assert_eq!(blocks(&overflow), blocks(&ordinary));
    }
    let work = provider.diagnostics().unwrap().since(&before);
    assert_eq!(
        (
            work.round_trips,
            work.prepares,
            work.write_transactions,
            work.write_commits
        ),
        (4, 1, 3, 3)
    );
    println!("BINDING_OVERFLOW_PARITY trips=4 prepares=1 committed=3 rows=10000");
}
