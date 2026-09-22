#!/usr/bin/env python3
"""One bounded native namespace/attribute selection with live C5 and an owned runtime."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import time

import mkdir_route as driver

# The row identity is the test function's own check id, so a passing marker and
# the registered assertion cannot drift apart.
CASES = {
    'setattr': 'setattr-atomic-mode-mtime-size-and-refusals',
    'mknod': 'mknod-empty-regular-file-without-a-handle',
    'link': 'link-shares-one-inode-and-one-saved-version',
    'unlink': 'unlink-removes-one-name-and-keeps-open-orphan-lifetime',
    'unlink_fresh': 'fresh-create-then-unlink-emits-no-unbound-declaration',
    'rmdir': 'rmdir-uses-the-effective-emptiness-check',
    'rename': 'rename-publishes-both-parent-edits-atomically',
    'rename_base': 'tombstone-hides-an-inherited-binding',
    'generation': 'g-plus-one-keeps-later-namespace-and-metadata-changes',
    'notification_failure': 'multi-entry-sdk-mutation-keeps-its-published-result',
}
TESTS = {case: f'namespace_{case}' for case in CASES}

driver.CASES = CASES
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_SOURCE = Path(__file__).with_name('namespace.rs')
driver.TEST_PREFIX = 'namespace_'
driver.TEST_MARKER = 'NAMESPACE_CHECK'
driver.MODE = 'functional-native-workspace-namespace'
driver.NOT_RUN = ['actual mounted unlink/rename/mknod/link syscalls', 'prepared npm workload',
                  'R6', 'hard RSS/cgroup memory bound', 'crash/restart recovery',
                  'directory deltas beyond the 128-name admission bound']


def execute(args, report, started):
    def remaining():
        seconds = 60 - (time.monotonic() - started)
        if seconds <= 0:
            raise TimeoutError('complete namespace selection exceeded 60 seconds')
        return seconds

    def command(argv, **options):
        return subprocess.run(argv, check=True, capture_output=True, timeout=remaining(), **options)

    name = 'layerfs-namespace-' + os.urandom(6).hex()
    volume = name + '-root'
    import json
    import shutil
    import stage_route as shared
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['logical_bytes'] == 64 * 1024 * 1024
    master = args.fixture.parent / 'service/store.sqlite'
    digest = driver.sha(master)
    assert digest == fixture['closed_master_sha256']['store.sqlite']
    service_dir = args.output / 'service'
    service_dir.mkdir()
    shutil.copyfile(master, service_dir / 'store.sqlite')
    assert driver.sha(service_dir / 'store.sqlite') == digest
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'closed_store_sha256': digest,
                               'clone_method': 'independent-byte-copy', 'cache_claim': None}
    private, server_private = os.urandom(32).hex(), os.urandom(32).hex()
    public, server_public = shared.route.public_key(private), shared.route.public_key(server_private)
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=server_private, LAYERFS_PEERS=f'1,{public},{int(time.time()) + 3600},255',
               LAYERFS_STORE=str(service_dir / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0',
               LAYERFS_TELEMETRY='off', LAYERFS_HISTORY_CATALOG=str(service_dir / 'history.sqlite'),
               LAYERFS_HISTORY_CREATE='1', LAYERFS_HISTORY_BINDING='pair1-stage',
               LAYERFS_HISTORY_INCARNATION='1', LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(),
               LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([shared.route.BIN / 'layerfs-service'], env=env,
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    created = made_volume = False
    readiness = ''
    child = None
    cleanup_errors = []
    report.update(owned_service_pid=service.pid, owned_container=name, owned_volume=volume)

    def cleanup(owner, action):
        try:
            action()
        except BaseException as error:
            cleanup_errors.append({'owner': owner, 'error': repr(error)})

    try:
        readiness = shared.mounted.line_until(service, timeout=min(10, remaining()))
        assert 'ready' in readiness, readiness
        port = int(readiness.strip().rsplit(':', 1)[1])
        report['fixture'] = shared.bootstrap(service_dir, port, private, server_public,
                                            bytes.fromhex(fixture['file_root']), False)
        command(['docker', 'volume', 'create', volume])
        made_volume = True
        command(['docker', 'run', '-d', '--privileged', '--cpus=2', '--name', name,
                 '--add-host', 'host.docker.internal:host-gateway',
                 '--mount', f'type=bind,src={args.test_binary.parent},dst=/runner,readonly',
                 '--mount', f'type=volume,src={volume},dst=/stage', args.image, 'sleep', 'infinity'])
        created = True
        command(['docker', 'exec', name, 'chmod', '700', '/stage'])
        invocation = ['docker', 'exec', '-i', '-e', f'LAYERFS_ENDPOINT=host.docker.internal:{port}',
                      '-e', 'LAYERFS_PRIVATE_KEY', '-e', 'LAYERFS_SERVER_KEY',
                      '-e', f'LAYERFS_STAGE_BRANCH={report["fixture"]["branch"]}',
                      '-e', 'LAYERFS_STAGE_TEST_ROOT=/stage', '-e', 'LAYERFS_CONSTRUCTION_WORKERS=1']
        if os.environ.get('LAYERFS_QQ'):
            invocation += ['-e', 'LAYERFS_QQ=/stage/qq.log']
        child_env = os.environ.copy()
        child_env.update(LAYERFS_PRIVATE_KEY=private, LAYERFS_SERVER_KEY=server_public,
                         LAYERFS_CONSTRUCTION_WORKERS='1')
        selection = f'linux::{TESTS[args.case]}'
        invocation += [name, '/runner/' + args.test_binary.name, '--ignored', '--nocapture',
                       '--test-threads=1', selection, '--exact']
        report['test_selection'] = selection
        report['invocation'] = invocation
        with (args.output / 'test.stdout').open('wb') as output, (args.output / 'test.stderr').open('wb') as errors:
            child = subprocess.Popen(invocation, env=child_env, stdin=subprocess.PIPE,
                                     stdout=output, stderr=errors)
            child.stdin.close()
            child.wait(timeout=remaining())
        report['test_exit'] = child.returncode
        text = (args.output / 'test.stdout').read_text(errors='replace')
        for row in report['checks']:
            marker = driver.TEST_MARKER
            if row['id'] == 'native-clean-close':
                # The shared driver names this row from its own fixture marker;
                # this selection's test binary prints this file's marker instead.
                marker = driver.TEST_MARKER
            if f'{marker} {row["id"]} PASS' in text:
                row['status'] = 'PASS'
        report['observations'] = [line for line in text.splitlines()
                                  if line.startswith('NAMESPACE_')]
        assert child.returncode == 0 and all(row['status'] == 'PASS' for row in report['checks']), text[-4000:]
    finally:
        if child is not None and child.poll() is None:
            report['test_forced_cleanup'] = True
            cleanup('test-client', lambda: (child.kill(), child.wait(timeout=6)))
        if created and os.environ.get('LAYERFS_QQ'):
            def collect_qq():
                with (args.output / 'qq.log').open('wb') as sink:
                    subprocess.run(['docker', 'exec', name, 'sh', '-c',
                                    'cat /stage/qq.log 2>/dev/null || true'],
                                   stdout=sink, check=False, timeout=10)
            cleanup('diagnostics', collect_qq)
        if created:
            def remove_container():
                nonlocal created
                subprocess.run(['docker', 'rm', '-f', name], check=True, capture_output=True, timeout=10)
                created = False
            cleanup('container', remove_container)
        if service.poll() is None:
            def stop_service():
                service.stdin.close()
                try:
                    service.wait(timeout=6)
                except subprocess.TimeoutExpired:
                    report['service_forced_cleanup'] = True
                    service.kill()
                    service.wait(timeout=6)
            cleanup('service', stop_service)
        if made_volume:
            def remove_volume():
                nonlocal made_volume
                subprocess.run(['docker', 'volume', 'rm', volume], check=True, capture_output=True, timeout=10)
                made_volume = False
            cleanup('volume', remove_volume)
        report['service_exit'] = service.poll()
        if service.poll() is not None:
            (args.output / 'service.stderr').write_bytes(readiness.encode() + service.stderr.read())
        report['cleanup_errors'] = cleanup_errors
        if created:
            report['retained_container'] = name
        if made_volume:
            report['retained_volume'] = volume
    assert not cleanup_errors and not created and not made_volume
    assert service.returncode == 0 and not report.get('service_forced_cleanup') and not report.get('test_forced_cleanup')
    assert driver.sha(master) == digest
    report['cleanup'] = 'PASS: native Workspace clean close marker, test/service exited, owned container/volume removed'
    report['closed_master_unchanged'] = True
    assert time.monotonic() - started <= 60


driver.execute = execute

if __name__ == '__main__':
    driver.main()
