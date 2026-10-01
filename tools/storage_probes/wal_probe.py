"""Fixed-provider Python WAL/sync cost; checkpoint work stays in collection."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import time

from catalog_probe import SCHEMA, check_publications, publish
from common import phase, write_json

SOURCE_ID = '2026-03-13 10:38:09 737ae4a34738ffa0c3ff7f9bb18df914dd1cad163f28fd6b6e114a344fe6d618'
PROFILES = {'delete-full': ('DELETE', 'FULL', 0),
            'delete-fullfsync': ('DELETE', 'FULL', 1),
            'wal-normal': ('WAL', 'NORMAL', 0),
            'wal-full': ('WAL', 'FULL', 0),
            'wal-fullfsync': ('WAL', 'FULL', 1)}
CASES = {f'W-{count}-{profile}-v4': (count, profile, False)
         for count in (128, 1024) for profile in PROFILES}
CASES.update({'W-reader-normal-v4': (1024, 'wal-normal', True),
              'W-reader-fullfsync-v4': (1024, 'wal-fullfsync', True)})


def provider_identity(provider):
    root = Path(provider).resolve()
    assert sqlite3.sqlite_version == '3.51.3', 'fixed provider unavailable; no fallback'
    db = sqlite3.connect(':memory:')
    source = db.execute('SELECT sqlite_source_id()').fetchone()[0]
    options = [r[0] for r in db.execute('PRAGMA compile_options')]
    db.close()
    assert source == SOURCE_ID
    class DlInfo(ctypes.Structure):
        _fields_ = [('name', ctypes.c_char_p), ('base', ctypes.c_void_p),
                    ('symbol', ctypes.c_char_p), ('address', ctypes.c_void_p)]
    lib = ctypes.CDLL('libsqlite3.dylib')
    api = ctypes.CDLL(None)
    api.dladdr.argtypes = [ctypes.c_void_p, ctypes.POINTER(DlInfo)]
    info = DlInfo()
    assert api.dladdr(ctypes.cast(lib.sqlite3_libversion, ctypes.c_void_p), ctypes.byref(info))
    loaded = Path(info.name.decode()).resolve()
    assert loaded == root / 'libsqlite3.dylib'
    actual_hash = hashlib.sha256(loaded.read_bytes()).hexdigest()
    seal = json.loads((root / 'build-command.json').read_text())
    assert actual_hash == seal['binary_sha256']
    return {'version': sqlite3.sqlite_version, 'source_id': source,
            'actual_library_path': str(loaded), 'binary_sha256': actual_hash,
            'compile_options': options}


def connect(path, profile):
    journal, sync, full = PROFILES[profile]
    db = sqlite3.connect(path, isolation_level=None, timeout=0)
    for sql in [f'PRAGMA journal_mode={journal}', f'PRAGMA synchronous={sync}',
                f'PRAGMA fullfsync={full}', 'PRAGMA checkpoint_fullfsync=0',
                'PRAGMA wal_autocheckpoint=1000', 'PRAGMA cache_size=-512',
                'PRAGMA mmap_size=0', 'PRAGMA busy_timeout=0', 'PRAGMA temp_store=FILE',
                'PRAGMA foreign_keys=ON']:
        db.execute(sql)
    return db


def settings(db):
    names = ['journal_mode', 'synchronous', 'fullfsync', 'checkpoint_fullfsync',
             'wal_autocheckpoint', 'cache_size', 'mmap_size', 'busy_timeout',
             'temp_store', 'foreign_keys', 'page_size']
    return {name: db.execute('PRAGMA ' + name).fetchone()[0] for name in names}


def wal_bytes(path):
    wal = Path(str(path) + '-wal')
    return wal.stat().st_size if wal.exists() else 0


def perform(case, provider, output):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    count, profile, pinned = CASES[case]
    identity = provider_identity(provider)
    path = output / 'catalog.sqlite'
    db = connect(path, profile)
    db.executescript(SCHEMA)
    db.execute('INSERT INTO branches VALUES(0,0)')
    observed = settings(db)
    journal, sync, full = PROFILES[profile]
    assert observed['journal_mode'] == journal.lower()
    assert observed['synchronous'] == (1 if sync == 'NORMAL' else 2)
    assert observed['fullfsync'] == full and observed['page_size'] == 4096
    assert observed['cache_size'] == -512 and observed['mmap_size'] == observed['busy_timeout'] == 0
    record = {'case': case, 'profile': profile, 'provider': identity, 'pragmas': observed,
              'locators_per_publication': count, 'publications': 8, 'reader_pinned': pinned,
              'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
              'cache_contract': 'OS/native residency unknown; read-after-write; no cold claim',
              'durability_scope': 'sync/checkpoint cost and transaction-visible state; no power-loss proof'}
    if journal == 'WAL':
        record['setup_checkpoint'] = db.execute('PRAGMA wal_checkpoint(TRUNCATE)').fetchone()
        assert record['setup_checkpoint'] == (0, 0, 0)
    reader = connect(path, profile) if pinned else None
    if reader:
        reader.execute('BEGIN')
        assert reader.execute('SELECT generation FROM branches WHERE id=0').fetchone() == (0,)
        record['reader_generation_before'] = 0
    events = phase(record, 'publication_ack', 8, 0,
                   lambda: [publish(db, 0, i, count, 0) for i in range(8)])
    assert all(event['outcome'] == 'COMMITTED' for event in events)
    record.update({'events': events, 'sql_commit_ns': sum(e['commit_or_rollback_ns'] for e in events),
                   'sql_mutation_ns': sum(e['mutation_ns'] for e in events),
                   'wal_bytes_before_checkpoint': wal_bytes(path)})
    if reader:
        assert reader.execute('SELECT generation FROM branches WHERE id=0').fetchone() == (0,)
        assert db.execute('SELECT generation FROM branches WHERE id=0').fetchone() == (8,)
        partial = phase(record, 'checkpoint_with_pinned_reader', 1, 0,
                        lambda: db.execute('PRAGMA wal_checkpoint(PASSIVE)').fetchone())
        assert partial[1] > partial[2] >= 0
        record['pinned_checkpoint_result'] = partial
        record['reader_generation_after_publications'] = 0
        phase(record, 'release_reader', 1, 0, lambda: reader.execute('ROLLBACK'))
        reader.close()
    if journal == 'WAL':
        final = phase(record, 'final_checkpoint', 1, 0,
                      lambda: db.execute('PRAGMA wal_checkpoint(TRUNCATE)').fetchone())
        assert final == (0, 0, 0)
        record['final_checkpoint_result'] = final
        assert wal_bytes(path) == 0
    else:
        record['final_checkpoint_result'] = 'NOT_APPLICABLE_DELETE'
    record['wal_bytes_after_checkpoint'] = wal_bytes(path)
    record['publication_and_checkpoint_ns'] = sum(p['elapsed_ns'] for p in record['phases'])
    close_started = time.monotonic_ns()
    db.close()
    record['close_ns'] = time.monotonic_ns() - close_started
    write_json(output / 'performance.json', record)


def verify(case, provider, output):
    output = Path(output)
    count, profile, pinned = CASES[case]
    provider_identity(provider)
    path = output / 'catalog.sqlite'
    started = time.monotonic_ns()
    record = json.loads((output / 'performance.json').read_text())
    db = connect(path, profile)
    assert db.execute('PRAGMA integrity_check').fetchone() == ('ok',)
    check_publications(db, 1, 8, count)
    assert record['wal_bytes_after_checkpoint'] == 0
    if pinned:
        assert record['reader_generation_before'] == record['reader_generation_after_publications'] == 0
        assert record['pinned_checkpoint_result'][1] > record['pinned_checkpoint_result'][2]
        assert record['final_checkpoint_result'] == [0, 0, 0]
    db.close()
    proof_ns = time.monotonic_ns() - started
    cleanup = time.monotonic_ns()
    for suffix in ['', '-wal', '-shm', '-journal']:
        artifact = Path(str(path) + suffix)
        if artifact.exists(): artifact.unlink()
    write_json(output / 'verification.json', {'case': case, 'proof': 'PASS', 'cleanup': 'PASS',
               'proof_ns': proof_ns, 'cleanup_ns': time.monotonic_ns() - cleanup,
               'scope': 'exact reopened catalog state and checkpoint/snapshot facts; no crash/power-loss proof'})


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['perf', 'verify'])
    parser.add_argument('--case', choices=CASES, required=True)
    parser.add_argument('--provider', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    (perform if args.action == 'perf' else verify)(args.case, args.provider, args.output)
