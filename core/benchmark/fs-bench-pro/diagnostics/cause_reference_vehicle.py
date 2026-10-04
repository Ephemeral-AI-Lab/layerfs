"""Narrow reference engine substitution for the shared diagnostic namespace caller."""
from pathlib import Path
import hashlib

BASE = '7edddbdb8e8512627aed0ed42533ef099d802384'

def generate(root: Path):
    source=root/'core/crates/layerfs-project/examples/cause_namespace.rs'
    text=source.read_text()
    paths=[source,*sorted((source.parent/'cause_support').glob('*.rs'))]
    paths += [root/'core/benchmark/fs-bench-pro/diagnostics/cause_engine_reference.rs',root/'core/benchmark/fs-bench-pro-storage-content/src/workload/digest.rs']
    seals={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
    def replace(old,new):
        nonlocal text
        if text.count(old)!=1: raise ValueError(f'expected one exact reference API seam: {old}')
        text=text.replace(old,new)
    for name in ['batch','engine','error','init','metadata','namespace','namespace_work','observer','scan','sql_observer']:
        p=source.parent/f'cause_support/{name}.rs'
        if name=='engine':p=root/'core/benchmark/fs-bench-pro/diagnostics/cause_engine_reference.rs'
        replace(f'#[path = "cause_support/{name}.rs"]',f'#[path = "{p}"]')
    replace('#[path="../../../benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"]',f'#[path="{root}/core/benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"]')
    replace('use layerfs_persistence::{Handles, PersistenceConfig};','use layerfs_history::sqlite;')
    replace('    let handles = std::sync::Arc::new(Handles::create(\n        PersistenceConfig::sqlite(&args[2]),\n        StoragePolicy::frozen_default(),\n        &config,\n    )?);\n    let storage = engine::Storage::new(\n        layerfs_storage::Storage::new(handles.storage.clone())?,\n        handles.clone(),\n    );\n','    let product = Timing::disabled("cause.create",|t|\n        layerfs_storage::Store::create(&args[2],StoragePolicy::frozen_default(),t.child("store"))).0?;\n    let history_path = std::path::PathBuf::from(format!("{}.history.sqlite",args[2]));\n    let history = sqlite::create(&history_path,&config)?;\n    let storage = engine::Storage::new(product);\n')
    replace('&handles.history,','&history,')
    replace('''    let checkpoint = handles.checkpoint()?;
    eprintln!(
        "CAUSE_CANDIDATE_SQL_NATIVE_VM_UNQUALIFIED {:?} C2 {:?}",
        handles.diagnostics()?,
        storage.inner.diagnostics()
    );
    let checkpoint_ns = checkpoint.wall_ns;''','''    // Original MEMORY/OFF profile has no WAL checkpoint/allocation-release step.
    let checkpoint_ns = 0_u64;''')
    replace('drop(handles);','drop(history);')
    return text,seals
