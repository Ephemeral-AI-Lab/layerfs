"""Generate a narrow Phase4.5 API adapter from the shared history producer.

The product checkout is unchanged. This vehicle alone supplies no admission.
Every source replacement is exact and fails closed when the shared driver moves.
"""
from pathlib import Path
import hashlib

BASE = '7edddbdb8e8512627aed0ed42533ef099d802384'


def generate(root: Path) -> tuple[str, dict[str, str]]:
    source = root / 'core/crates/layerfs-project/examples/benchmark_history.rs'
    text = source.read_text()
    seals = {str(source.relative_to(root)): hashlib.sha256(source.read_bytes()).hexdigest()}
    for name in ('producer', 'retained', 'support', 'workload'):
        path = source.parent / f'history_support/{name}.rs'
        seals[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
        old = f'#[path = "history_support/{name}.rs"]'
        assert text.count(old) == 1, f'module adapter moved: {name}'
        text = text.replace(old, f'#[path = "{path}"]')

    def replace(old, new):
        nonlocal text
        if text.count(old) != 1:
            raise ValueError(f'expected one reference API seam: {old}')
        text = text.replace(old, new)

    replace('use layerfs_persistence::{Handles, PersistenceConfig};',
            'use layerfs_history::sqlite;')
    replace('use layerfs_storage::{Storage, StoragePolicy};',
            'use layerfs_storage::{Store, StoreProvider, StoragePolicy};')
    replace('''    let handles = Handles::create(
        PersistenceConfig::sqlite(&args[2]),
        StoragePolicy::frozen_default(),
        &retained::config(),
    )?;
    let storage = Storage::new(handles.storage.clone())?;''', '''    let storage = Timing::disabled("history.create", |timer|
        Store::create(&args[2], StoragePolicy::frozen_default(), timer.child("store"))).0?;
    let history_path = PathBuf::from(format!("{}.history.sqlite", args[2]));
    let history = sqlite::create(&history_path, &retained::config())?;''')
    replace('let reader = storage.reader()?;', 'let reader = StoreProvider::new(&storage);')
    replace('let save = storage.begin_save()?;', '''let mut save = Timing::disabled("history.begin", |timer|
            storage.begin_save(timer.child("save"))).0?;''')
    replace('save.finish()?;', '''Timing::disabled("history.finish", |timer|
            save.finish(timer.child("save"))).0?;''')
    # Two public C5 call sites and the diagnostic/close tail, never altered C5 logic.
    replace('''                &handles.history,
                scope,''', '''                &history,
                scope,''')
    replace('''    let custody = retained::verify(&handles.history, scope, &roots)?;
    let checkpoint = handles.checkpoint()?;
    eprintln!(
        "DIAGNOSTIC sqlite={:?} storage={:?} checkpoint={checkpoint:?}",
        handles.diagnostics()?,
        storage.diagnostics()
    );
    drop(storage);
    drop(handles);''', '''    let custody = retained::verify(&history, scope, &roots)?;
    // Phase4.5 retains its original MEMORY/OFF profile; no WAL checkpoint exists.
    drop(storage);
    drop(history);''')
    return text, seals
