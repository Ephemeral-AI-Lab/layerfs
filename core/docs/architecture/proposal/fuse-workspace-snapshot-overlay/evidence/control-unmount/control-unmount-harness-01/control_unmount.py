#!/usr/bin/env python3
"""One authenticated daemon Unmount selection against an actual Linux RO mount."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
import uuid

import history_route as route
import mounted_read as mount
import control_status as status
sys.path.insert(0, str(route.ROOT / 'core/crates/layerfs-workspace/tests'))
from stage_route import lost_result_proxy
sys.path.insert(0, str(route.ROOT / 'core/benchmark/fs-bench-pro-storage-content/shared'))
import isolation

CASES = {
    'success': ['authenticated-Unmount-retains-Workspace-and-current-Status-without-service-work'],
    'authority': ['operation-grants-target-incarnation-and-service-authority-are-independent'],
    'expiry': ['expired-established-control-peer-cannot-begin-Unmount'],
    'timeout': ['checked-retained-drain-timeout-preserves-owner-for-explicit-continuation'],
    'loss': ['lost-Unmount-terminal-is-Unknown-without-replay-or-invented-receipt'],
    'partial_input': ['incomplete-authenticated-input-never-admits-native-Unmount'],
    'signal': ['concurrent-control-and-signal-shutdown-drain-the-same-mount-owner'],
    'shutdown_retained': ['incomplete-signal-cleanup-retains-process-until-next-explicit-signal'],
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def begin(client, identity, workspace=b'read', incarnation=b'\x71' * 32, budget=1000, end=True):
    body = struct_header(budget) + route.blob(workspace) + incarnation
    client.stdin.write(route.frame(2, identity, body))
    if end:
        client.stdin.write(route.frame(4, identity, (0).to_bytes(8, 'big')))
    client.stdin.flush()


def struct_header(budget):
    import struct
    return struct.pack('>QIHIQB', 0, 0, 3, budget, 0, 10)


def receive(client):
    kind, body = route.receive(client, timeout=6)
    if kind == 7:
        return {'kind': 'failure', 'code': body[0], 'unknown': body[1], 'cleanup': body[2]}
    assert kind == 6, (kind, body)
    value = route.Reader(body)
    assert value.u8() == 12, body
    result = {'kind': 'unmount', 'workspace': value.blob().decode(), 'incarnation': value.take(32).hex()}
    tag = value.u8()
    assert tag in (0, 1), tag
    result['outcome'] = 'Unmounted' if tag == 0 else 'Retained'
    if tag == 1:
        result['code'] = value.u8()
        assert result['code'] in (2, 10, 11, 13)
    value.done()
    assert result['workspace'] == 'read' and result['incarnation'] == '71' * 32
    return result


def unmount(client, identity, **options):
    begin(client, identity, **options)
    return receive(client)


def close(client, name, expected=0):
    status.close_controller(client, name, expected)


def held_file(name):
    reader = subprocess.Popen(['docker', 'exec', '-i', name, 'python3', '-u', '-c',
        "import os,sys; f=os.open('/layerfs/workspace/read/data.bin',os.O_RDONLY); "
        "print('opened',flush=True); sys.stdin.readline(); os.close(f); print('released',flush=True)"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert route.read_exact(reader.stdout.fileno(), 7, time.monotonic() + 8) == b'opened\n'
    return reader


def release(reader):
    output, error = reader.communicate(b'go\n', timeout=6)
    assert reader.returncode == 0 and output == b'released\n', (output, error)


def kernel_stopped(name):
    code = """import errno,os,time
end=time.monotonic()+3
while time.monotonic()<end:
    try:
        fd=os.open('/layerfs/workspace/read/unread.bin',os.O_RDONLY)
        os.close(fd)
    except OSError as error:
        if error.errno==errno.ENODEV:
            print('kernel-admission-stopped',flush=True)
            break
        raise
    time.sleep(.005)
else: raise AssertionError('native Unmount did not stop kernel admission')
"""
    observed = mount.checked(['docker', 'exec', name, 'python3', '-c', code], text=True).stdout.strip()
    assert observed == 'kernel-admission-stopped'
    return observed


def execute(args, report):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['mode'] == 'functional-mounted-proof'
    service_dir = args.output / 'service'; service_dir.mkdir()
    seals = {}
    for filename in ('store.sqlite', 'history.sqlite'):
        source = args.fixture.parent / 'service' / filename
        copied = service_dir / filename
        seals[filename] = sha(source)
        shutil.copyfile(source, copied)
        assert sha(copied) == seals[filename]
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': sha(args.fixture),
        'clone_method': 'independent-byte-copy', 'closed_master_sha256': seals,
        'history': 'read-only reopened fixture; no write authority', 'cache_claim': None}
    keys = {name: os.urandom(32).hex() for name in ('service', 'daemon', 'good', 'status', 'unmount', 'none', 'expiry')}
    public = {name: route.public_key(value) for name, value in keys.items()}
    environment = os.environ.copy()
    environment.update(LAYERFS_PRIVATE_KEY=keys['service'],
        LAYERFS_PEERS=f'1,{public["daemon"]},{int(time.time())+3600},255',
        LAYERFS_STORE=str(service_dir / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0', LAYERFS_TELEMETRY='off',
        LAYERFS_HISTORY_CATALOG=str(service_dir / 'history.sqlite'), LAYERFS_HISTORY_BINDING='pair1-mounted-read',
        LAYERFS_HISTORY_CREATE='0', LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([route.BIN / 'layerfs-service'], env=environment, stdin=subprocess.PIPE,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    name = 'layerfs-unmount-' + uuid.uuid4().hex[:12]
    daemon = client = reader = None
    proxy = None
    made_volume = made_container = paused = False
    ready_lines = ''
    try:
        ready = mount.line_until(service); assert 'ready' in ready, ready
        port = int(ready.strip().rsplit(':', 1)[1])
        mount.checked(['docker', 'volume', 'create', name + '-root']); made_volume = True
        expiry = int(time.time()) + 10
        peers = ';'.join(f'{selector},{public[key]},{expiry if key=="expiry" else int(time.time())+3600},{bits}'
            for selector, key, bits in [(1,'good',3),(2,'status',1),(3,'unmount',2),(4,'none',0),(5,'expiry',3)])
        environment = os.environ.copy()
        environment.update(LAYERFS_ENDPOINT=f'host.docker.internal:{port}', LAYERFS_SELECTOR='1',
            LAYERFS_PRIVATE_KEY=keys['daemon'], LAYERFS_SERVER_KEY=public['service'], LAYERFS_TELEMETRY='off',
            LAYERFS_WORKSPACE_ROOT='/layerfs', LAYERFS_WORKSPACE_MAX_COUNT='2',
            LAYERFS_CONTROL_LISTEN='0.0.0.0:23456', LAYERFS_CONTROL_PEERS=peers, LAYERFS_CONSTRUCTION_WORKERS='1')
        daemon = mount.mount_process(args, environment, fixture['root'], name); made_container = True
        first, second = mount.line_until(daemon), mount.line_until(daemon)
        ready_lines = first + second
        assert 'workspace control ready' in first and 'workspace ready' in second, ready_lines
        report['kernel'] = mount.checked(['docker','exec',name,'uname','-srmo'],text=True).stdout.strip()
        control_port = int(mount.checked(['docker','port',name,'23456/tcp'],text=True).stdout.strip().rsplit(':',1)[1])
        def controller(key='good', selector=1, endpoint=control_port):
            return status.controller(endpoint, keys[key], public['daemon'], selector)
        def current(connection, identity, mounted=True, stopping=False):
            kind, result = status.query(connection, identity)
            assert kind == 6 and result['mounted'] == mounted and result['stopping'] == stopping and not result['closed'], result
            return result
        client = controller()
        initial = current(client, 1); assert 0 < initial['consumer_accounted_bytes'] <= 8388608
        report['initial'] = initial
        if args.case == 'success':
            service.send_signal(signal.SIGSTOP); paused = True
            pid, stopped = os.waitpid(service.pid, os.WUNTRACED); assert pid == service.pid and os.WIFSTOPPED(stopped)
            result = unmount(client, 2); assert result['outcome'] == 'Unmounted', result
            observation = current(client, 3, mounted=False)
            assert daemon.poll() is None and observation['nodes'] > 0 and observation['consumer_accounted_bytes'] > 0
            mount.checked(['docker','exec',name,'test','-d','/layerfs/workspace/read'])
            report['result'] = result; report['after'] = observation
            report['service_boundary'] = 'SIGSTOP confirmed before Unmount; local result and Status complete before SIGCONT'
            service.send_signal(signal.SIGCONT); paused = False
        elif args.case == 'authority':
            close(client, name); client = None
            for label, key, selector, options in [
                ('status-only','status',2,{}), ('no-authority','none',4,{}),
                ('wrong-target','good',1,{'workspace':b'other'}),
                ('wrong-incarnation','good',1,{'incarnation':b'\x72'*32})]:
                client = controller(key, selector); result = unmount(client, 1, **options)
                assert result == {'kind':'failure','code':3,'unknown':0,'cleanup':0}, result
                close(client, name, 1); client = controller(); current(client, 1); close(client, name); client = None
                report.setdefault('refusals', []).append({'case':label,'result':result})
            client = controller('unmount', 3); kind, refusal = status.query(client, 1)
            assert kind == 7 and refusal['code'] == 3 and not refusal['unknown'], refusal
            close(client, name, 1); client = None
            service_client = status.controller(port, keys['daemon'], public['service'])
            result = unmount(service_client, 1)
            assert result == {'kind':'failure','code':2,'unknown':0,'cleanup':0}, result
            service_client.stdin.close(); assert service_client.wait(timeout=6) == 1
            report['service_refusal'] = result
            client = controller('unmount', 3); result = unmount(client, 1); assert result['outcome'] == 'Unmounted', result
            close(client, name); client = controller(); report['after'] = current(client, 1, mounted=False)
        elif args.case == 'expiry':
            close(client, name); client = controller('expiry', 5); current(client, 1)
            identity = 2
            while time.time() < expiry - 1:
                current(client, identity); identity += 1
                time.sleep(.1)
            while time.time() < expiry:
                time.sleep(min(.05, expiry - time.time()))
            result = unmount(client, identity)
            assert result == {'kind':'failure','code':3,'unknown':0,'cleanup':0}, result
            close(client, name, 1); client = controller(); report['after'] = current(client, 1)
            report['expired_result'] = result; report['expiry_unix'] = expiry
        elif args.case == 'timeout':
            reader = held_file(name)
            result = unmount(client, 2, budget=500)
            assert result['outcome'] == 'Retained' and result['code'] == 11, result
            report['retained'] = result; report['during'] = current(client, 3, stopping=True)
            assert report['during']['handles'] == 1
            release(reader); reader = None
            result = unmount(client, 4); assert result['outcome'] == 'Unmounted', result
            report['explicit_continuation'] = result; report['after'] = current(client, 5, mounted=False)
        elif args.case == 'loss':
            close(client, name); client = None
            proxy = lost_result_proxy(control_port)
            client = controller(endpoint=proxy[0]); result = unmount(client, 1)
            assert result == {'kind':'failure','code':12,'unknown':1,'cleanup':0}, result
            client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
            proxy[1].join(6)
            for worker in proxy[2]: worker.join(6)
            assert not proxy[1].is_alive() and 'error' not in proxy[3] and proxy[3].get('withheld_ciphertext_bytes',0)>0, proxy[3]
            status.control_threads(name, 1)
            client = controller(); observation = current(client, 1, mounted=False)
            report['unknown'] = result; report['opaque_proxy'] = proxy[3]
            report['after'] = observation; report['no_replay'] = 'Only one Unmount request submitted; subsequent request is Status, not a reconstructed receipt'
        elif args.case == 'partial_input':
            begin(client, 2, budget=500, end=False)
            status.control_threads(name, 2)
            mount.checked(['docker','exec',name,'python3','-c',
                "from pathlib import Path; assert Path('/layerfs/workspace/read/data.bin').read_bytes()[:4]==bytes(range(4))"])
            client.stdin.close()
            result = receive(client); assert result['kind'] == 'failure', result
            assert client.wait(timeout=6) == 1; client = None
            status.control_threads(name, 1); client = controller(); report['after'] = current(client, 1)
            report['partial_result'] = result
        elif args.case == 'signal':
            reader = held_file(name); begin(client, 2, budget=5000)
            report['admission_boundary'] = kernel_stopped(name)
            mount.stop_mount(name)
            result = receive(client); assert result['kind'] == 'failure' and result['unknown'], result
            client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
            report['socket_boundary'] = 'Client receives Unknown while actual projection FD is still held'
            release(reader); reader = None
            assert daemon.wait(timeout=12) == 0
            report['interrupted_client'] = result
        elif args.case == 'shutdown_retained':
            reader = held_file(name); close(client, name); client = None
            mount.stop_mount(name)
            retained = mount.line_until(daemon, timeout=12)
            assert 'workspace shutdown retained:' in retained and 'Deadline' in retained, retained
            ready_lines += retained
            assert daemon.poll() is None
            report['retained_shutdown'] = retained.strip()
            release(reader); reader = None
            mount.stop_mount(name); assert daemon.wait(timeout=12) == 0
        else:
            raise AssertionError(args.case)
        if client is not None:
            close(client, name); client = None
        if daemon.poll() is None:
            mount.stop_mount(name); assert daemon.wait(timeout=12) == 0
        errors = daemon.stderr.read(); (args.output/'daemon.stderr').write_bytes(ready_lines.encode()+errors)
        assert b'workspace closed' in errors, errors
        mount.checked(['docker','exec',name,'test','!','-e','/layerfs/workspace/read'])
        report['checks'] = [{'id':label,'status':'PASS'} for label in CASES[args.case]]
        report['cleanup'] = 'PASS: normal daemon shutdown checked closed Workspace; owned runtime and volume removed'
    finally:
        if paused: service.send_signal(signal.SIGCONT)
        if client is not None and client.poll() is None:
            client.kill(); client.wait(timeout=6); report['forced_client_cleanup'] = True
        if reader is not None and reader.poll() is None:
            reader.kill(); reader.wait(timeout=6); report['forced_reader_cleanup'] = True
        if proxy:
            proxy[1].join(6)
            for worker in proxy[2]: worker.join(6)
        if made_container:
            if daemon is not None and daemon.poll() is None: report['forced_daemon_cleanup'] = True
            mount.checked(['docker','rm','-f',name])
        if daemon is not None:
            daemon.wait(timeout=10)
            if not (args.output/'daemon.stderr').exists():
                (args.output/'failed-daemon.stderr').write_bytes(ready_lines.encode()+daemon.stderr.read())
        if service.poll() is None:
            service.stdin.close(); service.wait(timeout=10)
        (args.output/'service.stderr').write_bytes(service.stderr.read())
        if made_volume: mount.checked(['docker','volume','rm',name+'-root'])


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--fixture',type=Path,required=True)
    parser.add_argument('--binaries',type=Path,required=True)
    parser.add_argument('--linux-daemon',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--case',choices=CASES,required=True)
    parser.add_argument('--image',default='rust:1.85.1-bookworm')
    args=parser.parse_args()
    for field in ('fixture','binaries','linux_daemon','output'):setattr(args,field,getattr(args,field).resolve())
    route.BIN=args.binaries
    args.output.mkdir(parents=True,exist_ok=False)
    started=time.monotonic()
    def expired(_signal,_frame):raise TimeoutError('complete Unmount functional selection exceeded60 seconds')
    signal.signal(signal.SIGALRM,expired);signal.alarm(60)
    report={'status':'FAIL','mode':'functional-authenticated-unmount','case':args.case,'checks':[],
        'source':mount.checked(['git','rev-parse','HEAD'],text=True).stdout.strip(),
        'product_inputs_sha256':mount.product_inputs(),'hard_budget_seconds':60,'unmount_wire_max_ms':5000,
        'terminal_reserve_ms':100,'signal_attempt_seconds':10,'performance_claim':False,'cache_claim':None,
        'driver_sha256':sha(Path(__file__)),'status_driver_sha256':sha(Path(status.__file__)),
        'mount_driver_sha256':sha(Path(mount.__file__)),'linux_binary_sha256':sha(args.linux_daemon),
        'host_binary_sha256':{name:sha(route.BIN/name) for name in ('layerfs-service','layerfs-daemon','examples/public_key')},
        'not_run':['writable daemon startup and SDK edit/Commit controls','Attach/CloseClean controls','namespace/npm/R6','hard RSS/cgroup qualification']}
    try:
        space=isolation.namespace()
        for path in (args.fixture,args.binaries,args.linux_daemon,args.output):space.assert_owned(path,'Unmount proof input/output')
        work=isolation.concurrent_work()
        for item in work:item['command']=re.sub(r'(?i)([a-z_]*(?:key|secret|token|password)[a-z_]*=)[^\s]+',r'\1<redacted>',item['command'])
        report['resource_isolation']=space.as_fields()|{'artifact_root':str(args.output.parent),'run_output':str(args.output),
            'build_target':str(route.ROOT/'core/target'),'linux_build_target':str(route.ROOT/'core/target-linux'),
            'concurrent_work':work,'observation':'snapshot before functional driver; no quiet-host/performance claim'}
        report['runtime_image_id']=mount.checked(['docker','image','inspect','--format','{{.Id}}',args.image],text=True).stdout.strip()
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            execute(args,report)
        report['status']='PASS'
    except BaseException as error:
        report['failure']=repr(error)
        raise
    finally:
        signal.alarm(0);report['command_wall_seconds']=time.monotonic()-started
        (args.output/'result.json').write_text(json.dumps(report,indent=2)+'\n')

if __name__=='__main__':main()
