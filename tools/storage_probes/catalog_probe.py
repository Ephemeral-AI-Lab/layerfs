"""Standalone publication and indexed namespace experiments; no product adapter."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import time

from common import phase, write_json

CASES = {}
for count in (128, 1024):
    for profile in ('runtime', 'full'):
        CASES[f'D-{count}-{profile}-v2'] = ('publication', count, profile)
for count in (10000, 100000):
    CASES[f'S-flat-{count}-v2'] = ('scale', count, 0)
for depth in (10, 270):
    CASES[f'S-deep-{depth}-v2'] = ('scale', 100000, depth)

SCHEMA = '''
CREATE TABLE packs(id TEXT PRIMARY KEY, bytes INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE locators(id TEXT PRIMARY KEY, pack TEXT NOT NULL REFERENCES packs(id),
 offset INTEGER NOT NULL, size INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE commits(id TEXT PRIMARY KEY, parent INTEGER NOT NULL, generation INTEGER NOT NULL,
 branch INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE branches(id INTEGER PRIMARY KEY, generation INTEGER NOT NULL);
CREATE TABLE nodes(id INTEGER PRIMARY KEY, parent INTEGER NOT NULL, name TEXT NOT NULL,
 size INTEGER NOT NULL, UNIQUE(parent,name));
'''


def connect(path, profile='runtime', setup=False):
    db = sqlite3.connect(path, isolation_level=None, timeout=0)
    full = profile == 'full'
    for sql in [f"PRAGMA journal_mode={'DELETE' if full else 'MEMORY'}",
                f"PRAGMA synchronous={'FULL' if full else 'OFF'}",
                f'PRAGMA fullfsync={int(full)}', 'PRAGMA cache_size=-512',
                'PRAGMA mmap_size=0', 'PRAGMA busy_timeout=0', 'PRAGMA temp_store=FILE',
                'PRAGMA foreign_keys=ON']:
        db.execute(sql)
    if setup:
        db.executescript(SCHEMA)
    return db


def pragmas(db):
    return {k: db.execute('PRAGMA ' + k).fetchone()[0] for k in
            ['journal_mode', 'synchronous', 'fullfsync', 'cache_size', 'mmap_size',
             'busy_timeout', 'temp_store', 'foreign_keys', 'page_size']}


def publish(db, branch, expected, count, owner):
    started = time.monotonic_ns()
    db.execute('BEGIN IMMEDIATE')
    begin_ns = time.monotonic_ns() - started
    pack = f'pack-{owner}-{expected}'
    sql_start = time.monotonic_ns()
    db.execute('INSERT INTO packs VALUES(?,?)', (pack, count * 1024))
    db.executemany('INSERT INTO locators VALUES(?,?,?,?)',
                   [(f'object-{owner}-{expected}-{i}', pack, i * 1024, 1024) for i in range(count)])
    db.execute('INSERT INTO commits VALUES(?,?,?,?)',
               (f'commit-{owner}-{expected}', expected, expected + 1, branch))
    changed = db.execute('UPDATE branches SET generation=generation+1 '
                         'WHERE id=? AND generation=?', (branch, expected)).rowcount
    mutation_ns = time.monotonic_ns() - sql_start
    commit_start = time.monotonic_ns()
    db.execute('COMMIT' if changed == 1 else 'ROLLBACK')
    return {'branch': branch, 'expected': expected, 'owner': owner,
            'outcome': 'COMMITTED' if changed == 1 else 'CONFLICT',
            'begin_ns': begin_ns, 'mutation_ns': mutation_ns,
            'commit_or_rollback_ns': time.monotonic_ns() - commit_start,
            'ack_ns': time.monotonic_ns() - started}


def publication_perf(case, count, profile, output):
    db = connect(output / 'catalog.sqlite', profile, True)
    db.execute('INSERT INTO branches VALUES(0,0)')
    observed = pragmas(db)
    assert observed['journal_mode'] == ('delete' if profile == 'full' else 'memory')
    assert observed['synchronous'] == (2 if profile == 'full' else 0)
    assert observed['fullfsync'] == int(profile == 'full') and observed['busy_timeout'] == 0
    record = {'case': case, 'profile': profile, 'pragmas': observed,
              'locators_per_publication': count, 'publications': 8,
              'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
              'cache_contract': 'native/OS residency UNKNOWN; no cold claim',
              'durability_scope': 'synchronization cost; no power-loss proof',
              'upload_scope': 'simulated already-uploaded pack keys; no cross-store transaction proof'}
    events = phase(record, 'publication_ack', 8, 0,
                   lambda: [publish(db, 0, i, count, 0) for i in range(8)])
    record['events'] = events
    assert all(e['outcome'] == 'COMMITTED' for e in events)
    record['sql_commit_ns'] = sum(e['commit_or_rollback_ns'] for e in events)
    record['sql_mutation_ns'] = sum(e['mutation_ns'] for e in events)
    db.close()
    write_json(output / 'performance.json', record)


def check_publications(db, clients, per_client, width, conflict=False):
    for branch in range(clients if not conflict else 1):
        assert db.execute('SELECT generation FROM branches WHERE id=?', (branch,)).fetchone() == (per_client,)
    if conflict:
        owners = [r[0] for r in db.execute('SELECT id FROM commits')]
        assert len(owners) == 1
        owner = int(owners[0].split('-')[1])
        expected_owners = [owner]
    else:
        expected_owners = list(range(clients))
    expected_packs = sorted((f'pack-{owner}-{generation}', width * 1024)
                            for owner in expected_owners for generation in range(per_client))
    assert db.execute('SELECT id,bytes FROM packs ORDER BY id').fetchall() == expected_packs
    expected_commits = sorted((f'commit-{owner}-{g}', g, g + 1, 0 if conflict else owner)
                              for owner in expected_owners for g in range(per_client))
    assert db.execute('SELECT id,parent,generation,branch FROM commits ORDER BY id').fetchall() == expected_commits
    expected = sorted((f'object-{owner}-{g}-{i}', f'pack-{owner}-{g}', i * 1024, 1024)
                      for owner in expected_owners for g in range(per_client) for i in range(width))
    assert db.execute('SELECT id,pack,offset,size FROM locators ORDER BY id').fetchall() == expected


def prepare(fixtures):
    root = Path(fixtures)
    root.mkdir(exist_ok=False, parents=True)
    receipt = {}
    for case, (kind, count, depth) in CASES.items():
        if kind != 'scale':
            continue
        started = time.monotonic_ns()
        path = root / (case + '.sqlite')
        db = connect(path, setup=True)
        db.execute('INSERT INTO nodes VALUES(1,0,\'root\',0)')
        for lo in range(0, count, 128):
            db.execute('BEGIN IMMEDIATE')
            db.executemany('INSERT INTO nodes VALUES(?,1,?,1024)',
                           [(i + 2, f'f-{i:08d}') for i in range(lo, min(count, lo + 128))])
            db.execute('COMMIT')
        parent = 1
        for i in range(depth):
            node = count + 2 + i
            db.execute('INSERT INTO nodes VALUES(?,?,?,0)', (node, parent, f'd-{i:04d}'))
            parent = node
        assert db.execute('PRAGMA integrity_check').fetchone() == ('ok',)
        assert db.execute('SELECT count(*) FROM nodes').fetchone() == (count + depth + 1,)
        db.close()
        receipt[case] = {'bytes': path.stat().st_size,
                         'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                         'wall_ns': time.monotonic_ns() - started, 'qualification': 'PASS'}
    write_json(root / 'fixtures.json', receipt)


def scale_perf(case, count, depth, fixtures, output):
    master = Path(fixtures) / (case + '.sqlite')
    seal = json.loads((Path(fixtures) / 'fixtures.json').read_text())[case]
    assert hashlib.sha256(master.read_bytes()).hexdigest() == seal['sha256']
    shutil.copyfile(master, output / 'catalog.sqlite')
    db = connect(output / 'catalog.sqlite')
    queries = []
    db.set_trace_callback(queries.append)
    record = {'case': case, 'population': count, 'depth': depth,
              'clone_method': 'closed validated byte copy; not cold', 'master_sha256': seal['sha256'],
              'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
              'cache_contract': 'native/OS residency UNKNOWN; fixture clone/setup may warm pages',
              'pragmas': pragmas(db), 'plans': {}}
    for label, sql, params in [
            ('point', 'SELECT id,size FROM nodes WHERE parent=? AND name=?', (1, 'f-00000037')),
            ('list', 'SELECT name FROM nodes WHERE parent=? AND name>? ORDER BY name LIMIT 128', (1, 'f-00004999')),
            ('rename', 'UPDATE nodes SET name=? WHERE id=?', ('renamed', 39))]:
        record['plans'][label] = db.execute('EXPLAIN QUERY PLAN ' + sql, params).fetchall()
    queries.clear()
    def points():
        if depth:
            results = []
            for _ in range(8):
                parent = 1
                for i in range(depth):
                    row = db.execute('SELECT id FROM nodes WHERE parent=? AND name=?',
                                     (parent, f'd-{i:04d}')).fetchone()
                    if row is None:
                        raise RuntimeError('missing path component')
                    parent = row[0]
                results.append(parent)
            return results
        return [db.execute('SELECT id,size FROM nodes WHERE parent=1 AND name=?',
                           (f'f-{(k * 37) % count:08d}',)).fetchone() for k in range(128)]
    before = len(queries)
    results = phase(record, 'path_resolution' if depth else 'indexed_points', 8 if depth else 128, 0, points)
    record['point_statements'] = len(queries) - before
    record['point_results'] = results
    before = len(queries)
    rows = phase(record, 'one_keyset_page', 128, 0,
                 lambda: db.execute('SELECT name FROM nodes WHERE parent=1 AND name>? '
                                    'ORDER BY name LIMIT 128', ('f-00004999',)).fetchall())
    record['page_statements'] = len(queries) - before
    assert rows == [(f'f-{i:08d}',) for i in range(5000, 5128)]
    target = count + depth + 1 if depth else 39
    before = len(queries)
    phase(record, 'rename_one', 1, 0, lambda: db.execute('UPDATE nodes SET name=? WHERE id=?', ('renamed', target)))
    record['rename_visibility'] = db.execute('SELECT name FROM nodes WHERE id=?', (target,)).fetchone()[0]
    assert record['rename_visibility'] == 'renamed'
    phase(record, 'delete_one', 1, 0, lambda: db.execute('DELETE FROM nodes WHERE id=?', (target,)))
    record['mutation_statements'] = len(queries) - before
    db.close()
    write_json(output / 'performance.json', record)


def verify(case, fixtures, output):
    kind, count, profile = CASES[case]
    started = time.monotonic_ns()
    db = connect(output / 'catalog.sqlite', profile if kind == 'publication' else 'runtime')
    assert db.execute('PRAGMA integrity_check').fetchone() == ('ok',)
    if kind == 'publication':
        check_publications(db, 1, 8, count)
    else:
        depth = profile
        target = count + depth + 1 if depth else 39
        expected = [(1, 0, 'root', 0)] + [(i + 2, 1, f'f-{i:08d}', 1024) for i in range(count)]
        parent = 1
        for i in range(depth):
            node = count + 2 + i
            expected.append((node, parent, f'd-{i:04d}', 0)); parent = node
        expected = [row for row in expected if row[0] != target]
        assert db.execute('SELECT id,parent,name,size FROM nodes ORDER BY id').fetchall() == expected
        record = json.loads((output / 'performance.json').read_text())
        assert record['point_statements'] == (8 * depth if depth else 128)
        assert record['page_statements'] == 1 and record['mutation_statements'] == 3 and record['rename_visibility'] == 'renamed'
        assert all('SEARCH' in str(p) and 'SCAN nodes' not in str(p) for p in record['plans'].values())
        if depth:
            assert record['point_results'] == [target] * 8
        else:
            assert record['point_results'] == [[(k * 37) % count + 2, 1024] for k in range(128)]
        master = Path(fixtures) / (case + '.sqlite')
        assert hashlib.sha256(master.read_bytes()).hexdigest() == record['master_sha256']
    proof_ns = time.monotonic_ns() - started
    db.close()
    cleanup = time.monotonic_ns()
    (output / 'catalog.sqlite').unlink()
    write_json(output / 'verification.json', {'case': case, 'proof': 'PASS', 'cleanup': 'PASS',
               'proof_ns': proof_ns, 'cleanup_ns': time.monotonic_ns() - cleanup,
               'scope': 'exact rows, guarded publication or indexed query laws; no power-loss proof'})


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['setup', 'perf', 'verify'])
    parser.add_argument('--case', choices=CASES)
    parser.add_argument('--fixtures', required=True)
    parser.add_argument('--output')
    args = parser.parse_args()
    if args.action == 'setup':
        prepare(args.fixtures)
    else:
        output = Path(args.output)
        if args.action == 'perf':
            output.mkdir(exist_ok=False, parents=True)
            kind, count, profile = CASES[args.case]
            if kind == 'publication':
                publication_perf(args.case, count, profile, output)
            else:
                scale_perf(args.case, count, profile, args.fixtures, output)
        else:
            verify(args.case, args.fixtures, output)
