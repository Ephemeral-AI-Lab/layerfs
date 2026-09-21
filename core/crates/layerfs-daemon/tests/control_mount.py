#!/usr/bin/env python3
"""Authenticated Mount of the same CLI-attached Workspace through real Linux FUSE."""
import json
from pathlib import Path
import time

import control_unmount as driver

driver.MODE = 'functional-authenticated-mount'
driver.ENTRY_SOURCE = Path(__file__)
driver.CONTROL_PEERS = [(1, 'good', 15), (2, 'old_lifecycle', 7), (3, 'mount', 8),
                       (4, 'none', 0), (5, 'expiry', 15), (6, 'status', 1),
                       (7, 'unmount', 2), (8, 'close', 4)]
driver.CASES = {
    'mount_success': ['CLI-Attach-read-Unmount-Mount-same-incarnation-metadata-read-Unmount-CloseClean'],
    'mount_authority': ['Mount-bit-target-incarnation-and-service-authority-are-independent'],
    'mount_mounted': ['already-mounted-Mount-refusal-preserves-original-projection'],
    'mount_closed': ['closed-Workspace-Mount-refusal-does-not-attach-or-recreate'],
    'mount_stopping': ['stopping-cleanup-Mount-refusal-does-not-reset-or-adopt-ownership'],
    'mount_unresolved': ['retained-Unmount-owner-blocks-Mount-until-explicit-cleanup'],
    'mount_session_failure': ['entered-Session-failure-retains-owner-until-explicit-Unmount'],
    'mount_loss': ['lost-Mount-terminal-is-Unknown-without-replay-or-reconstructed-receipt'],
    'mount_partial_input': ['incomplete-authenticated-input-never-admits-native-Mount'],
    'mount_deadline': ['exhausted-native-headroom-refuses-Mount-before-admission'],
    'mount_expiry': ['expired-established-peer-cannot-begin-Mount'],
}
driver.NOT_RUN = ['remote Attach and writable daemon startup/edit/Commit controls',
                  'remote Mount binding/worker/post-INIT-deadline failure subsets',
                  'deterministic signal overlap inside actual remote Mount I/O',
                  'namespace/npm/R6', 'hard RSS/cgroup/performance qualification']


def observation(client, identity, mounted=False, stopping=False, closed=False):
    kind, result = driver.status.query(client, identity)
    assert kind == 6 and result['workspace'] == 'read' and result['incarnation'] == '71' * 32, result
    assert (result['mounted'], result['stopping'], result['closed']) == (mounted, stopping, closed), result
    return result


def mounted_read(name):
    code = """import errno,hashlib,json,os,stat
from pathlib import Path
p=Path('/layerfs/workspace/read')
rows=[line for line in Path('/proc/self/mountinfo').read_text().splitlines()
      if line.split()[4]==str(p)]
assert len(rows)==1, rows
filesystem,source=rows[0].split(' - ',1)[1].split()[:2]
assert filesystem in ('fuse','fuse.layerfs') and source=='layerfs', rows
expected=bytes(range(251))*2301
actual=(p/'data.bin').read_bytes()
s=(p/'data.bin').stat()
assert actual==expected and s.st_size==len(expected) and stat.S_IMODE(s.st_mode)==0o644
assert (p/'alias').stat().st_ino==s.st_ino and os.readlink(p/'link')=='data.bin'
try: os.open(p/'data.bin',os.O_WRONLY)
except OSError as error: assert error.errno==errno.EROFS, error
else: raise AssertionError('RO profile admitted write')
print(json.dumps({'inode':s.st_ino,'bytes':s.st_size,'mode':stat.S_IMODE(s.st_mode),
                  'mtime_ns':s.st_mtime_ns,'sha256':hashlib.sha256(actual).hexdigest(),
                  'root_inode':p.stat().st_ino,'readonly':True}))
"""
    return json.loads(driver.mount.checked(['docker', 'exec', name, 'python3', '-c', code], text=True).stdout)


def refused(result, code):
    assert result == {'kind': 'failure', 'code': code, 'unknown': 0, 'cleanup': 0}, result


def execute(case, report, client, controller, name, control_port, service_port, service_key, service_public, expiry):
    reader = proxy = None
    try:
        if case == 'mount_mounted':
            before = mounted_read(name)
            result = driver.remount(client, 2); refused(result, 13)
            driver.close(client, name, 1); client = controller()
            report['after'] = observation(client, 1, mounted=True)
            assert mounted_read(name) == before
            report['refusal'] = result
        elif case == 'mount_unresolved':
            reader = driver.held_file(name)
            retained = driver.unmount(client, 2, budget=500)
            assert retained['outcome'] == 'Retained' and retained['code'] == 11, retained
            report['retained'] = retained
            report['during'] = observation(client, 3, mounted=True, stopping=True)
            result = driver.remount(client, 4); refused(result, 13)
            driver.close(client, name, 1); client = controller()
            assert observation(client, 1, mounted=True, stopping=True)['handles'] == 1
            driver.release(reader); reader = None
            assert driver.unmount(client, 2)['outcome'] == 'Unmounted'
            assert driver.remount(client, 3)['outcome'] == 'Mounted'
            report['read'] = mounted_read(name); report['refusal'] = result
        else:
            if case == 'mount_success':
                report['before_read'] = mounted_read(name)
            assert driver.unmount(client, 2)['outcome'] == 'Unmounted'
            report['unmounted'] = observation(client, 3)
            if case == 'mount_success':
                result = driver.remount(client, 4); assert result['outcome'] == 'Mounted', result
                report['result'] = result; report['mounted'] = observation(client, 5, mounted=True)
                report['after_read'] = mounted_read(name)
                assert report['before_read'] == report['after_read']
                assert driver.unmount(client, 6)['outcome'] == 'Unmounted'
                assert driver.close_clean(client, 7)['outcome'] == 'Closed'
                report['after'] = observation(client, 8, stopping=True, closed=True)
            elif case == 'mount_authority':
                driver.close(client, name); client = None
                for label, key, selector, options in [
                        ('old-lifecycle-mask-7', 'old_lifecycle', 2, {}),
                        ('status-only', 'status', 6, {}), ('unmount-only', 'unmount', 7, {}),
                        ('close-only', 'close', 8, {}), ('no-authority', 'none', 4, {}),
                        ('wrong-target', 'good', 1, {'workspace': b'other'}),
                        ('wrong-incarnation', 'good', 1, {'incarnation': b'\x72' * 32})]:
                    client = controller(key, selector); result = driver.remount(client, 1, **options)
                    refused(result, 3); driver.close(client, name, 1); client = controller()
                    observation(client, 1); driver.close(client, name); client = None
                    report.setdefault('refusals', []).append({'case': label, 'result': result})
                for label, operation in [('Status', driver.status.query), ('Unmount', driver.unmount),
                                         ('CloseClean', driver.close_clean)]:
                    client = controller('mount', 3); result = operation(client, 1)
                    if label == 'Status':
                        assert result == (7, {'code': 3, 'unknown': 0, 'cleanup': 0}), result
                    else:
                        refused(result, 3)
                    driver.close(client, name, 1); client = None
                    report.setdefault('mount_only_refusals', []).append(label)
                client = driver.status.controller(service_port, service_key, service_public)
                result = driver.remount(client, 1); refused(result, 2)
                client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
                report['service_refusal'] = result
                client = controller('mount', 3); result = driver.remount(client, 1)
                assert result['outcome'] == 'Mounted', result
                driver.close(client, name); client = controller()
                report['after'] = observation(client, 1, mounted=True); report['read'] = mounted_read(name)
            elif case == 'mount_closed':
                assert driver.close_clean(client, 4)['outcome'] == 'Closed'
                result = driver.remount(client, 5); refused(result, 13)
                driver.close(client, name, 1); client = controller()
                report['after'] = observation(client, 1, stopping=True, closed=True)
                driver.mount.checked(['docker', 'exec', name, 'test', '!', '-e', '/layerfs/workspace/read'])
                report['refusal'] = result
            elif case == 'mount_stopping':
                driver.mount.checked(['docker', 'exec', name, 'python3', '-c',
                    "from pathlib import Path; Path('/layerfs/workspace/read/owned-stray').write_bytes(b'owned-test')"])
                result = driver.close_clean(client, 4)
                assert result['outcome'] == 'Retained' and result['code'] == 10, result
                report['retained_close'] = result; report['during'] = observation(client, 5, stopping=True)
                result = driver.remount(client, 6); refused(result, 13)
                driver.close(client, name, 1); client = controller()
                report['after'] = observation(client, 1, stopping=True); report['refusal'] = result
                driver.mount.checked(['docker', 'exec', name, 'python3', '-c',
                    "from pathlib import Path; p=Path('/layerfs/workspace/read/owned-stray'); assert p.read_bytes()==b'owned-test'; p.unlink()"])
                assert driver.close_clean(client, 2)['outcome'] == 'Closed'
            elif case == 'mount_session_failure':
                driver.mount.checked(['docker', 'exec', name, 'mv', '/layerfs/workspace/read', '/layerfs/workspace/held-read'])
                result = driver.remount(client, 4)
                assert result['outcome'] == 'Retained' and result['code'] == 10, result
                report['retained'] = result; report['during'] = observation(client, 5, mounted=True, stopping=True)
                result = driver.remount(client, 6); refused(result, 13)
                driver.close(client, name, 1); client = controller()
                report['refusal'] = result; observation(client, 1, mounted=True, stopping=True)
                assert driver.unmount(client, 2)['outcome'] == 'Unmounted'
                report['cleaned'] = observation(client, 3)
                driver.mount.checked(['docker', 'exec', name, 'mv', '/layerfs/workspace/held-read', '/layerfs/workspace/read'])
                assert driver.remount(client, 4)['outcome'] == 'Mounted'
                report['read'] = mounted_read(name)
                report['external_fault'] = 'Renamed only the empty owned mount leaf before Session construction; explicit Unmount checked absence before restoring it'
            elif case == 'mount_loss':
                driver.close(client, name); client = None
                proxy = driver.lost_result_proxy(control_port)
                client = controller(endpoint=proxy[0]); result = driver.remount(client, 1)
                assert result == {'kind': 'failure', 'code': 12, 'unknown': 1, 'cleanup': 0}, result
                client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
                proxy[1].join(6)
                for worker in proxy[2]: worker.join(6)
                assert not proxy[1].is_alive() and 'error' not in proxy[3] and proxy[3].get('withheld_ciphertext_bytes', 0) > 0, proxy[3]
                driver.status.control_threads(name, 1); client = controller()
                report['after'] = observation(client, 1, mounted=True)
                report['unknown'] = result; report['opaque_proxy'] = proxy[3]; report['read'] = mounted_read(name)
                report['no_replay'] = 'Exactly one Mount; later Status and read are current observations, followed by explicit Unmount and CloseClean'
                assert driver.unmount(client, 2)['outcome'] == 'Unmounted'
                assert driver.close_clean(client, 3)['outcome'] == 'Closed'
            elif case == 'mount_partial_input':
                driver.begin(client, 4, opcode=12, budget=500, end=False)
                driver.status.control_threads(name, 2)
                client.stdin.close(); result = driver.receive(client, result_tag=14)
                assert result['kind'] == 'failure', result
                assert client.wait(timeout=6) == 1; client = None
                driver.status.control_threads(name, 1); client = controller()
                report['after'] = observation(client, 1); report['partial_result'] = result
            elif case == 'mount_deadline':
                result = driver.remount(client, 4, budget=50); refused(result, 11)
                driver.close(client, name, 1); client = controller()
                report['after'] = observation(client, 1); report['deadline_result'] = result
            elif case == 'mount_expiry':
                driver.close(client, name); client = controller('expiry', 5)
                observation(client, 1); identity = 2
                while time.time() < expiry - 1:
                    observation(client, identity); identity += 1
                    time.sleep(.1)
                while time.time() < expiry:
                    time.sleep(min(.05, max(0, expiry - time.time())))
                result = driver.remount(client, identity); refused(result, 3)
                driver.close(client, name, 1); client = controller()
                report['after'] = observation(client, 1); report['expired_result'] = result
                report['expiry_unix'] = expiry
            else:
                raise AssertionError(case)
        driver.close(client, name); client = None
    finally:
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
