#!/usr/bin/env python3
"""R1 functional proof: Linux kernel -> production daemon -> authenticated host service.

No performance sample, SDK-control claim, Docker-owned Store or private source
include. Requires prebuilt host binaries/examples and Linux daemon, Rust 1.85.1.
Every invocation requires a fresh output directory and retains its failure log.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import struct
import subprocess
import time

import history_route as route

DATA = bytes(range(251)) * 2301
NAMES = [f"entry-{i:03d}-" + "x" * 180 for i in range(100)]


def checked(command, **kwargs):
    return subprocess.run(command, check=True, capture_output=True, timeout=30, **kwargs)


def product_inputs():
    """Include new product files even before their first Git commit."""
    paths = [route.ROOT / '.cargo/config.toml', route.ROOT / 'core/Cargo.toml',
             route.ROOT / 'core/Cargo.lock']
    for manifest in (route.ROOT / 'core/crates').rglob('Cargo.toml'):
        package = manifest.parent
        paths.append(manifest)
        for group in ('src', 'sql'):
            paths.extend(path for path in (package / group).rglob('*') if path.is_file())
    digest = hashlib.sha256()
    for path in sorted(paths):
        digest.update(str(path.relative_to(route.ROOT)).encode() + b'\0')
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def line_until(process, timeout=15):
    end, data = time.monotonic() + timeout, bytearray()
    while time.monotonic() < end:
        if not select.select([process.stderr], [], [], max(0, end - time.monotonic()))[0]:
            break
        part = os.read(process.stderr.fileno(), 1)
        if not part:
            break
        data.extend(part)
        if part == b"\n":
            return data.decode()
    raise RuntimeError(f"missing readiness: {data.decode(errors='replace')}")


def seed_namespace(daemon, executable):
    roots = []
    for identity, payload in enumerate((DATA, executable), 1):
        kind, body = route.exchange(daemon, identity, 20, route.save_file_metadata(len(payload)), body=route.save_file_body(payload))
        assert kind == 6 and body[0] == 2, body
        roots.append(body[1:33])
    # Consume serial 1 through ordinary initialization before the tested root.
    primer = (b"\x01" + b"\x50" * 16 + route.blob(b"primer") + bytes(range(32))
              + struct.pack(">H", 1) + route.manifest_entry(0, b"", 2, 0o755, 0, 0))
    kind, body = route.exchange(daemon, 3, route.COMMAND_OPCODE, primer, route.HISTORY_PROFILE)
    assert kind == 6, body
    entries = [route.manifest_entry(0, b"", 2, 0o755, 1700000000, 0)]
    for name in ("data.bin", "unread.bin", *NAMES):
        entries.append(route.manifest_entry(0, name.encode(), 1, 0o644,
                                           (1 << 64) - 1, 123456789, roots[0]))
    entries += [route.manifest_entry(0, b"tool", 1, 0o755, 1700000001, 0, roots[1]),
                route.manifest_entry(0, b"root-readable", 1, 0, 1700000001, 0, roots[0]),
                route.manifest_entry(0, b"link", 3, 0o777, 1700000002, 0, target=b"data.bin"),
                route.manifest_entry(0, b"dangling", 3, 0o777, 1700000003, 0, target=b"absent"),
                route.manifest_entry(0, b"empty", 2, 0o755, 1700000004, 0)]
    payload = (b"\x01" + b"\x51" * 16 + route.blob(b"mounted") + bytes(range(32))
               + struct.pack(">H", len(entries)) + b"".join(entries))
    kind, body = route.exchange(daemon, 4, route.COMMAND_OPCODE, payload, route.HISTORY_PROFILE)
    assert kind == 6, body
    tag, created = route.history(body)
    assert tag == "StackCreated"
    fork = (b"\x02" + created["stack"] + b"\x61" * 16 + route.blob(b"read")
            + b"\x01" + created["head_layer"])
    kind, body = route.exchange(daemon, 5, route.COMMAND_OPCODE, fork, route.HISTORY_PROFILE)
    assert kind == 6, body
    tag, snapshot = route.history(body)
    assert tag == "BranchSnapshot" and snapshot["root_serial"] is None
    branch = snapshot["branch"]["branch"]
    kind, body = route.exchange(daemon, 6, 2, created["root"] + b"\x01" + route.blob(b"data.bin"))
    assert kind == 6 and body[0] == 4, body
    serial = struct.unpack(">Q", body[1:9])[0]
    prepared = (b"\x05" + b"\x72" * 32 + branch + route.optional(None) + created["head_layer"]
                + struct.pack(">Q", 1) + created["root"] + snapshot["scope"]
                + struct.pack(">Q", created["root_serial"]) + struct.pack(">H", 1)
                + struct.pack(">QH", created["root_serial"], 1)
                + route.blob(b"alias") + struct.pack(">QH", serial, 0))
    kind, body = route.exchange(daemon, 7, route.COMMAND_OPCODE, prepared, route.HISTORY_PROFILE)
    assert kind == 6, body
    tag, committed = route.history(body)
    assert tag == "Committed" and created["root_serial"] != 1
    return committed["root"], branch, roots[0], created["root_serial"]




def mount_process(args, env, source, name, store=1, mode='--mount-readonly'):
    assert mode in ('--mount-readonly', '--mount-writable')
    command = ['docker', 'run', '-d', '--rm', '--name', name, '--cpus', '2', '--device', '/dev/fuse',
               '--cap-add', 'SYS_ADMIN', '--security-opt', 'apparmor=unconfined',
               '--add-host', 'host.docker.internal:host-gateway',
               '--mount', f'type=bind,src={args.linux_daemon},dst=/product/layerfs-daemon,readonly',
               '--mount', f'type=volume,src={name}-root,dst=/layerfs']
    if 'LAYERFS_CONTROL_LISTEN' in env:
        command += ['--publish', '127.0.0.1::23456']
    for key in env:
        if key.startswith('LAYERFS_'):
            command += ['-e', key]
    # Keep the executor alive while the daemon exits, so Docker cannot kill the
    # syscall oracle before it closes its FD and reports the shutdown result.
    command += [args.image, 'sleep', 'infinity']
    checked(command, env=env)
    launch = ['docker','exec','-i',name,'sh','-c',
              'echo $$ > /tmp/layerfs-daemon.pid; exec "$@"','sh',
              '/product/layerfs-daemon',mode,'read',
              '71' * 32,str(store),source,'0','0']
    return subprocess.Popen(launch, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def stop_mount(name):
    checked(['docker','exec',name,'python3','-c',
             "import os,signal; from pathlib import Path; "
             "os.kill(int(Path('/tmp/layerfs-daemon.pid').read_text()),signal.SIGTERM)"])


def wait_service_connection(name, port):
    probe = ("from pathlib import Path\nimport time\nend=time.monotonic()+3\n"
             "while time.monotonic()<end:\n"
             " rows=(Path('/proc/net/tcp').read_text()+Path('/proc/net/tcp6').read_text()).splitlines()\n"
             f" if any(len(r.split())>3 and r.split()[3]=='01' and r.split()[2].endswith(':{port:04X}') for r in rows): break\n"
             " time.sleep(.005)\nelse: raise AssertionError('no in-flight service connection')\n")
    checked(['docker','exec',name,'python3','-c',probe])


def delayed_unmount(service, name, port):
    """Causal external schedule: real opened FD, paused owned service, live TCP call."""
    code = ("import os,sys,errno,json\nf=os.open('/layerfs/workspace/read/unread.bin',os.O_RDONLY)\n"
            "print('opened',flush=True)\nsys.stdin.readline()\n"
            "try:\n os.read(f,19)\n"
            "except OSError as e:\n assert e.errno in (errno.EIO,errno.ETIMEDOUT,errno.ENODEV),e\n print(json.dumps({'read_errno':e.errno}),flush=True)\n"
            "else:\n raise AssertionError('paused service supplied bytes')\n"
            "finally:\n os.close(f)\n")
    reader = subprocess.Popen(['docker','exec','-i',name,'python3','-u','-c',code],
                              stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    try:
        assert route.read_exact(reader.stdout.fileno(), 7, time.monotonic()+10) == b'opened\n'
        service.send_signal(signal.SIGSTOP)
        reader.stdin.write(b'go\n'); reader.stdin.flush()
        wait_service_connection(name, port)
        stop_mount(name)
        stdout, stderr = reader.communicate(timeout=15)
        assert reader.returncode == 0, (stdout, stderr)
        return json.loads(stdout)
    finally:
        service.send_signal(signal.SIGCONT)
        if reader.poll() is None:
            reader.kill(); reader.wait(timeout=10)


def run(args, report):
    temp = args.output / 'service'
    temp.mkdir()
    route.provision_store(temp / 'store.sqlite')
    server_key, client_key, denied_key = (os.urandom(32).hex() for _ in range(3))
    server_public, client_public = route.public_key(server_key), route.public_key(client_key)
    # Socket port 0 belongs to production listener; readiness supplies actual port.
    env = os.environ.copy()
    peers = (f'1,{client_public},{int(time.time()) + 3600},127;'
             f'2,{route.public_key(denied_key)},{int(time.time()) + 3600},1')
    env.update(LAYERFS_PRIVATE_KEY=server_key, LAYERFS_PEERS=peers,
               LAYERFS_STORE=str(temp / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0',
               LAYERFS_TELEMETRY='off', LAYERFS_HISTORY_CATALOG=str(temp / 'history.sqlite'),
               LAYERFS_HISTORY_BINDING='pair1-mounted-read', LAYERFS_HISTORY_CREATE='1',
               LAYERFS_HISTORY_INCARNATION='1', LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(),
               LAYERFS_CONSTRUCTION_WORKERS='1')
    service_log = (args.output / 'service.stderr').open('w')
    service = subprocess.Popen([route.BIN / 'layerfs-server'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    daemon = mounted = None
    name = f'layerfs-pair1-read-{os.getpid()}'
    checked(['docker', 'volume', 'create', name + '-root'])
    try:
        ready = line_until(service); service_log.write(ready); service_log.flush()
        assert 'ready' in ready, ready
        port = int(ready.strip().rsplit(':', 1)[1])
        daemon, _ = route.start_daemon(temp, port, client_key, server_public, 1, None)
        executable = checked(['docker', 'run', '--rm', '--network', 'none', args.image,
                              'cat', '/usr/bin/true']).stdout
        assert len(executable) > 8192
        root, branch, file_root, root_serial = seed_namespace(daemon, executable)
        daemon.stdin.close(); assert daemon.wait(timeout=10) == 0
        daemon = None
        report.update(root=root.hex(), branch=branch.hex(), root_serial=root_serial,
                      executable_sha256=hashlib.sha256(executable).hexdigest(),
                      linux_binary_sha256=hashlib.sha256(args.linux_daemon.read_bytes()).hexdigest(),
                      host_binary_sha256=hashlib.sha256((route.BIN / 'layerfs-server').read_bytes()).hexdigest())
        mount_env = os.environ.copy()
        mount_env.update(LAYERFS_ENDPOINT=f'host.docker.internal:{port}', LAYERFS_SELECTOR='1',
                         LAYERFS_PRIVATE_KEY=client_key, LAYERFS_SERVER_KEY=server_public,
                         LAYERFS_TELEMETRY='off', LAYERFS_WORKSPACE_ROOT='/layerfs',
                         LAYERFS_WORKSPACE_MAX_COUNT='2', LAYERFS_CONSTRUCTION_WORKERS='1')
        for label, source, store, denied in [
                ('wrong-root-role', file_root.hex(), 1, False),
                ('missing-root', 'a3' * 32, 1, False),
                ('unauthorized-store', root.hex(), 2, False),
                ('unauthorized-inspect', root.hex(), 1, True)]:
            refusal_env = mount_env.copy()
            if denied:
                refusal_env.update(LAYERFS_PRIVATE_KEY=denied_key, LAYERFS_SELECTOR='2')
            mounted = mount_process(args, refusal_env, source, name, store)
            stdout, stderr = mounted.communicate(timeout=15)
            (args.output / f'{label}.stderr').write_bytes(stderr)
            assert mounted.returncode != 0 and b'workspace ready' not in stderr, stderr
            assert not stdout
            report['checks'].append({'id':'R-11' if denied else 'R-03',
                                      'route':label,'status':'PASS','error':stderr.decode()})
            mounted = None
            checked(['docker','rm','-f',name])
        for label, source in [('explicit-root', root.hex()), ('delayed-unmount', root.hex()),
                              ('branch', 'branch:' + branch.hex())]:
            mounted = mount_process(args, mount_env, source, name)
            readiness = line_until(mounted)
            assert 'workspace ready' in readiness, readiness
            if label == 'delayed-unmount':
                delayed_result = delayed_unmount(service, name, port)
            else:
                result = checked(['docker', 'exec', '-i', name, 'python3'],
                                 input=(Path(__file__).parents[2] / 'layerfs-fuse/tests/check_mount.py').read_bytes())
                (args.output / f'{label}.stdout').write_bytes(result.stdout)
                (args.output / f'{label}.stderr').write_bytes(result.stderr)
                report['checks'].append({'id':'R-01' if label == 'explicit-root' else 'R-02',
                                          'route':label, 'status':'PASS',
                                          'also_covers':['R-04','R-05','R-06','R-07','R-08','R-09','R-10'],
                                          'result':json.loads(result.stdout)})
            if label == 'branch':
                # unread.bin has attributes but no kernel payload pages.
                service.stdin.close(); assert service.wait(timeout=10) == 0
                result = checked(['docker', 'exec', name, 'python3', '-c',
                    "import errno,os; p='/layerfs/workspace/read/unread.bin'\n"
                    "try:\n f=os.open(p,os.O_RDONLY); os.read(f,19)\n"
                    "except OSError as e:\n assert e.errno in (errno.EIO,errno.ETIMEDOUT),e\n"
                    "else:\n raise AssertionError('peer-loss read succeeded')\n"])
                report['checks'].append({'id':'R1-peer-loss', 'route':'service-loss', 'status':'PASS'})
            if label != 'delayed-unmount':
                stop_mount(name)
            code = mounted.wait(timeout=15)
            stderr = mounted.stderr.read().decode()
            (args.output / f'{label}.lifecycle').write_text(readiness + stderr)
            assert code == 0 and 'workspace closed' in stderr, (code, stderr)
            if label == 'delayed-unmount':
                report['checks'].append({'id':'R-12','route':label,'status':'PASS',
                                          'result':delayed_result,
                                          'boundary':'opened FD, SIGSTOP service, established callback TCP, SIGTERM daemon, drained and closed'})
            mounted = None
            checked(['docker', 'exec', name, 'sh', '-c',
                     'test ! -e /layerfs/workspace/read && test ! -e /layerfs/private-backing'])
            checked(['docker','rm','-f',name])
        report['checks'].append({'id':'R1-clean-close','route':'unmount-and-clean-close','status':'PASS'})
    finally:
        if mounted is not None:
            subprocess.run(['docker', 'rm', '-f', name], capture_output=True, timeout=15)
            mounted.wait(timeout=15)
            (args.output / 'failed-mount.stderr').write_bytes(mounted.stderr.read())
        else:
            subprocess.run(['docker', 'rm', '-f', name], capture_output=True, timeout=15)
        if daemon is not None:
            daemon.kill(); daemon.wait(timeout=10)
        if service.poll() is None:
            service.stdin.close()
            try: service.wait(timeout=10)
            except subprocess.TimeoutExpired:
                service.kill(); service.wait(timeout=10)
        service_log.write(service.stderr.read().decode()); service_log.close()
        checked(['docker', 'volume', 'rm', name + '-root'])


def main():
    started = time.monotonic()
    parser = argparse.ArgumentParser()
    parser.add_argument('--linux-daemon', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--image', default='rust:1.85.1-bookworm')
    args = parser.parse_args()
    args.linux_daemon = args.linux_daemon.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    def hard_deadline(_signal, _frame):
        raise TimeoutError('complete mounted functional proof exceeded 60 seconds')
    signal.signal(signal.SIGALRM, hard_deadline)
    signal.alarm(60)
    report = {'status':'FAIL', 'mode':'functional-mounted-proof', 'performance_claim':False,
              'checks':[], 'source':checked(['git','rev-parse','HEAD']).stdout.decode().strip(),
              'dirty_diff_sha256':hashlib.sha256(checked(['git','diff','HEAD']).stdout).hexdigest(),
              'product_inputs_sha256':product_inputs(),
              'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'oracle_sha256':hashlib.sha256((Path(__file__).parents[2] / 'layerfs-fuse/tests/check_mount.py').read_bytes()).hexdigest(),
              'helper_sha256':hashlib.sha256(Path(route.__file__).read_bytes()).hexdigest(),
              'runtime_image':checked(['docker','image','inspect','--format','{{.Id}}',args.image]).stdout.decode().strip(),
              'limits':{'callback_seconds':10, 'working_bytes':8388608, 'max_workspaces':2},
              'not_run':['R1-C authenticated daemon control','W/snapshot/Commit/npm',
                         'R6 matched performance','hard RSS/cgroup bound']}
    try:
        run(args, report)
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error)
        if isinstance(error, subprocess.CalledProcessError):
            report['failure_stdout'] = error.stdout.decode(errors='replace')
            report['failure_stderr'] = error.stderr.decode(errors='replace')
        raise
    finally:
        signal.alarm(0)
        report['command_wall_seconds'] = time.monotonic() - started
        report['hard_budget_seconds'] = 60
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')

if __name__ == '__main__':
    main()
