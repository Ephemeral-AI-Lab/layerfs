"""Bounded public FUSE reads of explicitly supplied, sealed retained history."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

import runtime
import isolation
import runner

FIXTURE = runner.BENCH / 'families/historical_access/fixture.json'
SCHEMA = 'historical-access-v2'
LIMIT_NS = 15_000_000_000


def save(path, value):
    with path.open('x') as f:
        json.dump(value, f, indent=2, sort_keys=True)
        f.write('\n')


def definition(path=None):
    value = json.loads(Path(path or FIXTURE).read_text())
    assert value['schema'] == SCHEMA
    cases = value['cases']
    assert len(cases) == len({c['id'] for c in cases}) == 11
    assert all(c['cache'] in ('cold', 'warm') and 0 <= c['length'] <= 1048576 for c in cases)
    return value


def check_output(records, case):
    outputs = [r['output'] for r in records if r['kind'] == 'storage-smoke-execution']
    assert len(outputs) == (2 if case['cache'] == 'warm' else 1), 'workload cardinality'
    observed = []
    for output in outputs:
        pairs = [line.split('=', 1) for line in output.splitlines() if '=' in line]
        assert len(pairs) == len({k for k, v in pairs}), 'duplicate receipt fields'
        fields = dict(pairs)
        assert fields['storage_smoke_mount'] == 'fuse'
        assert fields['access_completed_count'] == '1'
        assert int(fields['access_operation_ns']) > 0
        assert int(fields['access_returned_bytes']) == case['length']
        for key, value in case['expected'].items():
            assert fields['access_' + key] == str(value), 'oracle mismatch: ' + key
        observed.append(fields)
    closed = [r for r in records if r['kind'] == 'historical-access-closed']
    assert len(closed) == 1 and closed[0]['cleanup_ok'] and closed[0]['success'], 'host cleanup'
    phases = [r for r in records if r.get('phase') == 'access-measured']
    assert len(phases) == 1 and phases[0]['success'], 'measured phase cardinality'
    return observed


def worker(config):
    runtime.PARENT_SUPERVISED = True
    args = json.loads(Path(config).read_text())
    output = Path(args['output']); deadline = runtime.Deadline(args['work_end'])
    result = {'status': 'INCOMPLETE', 'admission_eligible': False}
    try:
        fixture_path = Path(args.get("fixture") or FIXTURE)
        fixture = definition(fixture_path)
        if not Path(args['store']).is_file():
            raise FileNotFoundError('NOT_READY: supply the sealed closed history Store')
        case = next(c for c in fixture['cases'] if c['id'] == args['case'])
        current = runner.source_build_args()
        binary = args['host_binary']
        identity = json.loads(Path(binary + '.identity.json').read_text())
        image = runner.image_info(args['image'], deadline.end)
        labels = image['Config']['Labels']
        if (identity['binary_sha256'] != runtime.file_sha256(binary)
                or identity['LAYERFS_SOURCE_SEAL'] != current['LAYERFS_SOURCE_SEAL']
                or labels['dev.layerfs.source-seal'] != current['LAYERFS_SOURCE_SEAL']
                or labels['dev.layerfs.product-seal'] != current['LAYERFS_PRODUCT_SEAL']):
            raise ValueError('NOT_READY: stale build/image; build explicitly')
        custody = {'source': current, 'binary_sha256': identity['binary_sha256'],
                   'image_id': image['Id'], 'fixture_sha256': runtime.file_sha256(fixture_path)}
        result.update(schema=SCHEMA, family='historical_access', case=case,
                      custody=custody, mode=args['mode'], contract_commit=fixture['contract_commit'])
        if args['mode'] == 'verification':
            perf_path = Path(args['performance'])
            performance = json.loads(perf_path.read_text())
            manifest = json.loads((perf_path.parent / 'manifest.json').read_text())
            if manifest.get('result.json') != runtime.file_sha256(perf_path):
                raise ValueError('performance result seal mismatch')
            for name, digest in manifest.items():
                if Path(name).name != name or runtime.file_sha256(perf_path.parent / name) != digest:
                    raise ValueError('performance evidence seal mismatch')
            completion = json.loads((perf_path.parent / 'completion.json').read_text())
            if completion['status'] != 'PASS' or completion['total_including_receipts_ns'] >= LIMIT_NS:
                raise ValueError('performance outer deadline failed')
            if performance['status'] != 'PASS' or performance['case'] != case or performance['custody'] != custody or performance['mode'] != 'performance':
                raise ValueError('performance custody/selection mismatch')
            result['performance_sha256'] = runtime.file_sha256(perf_path)
        host = output / 'host-runtime'; host.mkdir()
        (host / 'tmp').mkdir()
        result['copy'] = runtime.closed_store_copy(Path(args['store']), host / 'store.sqlite', deadline=deadline)
        if result['copy']['master_store_sha256'] != fixture['store_sha256']:
            raise ValueError('NOT_READY: incompatible history Store')
        result['store_before'] = runner.sdk_store_observation(host / 'store.sqlite')
        (host / 'branch-id').write_text(fixture['branch_id'])
        (output / 'container-attempted').touch(exist_ok=False)
        sample = runtime.start_sample(image['Id'], args['container'], {'family': 'historical_access', 'run': output.name}, deadline=deadline)
        result['environment'] = sample.observation
        result['cgroup_before'] = runner.cgroup_snapshot(sample, deadline.end)
        command = [binary, 'historical-access-session', str(host), sample.id, case['commit_id'],
                   case['operation'], case['path'], str(case['offset']), str(case['length']), case['cache']]
        result['command'] = command
        result['preparation_ns'] = time.monotonic_ns() - args['started_ns']
        started = time.monotonic_ns()
        # Same process group as supervised worker: the outer watchdog kills the host too.
        with (output / 'host.stdout').open('xb') as stdout, (output / 'host.stderr').open('xb') as stderr:
            proc = subprocess.run(command, stdout=stdout, stderr=stderr,
                env={**os.environ, 'TMPDIR': str(host / 'tmp'), 'LAYERFS_EXEC_TRANSPORT': 'daemon', 'LAYERFS_FUSE_TRANSPORT': 'daemon'},
                timeout=deadline.require('host session'))
        result['host_session_ns'] = time.monotonic_ns() - started
        if proc.returncode: raise RuntimeError('host session exit ' + str(proc.returncode))
        records = [json.loads(line) for line in (output / 'host.stdout').read_text().splitlines()]
        result['records'] = records
        result['observed'] = check_output(records, case)
        result['cgroup_after'] = runner.cgroup_snapshot(sample, deadline.end)
        if any(result['cgroup_after'][k] for k in ('oom', 'oom_kill', 'swap_current')):
            raise RuntimeError('container OOM/swap')
        result['store_after'] = runner.sdk_store_observation(host / 'store.sqlite')
        result['store_growth_allocated_bytes'] = result['store_after']['allocated_bytes'] - result['store_before']['allocated_bytes']
        result['store_growth_scope'] = args['mode'] + '-only independent copy'
        result['attempted_operation_count'] = result['completed_operation_count'] = 1
        result['operation_surface'] = 'public SDK workspace / Linux FUSE POSIX'
        result['operation_entrypoint'] = 'storage-smoke-access'
        result['orchestration_executor'] = 'macOS host'
        result['timing_boundary_id'] = 'entry-through-receipts-and-teardown-15s-v1'
        result['clock_id'] = 'monotonic_ns'
        result['seed'] = 0
        result['repetition'] = 1
        result['source_arm'] = 'candidate'
        result['treatment'] = fixture.get('treatment', 'unpaired benchmark implementation qualification')
        result['custody_status'] = 'PASS'
        result['correctness_status'] = 'PASS'
        result['resource_status'] = 'PASS'
        result['metric_availability'] = {'unique_packs': None, 'phase_peak_rss_bytes': None,
            'reason': 'Store receipt has fetch counters; RSS receipts are point/lifetime observations, not exact phase peaks'}
        result['status'] = 'PASS'
    except Exception as error:
        result['status'] = 'NOT_READY' if 'NOT_READY' in str(error) or isinstance(error, FileNotFoundError) else 'FAIL'
        result['failure'] = type(error).__name__ + ': ' + str(error)
    save(output / 'worker-result.json', result)
    return 0 if result['status'] == 'PASS' else 1


def selected(args, started_ns):
    output = Path(args.output).resolve(); output.mkdir(parents=True, exist_ok=False)
    end = time.monotonic() + max(0, (LIMIT_NS - (time.monotonic_ns() - started_ns)) / 1e9)
    container = 'layerfs-ha-' + uuid.uuid4().hex[:12]
    config = {**vars(args), 'output': str(output), 'container': container,
              'started_ns': started_ns, 'work_end': end - 3}
    save(output / 'invocation.json', config)
    result = {'schema': SCHEMA, 'family': 'historical_access', 'status': 'FAIL', 'admission_eligible': False}
    lock = None
    try:
        lock = isolation.worktree_lock_path().open('a')
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        raw = runtime.run([sys.executable, str(Path(__file__).resolve()), '--worker', str(output / 'invocation.json')],
            deadline=runtime.Deadline(end - 3), check=False)
        (output / 'worker.stdout').write_bytes(raw.stdout)
        (output / 'worker.stderr').write_bytes(raw.stderr)
        if (output / 'worker-result.json').exists(): result.update(json.loads((output / 'worker-result.json').read_text()))
        if raw.timed_out or raw.returncode:
            result['status'] = 'FAIL' if raw.timed_out else result['status']
            result['worker_exit'] = {'returncode': raw.returncode, 'timed_out': raw.timed_out}
    except Exception as error:
        result['failure'] = type(error).__name__ + ': ' + str(error)
    finally:
        cleanup_started = time.monotonic_ns()
        try:
            if (output / 'container-attempted').exists():
                removed = runtime.run(['docker', 'rm', '--force', container], deadline=runtime.Deadline(end - 0.2), check=False)
                if removed.returncode and b'No such container' not in removed.stderr:
                    raise RuntimeError(removed.stderr.decode(errors="replace"))
            host = output / 'host-runtime'
            if host.exists() and result['status'] == 'PASS': shutil.rmtree(host)
            result['cleanup_status'] = 'PASS'
        except Exception as error:
            result['cleanup_status'] = 'FAIL'; result['cleanup_failure'] = str(error); result['status'] = 'FAIL'
        result['cleanup_ns'] = time.monotonic_ns() - cleanup_started
        if lock: lock.close()
    result['outer_wall_ns'] = time.monotonic_ns() - started_ns
    result['deadline_status'] = 'PASS' if result['outer_wall_ns'] < LIMIT_NS else 'FAIL'
    if result['deadline_status'] != 'PASS': result['status'] = 'FAIL'
    save(output / 'result.json', result)
    save(output / 'manifest.json', {p.name: runtime.file_sha256(p) for p in sorted(output.iterdir()) if p.is_file()})
    elapsed = time.monotonic_ns() - started_ns
    save(output / 'completion.json', {'total_including_receipts_ns': elapsed, 'status': result['status'] if elapsed < LIMIT_NS else 'FAIL'})
    print(json.dumps({'case': args.case, 'status': result['status'] if elapsed < LIMIT_NS else 'FAIL', 'total_ns': elapsed, 'output': str(output)}))
    return 0 if result['status'] == 'PASS' and elapsed < LIMIT_NS else 1


def main(argv=None, started_ns=None):
    started_ns = started_ns or time.monotonic_ns()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--family', choices=['historical_access'], default='historical_access')
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--self-check', action='store_true')
    parser.add_argument('--case')
    parser.add_argument('--all', action='store_true')
    parser.add_argument('--store', help='Explicit closed sealed Store; never constructed automatically')
    parser.add_argument('--fixture', help='Explicit prospectively defined fixture; default v2 remains unchanged')
    parser.add_argument('--image', default=os.environ.get('LAYERFS_BENCH_IMAGE'))
    parser.add_argument('--host-binary', default=str(runner.REPO / 'target/release/fs-benchmark-pro'))
    parser.add_argument('--output')
    parser.add_argument('--mode', choices=['performance', 'verification'], default='performance')
    parser.add_argument('--performance', help='Exact selected performance result.json (verification only)')
    args = parser.parse_args(argv)
    fixture = definition(args.fixture)
    if args.list or args.self_check:
        print(json.dumps(fixture if args.list else {'status':'PASS', 'cases':11})); return 0
    if not args.store or not args.image or not args.output or bool(args.case) == args.all:
        parser.error('explicit --store --image --output and exactly one of --case / --all required')
    if args.mode == 'verification' and (not args.performance or args.all):
        parser.error('verification requires one --case and its --performance result.json')
    if args.all:
        codes = []
        for case in fixture['cases']:
            child = argparse.Namespace(**vars(args)); child.case = case['id']; child.all = False
            child.output = str(Path(args.output) / case['id'])
            codes.append(selected(child, time.monotonic_ns()))
        return int(any(codes))
    if args.case not in {c['id'] for c in fixture['cases']}: parser.error('unregistered case')
    return selected(args, started_ns)


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--worker':
        sys.exit(worker(sys.argv[2]))
    sys.exit(main())
