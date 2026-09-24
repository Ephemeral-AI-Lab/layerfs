#!/usr/bin/env python3
"""One registered R3b functional selection via public Workspace and native service.

The service and Store/catalog stay on the host. Linux owns its private local
backing. Each selection uses an independent byte copy of the closed R1 fixture;
this is neither a mounted-write result nor a performance/cache comparison.
"""
import argparse
import fcntl
import re
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'core/crates/layerfs-daemon/tests'))
import history_route as route
import mounted_read as mounted
import payload_route as payload
sys.path.insert(0, str(ROOT / "core/benchmark/fs-bench-pro-storage-content/shared"))
import isolation

CASES = {
    'inode_frontier': ['all-104-existing-inodes-survive-global-index-splits',
                       'disk-dirty-frontier-matches-all-104-public-edits'],
    'native_shape': ['native-truncation-and-growth-invalidate-allocation-observation',
                     'healthy-cleanup-preserves-other-arena-incompleteness'],
    'ledger_write_failure': ['native-ledger-write-failure-retains-prospective-payload-custody'],
    'large_base': ['large-base-edit-keeps-inherited-bytes-as-references',
                   'large-base-edited-and-distant-window-oracles'],
    'ledger_collider': ['valid-ledger-header-does-not-confer-ownership',
                        'explicit-reclaim-after-known-collider-removal'],
    'corruption': ['corrupt-live-page-refuses-without-dropping-owned-state'],
    'metadata_failure': ['native-metadata-failure-preserves-visible-base-and-quarantine',
                         'explicit-metadata-cleanup-releases-known-partial-allocation'],
    'aggregate': ['shared-consumer-quota-and-memory-account', 'independent-local-roots-share-budget'],
    'semantics': ['explicit-local-access-and-unbound-projection-refusal', 'atomic-bytes-attributes-and-hardlinks',
                  'current-coordinate-overlap-insert-delete',
                  'node-forget-retains-dirty-index-and-branch', 'dirty-close-preserves-owned-state'],
    'refusals': ['range-deadline-kind-failure-atomicity', 'read-only-and-foreign-payload-refusal',
                 'shared-replay-input-cap'],
    'frontier': ['exact-256-normalized-edits-refuses-257', 'repeated-overwrite-reuses-frontier-capacity'],
    'metadata_quota': ['payload-fits-metadata-reserve-refuses-atomically',
                       'clean-refusal-releases-confirmed-resources'],
}


REQUIREMENTS = {
    'inode_frontier': ['B-16', 'B-21'],
    'native_shape': ['B-20', 'B-25'],
    'ledger_write_failure': ['B-20', 'B-22'],
    'large_base': ['B-09', 'B-14'],
    'semantics': ['W-01', 'W-07', 'S-15', 'B-09', 'B-10'],
    'refusals': ['W-12', 'B-04', 'B-05', 'B-15'],
    'frontier': ['B-05', 'B-14', 'B-16'],
    'metadata_quota': ['B-01', 'B-15'],
    'metadata_failure': ['B-15', 'B-20'],
    'aggregate': ['B-02', 'B-14'],
    'corruption': ['B-15', 'B-20'],
    'ledger_collider': ['B-15', 'B-20'],
}

TEST_SOURCE = Path(__file__).with_name('local_edit.rs')
ENTRY_SOURCE = Path(__file__)
TEST_PREFIX = 'local_range_edit_'
TEST_MARKER = 'LOCAL_EDIT_CHECK'
MODE = 'functional-local-range-edit'
REQUIREMENT_SCOPE = 'local operation subset only; no full mounted/S/B row completion'
NOT_RUN = ['mounted writes/coherence', 'snapshot G/live successor', 'Commit', 'npm', 'R6',
           'full 64MiB Workspace readback (large_base declares edited and distant window checks)',
           '128 dirty inode boundary: current fixture has fewer inodes and shared creation is bounded']
OBSERVATION_MARKERS = ('LOCAL_EDIT_RESOURCE ', 'LOCAL_EDIT_NATIVE_FAILURE ', 'LOCAL_EDIT_CORRUPTION ',
                       'LOCAL_EDIT_LARGE_BASE ', 'LOCAL_EDIT_NATIVE_SHAPE ')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, report, start):
    def remaining():
        value = 60 - (time.monotonic() - start)
        if value <= 0:
            raise TimeoutError('complete functional selection exceeded 60 seconds')
        return value
    def command(argv):
        return subprocess.run(argv, check=True, text=True, capture_output=True, timeout=remaining())
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS'
    if args.case == 'large_base':
        assert fixture['mode'] == 'prepared-local-edit-large-fixture' and fixture['logical_bytes'] == 64 * 1024 * 1024
    else:
        assert fixture['mode'] == 'functional-mounted-proof'
    directory = args.output / 'service'; directory.mkdir()
    seals = {}
    for name in ('store.sqlite', 'history.sqlite'):
        source, target = args.fixture.parent / 'service' / name, directory / name
        seals[name] = sha(source); shutil.copyfile(source, target); assert sha(target) == seals[name]
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': sha(args.fixture),
                               'closed_master_sha256': seals, 'clone_method': 'independent-byte-copy',
                               'cache_claim': None}
    server_key, client_key = os.urandom(32).hex(), os.urandom(32).hex()
    server_public, client_public = route.public_key(server_key), route.public_key(client_key)
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=server_key,
               LAYERFS_PEERS=f'1,{client_public},{int(time.time()) + 3600},127',
               LAYERFS_STORE=str(directory / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0',
               LAYERFS_TELEMETRY='off', LAYERFS_HISTORY_CATALOG=str(directory / 'history.sqlite'),
               LAYERFS_HISTORY_BINDING='pair1-mounted-read', LAYERFS_HISTORY_CREATE='0',
               LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([route.BIN / 'layerfs-server'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    name = 'layerfs-edit-' + uuid.uuid4().hex[:16]; volume = name + '-data'
    created = made_volume = False; readiness = ''
    try:
        readiness = mounted.line_until(service, timeout=min(10, remaining()))
        assert 'ready' in readiness, readiness
        port = int(readiness.strip().rsplit(':', 1)[1])
        command(['docker', 'volume', 'create', volume]); made_volume = True
        command(['docker', 'run', '-d', '--cpus=2', '--privileged', '--name', name,
                 '--add-host', 'host.docker.internal:host-gateway',
                 '--mount', f'type=bind,src={ROOT},dst=/work,readonly',
                 '--mount', f'type=volume,src={volume},dst=/local-edit',
                 args.image, 'sleep', 'infinity']); created = True
        report['runtime_nano_cpus'] = int(command(
            ['docker', 'inspect', '--format', '{{.HostConfig.NanoCpus}}', name]).stdout.strip())
        assert report['runtime_nano_cpus'] == 2_000_000_000, report['runtime_nano_cpus']
        command(['docker', 'exec', name, 'chmod', '700', '/local-edit'])
        report['kernel'] = command(['docker', 'exec', name, 'uname', '-srmo']).stdout.strip()
        report['filesystem'] = command(['docker', 'exec', name, 'findmnt', '-n', '-o', 'FSTYPE', '-T', '/local-edit']).stdout.strip()
        assert report['filesystem'] == 'ext4'
        invocation = ['docker', 'exec', '-e', f'LAYERFS_ENDPOINT=host.docker.internal:{port}',
                      '-e', 'LAYERFS_PRIVATE_KEY', '-e', 'LAYERFS_SERVER_KEY',
                      '-e', f'LAYERFS_EDIT_BRANCH={fixture["branch"]}',
                      '-e', 'LAYERFS_EDIT_TEST_ROOT=/local-edit', '-e', 'LAYERFS_CONSTRUCTION_WORKERS=1', name,
                      '/work/' + str(args.test_binary.relative_to(ROOT)), '--ignored', '--nocapture',
                      '--test-threads=1', f'linux::{TEST_PREFIX}{args.case}', '--exact']
        if args.case in ('metadata_failure', 'ledger_write_failure'):
            binary_at = invocation.index(name) + 1
            invocation[binary_at:binary_at] = ['sh', '-c', 'trap "" XFSZ; exec "$@"', 'sh']
        # Credentials are intentionally absent from the recorded invocation.
        report['test_selection'] = f'linux::{TEST_PREFIX}{args.case}'
        child_env = os.environ.copy()
        child_env.update(LAYERFS_PRIVATE_KEY=client_key, LAYERFS_SERVER_KEY=server_public)
        work = isolation.concurrent_work()
        for item in work:
            item['command'] = re.sub(r'(?i)([a-z_]*(?:key|secret|token|password)[a-z_]*=)[^\s]+', r'\1<redacted>', item['command'])
        report['resource_isolation']['concurrent_work'] = work
        result = subprocess.run(invocation, env=child_env, text=True, capture_output=True, timeout=remaining())
        (args.output / 'test.stdout').write_text(result.stdout); (args.output / 'test.stderr').write_text(result.stderr)
        for row in report['checks']:
            if f'{TEST_MARKER} {row["id"]} PASS' in result.stdout:
                row['status'] = 'PASS'
        report['test_exit'] = result.returncode
        report['observations'] = [line for line in result.stdout.splitlines() if line.startswith(OBSERVATION_MARKERS)]
        assert result.returncode == 0 and all(row['status'] == 'PASS' for row in report['checks']), result.stderr[-3000:]
        command(['docker', 'rm', '-f', name]); created = False
        command(['docker', 'volume', 'rm', volume]); made_volume = False
        report['test_environment_cleanup'] = 'PASS: owned runtime removed after test process exit; dirty close is not claimed'
    finally:
        if service.poll() is None:
            service.stdin.close()
            try:
                service.wait(timeout=min(6, max(0.01, 60 - (time.monotonic() - start))))
            except subprocess.TimeoutExpired:
                service.kill(); service.wait(timeout=6); report['service_forced_cleanup'] = True
        (args.output / 'service.stderr').write_bytes(readiness.encode() + service.stderr.read())
        if created: report['retained_container'] = name
        if made_volume: report['retained_volume'] = volume
    assert service.returncode == 0 and not report.get('service_forced_cleanup')
    for file, expected in seals.items():
        assert sha(args.fixture.parent / 'service' / file) == expected
        assert sha(directory / file) == expected, f'local mutation changed remote {file}'
    report['remote_files_unchanged'] = True
    report['command_wall_seconds'] = time.monotonic() - start
    assert report['command_wall_seconds'] <= 60


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--test-binary', type=Path, required=True)
    parser.add_argument('--binaries', type=Path, default=route.BIN)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--case', choices=CASES, required=True)
    parser.add_argument('--image', default='rust:1.85.1-bookworm')
    args = parser.parse_args()
    args.fixture = args.fixture.resolve(); args.test_binary = args.test_binary.resolve(); args.output = args.output.resolve()
    route.BIN = args.binaries.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    report = {'status': 'FAIL', 'mode': MODE, 'case': args.case,
              'checks': [{'id': key, 'status': 'NOT_RUN'} for key in CASES[args.case]],
              'hard_budget_seconds': 60, 'docker_cpus': 2, 'performance_claim': False, 'cache_claim': None,
              'packet_requirement_ids': REQUIREMENTS[args.case],
              'requirement_scope': REQUIREMENT_SCOPE, 'not_run': NOT_RUN}
    started = time.monotonic()
    try:
        report.update(source=payload.command(['git', 'rev-parse', 'HEAD'], cwd=ROOT).stdout.strip(),
                      product_inputs_sha256=payload.product_inputs(), driver_sha256=sha(Path(__file__)),
                      test_source_sha256=sha(TEST_SOURCE), entrypoint_sha256=sha(ENTRY_SOURCE),
                      helper_source_sha256=sha(Path(__file__).parent / 'support/native_workspace.rs'),
                      test_binary_sha256=sha(args.test_binary), service_binary_sha256=sha(route.BIN / 'layerfs-server'),
                      image=payload.command(['docker', 'image', 'inspect', args.image, '--format', '{{.Id}}']).stdout.strip())
        space = isolation.namespace()
        for path in (args.output, args.fixture, args.test_binary, route.BIN):
            space.assert_owned(path, 'functional proof input/output')
        report['resource_isolation'] = space.as_fields() | {
            'artifact_root': str(args.output.parent), 'run_output': str(args.output),
            'build_target': str(ROOT / 'core/target'),
            'linux_build_target': str(ROOT / 'core/target-linux'),
            'shared_read_only': [str(args.fixture), str(args.test_binary)],
            'observation': 'ps snapshot immediately before the public test process; empty means none observed, not a quiet-host claim',
        }
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            run(args, report, started)
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error); raise
    finally:
        report.setdefault('command_wall_seconds', time.monotonic() - started)
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
