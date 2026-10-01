"""Standalone metadata throughput; file-backed SQLite, no file payload BLOBs."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import time

from common import phase, write_json

CASES = {
    'S-directory-10000-v1': (10000, 128, 'files'),
    'S-tiny-1024-b1-v1': (1024, 1, 'files'),
    'S-tiny-1024-b128-v1': (1024, 128, 'files'),
    'S-big-index-4096-v1': (4096, 128, 'ranges'),
}
SCHEMA = '''
CREATE TABLE inodes(id INTEGER PRIMARY KEY, size INTEGER NOT NULL, content BLOB NOT NULL);
CREATE TABLE entries(parent INTEGER NOT NULL, name TEXT NOT NULL,
 inode INTEGER NOT NULL REFERENCES inodes(id), PRIMARY KEY(parent,name)) WITHOUT ROWID;
CREATE INDEX entries_inode ON entries(inode);
CREATE TABLE locators(file INTEGER NOT NULL, offset INTEGER NOT NULL,
 object_id BLOB NOT NULL, pack INTEGER NOT NULL, record INTEGER NOT NULL,
 PRIMARY KEY(file,offset)) WITHOUT ROWID;
'''


def identity(i):
    return hashlib.sha256(('fixture-object-' + str(i)).encode()).digest()


def connect(path, setup=False):
    db = sqlite3.connect(path, timeout=0, isolation_level=None)
    for sql in ['PRAGMA journal_mode=MEMORY', 'PRAGMA synchronous=OFF',
                'PRAGMA cache_size=-512', 'PRAGMA mmap_size=0', 'PRAGMA busy_timeout=0',
                'PRAGMA temp_store=FILE', 'PRAGMA foreign_keys=ON']:
        db.execute(sql)
    if setup:
        db.executescript(SCHEMA)
    return db


def insert(db, count, batch, kind):
    transactions = 0
    if kind == 'ranges':
        db.execute('INSERT INTO inodes VALUES(1,?,?)', (64 * 1048576, identity(1)))
    for lo in range(0, count, batch):
        db.execute('BEGIN IMMEDIATE')
        for i in range(lo, min(count, lo + batch)):
            if kind == 'files':
                db.execute('INSERT INTO inodes VALUES(?,?,?)', (i + 1, 1024, identity(i)))
                db.execute('INSERT INTO entries VALUES(1,?,?)', (f'f-{i:08d}', i + 1))
            else:
                db.execute('INSERT INTO locators VALUES(1,?,?,?,?)',
                           (i * 16384, identity(i), i // 16, i % 16))
        db.execute('COMMIT')
        transactions += 1
    return transactions


def scan(db, kind):
    lower = '' if kind == 'files' else -1
    count, pages = 0, 0
    while True:
        if kind == 'files':
            rows = db.execute('SELECT name,inode FROM entries WHERE parent=1 AND name>? '
                              'ORDER BY name LIMIT 128', (lower,)).fetchall()
        else:
            rows = db.execute('SELECT offset,object_id FROM locators WHERE file=1 AND offset>? '
                              'ORDER BY offset LIMIT 128', (lower,)).fetchall()
        pages += 1
        if not rows:
            return count, pages
        count += len(rows)
        lower = rows[-1][0]


def perform(case, output):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    count, batch, kind = CASES[case]
    db = connect(output / 'metadata.sqlite', setup=True)
    pragmas = {name: db.execute('PRAGMA ' + name).fetchone()[0] for name in
               ['journal_mode', 'synchronous', 'cache_size', 'mmap_size', 'busy_timeout',
                'temp_store', 'foreign_keys', 'page_size']}
    assert pragmas == {'journal_mode': 'memory', 'synchronous': 0, 'cache_size': -512,
                       'mmap_size': 0, 'busy_timeout': 0, 'temp_store': 1,
                       'foreign_keys': 1, 'page_size': 4096}, pragmas
    record = {'case': case, 'sqlite_version': sqlite3.sqlite_version, 'pragmas': pragmas,
              'records': count, 'insert_batch': batch,
              'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
              'cache_contract': 'native/OS residency UNKNOWN; reads follow own insert',
              'client_scope': 'Python sqlite3 adapter, bindings and fixture identity work included',
              'durability': 'runtime-only; MEMORY journal and synchronous OFF'}
    txs = phase(record, 'insert_metadata', count, 0,
                lambda: insert(db, count, batch, kind))
    points = 256 if kind == 'ranges' else 128
    def lookup():
        for k in range(points):
            i = (k * 37) % count
            if kind == 'files':
                row = db.execute('SELECT i.size,i.content FROM entries e JOIN inodes i '
                                 'ON i.id=e.inode WHERE e.parent=1 AND e.name=?',
                                 (f'f-{i:08d}',)).fetchone()
            else:
                row = db.execute('SELECT object_id,pack,record FROM locators WHERE file=1 '
                                 'AND offset=?', (i * 16384,)).fetchone()
            if row is None:
                raise RuntimeError('missing point result')
    phase(record, 'point_lookup', points, 0, lookup)
    observed, pages = phase(record, 'keyset_list', count, 0, lambda: scan(db, kind))
    assert observed == count
    if kind == 'files':
        def rename():
            db.execute('BEGIN IMMEDIATE')
            for i in range(128):
                db.execute('UPDATE entries SET name=? WHERE parent=1 AND name=?',
                           (f'r-{i:08d}', f'f-{i:08d}'))
            db.execute('COMMIT')
        phase(record, 'rename', 128, 0, rename)
    record['insert_transactions'] = txs
    record['list_queries'] = pages
    record['metadata_file_bytes'] = (output / 'metadata.sqlite').stat().st_size
    record['command_status'] = 'COMPLETE'
    write_json(output / 'performance.json', record)
    db.close()
    print(case, 'performance complete; numerical cache INELIGIBLE')


def verify(case, output):
    output = Path(output)
    count, batch, kind = CASES[case]
    db = connect(output / 'metadata.sqlite')
    start = time.monotonic_ns()
    assert db.execute('PRAGMA integrity_check').fetchone() == ('ok',)
    if kind == 'files':
        actual = db.execute('SELECT e.name,i.id,i.size,i.content FROM entries e JOIN inodes i '
                            'ON i.id=e.inode WHERE e.parent=1 ORDER BY e.name').fetchall()
        expected = sorted((('r-' if i < 128 else 'f-') + f'{i:08d}', i + 1, 1024, identity(i))
                          for i in range(count))
        assert actual == expected
    else:
        actual = db.execute('SELECT offset,object_id,pack,record FROM locators WHERE file=1 '
                            'ORDER BY offset').fetchall()
        expected = [(i * 16384, identity(i), i // 16, i % 16) for i in range(count)]
        assert actual == expected
        assert db.execute('SELECT size FROM inodes WHERE id=1').fetchone() == (64 * 1048576,)
    proof_ns = time.monotonic_ns() - start
    cleanup_start = time.monotonic_ns()
    transactions = 0
    for lo in range(0, count, 128):
        db.execute('BEGIN IMMEDIATE')
        if kind == 'files':
            db.execute('DELETE FROM entries WHERE inode>? AND inode<=?', (lo, min(count, lo + 128)))
            db.execute('DELETE FROM inodes WHERE id>? AND id<=?', (lo, min(count, lo + 128)))
        else:
            db.execute('DELETE FROM locators WHERE file=1 AND offset>=? AND offset<?',
                       (lo * 16384, min(count, lo + 128) * 16384))
        db.execute('COMMIT')
        transactions += 1
    if kind == 'ranges':
        db.execute('DELETE FROM inodes WHERE id=1')
    assert all(db.execute('SELECT count(*) FROM ' + t).fetchone()[0] == 0
               for t in ['inodes', 'entries', 'locators'])
    cleanup_ns = time.monotonic_ns() - cleanup_start
    db.close()
    write_json(output / 'verification.json', {'case': case, 'proof': 'PASS',
               'rows_checked': count, 'proof_ns': proof_ns, 'cleanup': 'PASS',
               'cleanup_ns': cleanup_ns, 'cleanup_transactions': transactions,
               'retained_empty_db_bytes': (output / 'metadata.sqlite').stat().st_size})
    print(case, 'independent record proof and bounded-row cleanup PASS')


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('action', choices=['perf', 'verify'])
    p.add_argument('--case', choices=CASES, required=True)
    p.add_argument('--output', required=True)
    a = p.parse_args()
    (perform if a.action == 'perf' else verify)(a.case, a.output)
