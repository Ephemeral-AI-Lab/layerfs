#!/usr/bin/env python3
"""Real daemon/service error and admission cases; no benchmark timings."""
import argparse, json, os, socket, struct, subprocess, tempfile, time, sqlite3, hashlib, threading, signal, select
from pathlib import Path
from docker_route import BIN, ROOT, Diagnostics, start_daemon, public, frame, request, receive, exchange, prepared_fixture, verify_artifacts

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--image",required=True);parser.add_argument("--output",type=Path,required=True);parser.add_argument("--selection",choices=["errors","admission","blocked-stderr","cleanup","slow","lost-response","additional","envelope","writers","large-repeat","large-distinct"],default="errors");parser.add_argument("--size-mib",type=int,choices=[1,256,4096],default=256);args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    signal.signal(signal.SIGALRM, lambda *_: (_ for _ in ()).throw(TimeoutError("functional command budget")))
    signal.alarm(50)
    evidence={"selection":args.selection,"kind":"functional-deployment","performance":"NOT_RUN","cases":[],"status":"INCOMPLETE"};children=[];service=None;names=[];diagnostics=[]
    def passed(name,detail):evidence["cases"].append({"name":name,"status":"PASS","detail":detail})
    try:
      evidence["artifacts"]=verify_artifacts(args.image)
      with tempfile.TemporaryDirectory(prefix="layerfs-issue192-faults-") as temp:
        fixture=prepared_fixture(Path(temp)/"store.sqlite");evidence["setup"]=fixture["setup"];root=bytes.fromhex(fixture["file"]);tree=bytes.fromhex(fixture["root"])
        server_key=os.urandom(32).hex();keys=[os.urandom(32).hex(),os.urandom(32).hex()];pub=[public(k)for k in keys];server_public=public(server_key)
        with socket.socket() as s:s.bind(("127.0.0.1",0));port=s.getsockname()[1]
        env=os.environ.copy();env.update(LAYERFS_PRIVATE_KEY=server_key,LAYERFS_PEERS=f"1,{pub[0]},{int(time.time())+3600},31;2,{pub[1]},{int(time.time())+3600},1",LAYERFS_STORE=str(Path(temp)/"store.sqlite"),LAYERFS_LISTEN=f"0.0.0.0:{port}",LAYERFS_TELEMETRY="forward"if args.selection in ("envelope","large-repeat","large-distinct")else"off",LAYERFS_RUN_ID="192",LAYERFS_NAMESPACE="1")
        service=subprocess.Popen([BIN/"layerfs-service"],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE);assert b"ready" in service.stderr.readline();diagnostics.append(Diagnostics(service.stderr))
        def spawn(principal=1,telemetry="off",drain=True,endpoint=None):
            name=f"layerfs-issue192-fault-{os.getpid()}-{len(names)}";names.append(name)
            denv=env.copy();denv.update(LAYERFS_PRIVATE_KEY=keys[principal-1],LAYERFS_SERVER_KEY=server_public,LAYERFS_ENDPOINT=endpoint or f"host.docker.internal:{port}",LAYERFS_SELECTOR=str(principal),LAYERFS_TELEMETRY=telemetry,LAYERFS_RUN_ID="192",LAYERFS_NAMESPACE=str(len(names)+10))
            cmd=["docker","run","--name",name,"--label=io.layerfs.task=issue192","--log-driver=none","--read-only","--cap-drop=ALL","--security-opt=no-new-privileges","--cpus=1","--memory=128m","--memory-swap=128m","--pids-limit=16","-i"]
            for key in ["LAYERFS_PRIVATE_KEY","LAYERFS_SERVER_KEY","LAYERFS_ENDPOINT","LAYERFS_SELECTOR","LAYERFS_TELEMETRY","LAYERFS_RUN_ID","LAYERFS_NAMESPACE"]:cmd += ["-e",key]
            child=start_daemon(cmd+[args.image],denv,diagnostics,drain);children.append(child)
            return child
        if args.selection=="errors":
            cases=[("path-absence",2,tree+b"\1"+struct.pack(">H",7)+b"missing",1,1,7),("object-absence",1,bytes(32)+struct.pack(">QQ",0,1),1,1,6),("range-error",1,root+struct.pack(">QQ",0,9),1,1,1),("store-denied",1,root+struct.pack(">QQ",0,8),1,2,3),("operation-denied",3,struct.pack(">Q",0),2,1,3)]
            for name,op,data,principal,store,code in cases:
                child=spawn(principal);result=exchange(child,1,op,data,store=store);assert result[:2]==(7,bytes([code,0,0])),(name,result);child.stdin.close();child.wait(timeout=3);passed(name,{"code":code,"unknown":False})
            child=spawn(2);child.stdin.write(request(1,3,struct.pack(">Q",64*1024*1024)));child.stdin.flush();result=receive(child);assert result[:2]==(7,b"\3\0\0");child.wait(timeout=2);passed("early-refusal-unread-stdin",{"terminal":"Denied","input_body_sent":0})
            for name,wire in [("body-before-begin",frame(3,1,b"x")),("oversized-frame",b"LFB1"+struct.pack(">BBHQI",2,0,0,1,2**32-1)),("truncated-header",b"LFB1\2\0"),("truncated-body",frame(3,1,b"partial-body")[:-3]),("zero-body",frame(3,1,b"")),("unsupported-profile",request(1,3,struct.pack(">Q",0))[:32]+b"\0\2"+request(1,3,struct.pack(">Q",0))[34:])]:
                child=spawn();child.stdin.write(wire);child.stdin.close();child.wait(timeout=3);assert child.returncode!=0,(name,child.returncode);assert child.stdout.read(1)==b"";passed(name,{"session_closed":True})
            child=spawn();child.stdin.write(request(1,3,struct.pack(">Q",1))+frame(3,2,b"x")+frame(4,1,struct.pack(">Q",1)));child.stdin.close();result=receive(child);assert result[0]==7,result;child.wait(timeout=3);passed("wrong-operation-id",{"code":result[1][0],"unknown":bool(result[1][1])})
            child=spawn();child.stdin.write(request(1,3,struct.pack(">Q",0))+frame(4,1,struct.pack(">Q",0))+frame(3,1,b"surplus"));child.stdin.close();result=receive(child);assert result[0]==6,result;child.wait(timeout=3);assert child.returncode!=0;passed("surplus-after-completed-operation",{"first_operation":"valid success after EndInput","surplus":"session refused; never another operation"})
            child=spawn();child.stdin.write(request(1,3,struct.pack(">Q",300000))+frame(3,1,b"partial"));child.stdin.close();result=receive(child);assert result[0]==7,result;child.wait(timeout=3);passed("disconnect-before-end-input",{"code":result[1][0],"unknown":bool(result[1][1]),"replay":False})
            child=spawn();result=exchange(child,1,1,root+struct.pack(">QQ",0,8));assert result[0]==6 and result[2]==8;child.stdin.close();child.wait(timeout=3);passed("old-root-after-failures",{"bytes":8})
        elif args.selection in ("large-repeat","large-distinct"):
            size=args.size_mib*1024*1024
            child=spawn(telemetry="forward")
            template=hashlib.shake_256(b"issue192-large-repeat-v1").digest(16384)
            expected=hashlib.sha256();errors=[]
            def stream_input():
                try:
                    child.stdin.write(request(1,3,struct.pack(">Q",size),deadline_ms=600000,response_bytes=size))
                    for index in range(size//16384):
                        data=template if args.selection=="large-repeat" else hashlib.shake_256(b"issue192-large-distinct-v1"+struct.pack(">Q",index)).digest(16384)
                        expected.update(data);child.stdin.write(frame(3,1,data))
                    child.stdin.write(frame(4,1,struct.pack(">Q",size)));child.stdin.flush()
                except BaseException as error:errors.append(repr(error))
            writer=threading.Thread(target=stream_input,daemon=True);writer.start()
            value=receive(child,45);writer.join(timeout=1)
            assert not writer.is_alive() and not errors and value[0]==6,(value,errors)
            saved=value[1][1:33];assert struct.unpack(">Q",value[1][33:41])[0]==size
            child.stdin.write(request(2,1,saved+struct.pack(">QQ",0,size),deadline_ms=600000,response_bytes=size)+frame(4,2,struct.pack(">Q",0)));child.stdin.flush()
            value=receive(child,45);assert value[0]==6 and value[2]==size and value[3]==expected.hexdigest(),value
            passed("large-stream-and-exact-readback",{"size_bytes":size,"pattern":args.selection,"sha256":expected.hexdigest(),"root":saved.hex(),"input_buffer_bytes":16384,"whole_payload_buffer":False,"performance":"NOT_RUN"})
            child.stdin.close();child.wait(timeout=3)
        elif args.selection=="writers":
            holders=[spawn(),spawn(),spawn()]
            for child in holders:
                assert exchange(child,1,1,root+struct.pack(">QQ",0,8))[0]==6
            payload=hashlib.shake_256(b"issue192-two-writers-v1").digest(2*1024*1024)
            prefix=1536*1024
            def begin(child,identity,body):
                child.stdin.write(request(identity,3,struct.pack(">Q",len(body))))
                for offset in range(0,prefix,16384):child.stdin.write(frame(3,identity,body[offset:offset+16384]))
                child.stdin.flush()
            def finish(child,identity,body):
                for offset in range(prefix,len(body),16384):child.stdin.write(frame(3,identity,body[offset:offset+16384]))
                child.stdin.write(frame(4,identity,struct.pack(">Q",len(body))));child.stdin.flush()
                value=receive(child);assert value[0]==6,value
                return value[1][1:33]
            def private_packs():
                with sqlite3.connect(Path(temp)/"store.sqlite",timeout=0)as db:
                    return db.execute("SELECT s.save_id, count(p.pack_id) FROM saves s LEFT JOIN object_packs p USING(save_id) WHERE s.active_slot IS NOT NULL GROUP BY s.save_id").fetchall()
            begin(holders[0],2,payload);begin(holders[1],2,payload)
            until=time.monotonic()+2
            while True:
                rows=private_packs()
                if len(rows)==2 and all(count>0 for _,count in rows):break
                if time.monotonic()>=until:raise TimeoutError("both private saves must own physical packs")
                time.sleep(.01)
            excess=exchange(holders[2],2,3,struct.pack(">Q",0))
            if excess[:2]!=(7,b"\4\0\0"):
                early=[]
                for child in holders[:2]:
                    if select.select([child.stdout],[],[],0)[0]:
                        reply=receive(child);early.append({"kind":reply[0],"body_hex":reply[1].hex()})
                    else:early.append({"terminal":"not yet available"})
                evidence["early_writer_terminals"]=early
            assert excess[:2]==(7,b"\4\0\0"),excess
            holders[2].stdin.close();holders[2].wait(timeout=3)
            b=finish(holders[1],2,payload)
            value=exchange(holders[1],3,1,b+struct.pack(">QQ",0,len(payload)))
            assert value[0]==6 and value[2]==len(payload) and value[3]==hashlib.sha256(payload).hexdigest(),value
            assert len(private_packs())==1
            a=finish(holders[0],2,payload);assert a==b
            with sqlite3.connect(Path(temp)/"store.sqlite",timeout=0)as db:
                duplicate=db.execute("SELECT count(*) FROM (SELECT object_id FROM objects GROUP BY object_id HAVING count(*)=2)").fetchone()[0]
                assert duplicate>0
                assert db.execute("SELECT count(*) FROM saves WHERE active_slot IS NOT NULL").fetchone()[0]==0
            passed("overlapping-identical-writers-reverse-finish",{"root":a.hex(),"input_sha256":hashlib.sha256(payload).hexdigest(),"bytes":len(payload),"both_private_pack_owners":rows,"duplicate_locator_identities":duplicate,"B_read_before_A_finished":True,"equal_session_request_ids":2,"third_operation":"Capacity"})
            # A later pair uses fresh canonical data so abort has physical work.
            left=hashlib.shake_256(b"issue192-abort-A-v1").digest(len(payload))
            right=hashlib.shake_256(b"issue192-survive-B-v1").digest(len(payload))
            begin(holders[0],3,left);begin(holders[1],4,right)
            until=time.monotonic()+2
            while len(private_packs())!=2:
                if time.monotonic()>=until:raise TimeoutError("second overlapping pair")
                time.sleep(.01)
            holders[0].stdin.close();failure=receive(holders[0]);assert failure[0]==7,failure;holders[0].wait(timeout=3)
            survivor=finish(holders[1],4,right)
            value=exchange(holders[1],5,1,survivor+struct.pack(">QQ",0,len(right)))
            assert value[0]==6 and value[3]==hashlib.sha256(right).hexdigest(),value
            assert private_packs()==[]
            value=exchange(holders[1],6,1,a+struct.pack(">QQ",0,len(payload)))
            assert value[0]==6 and value[3]==hashlib.sha256(payload).hexdigest(),value
            passed("abort-isolated-from-other-writer",{"surviving_root":survivor.hex(),"abort_code":failure[1][0],"private_owners_after_cleanup":0,"previous_root_unchanged":True})
            holders[1].stdin.close();holders[1].wait(timeout=3)
        elif args.selection in ("admission","envelope"):
            holders=[]
            # Each completed read establishes the real daemon connection. A
            # two partially supplied mutations hold both service slots.
            for slot in range(4):
                child=spawn(telemetry="forward"if args.selection in ("envelope","large-repeat","large-distinct")else"off");result=exchange(child,slot*10+1,1,root+struct.pack(">QQ",slot,slot+1));assert result[0]==6 and result[3]==hashlib.sha256(b"original"[slot:slot+1]).hexdigest();holders.append(child)
            excess=spawn();excess.wait(timeout=3);assert excess.returncode!=0;passed("C-plus-one",{"established":4,"excess_refused":True})
            for slot in range(2):
                holders[slot].stdin.write(request(slot*10+2,3,struct.pack(">Q",300000))+frame(3,slot*10+2,b"held"));holders[slot].stdin.flush()
            # One scheduling turn gives the authenticated BEGIN to the host.
            time.sleep(.1)
            if args.selection=="envelope":
                configs=json.loads(subprocess.check_output(["docker","inspect",*names[:4]],text=True))
                endpoint=subprocess.check_output(["docker","context","inspect","--format","{{.Endpoints.docker.Host}}"],text=True).strip();assert endpoint.startswith("unix://")
                observations=[]
                for config in configs:
                    assert not config["Mounts"] and config["HostConfig"]["Memory"]==128*1024*1024 and config["HostConfig"]["PidsLimit"]==16
                    stats=json.loads(subprocess.check_output(["curl","-sS","--max-time","2","--max-filesize","65536","--unix-socket",endpoint[7:],f"http://localhost/containers/{config['Id']}/stats?stream=false&one-shot=true"]))
                    observations.append({"id":config["Id"],"image":config["Image"],"memory_limit":config["HostConfig"]["Memory"],"pids_limit":config["HostConfig"]["PidsLimit"],"memory":stats.get("memory_stats"),"pids":stats.get("pids_stats")})
                stacks=subprocess.run(["vmmap","-summary",str(service.pid)],capture_output=True,text=True,timeout=3)
                sockets=subprocess.check_output(["/usr/sbin/lsof","-a","-p",str(service.pid),"-iTCP","-nP"],text=True)
                thread_rows=subprocess.check_output(["ps","-M","-p",str(service.pid)],text=True)
                evidence["physical_envelope"]={"containers":observations,"service_pid":service.pid,"service_threads":thread_rows,"service_sockets":sockets,"stack_regions":[line for line in stacks.stdout.splitlines()if "Stack"in line],"vmmap_status":stacks.returncode,"vmmap_unavailable":stacks.stderr[:1024]if stacks.returncode else None,"service_connection_stack_request_bytes":4*2*1024*1024,"service_telemetry_stack_request_bytes":2*256*1024,"active_daemon_upload_stack_request_bytes":2*2*1024*1024,"daemon_telemetry_stack_request_bytes":4*2*256*1024,"socket_kernel_memory":"unavailable; ordinary TCP has no universal option-size physical-memory ceiling","phase_peak":"unavailable; sampled process/cgroup observations are not additive exclusive-operation costs"}
                passed("maximum-connected-native-roles",{"service":1,"daemons":4,"active_mutations":2,"telemetry":"forward","response_isolation":"four distinct byte ranges and hashes"})
            result=exchange(holders[2],22,3,struct.pack(">Q",0));assert result[:2]==(7,b"\4\0\0"),result;passed("A-plus-one-Q-zero",{"active":2,"excess_code":"Capacity"})
            for child in holders:child.stdin.close()
            for child in holders:child.wait(timeout=3)
        elif args.selection=="cleanup":
            connection=sqlite3.connect(str(Path(temp)/"store.sqlite"));connection.execute("PRAGMA journal_mode=MEMORY");connection.execute("PRAGMA synchronous=OFF")
            connection.execute("CREATE TRIGGER refuse_cleanup BEFORE DELETE ON object_packs BEGIN SELECT RAISE(FAIL, 'external cleanup fault'); END");connection.commit()
            payload=hashlib.shake_256(b"issue192-cleanup-fault-v1").digest(6*1024*1024)
            child=spawn();result=exchange(child,1,3,struct.pack(">Q",8*1024*1024),payload);assert result[0]==7 and result[1][2]!=0,result;child.stdin.close();child.wait(timeout=3)
            passed("cleanup-failure-retains-original",{"code":result[1][0],"unknown":bool(result[1][1]),"cleanup_code":result[1][2],"input_sha256":hashlib.sha256(payload).hexdigest()})
            child=spawn();result=exchange(child,1,3,struct.pack(">Q",0));assert result[0]==6,result;child.stdin.close();child.wait(timeout=3);passed("one-failed-cleanup-leaves-one-slot",{"successful_empty_save":True})
            payload=hashlib.shake_256(b"issue192-second-cleanup-fault-v1").digest(6*1024*1024)
            child=spawn();result=exchange(child,1,3,struct.pack(">Q",8*1024*1024),payload);assert result[0]==7 and result[1][2]!=0,result;child.stdin.close();child.wait(timeout=3)
            child=spawn();result=exchange(child,1,3,struct.pack(">Q",0));assert result[0]==7 and result[1][0]==5,result;child.stdin.close();child.wait(timeout=3);passed("two-retained-owners-refuse-new-mutation",{"code":"Ownership"})
            child=spawn();result=exchange(child,1,1,root+struct.pack(">QQ",0,8));assert result[0]==6 and result[2]==8;child.stdin.close();child.wait(timeout=3);passed("old-root-survives-cleanup-failure",{"bytes":8})
            connection.close()
        elif args.selection=="additional":
            scope=bytes.fromhex(fixture["scope"]);meta=bytes.fromhex(fixture["metadata"])
            data=tree+scope+struct.pack(">QH",1,1)+struct.pack(">QH",1,2)+struct.pack(">H",1)+b"f"+struct.pack(">QH",0,7)+b"renamed"+struct.pack(">QH",2,0)
            child=spawn();value=exchange(child,1,5,data);assert value[0]==6,value;renamed=value[1][1:33]
            value=exchange(child,2,2,renamed+b"\1"+struct.pack(">H",7)+b"renamed");assert value[0]==6 and value[1][18:50]==root,value
            value=exchange(child,3,2,tree+b"\1"+struct.pack(">H",1)+b"f");assert value[0]==6 and value[1][18:50]==root,value;child.stdin.close();child.wait(timeout=3)
            passed("changed-name-tree-original-intact",{"root":renamed.hex()})
            for name,data in [("scope",tree+bytes(32)+struct.pack(">QHH",1,0,0)),("allocation",tree+scope+struct.pack(">QHHQB",1,0,1,999,1)+root+meta)]:
                child=spawn();value=exchange(child,1,5,data);assert value[:2]==(7,bytes([1,0,0])),value;child.stdin.close();child.wait(timeout=3);passed("reject-"+name,{"code":"Invalid"})
            for name,edits in [("coordinates",[(9,9,0)]),("overlap",[(0,0,3),(1,1,0)]),("overflow",[(0,0,2**64-1)]),("replay-cap",[(0,0,8*1024*1024+1)])]:
                child=spawn();child.stdin.write(request(1,4,root+struct.pack(">QH",8,len(edits))+b"".join(struct.pack(">QQQ",*e)for e in edits)));child.stdin.close();child.wait(timeout=3);assert child.returncode!=0;passed("edit-reject-"+name,{"body_sent":0})
            child=spawn();wire=bytearray(request(1,3,struct.pack(">Q",0)));wire[20+26]=255;child.stdin.write(wire);child.stdin.close();child.wait(timeout=3);assert child.returncode!=0;passed("unsupported-opcode",{"body_sent":0})
            peer_env=env.copy();peer_env.update(LAYERFS_PRIVATE_KEY=keys[0],LAYERFS_SERVER_KEY=server_public,LAYERFS_ENDPOINT=f"127.0.0.1:{port}")
            peer=subprocess.check_output([BIN/"examples/fault_peer"],env=peer_env,text=True,timeout=3);passed("authenticated-malformed-peer",{"output":peer.strip(),"scope":"external host peer; native service parser, daemon bypassed"})
            original=keys[0];keys[0]=os.urandom(32).hex();child=spawn();child.stdin.close();child.wait(timeout=3);assert child.returncode!=0;keys[0]=original;passed("untrusted-key",{"session_closed":True})
        elif args.selection=="lost-response":
            # External opaque fault proxy: pass the handshake and Hello, consume
            # the first encrypted operation result, then disconnect without delivery.
            proxy=socket.socket();proxy.bind(("0.0.0.0",0));proxy.listen(1);proxy.settimeout(3);proxy_port=proxy.getsockname()[1];fault={};threads=[]
            def exact(sock,n):
                out=bytearray()
                while len(out)<n:
                    data=sock.recv(n-len(out))
                    if not data:raise EOFError()
                    out.extend(data)
                return bytes(out)
            def relay():
                try:
                    with proxy.accept()[0] as down,socket.create_connection(("127.0.0.1",port),timeout=3)as up:
                        down.settimeout(3)
                        def upload():
                            try:
                                while chunk:=down.recv(16384):up.sendall(chunk)
                            except OSError:pass
                        thread=threading.Thread(target=upload,daemon=True);thread.start();threads.append(thread)
                        for index in range(3):
                            header=exact(up,4);size=struct.unpack(">I",header)[0];assert size<=32804
                            message=exact(up,size)
                            if index<2:down.sendall(header+message)
                            else:fault["withheld_ciphertext_bytes"]=size
                        down.shutdown(socket.SHUT_RDWR);up.shutdown(socket.SHUT_RDWR)
                except BaseException as error:fault["error"]=repr(error)
                finally:proxy.close()
            controller=threading.Thread(target=relay,daemon=True);controller.start()
            child=spawn(endpoint=f"host.docker.internal:{proxy_port}");payload=bytes(i%251 for i in range(500000));result=exchange(child,1,3,struct.pack(">Q",len(payload)),payload);assert result[:2]==(7,bytes([12,1,0])),result;child.stdin.close();child.wait(timeout=3);controller.join(timeout=3);assert not controller.is_alive() and "error"not in fault,fault
            for thread in threads:thread.join(timeout=1);assert not thread.is_alive()
            saved=bytes.fromhex("5e8a806ac5d657b041947f7f385762483e2ebc4e2980e67427b4a1dcf67a53be")
            child=spawn();read=exchange(child,1,1,saved+struct.pack(">QQ",0,len(payload)));assert read[0]==6 and read[3]==hashlib.sha256(payload).hexdigest(),read;child.stdin.close();child.wait(timeout=3)
            passed("lost-result-after-confirmed-save",{**fault,"daemon_code":"Unknown","unknown":True,"saved_root_readable":saved.hex(),"replayed":False,"proxy_scope":"external fault controller; encrypted result deliberately withheld"})
        elif args.selection=="slow":
            child=spawn();child.stdin.write(request(1,3,struct.pack(">Q",300000),deadline_ms=250)+frame(3,1,b"partial"));child.stdin.flush()
            try:
                result=receive(child);assert result[0]==7,result;outcome={"code":result[1][0],"unknown":bool(result[1][1])}
            except EOFError:outcome={"terminal":"missing after deadline","unknown":True}
            child.stdin.close();child.wait(timeout=3);assert child.returncode!=0;passed("slow-upload-deadline",{**outcome,"requested_deadline_ms":250,"replay":False})
            child=spawn();payload=hashlib.shake_256(b"issue192-slow-output-v1").digest(2*1024*1024);result=exchange(child,1,3,struct.pack(">Q",len(payload)),payload);assert result[0]==6,result;large=result[1][1:33]
            child.stdin.write(request(2,1,large+struct.pack(">QQ",0,len(payload)),deadline_ms=250)+frame(4,2,struct.pack(">Q",0)));child.stdin.flush()
            end=time.monotonic()+3
            while True:
                state=json.loads(subprocess.check_output(["docker","inspect",names[-1]],text=True))[0]["State"]
                if not state["Running"]:break
                if time.monotonic()>=end:raise TimeoutError("blocked stdout did not close at deadline")
                time.sleep(.1)
            child.stdout.close();child.stdin.close();child.wait(timeout=3);passed("blocked-result-consumer",{"requested_deadline_ms":250,"container_exit":state["ExitCode"],"partial_read":"provisional; no terminal success consumed"})
        else:
            child=spawn(telemetry="forward",drain=False)
            for identity in range(1,201):
                result=exchange(child,identity,1,root+struct.pack(">QQ",0,8));assert result[0]==6 and result[2]==8
            child.stdin.close()
            end=time.monotonic()+4
            while True:
                state=json.loads(subprocess.check_output(["docker","inspect",names[-1]],text=True))[0]["State"]
                if not state["Running"]:break
                if time.monotonic()>=end:raise TimeoutError("container exit with blocked diagnostics")
                time.sleep(.05)
            diagnostics.append(Diagnostics(child.diagnostics_stream,framed=True));child.wait(timeout=4)
            passed("blocked-diagnostic-consumer",{"successful_product_calls":200,"container_exit":state["ExitCode"],"daemon_client_exit":child.returncode})
        service.stdin.write(b"q");service.stdin.flush();service.wait(timeout=4);assert service.returncode==0;evidence["status"]="PASS"
    except BaseException as e:evidence["status"]="FAIL";evidence["error"]=repr(e);raise
    finally:
        signal.alarm(0)
        for child in children+([service] if service else []):
            if child.poll() is None:child.kill();child.wait(timeout=3)
        for name in names:subprocess.run(["docker","rm","-f",name],check=True,stdout=subprocess.DEVNULL)
        for d in diagnostics:d.close()
        evidence["diagnostics"]=[{"records":d.records,"overflow":d.overflow}for d in diagnostics];evidence["cleanup"]="PASS"
        evidence["source_head"]=subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip();evidence["image"]=args.image
        (args.output/"result.json").write_text(json.dumps(evidence,indent=2)+"\n")
if __name__=="__main__":main()
