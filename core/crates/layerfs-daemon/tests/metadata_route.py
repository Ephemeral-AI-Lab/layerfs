#!/usr/bin/env python3
"""R2 metadata update through the production host daemon and authenticated service.

Reuses a closed R1 Store/catalog through independent byte copies. Metadata save,
the separate prepared filesystem update, and unchanged Branch publication are
observed independently. This functional proof makes no performance claim.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import time

import history_route as route
import mounted_read as mount

METADATA_OPCODE = 9
CHECKS = ('metadata-save', 'separate-filesystem-save', 'unchanged-branch',
          'metadata-noop', 'wrong-grant', 'missing-metadata', 'wrong-root-role',
          'invalid-kind', 'invalid-mode', 'invalid-nanoseconds', 'cleanup')


@contextmanager
def check(report, name):
    row = next(row for row in report['checks'] if row['id'] == name)
    row['status'] = 'INCOMPLETE'
    try:
        yield row
    except BaseException as error:
        row.update(status='FAIL', failure=repr(error))
        raise
    else:
        row['status'] = 'PASS'


class Client:
    """Test driver for an unmodified production daemon's framed stdin/stdout."""
    def __init__(self, directory, port, private, public, selector=1):
        self.process, _ = route.start_daemon(directory, port, private, public, selector, None)
        self.identity = 0

    def call(self, opcode, payload, profile=1, response_bytes=0):
        self.identity += 1
        request = struct.pack('>QIHIQB', 1, 1, profile, 5000, response_bytes, opcode) + payload
        self.process.stdin.write(route.frame(2, self.identity, request))
        self.process.stdin.write(route.frame(4, self.identity, struct.pack('>Q', 0)))
        self.process.stdin.flush()
        output = bytearray()
        deadline = time.monotonic() + 6
        while True:
            header = route.read_exact(self.process.stdout.fileno(), 20, deadline)
            assert header[:4] == b'LFB1', header
            kind, flags, reserved, identity, size = struct.unpack('>BBHQI', header[4:])
            assert flags == reserved == 0 and identity == self.identity and size <= 32768, header
            body = route.read_exact(self.process.stdout.fileno(), size, deadline)
            if kind == 5:
                assert size <= route.FRAME_BYTES and len(output) + size <= response_bytes
                output.extend(body)
            else:
                assert kind in (6, 7), kind
                return kind, body, bytes(output)

    def close(self, expected=0):
        try:
            self.process.stdin.close()
        except BrokenPipeError:
            pass
        code = self.process.wait(timeout=6)
        stderr = self.process.stderr.read().decode()
        assert code == expected, (code, stderr)
        return stderr

    def terminate(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=6)


def success(result):
    kind, body, output = result
    assert kind == 6 and not output, (kind, body, len(output))
    return body


def attributes(client, root, path):
    body = success(client.call(2, root + b'\x04' + route.blob(path)))
    reader = route.Reader(body)
    assert reader.u8() == 9, body
    value = {'serial': reader.u64(), 'kind': reader.u8(), 'references': reader.u64(),
             'content': reader.take(32), 'metadata': reader.take(32),
             'mode': struct.unpack('>I', reader.take(4))[0],
             'mtime': struct.unpack('>q', reader.take(8))[0],
             'nanoseconds': struct.unpack('>I', reader.take(4))[0], 'size': reader.u64()}
    reader.done()
    return value


def metadata_payload(base, kind=1, mode=0o600, seconds=-2, nanoseconds=987654321):
    return base + struct.pack('>BIqI', kind, mode, seconds, nanoseconds)


def metadata_result(body):
    reader = route.Reader(body)
    assert reader.u8() == 11 and len(body) == 98, body
    value = {'base': reader.take(32), 'kind': reader.u8(),
             'mode': struct.unpack('>I', reader.take(4))[0],
             'mtime': struct.unpack('>q', reader.take(8))[0],
             'nanoseconds': struct.unpack('>I', reader.take(4))[0],
             'metadata': reader.take(32), 'inserted': reader.u64(), 'reused': reader.u64()}
    reader.done()
    return value


def printable(value):
    return {key: item.hex() if isinstance(item, bytes) else item for key, item in value.items()}


def branch(client, identity):
    body = success(client.call(route.QUERY_OPCODE, b'\x03' + identity, route.HISTORY_PROFILE))
    tag, snapshot = route.history(body)
    assert tag == 'BranchSnapshot' and snapshot['branch']['branch'] == identity
    assert snapshot['root_serial'] is not None
    return body, snapshot


def run(args, report):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['mode'] == 'functional-mounted-proof'
    directory = args.output / 'service'
    directory.mkdir()
    seals = {}
    for name in ('store.sqlite', 'history.sqlite'):
        source, target = args.fixture.parent / 'service' / name, directory / name
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        shutil.copyfile(source, target)
        assert hashlib.sha256(target.read_bytes()).hexdigest() == digest
        seals[name] = digest
    report['fixture_reuse'] = {'receipt': str(args.fixture),
                               'receipt_sha256': hashlib.sha256(args.fixture.read_bytes()).hexdigest(),
                               'clone_method': 'independent-byte-copy',
                               'closed_master_sha256': seals, 'cache_claim': None}
    server_key, full_key, legacy_key = [os.urandom(32).hex() for _ in range(3)]
    server_public, full_public, legacy_public = [route.public_key(key) for key in (server_key, full_key, legacy_key)]
    expiry = int(time.time()) + 3600
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=server_key,
               LAYERFS_PEERS=f'1,{full_public},{expiry},255;2,{legacy_public},{expiry},127',
               LAYERFS_STORE=str(directory / 'store.sqlite'), LAYERFS_LISTEN='127.0.0.1:0',
               LAYERFS_TELEMETRY='off', LAYERFS_HISTORY_CATALOG=str(directory / 'history.sqlite'),
               LAYERFS_HISTORY_BINDING='pair1-mounted-read', LAYERFS_HISTORY_CREATE='0',
               LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([route.BIN / 'layerfs-server'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    client = failed = None
    readiness = ''
    try:
        readiness = mount.line_until(service)
        assert 'ready' in readiness, readiness
        port = int(readiness.strip().rsplit(':', 1)[1])
        client = Client(directory, port, full_key, server_public)
        root, branch_id = bytes.fromhex(fixture['root']), bytes.fromhex(fixture['branch'])
        branch_before, snapshot = branch(client, branch_id)
        assert snapshot['effective_root'] == root
        root_serial = struct.unpack('>Q', snapshot['root_serial'])[0]
        assert root_serial == fixture['root_serial']
        old = attributes(client, root, b'data.bin')
        alias = attributes(client, root, b'alias')
        assert old == alias and old['kind'] == 1 and old['references'] == 2
        with check(report, 'metadata-save') as row:
            saved = metadata_result(success(client.call(METADATA_OPCODE, metadata_payload(old['metadata']))))
            assert saved['base'] == old['metadata'] and saved['kind'] == old['kind']
            assert (saved['mode'], saved['mtime'], saved['nanoseconds']) == (0o600, -2, 987654321)
            assert saved['metadata'] != old['metadata']
            assert attributes(client, root, b'data.bin') == old
            assert branch(client, branch_id)[0] == branch_before
            row.update(result=printable(saved), original_inode=printable(old),
                       boundary='metadata saved; original filesystem and Branch remain unchanged')
        with check(report, 'separate-filesystem-save') as row:
            # Zero changed directory names; one existing inode retains its exact
            # content root, identity and shared hard-link reference count.
            update = (root + snapshot['scope'] + struct.pack('>QHHQB', root_serial, 0, 1, old['serial'], old['kind'])
                      + old['content'] + saved['metadata'])
            reader = route.Reader(success(client.call(5, update)))
            assert reader.u8() == 7
            candidate, inserted, reused = reader.take(32), reader.u64(), reader.u64()
            reader.done()
            changed = attributes(client, candidate, b'data.bin')
            expected = old | {'metadata': saved['metadata'], 'mode': 0o600, 'mtime': -2, 'nanoseconds': 987654321}
            assert changed == expected and attributes(client, candidate, b'alias') == expected
            kind, terminal, content = client.call(1, changed['content'] + struct.pack('>QQ', 0, changed['size']),
                                                   response_bytes=changed['size'])
            assert kind == 6 and terminal == b'\x01' + struct.pack('>Q', len(content))
            assert content == mount.DATA and len(content) == old['size']
            assert attributes(client, root, b'data.bin') == old
            row.update(candidate=candidate.hex(), changed_inode=printable(changed), inserted=inserted, reused=reused,
                       payload_bytes=len(content), payload_sha256=hashlib.sha256(content).hexdigest())
        with check(report, 'unchanged-branch') as row:
            assert branch(client, branch_id)[0] == branch_before
            row.update(branch=branch_id.hex(), effective_root=root.hex(),
                       boundary='exact Branch descriptor unchanged after separate C2 saves; no C5 command submitted')
        with check(report, 'metadata-noop') as row:
            noop = metadata_result(success(client.call(METADATA_OPCODE, metadata_payload(saved['metadata']))))
            assert noop['base'] == noop['metadata'] == saved['metadata'] and noop['inserted'] == 0
            row.update(result=printable(noop))
        report['success_daemon_stderr'] = client.close()
        client = None
        cases = (
            ('wrong-grant', legacy_key, 2, metadata_payload(old['metadata']), 3, False),
            ('missing-metadata', full_key, 1, metadata_payload(b'\xee' * 32), 6, False),
            ('wrong-root-role', full_key, 1, metadata_payload(old['content']), (1, 2), False),
            ('invalid-kind', full_key, 1, metadata_payload(old['metadata'], kind=0), 1, True),
            ('invalid-mode', full_key, 1, metadata_payload(old['metadata'], mode=0o4755), 1, True),
            ('invalid-nanoseconds', full_key, 1, metadata_payload(old['metadata'], nanoseconds=1_000_000_000), 1, True),
        )
        for name, private, selector, payload, expected_code, local_validation in cases:
            with check(report, name) as row:
                failed = Client(directory, port, private, server_public, selector)
                try:
                    kind, body, output = failed.call(METADATA_OPCODE, payload)
                except (EOFError, BrokenPipeError):
                    assert local_validation, 'a valid remote operation lost its failure receipt'
                    stderr = failed.close(1)
                    assert 'InvalidInput' in stderr and 'unknown=true' not in stderr, stderr
                    row.update(route='production daemon shared request validation', stderr=stderr,
                               remote_result='NOT_SUBMITTED')
                else:
                    assert kind == 7 and not output and len(body) == 3, (kind, body, len(output))
                    allowed = expected_code if isinstance(expected_code, tuple) else (expected_code,)
                    assert body[0] in allowed and body[1:] == b'\0\0', body
                    stderr = failed.close(1)
                    row.update(route='authenticated service terminal refusal', failure_code=body[0],
                               unknown=False, cleanup=None, stderr=stderr)
                failed = None
        # All negative attempts preserve the original published view and alias.
        client = Client(directory, port, full_key, server_public)
        assert branch(client, branch_id)[0] == branch_before
        assert attributes(client, root, b'data.bin') == attributes(client, root, b'alias') == old
        client.close(); client = None
    finally:
        if client is not None:
            client.terminate()
            (args.output / 'failed-daemon.stderr').write_bytes(client.process.stderr.read())
        if failed is not None:
            failed.terminate()
            (args.output / 'failed-negative-daemon.stderr').write_bytes(failed.process.stderr.read())
        if service.poll() is None:
            service.stdin.close()
            try:
                service.wait(timeout=6)
            except subprocess.TimeoutExpired:
                service.kill(); service.wait(timeout=6)
                report['service_forced_cleanup'] = True
        (args.output / 'service.stderr').write_bytes(readiness.encode() + service.stderr.read())
    with check(report, 'cleanup') as row:
        assert service.returncode == 0 and not report.get('service_forced_cleanup'), service.returncode
        for name, expected in seals.items():
            assert hashlib.sha256((args.fixture.parent / 'service' / name).read_bytes()).hexdigest() == expected
        row.update(service_exit=service.returncode, fixture_master_unchanged=True,
                   history_clone_unchanged=hashlib.sha256((directory / 'history.sqlite').read_bytes()).hexdigest() == seals['history.sqlite'])
        assert row['history_clone_unchanged']


def main():
    started = time.monotonic()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    args.fixture = args.fixture.resolve(); args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    report = {'status': 'FAIL', 'mode': 'functional-metadata-route', 'performance_claim': False,
              'checks': [{'id': name, 'status': 'NOT_RUN'} for name in CHECKS],
              'not_run': ['mounted metadata mutation', 'writable Workspace backing/snapshot/Commit',
                          'R6 matched performance', 'generic-attribute public transport inspection']}
    def expired(_signum, _frame):
        raise TimeoutError('metadata functional proof exceeded 60 seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        report.update(source=mount.checked(['git', 'rev-parse', 'HEAD'], text=True).stdout.strip(),
                      product_inputs_sha256=mount.product_inputs(),
                      driver_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      binary_sha256={name: hashlib.sha256((route.BIN / name).read_bytes()).hexdigest()
                                     for name in ('layerfs-daemon', 'layerfs-server')})
        run(args, report)
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error)
        raise
    finally:
        signal.alarm(0)
        report['command_wall_seconds'] = time.monotonic() - started
        report['hard_budget_seconds'] = 60
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
