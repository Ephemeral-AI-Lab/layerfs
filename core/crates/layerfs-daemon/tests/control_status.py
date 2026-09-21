#!/usr/bin/env python3
"""R1-C Status: authenticated host Client -> container daemon -> mounted Workspace.

Reuses a closed successful R1 fixture through independent byte copies. No mount
or input preparation is attributed to a Status call; this is functional proof,
not measurement. No status is routed through the content service.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import struct
import subprocess
import time

import history_route as route
import mounted_read as mount
import control_commit_wire as commit_wire


def query(process, identity, workspace=b'read', incarnation=b'\x71' * 32):
    payload = struct.pack('>QIHIQB', 0, 0, 3, 5000, 0, 8) + route.blob(workspace) + incarnation
    process.stdin.write(route.frame(2, identity, payload))
    process.stdin.write(route.frame(4, identity, struct.pack('>Q', 0)))
    process.stdin.flush()
    kind, body = route.receive(process, timeout=6)
    if kind == 7:
        return kind, {'code':body[0], 'unknown':body[1], 'cleanup':body[2]}
    reader = route.Reader(body)
    tag = reader.u8()
    assert tag in (10, 16, 18), body
    result = {'workspace':reader.blob().decode(), 'incarnation':reader.take(32).hex()}
    if tag == 16:
        state = reader.u8()
        assert state in (0, 1), state
        result['attachment'] = 'Attaching' if state == 0 else 'Failed'
        if state == 1:
            result.update(cause=reader.u8(), cleanup=reader.u8())
            progress = reader.u8()
            assert progress in (0, 1), progress
            result['progress'] = {'state': 'Running' if progress == 0 else 'Retained'}
            if progress == 1:
                flags = reader.u8()
                assert flags & ~7 == 0, flags
                result['progress'].update(mount_directory=bool(flags & 1),
                                         metadata_arena=bool(flags & 2), backing_directory=bool(flags & 4))
        reader.done()
        return kind, result
    flags = reader.u8()
    assert flags & ~7 == 0
    result.update(mounted=bool(flags & 1), stopping=bool(flags & 2), closed=bool(flags & 4))
    for field in ('active_operations','nodes','handles','cookies','consumer_accounted_bytes'):
        result[field] = reader.u64()
    if tag == 18:
        result.update(commit_wire.writable_fields(reader))
    reader.done()
    return kind, result


def controller(port, private, daemon_public, selector=1):
    env = os.environ.copy()
    env.update(LAYERFS_ENDPOINT=f'127.0.0.1:{port}', LAYERFS_SELECTOR=str(selector),
               LAYERFS_PRIVATE_KEY=private, LAYERFS_SERVER_KEY=daemon_public,
               LAYERFS_TELEMETRY='off')
    # This binary uses the existing public Client::call_until and Source machinery.
    return subprocess.Popen([route.BIN/'layerfs-daemon'], env=env, stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def control_threads(name, expected):
    script = ("from pathlib import Path\nimport time\n"
              "p=Path('/proc')/Path('/tmp/layerfs-daemon.pid').read_text().strip()/'task'\n"
              "end=time.monotonic()+3\nwhile time.monotonic()<end:\n"
              " try: n=sum(x.read_text().strip().startswith('layerfs-control') for x in p.glob('*/comm'))\n"
              " except FileNotFoundError: continue\n"
              f" if n=={expected}: break\n"
              " time.sleep(.005)\nelse: raise AssertionError(('control thread count',n))\n")
    mount.checked(['docker','exec',name,'python3','-c',script])


def close_controller(process, name, expect=0):
    process.stdin.close()
    assert process.wait(timeout=6) == expect, process.stderr.read()
    control_threads(name, 1)


def run(args, report):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['mode'] == 'functional-mounted-proof'
    service_dir = args.output/'service'; service_dir.mkdir()
    seals = {}
    for name in ('store.sqlite','history.sqlite'):
        source = args.fixture.parent/'service'/name
        target = service_dir/name
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        shutil.copyfile(source,target)
        assert hashlib.sha256(target.read_bytes()).hexdigest() == digest
        seals[name] = digest
    report['fixture_reuse'] = {'receipt':str(args.fixture),'clone_method':'independent-byte-copy',
                               'closed_master_sha256':seals, 'cache_claim':None}
    server_key, daemon_key, good_key, denied_key, expiry_key = [os.urandom(32).hex() for _ in range(5)]
    server_public, daemon_public, good_public, denied_public, expiry_public = [
        route.public_key(key) for key in (server_key,daemon_key,good_key,denied_key,expiry_key)]
    service_env = os.environ.copy()
    service_env.update(LAYERFS_PRIVATE_KEY=server_key,
                       LAYERFS_PEERS=f'1,{daemon_public},{int(time.time())+3600},255',
                       LAYERFS_STORE=str(service_dir/'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0',
                       LAYERFS_TELEMETRY='off', LAYERFS_HISTORY_CATALOG=str(service_dir/'history.sqlite'),
                       LAYERFS_HISTORY_BINDING='pair1-mounted-read',LAYERFS_HISTORY_CREATE='0',
                       LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([route.BIN/'layerfs-service'],env=service_env,stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    daemon = client = reader = None
    name = f'layerfs-status-{os.getpid()}'
    mount.checked(['docker','volume','create',name+'-root'])
    try:
        ready = mount.line_until(service)
        assert 'ready' in ready, ready
        port = int(ready.strip().rsplit(':',1)[1])
        # The expiry peer gets a finite window long enough to establish its session;
        # later explicit observations keep that session active across actual expiry.
        expiry = int(time.time()) + 25
        env = os.environ.copy()
        env.update(LAYERFS_ENDPOINT=f'host.docker.internal:{port}',LAYERFS_SELECTOR='1',
                   LAYERFS_PRIVATE_KEY=daemon_key,LAYERFS_SERVER_KEY=server_public,LAYERFS_TELEMETRY='off',
                   LAYERFS_WORKSPACE_ROOT='/layerfs',LAYERFS_WORKSPACE_MAX_COUNT='2',
                   LAYERFS_CONTROL_LISTEN='0.0.0.0:23456',
                   LAYERFS_CONTROL_PEERS=f'1,{good_public},{int(time.time())+3600},1;2,{denied_public},{int(time.time())+3600},0;3,{expiry_public},{expiry},1',
                   LAYERFS_CONSTRUCTION_WORKERS='1')
        daemon = mount.mount_process(args,env,fixture['root'],name)
        first, second = mount.line_until(daemon), mount.line_until(daemon)
        assert 'workspace control ready' in first and 'workspace ready' in second,(first,second)
        published = mount.checked(['docker','port',name,'23456/tcp'],text=True).stdout.strip()
        control_port = int(published.rsplit(':',1)[1])
        client = controller(control_port,good_key,daemon_public)
        kind, value = query(client,1)
        assert kind == 6 and value['mounted'] and not value['closed'] and not value['stopping'],value
        assert value['workspace']=='read' and value['incarnation']=='71'*32
        assert 0 < value['consumer_accounted_bytes'] <= 8388608
        report['checks'].append({'id':'STATUS-01','status':'PASS','observation':value})
        # Holding one real semantic/FUSE FD is visible through the control endpoint.
        reader = subprocess.Popen(['docker','exec','-i',name,'python3','-u','-c',
            "import os,sys; f=os.open('/layerfs/workspace/read/data.bin',os.O_RDONLY); print('opened',flush=True); sys.stdin.readline(); assert os.read(f,19)==bytes(range(19)); os.close(f)"],
            stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        assert route.read_exact(reader.stdout.fileno(),7,time.monotonic()+8)==b'opened\n'
        kind, opened = query(client,2)
        assert kind==6 and opened['handles']==1 and opened['nodes']>=2,opened
        service.send_signal(signal.SIGSTOP)
        try:
            reader.stdin.write(b'go\n'); reader.stdin.flush()
            mount.wait_service_connection(name,port)
            kind,pending=query(client,3)
            assert kind==6 and pending['active_operations']>=1,pending
            report['checks'].append({'id':'STATUS-LOCAL-PROGRESS','status':'PASS',
                                      'boundary':'FUSE read waiting on paused service; control status completes',
                                      'observation':pending})
        finally:
            service.send_signal(signal.SIGCONT)
        stdout,stderr=reader.communicate(timeout=10); assert reader.returncode==0,(stdout,stderr)
        reader=None
        kind, closed = query(client,4)
        assert kind==6 and closed['handles']==0,closed
        report['checks'].append({'id':'STATUS-02','status':'PASS','opened':opened,'closed':closed})
        # Existing control session stays usable while another is immediately refused.
        excess=controller(control_port,good_key,daemon_public)
        try:
            try: query(excess,1)
            except (EOFError,BrokenPipeError): pass
            else: raise AssertionError('excess control session accepted')
            assert excess.wait(timeout=6)!=0
        finally:
            if excess.poll() is None: excess.kill(); excess.wait(timeout=6)
        assert query(client,5)[0]==6
        report['checks'].append({'id':'STATUS-03','status':'PASS','policy':'one control session/Q0'})
        close_controller(client,name);client=None
        for label, key, selector, target, incarnation in [
            ('wrong-workspace',good_key,1,b'other',b'\x71'*32),
            ('wrong-incarnation',good_key,1,b'read',b'\x72'*32),
            ('no-status-grant',denied_key,2,b'read',b'\x71'*32)]:
            client=controller(control_port,key,daemon_public,selector)
            kind,failure=query(client,1,target,incarnation)
            assert kind==7 and failure=={'code':3,'unknown':0,'cleanup':0},failure
            close_controller(client,name,1);client=None
            report['checks'].append({'id':'STATUS-04','case':label,'status':'PASS','failure':failure})
        # The actual service authority refuses daemon control despite all Store bits.
        service_client=controller(port,daemon_key,server_public)
        kind,failure=query(service_client,1)
        assert kind==7 and failure['code']==2 and not failure['unknown'],failure
        service_client.stdin.close();assert service_client.wait(timeout=6)==1
        report['checks'].append({'id':'STATUS-05','status':'PASS','service_refusal':failure})
        # Explicit new Status queries across expiry; never replay an earlier result.
        client=controller(control_port,expiry_key,daemon_public,3)
        assert int(time.time())<expiry,'expiry fixture exhausted before handshake'
        identity=1
        while True:
            kind,value=query(client,identity);identity+=1
            if kind==7:
                assert value['code']==3 and int(time.time())>=expiry,value
                break
            assert kind==6
            time.sleep(min(.25,max(0,expiry-time.time())))
        close_controller(client,name,1);client=None
        report['checks'].append({'id':'STATUS-06','status':'PASS','expiry_unix':expiry,
                                  'explicit_observations':identity-1})
        # An accepted silent handshake cannot delay daemon shutdown indefinitely.
        pending=socket.create_connection(('127.0.0.1',control_port),timeout=5)
        try:
            control_threads(name,2)
            mount.stop_mount(name)
            assert daemon.wait(timeout=10)==0
            pending.settimeout(3); assert pending.recv(1)==b''
        finally: pending.close()
        stderr=daemon.stderr.read(); (args.output/'daemon.stderr').write_bytes(first.encode()+second.encode()+stderr)
        assert b'workspace closed' in stderr,stderr
        daemon=None
        mount.checked(['docker','exec',name,'test','!','-e','/layerfs/workspace/read'])
        report['checks'].append({'id':'STATUS-07','status':'PASS','shutdown':'closes accepted silent handshake, joins, unmounts and cleans'})
    finally:
        if client is not None:
            client.kill();client.wait(timeout=6)
        if reader is not None:
            reader.kill();reader.wait(timeout=6)
        subprocess.run(['docker','rm','-f',name],capture_output=True,timeout=15)
        if daemon is not None:
            daemon.wait(timeout=10)
            (args.output/'failed-daemon.stderr').write_bytes(daemon.stderr.read())
        if service.poll() is None:
            service.stdin.close();service.wait(timeout=10)
        (args.output/'service.stderr').write_bytes(service.stderr.read())
        mount.checked(['docker','volume','rm',name+'-root'])


def main():
    start=time.monotonic()
    parser=argparse.ArgumentParser()
    parser.add_argument('--linux-daemon',type=Path,required=True)
    parser.add_argument('--fixture',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--image',default='rust:1.85.1-bookworm')
    args=parser.parse_args()
    args.linux_daemon=args.linux_daemon.resolve();args.fixture=args.fixture.resolve();args.output=args.output.resolve()
    args.output.mkdir(parents=True,exist_ok=False)
    def timeout(_signum,_frame): raise TimeoutError('Status proof exceeded 60 seconds')
    signal.signal(signal.SIGALRM,timeout);signal.alarm(60)
    report={'status':'FAIL','mode':'functional-control-status','checks':[],'performance_claim':False,
            'source':mount.checked(['git','rev-parse','HEAD'],text=True).stdout.strip(),
            'product_inputs_sha256':mount.product_inputs(),
            'linux_binary_sha256':hashlib.sha256(args.linux_daemon.read_bytes()).hexdigest(),
            'host_binary_sha256':hashlib.sha256((route.BIN/'layerfs-daemon').read_bytes()).hexdigest(),
            'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            'runtime_image':mount.checked(['docker','image','inspect','--format','{{.Id}}',args.image],text=True).stdout.strip(),
            'not_run':['remote Attach/Mount/Unmount/CloseClean','SDK edits/Commit','malformed authenticated frames against daemon','control during actual service save','performance/RSS qualification']}
    try:
        run(args,report);report['status']='PASS'
    except BaseException as error:
        report['failure']=repr(error)
        if isinstance(error,subprocess.CalledProcessError):
            report['failure_stdout']=error.stdout.decode(errors='replace');report['failure_stderr']=error.stderr.decode(errors='replace')
        raise
    finally:
        signal.alarm(0);report['command_wall_seconds']=time.monotonic()-start
        (args.output/'result.json').write_text(json.dumps(report,indent=2)+'\n')

if __name__=='__main__': main()
