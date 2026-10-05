#!/usr/bin/env python3
"""Authenticated selector-only Attach through the existing real Linux lifecycle route."""
import os
from pathlib import Path
import signal
import subprocess
import time

import control_mount as mounted
import control_unmount as driver

driver.MODE = 'functional-authenticated-attach'
driver.ENTRY_SOURCE = Path(__file__)
driver.CONTROL_PEERS = [(1, 'good', 31), (2, 'old_lifecycle', 15), (3, 'attach', 16),
                       (4, 'none', 0), (5, 'expiry', 31), (6, 'status', 1),
                       (7, 'unmount', 2), (8, 'close', 4), (9, 'mount', 8)]
driver.CASES = {
    'attach_success': ['Unmount-CloseClean-Attach-new-identity-Status-Mount-read-Unmount-CloseClean'],
    'attach_signal': ['signal-shutdown-cleans-replacement-target-and-native-mount-owner'],
    'attach_authority': ['Attach-grant-authentication-stale-target-and-service-authority-are-independent'],
    'attach_open': ['mounted-and-unmounted-open-owners-refuse-Attach-without-replacement'],
    'attach_stopping': ['stopping-nonclosed-owner-refuses-Attach-until-explicit-cleanup'],
    'attach_unresolved': ['retained-Unmount-owner-refuses-Attach-without-losing-custody'],
    'attach_loss': ['lost-Attach-terminal-is-Unknown-with-one-attempt-and-observed-current-custody'],
    'attach_refusals': ['invalid-selector-incomplete-input-and-expired-headroom-never-admit-Attach'],
    'attach_expiry': ['expired-established-peer-cannot-begin-Attach'],
    'attach_collision': ['unowned-mount-leaf-collision-preserves-closed-prior-target-and-foreign-leaf'],
    'attach_failed': ['failed-Attach-Status-and-explicit-CloseClean-dispose-retained-empty-owner'],
}
driver.NOT_RUN = ['writable daemon startup/edit/Commit controls',
                  'remote Attach with retained acquired native mount/backing/arena resources',
                  'deterministic signal overlap inside remote Attach I/O',
                  'namespace/npm/R6', 'hard RSS/cgroup/performance qualification']

NEXT = {'workspace': b'next', 'incarnation': b'\x72' * 32}
THIRD = {'workspace': b'third', 'incarnation': b'\x74' * 32}


def attach(client, identity, workspace=b'next', incarnation=b'\x72' * 32, **options):
    driver.begin(client, identity, workspace, incarnation, opcode=13, **options)
    return driver.receive(client, result_tag=15, workspace=workspace, incarnation=incarnation)


def current(client, identity, target=None, mounted=False, stopping=False, closed=False):
    target = NEXT if target is None else target
    kind, result = driver.status.query(client, identity, **target)
    assert kind == 6 and 'attachment' not in result, result
    assert result['workspace'] == target.get('workspace', b'read').decode(), result
    assert result['incarnation'] == target.get('incarnation', b'\x71' * 32).hex(), result
    assert (result['mounted'], result['stopping'], result['closed']) == (mounted, stopping, closed), result
    return result


def closed_initial(client):
    assert driver.unmount(client, 2)['outcome'] == 'Unmounted'
    assert driver.close_clean(client, 3)['outcome'] == 'Closed'
    return current(client, 4, {}, stopping=True, closed=True)


def check_new_read(client, name, first, target=None, close=True):
    target = NEXT if target is None else target
    assert driver.remount(client, first, **target)['outcome'] == 'Mounted'
    observed = current(client, first + 1, target, mounted=True)
    value = mounted.mounted_read(name, target['workspace'].decode())
    if close:
        assert driver.unmount(client, first + 2, **target)['outcome'] == 'Unmounted'
        assert driver.close_clean(client, first + 3, **target)['outcome'] == 'Closed'
        current(client, first + 4, target, stopping=True, closed=True)
    return {'status': observed, 'read': value}


def expect_denied_status(client, identity, target):
    result = driver.status.query(client, identity, **target)
    assert result == (7, {'code': 3, 'unknown': 0, 'cleanup': 0}), result
    return result


def finish_loss(proxy):
    proxy[1].join(6)
    for worker in proxy[2]:
        worker.join(6)
    assert not proxy[1].is_alive() and all(not worker.is_alive() for worker in proxy[2]), proxy[3]
    assert 'error' not in proxy[3] and proxy[3].get('withheld_ciphertext_bytes', 0) > 0, proxy[3]


def execute(case, report, client, controller, name, control_port, service_port,
            service_key, service_public, expiry):
    reader = proxy = None
    paused = False
    report['shutdown_workspace_names'] = ['read', 'next', 'third']
    report['attach_profile'] = 'selector only; daemon startup Store/base/owner/ReadOnly profile retained'
    try:
        if case == 'attach_open':
            report['before_read'] = mounted.mounted_read(name)
            result = attach(client, 2); mounted.refused(result, 13)
            driver.close(client, name, 1); client = controller()
            report['after_mounted_refusal'] = current(client, 1, {}, mounted=True)
            assert mounted.mounted_read(name) == report['before_read']
            assert driver.unmount(client, 2)['outcome'] == 'Unmounted'
            result = attach(client, 3); mounted.refused(result, 13)
            driver.close(client, name, 1); client = controller()
            report['after_open_refusal'] = current(client, 1, {})
            assert driver.close_clean(client, 2)['outcome'] == 'Closed'
        elif case == 'attach_stopping':
            assert driver.unmount(client, 2)['outcome'] == 'Unmounted'
            driver.mount.checked(['docker', 'exec', name, 'python3', '-c',
                "from pathlib import Path; Path('/layerfs/workspace/read/owned-stray').write_bytes(b'owned-test')"])
            result = driver.close_clean(client, 3)
            assert result['outcome'] == 'Retained' and result['code'] == 10, result
            report['retained_close'] = result
            report['during'] = current(client, 4, {}, stopping=True)
            mounted.refused(attach(client, 5), 13)
            driver.close(client, name, 1); client = controller()
            report['after_refusal'] = current(client, 1, {}, stopping=True)
            driver.mount.checked(['docker', 'exec', name, 'python3', '-c',
                "from pathlib import Path; p=Path('/layerfs/workspace/read/owned-stray'); assert p.read_bytes()==b'owned-test'; p.unlink()"])
            assert driver.close_clean(client, 2)['outcome'] == 'Closed'
        elif case == 'attach_unresolved':
            reader = driver.held_file(name)
            result = driver.unmount(client, 2, budget=500)
            assert result['outcome'] == 'Retained' and result['code'] == 11, result
            report['retained_unmount'] = result
            report['during'] = current(client, 3, {}, mounted=True, stopping=True)
            mounted.refused(attach(client, 4), 13)
            driver.close(client, name, 1); client = controller()
            assert current(client, 1, {}, mounted=True, stopping=True)['handles'] == 1
            driver.release(reader); reader = None
            assert driver.unmount(client, 2)['outcome'] == 'Unmounted'
            assert driver.close_clean(client, 3)['outcome'] == 'Closed'
        else:
            report['previous_closed'] = closed_initial(client)
            if case in ('attach_success', 'attach_signal'):
                result = attach(client, 5); assert result['outcome'] == 'Attached', result
                report['result'] = result; report['attached'] = current(client, 6)
                driver.mount.checked(['docker', 'exec', name, 'test', '-d', '/layerfs/workspace/next'])
                report['new_target'] = check_new_read(client, name, 7, close=case != 'attach_signal')
                if case == 'attach_signal':
                    report['signal_target'] = 'next: mounted and unclosed when the shared driver sends SIGTERM'
            elif case == 'attach_authority':
                driver.close(client, name); client = None
                for label, key, selector in [('old-lifecycle-mask-15', 'old_lifecycle', 2),
                        ('Status-only', 'status', 6), ('Unmount-only', 'unmount', 7),
                        ('CloseClean-only', 'close', 8), ('Mount-only', 'mount', 9), ('no-grant', 'none', 4)]:
                    client = controller(key, selector); result = attach(client, 1)
                    mounted.refused(result, 3); driver.close(client, name, 1); client = controller()
                    current(client, 1, {}, stopping=True, closed=True)
                    driver.close(client, name); client = None
                    report.setdefault('grant_refusals', []).append({'case': label, 'result': result})
                client = controller('none', 1)
                try:
                    attach(client, 1)
                except (EOFError, BrokenPipeError) as error:
                    report['authentication_refusal'] = type(error).__name__
                else:
                    raise AssertionError('wrong key authenticated as selector 1')
                try: client.stdin.close()
                except BrokenPipeError: pass
                assert client.wait(timeout=6) == 1
                report['authentication_diagnostic'] = client.stderr.read().decode(errors='replace')
                client = None; driver.status.control_threads(name, 1)
                client = driver.status.controller(service_port, service_key, service_public)
                result = attach(client, 1); mounted.refused(result, 2)
                client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
                report['service_refusal'] = result
                client = controller('attach', 3); result = attach(client, 1)
                assert result['outcome'] == 'Attached', result
                driver.close(client, name); client = None
                for label, operation in [('Status', driver.status.query), ('Mount', driver.remount),
                                         ('Unmount', driver.unmount), ('CloseClean', driver.close_clean)]:
                    client = controller('attach', 3)
                    if label == 'Status': expect_denied_status(client, 1, NEXT)
                    else: mounted.refused(operation(client, 1, **NEXT), 3)
                    driver.close(client, name, 1); client = None
                    report.setdefault('attach_only_refusals', []).append(label)
                for target_label, target in [('old-target', {}),
                        ('wrong-incarnation', {'workspace': b'next', 'incarnation': b'\x73' * 32})]:
                    for label, operation in [('Status', driver.status.query), ('Mount', driver.remount),
                                             ('Unmount', driver.unmount), ('CloseClean', driver.close_clean)]:
                        client = controller()
                        if label == 'Status': expect_denied_status(client, 1, target)
                        else: mounted.refused(operation(client, 1, **target), 3)
                        driver.close(client, name, 1); client = controller()
                        current(client, 1)
                        driver.close(client, name); client = None
                        report.setdefault('stale_target_refusals', []).append(target_label + ':' + label)
                client = controller(); report['after'] = current(client, 1)
                report['new_target'] = check_new_read(client, name, 2)
            elif case == 'attach_loss':
                driver.close(client, name); client = None
                proxy = driver.lost_result_proxy(control_port)
                client = controller(endpoint=proxy[0]); result = attach(client, 1)
                assert result == {'kind': 'failure', 'code': 12, 'unknown': 1, 'cleanup': 0}, result
                client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
                finish_loss(proxy); driver.status.control_threads(name, 1)
                client = controller(); report['after'] = current(client, 1)
                report['unknown'] = result; report['opaque_proxy'] = proxy[3]
                report['new_target'] = check_new_read(client, name, 2)
                report['no_replay'] = 'One Attach only; later Status/read observe current custody and do not reconstruct its lost receipt'
            elif case == 'attach_refusals':
                driver.close(client, name); client = None
                for label, options in [('path-escape', {'workspace': b'../escape'}),
                        ('empty-name', {'workspace': b''}), ('long-name', {'workspace': b'x' * 64}),
                        ('zero-incarnation', {'incarnation': b'\0' * 32})]:
                    client = controller()
                    try: attach(client, 1, **options)
                    except (EOFError, BrokenPipeError): pass
                    else: raise AssertionError('invalid selector entered public Client')
                    client.stdin.close(); assert client.wait(timeout=6) == 1
                    errors = client.stderr.read().decode(errors='replace')
                    expected = 'Capacity' if label == 'long-name' else 'InvalidInput'
                    assert f'Error: {expected} unknown=false' in errors, errors
                    client = None; driver.status.control_threads(name, 1)
                    report.setdefault('invalid_selectors', []).append({'case': label,
                        'boundary': 'public headless decode rejected before native request delivery', 'stderr': errors})
                client = controller(); current(client, 1, {}, stopping=True, closed=True)
                driver.begin(client, 2, opcode=13, budget=500, end=False, **NEXT)
                driver.status.control_threads(name, 2); client.stdin.close()
                result = driver.receive(client, result_tag=15, **NEXT)
                assert result['kind'] == 'failure', result
                assert client.wait(timeout=6) == 1; client = None
                driver.status.control_threads(name, 1); client = controller()
                report['after_partial'] = current(client, 1, {}, stopping=True, closed=True)
                report['partial_result'] = result
                mounted.refused(attach(client, 2, budget=50), 11)
                driver.close(client, name, 1); client = controller()
                report['after_deadline'] = current(client, 1, {}, stopping=True, closed=True)
                driver.mount.checked(['docker', 'exec', name, 'test', '!', '-e', '/layerfs/workspace/next'])
            elif case == 'attach_expiry':
                driver.close(client, name); client = controller('expiry', 5)
                identity = 1
                while time.time() < expiry - 1:
                    current(client, identity, {}, stopping=True, closed=True); identity += 1
                    time.sleep(.1)
                while time.time() < expiry:
                    time.sleep(min(.05, max(0, expiry - time.time())))
                result = attach(client, identity); mounted.refused(result, 3)
                driver.close(client, name, 1); client = controller()
                report['after'] = current(client, 1, {}, stopping=True, closed=True)
                report['expired_result'] = result; report['expiry_unix'] = expiry
            elif case == 'attach_collision':
                driver.mount.checked(['docker', 'exec', name, 'python3', '-c',
                    "from pathlib import Path; p=Path('/layerfs/workspace/next'); p.mkdir(); (p/'foreign').write_bytes(b'unowned')"])
                result = attach(client, 5); mounted.refused(result, 10)
                driver.close(client, name, 1); client = controller()
                report['restored_prior_target'] = current(client, 1, {}, stopping=True, closed=True)
                expect_denied_status(client, 2, NEXT)
                driver.close(client, name, 1); client = controller()
                driver.mount.checked(['docker', 'exec', name, 'python3', '-c',
                    "from pathlib import Path; p=Path('/layerfs/workspace/next/foreign'); assert p.read_bytes()==b'unowned'; p.unlink(); p.parent.rmdir()"])
                report['collision_refusal'] = result
                result = attach(client, 1, **THIRD); assert result['outcome'] == 'Attached', result
                report['new_target'] = check_new_read(client, name, 2, THIRD)
            elif case == 'attach_failed':
                service_pid = report['service_pid']
                process = subprocess.check_output(['ps', '-p', str(service_pid), '-o', 'ppid=,command='], text=True).strip().split(None, 1)
                assert process == [str(os.getpid()), str(driver.route.BIN / 'layerfs-server')], process
                os.kill(service_pid, signal.SIGSTOP); paused = True
                pid, stopped = os.waitpid(service_pid, os.WUNTRACED)
                assert pid == service_pid and os.WIFSTOPPED(stopped)
                result = attach(client, 5)
                assert result['outcome'] == 'Retained' and result['code'] in (10, 11), result
                kind, failed = driver.status.query(client, 6, **NEXT)
                assert kind == 6 and failed['attachment'] == 'Failed' and failed['cause'] == result['code'], failed
                assert failed['cleanup'] == 11 and failed['progress'] == {
                    'state': 'Retained', 'mount_directory': False, 'metadata_arena': False, 'backing_directory': False}, failed
                report['retained_attach'] = result; report['failed_status'] = failed
                report['service_boundary'] = 'Owned service SIGSTOP confirmed before native Attach; timeout retains an empty owner, and explicit CloseClean is local while still paused'
                mounted.refused(driver.remount(client, 7, **NEXT), 13)
                driver.close(client, name, 1); client = controller()
                assert driver.status.query(client, 1, **NEXT) == (kind, failed)
                mounted.refused(attach(client, 2, **THIRD), 13)
                driver.close(client, name, 1); client = controller()
                assert driver.close_clean(client, 1, **NEXT)['outcome'] == 'Closed'
                expect_denied_status(client, 2, NEXT)
                driver.close(client, name, 1); client = None
                os.kill(service_pid, signal.SIGCONT); paused = False
                client = controller(); result = attach(client, 1, **THIRD)
                assert result['outcome'] == 'Attached', result
                report['new_target'] = check_new_read(client, name, 2, THIRD)
            else:
                raise AssertionError(case)
        driver.close(client, name); client = None
    finally:
        if paused: os.kill(report['service_pid'], signal.SIGCONT)
        if reader is not None and reader.poll() is None:
            reader.kill(); reader.wait(timeout=6); report['forced_reader_cleanup'] = True
        if client is not None and client.poll() is None:
            client.kill(); client.wait(timeout=6); report['forced_client_cleanup'] = True
        if proxy:
            proxy[1].join(6)
            for worker in proxy[2]: worker.join(6)


driver.EXTRA_CASE = execute

if __name__ == '__main__':
    driver.main()
