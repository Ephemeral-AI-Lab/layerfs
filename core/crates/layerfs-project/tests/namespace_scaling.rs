//! Backed-count growth with fixed resident windows; not a whole-importer memory proof.
mod support;
use std::sync::Arc;
use support::*;
#[test]
fn backed_counts_grow_while_resident_buffers_stay_fixed() {
    let mut windows = Vec::new();
    let mut backing = Vec::new();
    for count in [100, 3000, 6000] {
        let fixture = Fixture::new(count);
        let storage = storage(Arc::new(memory_metadata::MemoryMetadata::default()));
        let history = memory_history::MemoryHistory::default();
        let initialized = fixture.run(&storage, &history);
        let w = initialized.namespace_work;
        assert_eq!(w.entries, count + 11);
        assert_eq!(w.jobs, count);
        assert_eq!(w.unique_files, count);
        assert_eq!(w.regular_aliases, 0);
        assert_eq!(w.directory_bindings, count + 10);
        assert_eq!(w.directory_children, (count / 10).max(10));
        assert_eq!(w.frontier, 10);
        assert_eq!(w.wide_directories, if count == 6000 { 10 } else { 0 });
        assert!(w.read_units > 0 && w.write_units > 0 && w.discard_units > 0);
        // Every entry and every native identity is one working row.
        assert_eq!(w.backing_rows, 2 * count as u64 + 11);
        windows.push((w.child_window_rows, w.read_window_rows, w.write_window_rows));
        backing.push(w.backing_bytes);
        println!("DIAGNOSTIC namespace-count-{count} {w:?}");
    }
    // The child buffer and both windows are fixed: a larger root holds no
    // larger one. The 6000-entry root orders its 600-child directories in
    // backing and fills whole windows.
    assert_eq!((windows[0].0, windows[1].0), (10, 300));
    assert_eq!((windows[2].0, windows[2].1), (512, 512));
    // File completion no longer supplies a4096-row SQL write window. The
    // remaining writes obey the same bound without requiring it to be filled.
    assert!(windows[2].2 > 0 && windows[2].2 <= 4096);
    assert!(windows.iter().all(|w| w.1 <= 512 && w.2 <= 4096));
    assert!(backing[0] < backing[1] && backing[1] < backing[2]);
}
