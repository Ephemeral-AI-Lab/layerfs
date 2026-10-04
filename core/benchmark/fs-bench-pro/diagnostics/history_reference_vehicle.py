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
    for name in ('observer', 'cold', 'producer', 'retained', 'support', 'workload'):
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

    replace('use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};',
            'use layerfs_history::sqlite;')
    replace('''    let selected_profile = match args.get(6).map(String::as_str).unwrap_or("durable") {
        "durable" => SqlitePersistenceProfile::Durable,
        "disposable" => SqlitePersistenceProfile::Disposable,
        _ => return Err("explicit durable/disposable profile required".into()),
    };''','''    if !matches!(args.get(6).map(String::as_str).unwrap_or("durable"), "durable" | "disposable") {
        return Err("explicit durable/disposable profile required".into());
    }''')
    replace('use layerfs_storage::{Storage, StoragePolicy};',
            'use layerfs_storage::{Store, StoreProvider, StoragePolicy};')
    replace('''    let handles = Handles::create(
        PersistenceConfig::sqlite(&args[2]).with_sqlite_profile(selected_profile),
        StoragePolicy::frozen_default(),
        &retained::config(),
    )?;
    let storage = Storage::new(handles.storage.clone())?;''', '''    let storage = Timing::disabled("history.create", |timer|
        Store::create(&args[2], StoragePolicy::frozen_default(), timer.child("store"))).0?;
    let history_path = PathBuf::from(format!("{}.history.sqlite", args[2]));
    let history = sqlite::create(&history_path, &retained::config())?;''')
    replace('cold::Boundary::new(vec![PathBuf::from(&args[2])], probe_states.is_none())?',
            'cold::Boundary::new(vec![PathBuf::from(&args[2]), history_path.clone()], probe_states.is_none())?')
    for stage in ('construction', 'filesystem', 'save_custody'):
        old = f'''eprintln!(
            "HISTORY_PROVIDER_WORK state={{}} stage={stage} cumulative={{:?}}",
            position + 1,
            storage.diagnostics()
        );'''
        replace(old, f'eprintln!("HISTORY_PROVIDER_WORK state={{}} stage={stage} scope=operation-reader-only opens={{}} group_decodes={{}} pooled={{:?}}", position + 1, reader.connection_opens(), reader.group_decodes(), reader.pooled_read_counters());')
    replace('let reader = storage.reader()?;', 'let reader = StoreProvider::new(&storage);')
    replace('let save = storage.begin_save()?;', '''let mut save = Timing::disabled("history.begin", |timer|
            storage.begin_save(timer.child("save"))).0?;''')
    replace('save.finish()?;', '''Timing::disabled("history.finish", |timer|
            save.finish(timer.child("save"))).0?;''')
    # Public C5 calls and profile-aware lifecycle seams; product code is unchanged.
    replace('let profile_identity = handles.profile().identity;',
            'let profile_identity = "phase4.5-memory-off";')
    replace('''                &handles.history,
                scope,''','''                &history,
                scope,''')
    replace('let custody = retained::verify(&handles.history, scope, &roots)?;',
            'let custody = retained::verify(&history, scope, &roots)?;')
    replace('let profile = handles.profile();', '// Native reference settings remain original.')
    replace('eprintln!("EFFECTIVE_PROFILE {{\\"identity\\":\\"{}\\",\\"journal_mode\\":\\"{}\\",\\"synchronous\\":{},\\"foreign_keys\\":{},\\"fullfsync\\":{},\\"checkpoint_fullfsync\\":{},\\"page_size\\":{},\\"cache_size\\":{},\\"mmap_size\\":{},\\"temp_store\\":{},\\"wal_checkpoint_performed\\":{}}}",profile.identity,profile.journal_mode,profile.synchronous,profile.foreign_keys,profile.fullfsync,profile.checkpoint_fullfsync,profile.page_size,profile.cache_size,profile.mmap_size,profile.temp_store,checkpoint.wal_checkpoint_performed);', '// Reference effective profile remains native MEMORY/OFF.')
    replace('if checkpoint.busy {\n        return Err("final checkpoint obstructed".into());\n    }', '// Native reference has no WAL checkpoint.')
    replace('let checkpoint = handles.checkpoint()?;',
            'let checkpoint_ns = 0_u64;')
    replace('''    eprintln!(
        "DIAGNOSTIC sqlite={:?} storage={:?} checkpoint={checkpoint:?}",
        handles.diagnostics()?,
        storage.diagnostics()
    );''', '    // Original MEMORY/OFF engine has no shared-persistence/WAL diagnostics.')
    replace('let checkpoint_ns = checkpoint.wall_ns;',
            '// Reference retains original MEMORY/OFF completion semantics.')
    replace('drop(handles);','drop(history);')
    return text, seals


def generate_verifier(root: Path, helper_path: Path) -> tuple[str, str, dict[str, str]]:
    """Reference read-back over original APIs and independently exported C2 metadata.

    The TSV is an actual closed-Store census, not expected-result data. Its export
    and validation belong to the separate proof budget. No product is modified.
    """
    source = root / 'core/crates/layerfs-project/examples/verify_history.rs'
    helper = source.parent / 'history_support/verify.rs'
    text = source.read_text()
    verification = helper.read_text()
    seals = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
             for p in [source, helper] + [source.parent / f'history_support/{n}.rs'
                                        for n in ('canonical_memo', 'observer', 'producer', 'retained', 'support', 'workload')]}

    def replace(old, new):
        nonlocal text
        if text.count(old) != 1:
            raise ValueError(f'expected one reference proof seam: {old}')
        text = text.replace(old, new)

    for name in ('canonical_memo', 'observer', 'producer', 'retained', 'support', 'workload'):
        replace(f'#[path = "history_support/{name}.rs"]',
                f'#[path = "{source.parent / f"history_support/{name}.rs"}"]')
    replace('#[path = "history_support/verify.rs"]', f'#[path = "{helper_path}"]')
    replace('use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};',
            'use layerfs_history::sqlite;\nuse layerfs_telemetry::timer::Timing;')
    replace('use layerfs_storage::Storage;', 'use layerfs_storage::{Store, StoreProvider};')
    replace('if !matches!(args.len(), 6..=8)', 'if args.len() != 9')
    replace('    let selected_profile = match args.get(6).map(String::as_str).unwrap_or("durable") {\n        "durable" => SqlitePersistenceProfile::Durable,\n        "disposable" => SqlitePersistenceProfile::Disposable,\n        _ => return Err("explicit durable/disposable profile required".into()),\n    };', '    if args[6] != "reference" || args[7] != "independent-reference" {\n        return Err("explicit independent-reference proof required".into());\n    }')
    begin = text.index('    let profile_identity = match selected_profile')
    end = text.index('    if probe_states.is_none() {', begin)
    text = text[:begin] + '    if child.get("profile_identity").and_then(json::Value::as_str) != Some("phase4.5-memory-off") {\n        return Err("reference effective profile identity mismatch".into());\n    }\n    let independent_pins = false;\n' + text[end:]
    replace('        if !independent_pins {\n            return Err("complete proof requires independent root pins".into());\n        }\n', '')
    replace('    let handles = Handles::open_read_only(\n        PersistenceConfig::sqlite(&args[2]).with_sqlite_profile(selected_profile),\n        &config.binding_key,\n        config.cursor_key,\n    )?;\n    let custody = retained::verify(&handles.history, producer::scope_of(row), &roots)?;\n    let storage = Storage::new(handles.storage.clone())?;\n    let reader = storage.reader()?;', '    let history_path = std::path::PathBuf::from(format!("{}.history.sqlite", args[2]));\n    let history = sqlite::open_read_only(&history_path, &config.binding_key, config.cursor_key)?;\n    let custody = retained::verify(&history, producer::scope_of(row), &roots)?;\n    let storage = Timing::disabled("reference.verify.open", |scope| Store::open(&args[2], scope.child("store"))).0?;\n    let reader = StoreProvider::new(&storage);\n    let mut metadata = std::collections::BTreeMap::new();\n    for line in std::fs::read_to_string(&args[8])?.lines() {\n        let fields: Vec<_> = line.split(\'\\t\').collect();\n        if fields.len() != 3 { return Err("reference metadata row shape".into()); }\n        let bytes = workload::digest::unhex(fields[0]).ok_or("reference metadata id hex")?;\n        let id = ObjectId::from_bytes(&bytes)?;\n        let role = fields[1].parse::<u8>()?;\n        let length = fields[2].parse::<u64>()?;\n        if metadata.insert(id, (role, length)).is_some() { return Err("duplicate reference metadata id".into()); }\n    }\n    let actual_bytes: u64 = metadata.values().map(|(_, length)| *length).sum();\n    if child.get("canonical_objects").and_then(json::Value::as_i64) != Some(metadata.len() as i64)\n        || child.get("canonical_bytes").and_then(json::Value::as_i64) != Some(actual_bytes as i64) {\n        return Err("closed reference census disagrees with producer".into());\n    }')
    replace('handles.storage.as_ref()', '&metadata')
    verification = verification.replace('dyn layerfs_storage::port::PackPersistence',
                                        'BTreeMap<ObjectId, (u8, u64)>')
    start = verification.index('        for chunk in ids.chunks(512) {')
    end = verification.index('        for (_, id, kind, _, _, _) in &file_entries {', start)
    verification = verification[:start] + '        for id in &ids {\n            let value = self.persistence.get(id)\n                .ok_or_else(|| OpError::Io(format!("reference metadata missing {id}")))?;\n            metadata.insert(*id, *value);\n        }\n' + verification[end:]
    if 'handles.' in text or 'layerfs_persistence' in text or 'PackPersistence' in verification:
        raise ValueError('reference proof adapter left candidate API behind')
    return text, verification, seals
