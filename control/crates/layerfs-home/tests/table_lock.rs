//! `helpers::lock` is the lock of a process table each holder changes in
//! one step, so it is taken back from a holder that panicked
//! (layerfs-project#177): a panic in one call of a worker never refuses the
//! calls after it.
use layerfs_home::helpers::lock;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[test]
fn a_table_lock_is_taken_back_from_a_holder_that_panicked() {
    let table = Arc::new(Mutex::new(BTreeMap::from([("kept", 1)])));
    let holder = {
        let table = table.clone();
        std::thread::spawn(move || {
            let mut held = lock(&table).unwrap();
            held.insert("added", 2);
            panic!("a holder panics with the table's guard");
        })
    };
    assert!(holder.join().is_err());
    assert!(table.is_poisoned());

    // the table is whole, and later holders read and change it
    let mut held = lock(&table).expect("taken back, not refused");
    assert_eq!(held.len(), 2);
    held.remove("added");
    drop(held);
    assert_eq!(*lock(&table).unwrap(), BTreeMap::from([("kept", 1)]));
}
