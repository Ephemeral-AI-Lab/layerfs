"""External thread/SQL/socket coordinated concurrency; no sleep-based overlap."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import threading
import time

from common import BLOCK, S3, body_hash, phase, write_json
from catalog_probe import check_publications, connect, pragmas, publish

CASES = {}
for clients in (1, 2, 4):
    CASES[f'C-upload-{clients}-v2'] = ('upload', clients)
    CASES[f'C-metadata-{clients}-v2'] = ('metadata', clients)
CASES['C-branch-conflict-2-v2'] = ('conflict', 2)
CASES['C-sqlite-busy-2-v2'] = ('busy', 2)


def threads(count, worker):
    results, errors = [None] * count, []
    def entry(i):
        try:
            results[i] = worker(i)
        except BaseException as e:
            errors.append((i, type(e).__name__, str(e)))
    jobs = [threading.Thread(target=entry, args=(i,), daemon=True) for i in range(count)]
    for job in jobs:
        job.start()
    for job in jobs:
        job.join(8)
    if any(job.is_alive() for job in jobs):
        raise RuntimeError('worker incomplete within8s; outer command retains artifacts')
    if errors:
        raise RuntimeError(str(errors))
    return results


def upload(output, count, config, prefix):
    barrier = threading.Barrier(count)
    arrivals, release = [], []
    mutex = threading.Lock()
    size = 256 * 1024
    def worker(i):
        client = S3(config)
        block = bytearray(BLOCK)
        block[:16] = f'{i:016d}'.encode()
        block = bytes(block)
        digest = body_hash(size, block)
        start = time.monotonic_ns()
        def gate():
            with mutex:
                arrivals.append({'client': i, 'first_64k_sent_ns': time.monotonic_ns()})
            barrier.wait(4)
            release.append(time.monotonic_ns())
        acknowledgements = []
        for n in range(8):
            before = time.monotonic_ns()
            client.call('PUT', prefix + f'{i}-{n}', size=size, payload_hash=digest,
                        block=block, body_gate=gate if n == 0 else None)
            acknowledgements.append({'index': n, 'start_ns': before, 'ack_ns': time.monotonic_ns()})
        result = {'client': i, 'start_ns': start, 'end_ns': time.monotonic_ns(),
                  'requests': client.calls, 'sent_bytes': client.sent,
                  'sha256': digest, 'acks': acknowledgements}
        client.close()
        return result
    record = {'clients': count, 'overlap_method': 'all first signed PUTs send64KiB and withhold192KiB before barrier release',
              'phase_scope': 'thread startup, signing, socket transfer and coordination included'}
    results = phase(record, 'concurrent_put', count * 8, count * 8 * size,
                    lambda: threads(count, worker))
    latest_arrival = max(a['first_64k_sent_ns'] for a in arrivals)
    assert len(arrivals) == count and min(release) >= latest_arrival
    assert min(r['acks'][0]['ack_ns'] for r in results) > latest_arrival
    record.update({'events': results, 'partial_request_arrivals': arrivals,
                   'overlap_proof': 'PASS', 'requests': count * 8})
    return record


def metadata(output, count, conflict):
    db = connect(output / 'catalog.sqlite', 'full', True)
    db.executemany('INSERT INTO branches VALUES(?,0)', [(i,) for i in range(1 if conflict else count)])
    settings = pragmas(db); db.close()
    barrier = threading.Barrier(count + 1)
    attempted = [threading.Event() for _ in range(count)]
    admission = threading.Lock()
    admission.acquire()
    def worker(i):
        db = connect(output / 'catalog.sqlite', 'full')
        branch = 0 if conflict else i
        selected = db.execute('SELECT generation FROM branches WHERE id=?', (branch,)).fetchone()[0]
        assert selected == 0
        barrier.wait(4)
        events = []
        for j in range(1 if conflict else 8):
            requested = time.monotonic_ns()
            if j == 0:
                assert not admission.acquire(blocking=False)
                attempted[i].set()
            with admission:
                granted = time.monotonic_ns()
                result = publish(db, branch, selected if conflict else j, 128, i)
                result.update({'request_ns': requested, 'grant_ns': granted,
                               'acknowledged_ns': time.monotonic_ns(), 'queue_wait_ns': granted - requested})
                events.append(result)
        db.close()
        return {'client': i, 'events': events}
    results, errors = [None] * count, []
    def entry(i):
        try: results[i] = worker(i)
        except BaseException as e: errors.append((i, type(e).__name__, str(e)))
    def execute():
        jobs = [threading.Thread(target=entry, args=(i,), daemon=True) for i in range(count)]
        for job in jobs: job.start()
        barrier.wait(4)
        for event in attempted:
            if not event.wait(4): raise RuntimeError('client did not attempt admission')
        released = time.monotonic_ns()
        admission.release()
        for job in jobs: job.join(4)
        if errors or any(job.is_alive() for job in jobs):
            raise RuntimeError(str(errors) or 'publication owner incomplete')
        return released
    record = {'clients': count, 'pragmas': settings,
              'concurrency_scope': 'application writer gate contention; SQLite writes serialized',
              'native_busy_timeout': 0, 'publications_per_client': 1 if conflict else 8}
    released = phase(record, 'queued_publication_ack', count * (1 if conflict else 8), 0, execute)
    assert all(r['events'][0]['request_ns'] < released <= r['events'][0]['grant_ns'] for r in results)
    outcomes = [e['outcome'] for r in results for e in r['events']]
    assert sorted(outcomes) == (['COMMITTED', 'CONFLICT'] if conflict else ['COMMITTED'] * count * 8)
    record.update({'events': results, 'gate_release_ns': released,
                   'overlap_proof': 'PASS', 'outcomes': outcomes})
    return record


def native_busy(output):
    db = connect(output / 'catalog.sqlite', 'full', True); db.close()
    held, attempted, done = threading.Event(), threading.Event(), threading.Event()
    ready = threading.Barrier(2)
    events = []
    def worker(i):
        db = connect(output / 'catalog.sqlite', 'full')
        ready.wait(4)
        if i == 0:
            db.execute('BEGIN IMMEDIATE')
            db.execute('INSERT INTO branches VALUES(0,1)')
            events.append({'event': 'actual_insert_lock_held', 'ns': time.monotonic_ns()})
            held.set()
            assert attempted.wait(4)
            db.execute('COMMIT'); done.set()
            result = 'COMMITTED'
        else:
            assert held.wait(4)
            started = time.monotonic_ns()
            try:
                db.execute('BEGIN IMMEDIATE')
                raise RuntimeError('second writer unexpectedly admitted')
            except sqlite3.OperationalError as e:
                assert e.sqlite_errorcode == sqlite3.SQLITE_BUSY
                events.append({'event': 'SQLITE_BUSY', 'ns': time.monotonic_ns(),
                               'attempt_ns': started, 'code': e.sqlite_errorcode})
            attempted.set(); assert done.wait(4)
            assert db.execute('SELECT generation FROM branches WHERE id=0').fetchone() == (1,)
            result = 'BUSY_NO_RETRY'
        db.close()
        return result
    record = {'clients': 2, 'native_busy_timeout': 0,
              'concurrency_scope': 'real held SQLite write transaction; direct competing BEGIN'}
    result = phase(record, 'native_contention_proof', 2, 0, lambda: threads(2, worker))
    assert result == ['COMMITTED', 'BUSY_NO_RETRY']
    record.update({'events': events, 'outcomes': result, 'overlap_proof': 'PASS'})
    return record


def execute(action, case, provider, output):
    output = Path(output)
    kind, count = CASES[case]
    prefix = case + '/'
    config = json.loads((Path(provider) / 'private-config.json').read_text()) if kind == 'upload' else None
    if action == 'perf':
        output.mkdir(parents=True, exist_ok=False)
        if kind == 'upload': record = upload(output, count, config, prefix)
        elif kind in ('metadata', 'conflict'): record = metadata(output, count, kind == 'conflict')
        else: record = native_busy(output)
        record.update({'case': case, 'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
                       'cache_contract': 'OS/provider/native residency UNKNOWN; no cold claim'})
        write_json(output / 'performance.json', record)
    else:
        started = time.monotonic_ns()
        record = json.loads((output / 'performance.json').read_text())
        assert record['overlap_proof'] == 'PASS'
        if kind == 'upload':
            client = S3(config)
            expected = sorted(prefix + f'{i}-{n}' for i in range(count) for n in range(8))
            assert client.listing(prefix)[0] == expected
            for i in range(count):
                block = bytearray(BLOCK); block[:16] = f'{i:016d}'.encode()
                digest = body_hash(262144, bytes(block))
                for n in range(8):
                    assert client.call('GET', prefix + f'{i}-{n}', consume=True, verify_hash=True) == (262144, digest)
            proof_ns = time.monotonic_ns() - started;cleanup = time.monotonic_ns()
            for key in expected: client.call('DELETE', key)
            assert client.listing(prefix)[0] == []; client.close()
        else:
            db = connect(output / 'catalog.sqlite', 'full')
            assert db.execute('PRAGMA integrity_check').fetchone() == ('ok',)
            if kind == 'busy':
                assert db.execute('SELECT * FROM branches').fetchall() == [(0, 1)]
                assert db.execute('SELECT count(*) FROM commits').fetchone() == (0,)
            else: check_publications(db, count, 1 if kind == 'conflict' else 8, 128, kind == 'conflict')
            proof_ns = time.monotonic_ns() - started; db.close();cleanup = time.monotonic_ns()
            (output / 'catalog.sqlite').unlink()
        write_json(output / 'verification.json', {'case': case, 'proof': 'PASS', 'cleanup': 'PASS',
                   'proof_ns': proof_ns, 'cleanup_ns': time.monotonic_ns() - cleanup,
                   'scope': 'exact objects/rows, overlap intervals, conflict rollback and native BUSY custody'})


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['perf', 'verify'])
    parser.add_argument('--case', choices=CASES, required=True)
    parser.add_argument('--provider')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    execute(args.action, args.case, args.provider, args.output)
