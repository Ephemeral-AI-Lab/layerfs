#!/usr/bin/env python3
"""Real daemon/service error and admission cases; no benchmark timings."""
import argparse, json, os, socket, struct, subprocess, tempfile, time, sqlite3, hashlib, threading
from pathlib import Path
from docker_route import TARGET, ROOT, Diagnostics, start_daemon, public, frame, request, receive, exchange

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--image",required=True);parser.add_argument("--output",type=Path,required=True);parser.add_argument("--selection",choices=["errors","admission","blocked-stderr","cleanup","slow","lost-response","additional"],default="errors");args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    evidence={"selection":args.selection,"kind":"functional-deployment","performance":"NOT_RUN","cases":[],"status":"INCOMPLETE"};children=[];service=None;names=[];diagnostics=[]
    def passed(name,detail):evidence["cases"].append({"name":name,"status":"PASS","detail":detail})
    try:
      with tempfile.TemporaryDirectory(prefix="layerfs-issue192-faults-") as temp:
        fixture=json.loads(subprocess.check_output([TARGET/"debug/examples/prepare_store",Path(temp)/"store.sqlite"],text=True));root=bytes.fromhex(fixture["file"]);tree=bytes.fromhex(fixture["root"])
        server_key=os.urandom(32).hex();keys=[os.urandom(32).hex(),os.urandom(32).hex()];pub=[public(k)for k in keys];server_public=public(server_key)
        with socket.socket() as s:s.bind(("127.0.0.1",0));port=s.getsockname()[1]
        env=os.environ.copy();env.update(LAYERFS_PRIVATE_KEY=server_key,LAYERFS_PEERS=f"1,{pub[0]},{int(time.time())+3600},31;2,{pub[1]},{int(time.time())+3600},1",LAYERFS_STORE=str(Path(temp)/"store.sqlite"),LAYERFS_LISTEN=f"0.0.0.0:{port}",LAYERFS_TELEMETRY="off")
        service=subprocess.Popen([TARGET/"debug/layerfs-service"],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE);assert b"ready" in service.stderr.readline();diagnostics.append(Diagnostics(service.stderr))
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
        elif args.selection=="admission":
            holders=[]
            # Each completed read establishes the real daemon connection. A
            # partially supplied next mutation then holds the sole service slot.
            for _ in range(4):
                child=spawn();result=exchange(child,1,1,root+struct.pack(">QQ",0,8));assert result[0]==6;holders.append(child)
            excess=spawn();excess.stdin.write(request(1,1,root+struct.pack(">QQ",0,8)));excess.stdin.flush();excess.wait(timeout=3);assert excess.returncode!=0;passed("C-plus-one",{"established":4,"excess_refused":True})
            holders[0].stdin.write(request(2,3,struct.pack(">Q",300000))+frame(3,2,b"held"));holders[0].stdin.flush()
            # One scheduling turn gives the authenticated BEGIN to the host.
            time.sleep(.1)
            result=exchange(holders[1],2,3,struct.pack(">Q",0));assert result[:2]==(7,b"\4\0\0"),result;passed("A-plus-one-Q-zero",{"active":1,"excess_code":"Capacity"})
            for child in holders:child.stdin.close()
            for child in holders:child.wait(timeout=3)
        elif args.selection=="cleanup":
            connection=sqlite3.connect(str(Path(temp)/"store.sqlite"));connection.execute("PRAGMA journal_mode=MEMORY");connection.execute("PRAGMA synchronous=OFF")
            connection.execute("CREATE TRIGGER refuse_cleanup BEFORE DELETE ON object_packs BEGIN SELECT RAISE(FAIL, 'external cleanup fault'); END");connection.commit()
            payload=hashlib.shake_256(b"issue192-cleanup-fault-v1").digest(6*1024*1024)
            child=spawn();result=exchange(child,1,3,struct.pack(">Q",8*1024*1024),payload);assert result[0]==7 and result[1][2]!=0,result;child.stdin.close();child.wait(timeout=3)
            passed("cleanup-failure-retains-original",{"code":result[1][0],"unknown":bool(result[1][1]),"cleanup_code":result[1][2],"input_sha256":hashlib.sha256(payload).hexdigest()})
            child=spawn();result=exchange(child,1,3,struct.pack(">Q",0));assert result[0]==7 and result[1][0]==5,result;child.stdin.close();child.wait(timeout=3);passed("uninspected-store-refuses-new-mutation",{"code":"Ownership"})
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
            peer=subprocess.check_output([TARGET/"debug/examples/fault_peer"],env=peer_env,text=True,timeout=3);passed("authenticated-malformed-peer",{"output":peer.strip(),"scope":"external host peer; native service parser, daemon bypassed"})
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
        for child in children+([service] if service else []):
            if child.poll() is None:child.kill();child.wait(timeout=3)
        for name in names:subprocess.run(["docker","rm","-f",name],check=True,stdout=subprocess.DEVNULL)
        for d in diagnostics:d.close()
        evidence["diagnostics"]=[{"records":d.records,"overflow":d.overflow}for d in diagnostics];evidence["cleanup"]="PASS"
        evidence["source_head"]=subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip();evidence["image"]=args.image
        (args.output/"result.json").write_text(json.dumps(evidence,indent=2)+"\n")
if __name__=="__main__":main()
