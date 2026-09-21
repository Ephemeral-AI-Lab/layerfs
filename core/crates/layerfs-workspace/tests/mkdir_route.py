#!/usr/bin/env python3
"""One bounded native mkdir selection with live C5 and an owned Linux runtime."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import uuid

import stage_route as shared

CASES = {
    'semantics': 'nested-mode-umask-forget-listing-and-two-explicit-generations',
    'successor': 'captured-new-parent-retains-D1-through-CommitStaged-and-next-Commit',
    'capacity': 'fixed-long-name-envelope-tree-pages-and-mixed-file-Commit',
    'refusals': 'invalid-readonly-access-deadline-and-mounted-refuse-before-Reserve',
    'reserve_denied': 'denied-Reserve-consumed-none-no-replay-or-namespace-publication',
    'reserve_unknown': 'unknown-Reserve-consumed-once-no-replay-or-namespace-publication',
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def execute(args, report, started):
    def remaining():
        seconds = 60 - (time.monotonic() - started)
        if seconds <= 0:
            raise TimeoutError('complete mkdir selection exceeded 60 seconds')
        return seconds

    def command(argv, **options):
        return subprocess.run(argv, check=True, capture_output=True, timeout=remaining(), **options)

    name = 'layerfs-mkdir-' + uuid.uuid4().hex[:12]; volume = name + '-root'
    owners = args.output / 'owners.jsonl'
    owners.touch(exist_ok=False)

    def ownership(event, **fields):
        # JSON-lines permits appending PID/acquisition/cleanup observations without
        # rewriting an earlier custody record. Closing each append flushes it.
        with owners.open('a') as log:
            log.write(json.dumps({'event': event, 'monotonic': time.monotonic(), **fields}) + '\n')

    ownership('planned', format='json-lines', case=args.case, output=str(args.output),
        worktree=str(shared.ROOT), driver_pid=os.getpid(), driver_process_group=os.getpgrp(),
        service_pid=None, service_executable=str(shared.route.BIN / 'layerfs-service'),
        service_directory=str(args.output / 'service'), container=name, volume=volume,
        owner_permission_root='/stage/owner-host' if args.case == 'refusals' else None)
    report['ownership_journal'] = str(owners)
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['logical_bytes'] == 64 * 1024 * 1024
    master = args.fixture.parent / 'service/store.sqlite'
    digest = sha(master)
    assert digest == fixture['closed_master_sha256']['store.sqlite']
    for suffix in ('-wal', '-shm', '-journal'):
        assert not Path(str(master) + suffix).exists()
    service_dir = args.output / 'service'; service_dir.mkdir()
    copied = time.monotonic()
    shutil.copyfile(master, service_dir / 'store.sqlite')
    assert sha(service_dir / 'store.sqlite') == digest
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': sha(args.fixture),
        'closed_store_sha256': digest, 'clone_method': 'independent-byte-copy',
        'copy_seconds': time.monotonic() - copied, 'history': 'fresh live CREATE=1; no copied catalog',
        'cache_claim': None}
    private, server_private = os.urandom(32).hex(), os.urandom(32).hex()
    public, server_public = shared.route.public_key(private), shared.route.public_key(server_private)
    denied = os.urandom(32).hex() if args.case == 'reserve_denied' else None
    peers = f'1,{public},{int(time.time()) + 3600},255'
    if denied:
        peers += f';2,{shared.route.public_key(denied)},{int(time.time()) + 3600},63'
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=server_private, LAYERFS_PEERS=peers,
        LAYERFS_STORE=str(service_dir / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0', LAYERFS_TELEMETRY='off',
        LAYERFS_HISTORY_CATALOG=str(service_dir / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
        LAYERFS_HISTORY_BINDING='pair1-stage', LAYERFS_HISTORY_INCARNATION='1',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([shared.route.BIN / 'layerfs-service'], env=env,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    ownership('service-started', service_pid=service.pid)
    created = made_volume = False
    readiness = ''; child = None; proxy = None
    report.update(owned_service_pid=service.pid, owned_container=name, owned_volume=volume)
    cleanup_errors = []

    def cleanup(owner, action):
        try:
            action()
        except BaseException as error:
            cleanup_errors.append({'owner': owner, 'error': repr(error)})

    try:
        readiness = shared.mounted.line_until(service, timeout=min(10, remaining()))
        assert 'ready' in readiness
        port = int(readiness.strip().rsplit(':', 1)[1])
        report['fixture'] = shared.bootstrap(service_dir, port, private, server_public,
                                             bytes.fromhex(fixture['file_root']), False)
        command(['docker', 'volume', 'create', volume]); made_volume = True
        command(['docker', 'run', '-d', '--privileged', '--cpus=2', '--name', name,
            '--add-host', 'host.docker.internal:host-gateway',
            '--mount', f'type=bind,src={args.test_binary.parent},dst=/runner,readonly',
            '--mount', f'type=volume,src={volume},dst=/stage', args.image, 'sleep', 'infinity'])
        created = True
        nano_cpus = int(command(['docker', 'inspect', '--format', '{{.HostConfig.NanoCpus}}', name], text=True).stdout)
        assert nano_cpus == 2_000_000_000
        report['actual_nano_cpus'] = nano_cpus
        command(['docker', 'exec', name, 'chmod', '700', '/stage'])
        if args.case == 'refusals':
            command(['docker', 'exec', name, 'mkdir', '-p', '/stage/owner-host/workspace'])
            command(['docker', 'exec', name, 'chmod', '700', '/stage/owner-host', '/stage/owner-host/workspace'])
            command(['docker', 'exec', name, 'chown', '1000:1000', '/stage/owner-host', '/stage/owner-host/workspace'])
            owner_paths = command(['docker', 'exec', name, 'stat', '-c', '%u:%g %a %n',
                '/stage/owner-host', '/stage/owner-host/workspace'], text=True).stdout.splitlines()
            assert owner_paths == ['1000:1000 700 /stage/owner-host', '1000:1000 700 /stage/owner-host/workspace']
            report['owner_permission_fixture'] = {'paths': owner_paths, 'configured_uid': 1000,
                'configured_gid': 1000, 'test_process_uid': 0, 'scope': 'native configured owner access checks'}
        report['kernel'] = command(['docker', 'exec', name, 'uname', '-srmo'], text=True).stdout.strip()
        report['filesystem'] = command(['docker', 'exec', name, 'findmnt', '-n', '-o', 'FSTYPE', '-T', '/stage'], text=True).stdout.strip()
        assert report['filesystem'] == 'ext4'
        invocation = ['docker', 'exec', '-i', '-e', f'LAYERFS_ENDPOINT=host.docker.internal:{port}',
            '-e', 'LAYERFS_PRIVATE_KEY', '-e', 'LAYERFS_SERVER_KEY',
            '-e', f'LAYERFS_STAGE_BRANCH={report["fixture"]["branch"]}',
            '-e', 'LAYERFS_STAGE_TEST_ROOT=/stage', '-e', 'LAYERFS_CONSTRUCTION_WORKERS=1']
        child_env = os.environ.copy()
        child_env.update(LAYERFS_PRIVATE_KEY=private, LAYERFS_SERVER_KEY=server_public,
                         LAYERFS_CONSTRUCTION_WORKERS='1')
        if denied:
            child_env['LAYERFS_RESERVE_PRIVATE_KEY'] = denied
            invocation += ['-e', 'LAYERFS_RESERVE_PRIVATE_KEY']
        if args.case == 'reserve_unknown':
            proxy = shared.lost_result_proxy(port)
            invocation += ['-e', f'LAYERFS_RESERVE_ENDPOINT=host.docker.internal:{proxy[0]}']
        selection = f'linux::mkdir_{args.case}'
        invocation += [name, '/runner/' + args.test_binary.name, '--ignored', '--nocapture', '--test-threads=1', selection, '--exact']
        report['test_selection'] = selection
        report['invocation'] = invocation
        with (args.output / 'test.stdout').open('wb') as output, (args.output / 'test.stderr').open('wb') as errors:
            child = subprocess.Popen(invocation, env=child_env, stdin=subprocess.PIPE, stdout=output, stderr=errors)
            child.stdin.close(); child.wait(timeout=remaining())
        report['test_exit'] = child.returncode
        text = (args.output / 'test.stdout').read_text(errors='replace')
        for row in report['checks']:
            if f'MKDIR_CHECK {row["id"]} PASS' in text:
                row['status'] = 'PASS'
        report['observations'] = [line for line in text.splitlines()
            if any(marker in line for marker in ('MKDIR_CAPACITY ', 'MKDIR_RESERVE ', 'MKDIR_RESOURCE '))]
        if proxy:
            proxy[1].join(timeout=min(5, remaining()))
            for thread in proxy[2]: thread.join(timeout=min(5, remaining()))
            report['native_result_loss'] = proxy[3] | {
                'scope': 'actual encrypted Reserve terminal withheld once; later reservations are independent observations'}
            assert not proxy[1].is_alive() and all(not thread.is_alive() for thread in proxy[2])
            assert 'error' not in proxy[3] and proxy[3].get('withheld_ciphertext_bytes', 0) > 0
        assert child.returncode == 0 and all(row['status'] == 'PASS' for row in report['checks'])
    finally:
        if child is not None and child.poll() is None:
            report['test_forced_cleanup'] = True
            cleanup('test-client', lambda: (child.kill(), child.wait(timeout=6)))
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
                    service.kill(); service.wait(timeout=6)
            cleanup('service', stop_service)
        if proxy:
            def stop_proxy():
                proxy[1].join(timeout=5)
                for thread in proxy[2]: thread.join(timeout=5)
                report['native_result_loss'] = proxy[3] | {
                    'scope': 'actual encrypted Reserve terminal withheld once; later reservations are independent observations'}
                assert not proxy[1].is_alive() and all(not thread.is_alive() for thread in proxy[2])
            cleanup('proxy', stop_proxy)
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
        if created: report['retained_container'] = name
        if made_volume: report['retained_volume'] = volume
    assert not cleanup_errors and not created and not made_volume
    assert service.returncode == 0 and not report.get('service_forced_cleanup') and not report.get('test_forced_cleanup')
    assert sha(master) == digest
    report['cleanup'] = 'PASS: native Workspace clean close marker, test/service exited, owned container/volume removed'
    report['closed_master_unchanged'] = True
    assert time.monotonic() - started <= 60


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--binaries', type=Path, required=True)
    parser.add_argument('--test-binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--image', required=True)
    parser.add_argument('--case', choices=CASES, required=True)
    args = parser.parse_args()
    for key in ('fixture', 'binaries', 'test_binary', 'output'):
        setattr(args, key, getattr(args, key).resolve())
    args.output.mkdir(parents=True, exist_ok=False)
    shared.route.BIN = args.binaries
    os.environ['LAYERFS_CONSTRUCTION_WORKERS'] = '1'
    report = {'status': 'FAIL', 'mode': 'functional-native-workspace-mkdir', 'case': args.case,
        'checks': [{'id': name, 'status': 'NOT_RUN'} for name in (CASES[args.case], 'native-clean-close')],
        'hard_budget_seconds': 60, 'callback_deadline_seconds': 10, 'construction_workers': 1,
        'performance_claim': False, 'cache_claim': None,
        'not_run': ['successful mounted/kernel mkdir', 'create/unlink/rename/symlink',
                    'prepared npm workload', 'R6', 'hard RSS/cgroup memory bound', 'crash/restart recovery']}
    started = time.monotonic()
    def expired(_signal, _frame):
        raise TimeoutError('complete mkdir selection exceeded 60 seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        space = shared.isolation.namespace()
        for path in (args.fixture, args.binaries, args.test_binary, args.output):
            space.assert_owned(path, 'native mkdir input/output')
        report.update(source=shared.mounted.checked(['git', 'rev-parse', 'HEAD'], text=True).stdout.strip(),
            product_inputs_sha256=shared.mounted.product_inputs(), driver_sha256=sha(Path(__file__)),
            test_source_sha256=sha(Path(__file__).with_name('mkdir.rs')), test_binary_sha256=sha(args.test_binary),
            helper_source_sha256=sha(Path(__file__).parent / 'support/native_workspace.rs'),
            binaries={name: sha(args.binaries / name) for name in ('layerfs-service', 'layerfs-daemon', 'examples/public_key')},
            resource_isolation=space.as_fields(),
            image_id=shared.mounted.checked(['docker', 'image', 'inspect', '--format', '{{.Id}}', args.image], text=True).stdout.strip(),
            caller_dependencies_sha256={str(Path(module.__file__).resolve().relative_to(shared.ROOT)): sha(Path(module.__file__))
                for module in tuple(sys.modules.values()) if getattr(module, '__file__', None)
                and Path(module.__file__).resolve().is_relative_to(shared.ROOT) and Path(module.__file__).suffix == '.py'})
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            execute(args, report, started)
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error)
        raise
    finally:
        signal.alarm(0)
        report['command_wall_seconds'] = time.monotonic() - started
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
