#!/usr/bin/env python3
"""A running daemon must exit on invalid input even with a full diagnostic pipe."""
import fcntl, json, os, select, socket, struct, subprocess, sys, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'core/crates/layerfs-daemon/tests'))
from docker_route import BIN, Diagnostics, exchange, frame, public, prepared_fixture
reader,writer=os.pipe();service=None;daemon=None;diagnostics=None
try:
    with tempfile.TemporaryDirectory(prefix='layerfs-exit-proof-')as temp:
        store=Path(temp)/'store.sqlite'
        fixture=prepared_fixture(store)
        server=os.urandom(32).hex();client=os.urandom(32).hex()
        env=os.environ.copy();env.update(LAYERFS_PRIVATE_KEY=server,LAYERFS_PEERS=f'1,{public(client)},{int(time.time())+3600},31',LAYERFS_STORE=str(store),LAYERFS_LISTEN='127.0.0.1:0',LAYERFS_TELEMETRY='off')
        service=subprocess.Popen([BIN/'layerfs-service'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        assert select.select([service.stderr],[],[],5)[0], 'service startup'
        ready=service.stderr.readline();assert b'ready'in ready,ready;port=int(ready.rsplit(b':',1)[1]);diagnostics=Diagnostics(service.stderr)
        env.update(LAYERFS_PRIVATE_KEY=client,LAYERFS_SERVER_KEY=public(server),LAYERFS_SELECTOR='1',LAYERFS_ENDPOINT=f'127.0.0.1:{port}')
        daemon=subprocess.Popen([BIN/'layerfs-daemon'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=writer)
        result=exchange(daemon,1,1,bytes.fromhex(fixture['file'])+struct.pack('>QQ',0,8));assert result[0]==6 and result[2]==8
        flags=fcntl.fcntl(writer,fcntl.F_GETFL);fcntl.fcntl(writer,fcntl.F_SETFL,flags|os.O_NONBLOCK);filled=0
        while True:
            try:filled+=os.write(writer,b'x'*4096);assert filled<=1024*1024
            except BlockingIOError:break
        fcntl.fcntl(writer,fcntl.F_SETFL,flags)
        started=time.monotonic();daemon.stdin.write(frame(3,2,b'invalid state'));daemon.stdin.flush()
        daemon.wait(timeout=1)
        assert daemon.returncode!=0 and daemon.stdout.read()==b''
        assert fcntl.fcntl(writer,fcntl.F_GETFL)&os.O_NONBLOCK
        print(f'PASS: established connection; full diagnostic pipe={filled} bytes; invalid-input exit={daemon.returncode}; error/cleanup wall={time.monotonic()-started:.3f}s')
        service.stdin.close();service.wait(timeout=3);assert service.returncode==0
finally:
    for child in (daemon,service):
        if child is not None and child.poll()is None:child.kill();child.wait()
    os.close(writer);os.close(reader)
    if diagnostics:diagnostics.close()
