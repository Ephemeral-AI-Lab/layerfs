#!/usr/bin/env python3
"""Explicit software-provider reference against the new ARM service; no fallback."""
import hashlib, json, os, select, struct, subprocess, sys, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'core/crates/layerfs-daemon/tests'))
from docker_route import BIN, Diagnostics, exchange, public, prepared_fixture
EVIDENCE=ROOT/'core/docs/architecture/proposal/service-daemon-transport/implementation/evidence'
baseline=json.loads((EVIDENCE/'optimization-release-artifacts-20260920.json').read_text())
current=json.loads((EVIDENCE/'optimization-neon-artifacts-20260920.json').read_text())
assert baseline['runtime_sha256']==current['runtime_sha256']
old_path=ROOT/'core/target/release/layerfs-daemon';digest=baseline['binaries'][str(old_path)]
reference=ROOT/'core/target/issue192-binary-archive'/digest/'layerfs-daemon'
assert hashlib.sha256(reference.read_bytes()).hexdigest()==digest
for name,value in current['binaries'].items():assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==value,name
service=None;daemon=None;diagnostics=[]
try:
    with tempfile.TemporaryDirectory(prefix='layerfs-interop-proof-')as temp:
        path=Path(temp)/'store.sqlite';prepared_fixture(path)
        server=os.urandom(32).hex();client=os.urandom(32).hex()
        env=os.environ.copy();env.update(LAYERFS_PRIVATE_KEY=server,LAYERFS_PEERS=f'1,{public(client)},{int(time.time())+3600},31',LAYERFS_STORE=str(path),LAYERFS_LISTEN='127.0.0.1:0',LAYERFS_TELEMETRY='off')
        service=subprocess.Popen([BIN/'layerfs-server'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        assert select.select([service.stderr],[],[],5)[0]
        ready=service.stderr.readline();assert b'ready'in ready;port=int(ready.rsplit(b':',1)[1]);diagnostics.append(Diagnostics(service.stderr))
        env.update(LAYERFS_PRIVATE_KEY=client,LAYERFS_SERVER_KEY=public(server),LAYERFS_SELECTOR='1',LAYERFS_ENDPOINT=f'127.0.0.1:{port}')
        daemon=subprocess.Popen([reference],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE);diagnostics.append(Diagnostics(daemon.stderr))
        payload=hashlib.shake_256(b'issue192-software-neon-interoperability-v1').digest(500000)
        saved=exchange(daemon,1,3,struct.pack('>Q',len(payload)),payload);assert saved[0]==6,saved
        root=saved[1][1:33]
        value=exchange(daemon,2,1,root+struct.pack('>QQ',0,len(payload)))
        assert value[0]==6 and value[2]==len(payload) and value[3]==hashlib.sha256(payload).hexdigest(),value
        daemon.stdin.close();daemon.wait(timeout=3);assert daemon.returncode==0
        service.stdin.close();service.wait(timeout=3);assert service.returncode==0
        print(json.dumps({'status':'PASS','reference_role':'software-provider interoperability peer only; never fallback','reference_daemon_sha256':digest,'current_service_sha256':current['binaries'][str(BIN/'layerfs-server')],'runtime_sha256':current['runtime_sha256'],'bytes_each_direction':len(payload),'root':root.hex(),'input_sha256':hashlib.sha256(payload).hexdigest()}))
finally:
    for child in (daemon,service):
        if child is not None and child.poll()is None:child.kill();child.wait(timeout=3)
    for diagnostic in diagnostics:diagnostic.close()
