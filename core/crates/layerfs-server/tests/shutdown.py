#!/usr/bin/env python3
"""Bounded executable shutdown with two active saves and two unfinished handshakes."""
import json, os, select, socket, sqlite3, struct, subprocess, sys, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'core/crates/layerfs-daemon/tests'))
from docker_route import BIN, Diagnostics, exchange, frame, public, request, receive, prepared_fixture
service=None;children=[];sockets=[];diagnostics=[]
try:
    with tempfile.TemporaryDirectory(prefix='layerfs-shutdown-proof-')as temp:
        path=Path(temp)/'store.sqlite';fixture=prepared_fixture(path)
        server=os.urandom(32).hex();client=os.urandom(32).hex()
        env=os.environ.copy();env.update(LAYERFS_PRIVATE_KEY=server,LAYERFS_PEERS=f'1,{public(client)},{int(time.time())+3600},31',LAYERFS_STORE=str(path),LAYERFS_LISTEN='127.0.0.1:0',LAYERFS_TELEMETRY='off')
        service=subprocess.Popen([BIN/'layerfs-server'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        assert select.select([service.stderr],[],[],5)[0], 'service startup'
        ready=service.stderr.readline();assert b'ready'in ready,ready;port=int(ready.rsplit(b':',1)[1]);diagnostics.append(Diagnostics(service.stderr))
        env.update(LAYERFS_PRIVATE_KEY=client,LAYERFS_SERVER_KEY=public(server),LAYERFS_SELECTOR='1',LAYERFS_ENDPOINT=f'127.0.0.1:{port}')
        for _ in range(2):
            child=subprocess.Popen([BIN/'layerfs-daemon'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE);children.append(child);diagnostics.append(Diagnostics(child.stderr))
            assert exchange(child,1,1,bytes.fromhex(fixture['file'])+struct.pack('>QQ',0,8))[0]==6
        for _ in range(2):sockets.append(socket.create_connection(('127.0.0.1',port),timeout=2))
        for child in children:
            child.stdin.write(request(2,3,struct.pack('>Q',300000))+frame(3,2,b'partial'));child.stdin.flush()
        end=time.monotonic()+2
        while True:
            with sqlite3.connect(path,timeout=0)as db:active=db.execute('SELECT count(*) FROM saves WHERE active_slot IS NOT NULL').fetchone()[0]
            if active==2:break
            if time.monotonic()>=end:raise TimeoutError('both operations must be active before shutdown')
            time.sleep(.01)
        started=time.monotonic();service.stdin.close();service.wait(timeout=3);assert service.returncode==0
        outcomes=[]
        for child in children:
            try:
                result=receive(child,3);assert result[0]==7,result;outcomes.append({'code':result[1][0],'unknown':bool(result[1][1])})
            except EOFError:outcomes.append({'terminal':'absent','unknown':True})
            child.stdin.close();child.wait(timeout=3);assert child.returncode!=0
        for stream in sockets:assert stream.recv(1)==b''
        with sqlite3.connect(path,timeout=0)as db:assert db.execute('SELECT count(*) FROM saves WHERE active_slot IS NOT NULL').fetchone()[0]==0
        print(json.dumps({'status':'PASS','service_exit':service.returncode,'active_saves':2,'unfinished_handshakes':2,'outcomes':outcomes,'shutdown_wall_seconds':time.monotonic()-started,'private_slots_after_known_cleanup':0,'replay':False,'nonpreemptible_core_shutdown':'not forced by this cooperative-I/O case'}))
finally:
    for child in children+([service]if service else[]):
        if child.poll()is None:child.kill();child.wait(timeout=3)
    for stream in sockets:stream.close()
    for diagnostic in diagnostics:diagnostic.close()
