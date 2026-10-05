#!/usr/bin/env python3
"""External functional deployment proof; not a performance/benchmark driver."""
import argparse, hashlib, json, os, select, socket, struct, subprocess, tempfile, threading, time, shutil
from pathlib import Path
ROOT = Path(__file__).resolve().parents[4]
TARGET = ROOT / "core/target"
PROFILE = os.environ.get("LAYERFS_PROOF_PROFILE", "debug")
if PROFILE not in ("debug", "release"):
    raise ValueError("unsupported proof build profile")
BIN = TARGET / PROFILE

def prepared_fixture(destination):
    """Reuse one closed master per exact fixture executable; copy writable bytes."""
    executable=BIN/"examples/prepare_store"
    key=hashlib.sha256(executable.read_bytes()).hexdigest()
    folder=TARGET/"issue192-prepared"/key
    manifest=folder/"fixture.json";master=folder/"store.sqlite"
    reused=folder.exists()
    if not reused:
        folder.mkdir(parents=True)
        fixture=json.loads(subprocess.check_output([executable,master],text=True))
        digest=hashlib.sha256(master.read_bytes()).hexdigest()
        manifest.write_text(json.dumps({"binary_sha256":key,"store_sha256":digest,"fixture":fixture},indent=2)+"\n")
        master.chmod(0o400)
    recorded=json.loads(manifest.read_text())
    assert recorded["binary_sha256"]==key and hashlib.sha256(master.read_bytes()).hexdigest()==recorded["store_sha256"], "prepared master identity"
    shutil.copyfile(master,destination)
    return recorded["fixture"] | {"setup":{"master_sha256":recorded["store_sha256"],"executable_sha256":key,"reused":reused,"clone_method":"closed independent writable byte copy; not an OS cache claim"}}

def verify_artifacts(image):
    """Require a current-source manifest before a new product-route proof."""
    path=os.environ.get("LAYERFS_PROOF_IDENTITIES")
    if not path: raise ValueError("LAYERFS_PROOF_IDENTITIES is required")
    recorded=json.loads(Path(path).read_text())
    assert recorded["profile"]==PROFILE
    for name,digest in recorded["runtime"].items():
        assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest, name
    for name,digest in recorded["binaries"].items():
        assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest, name
    selected=json.loads(subprocess.check_output(["docker","image","inspect",image],text=True))[0]
    assert selected["Id"]==recorded["image_id"]
    assert selected["Config"]["Labels"]["org.layerfs.binary-sha256"]==recorded["linux_daemon_sha256"]
    return {"manifest":str(Path(path).resolve()),"image_id":selected["Id"],"profile":PROFILE,"runtime_sha256":recorded["runtime_sha256"],"binaries":recorded["binaries"]}

def frame(kind, identity, data=b""):
    return b"LFB1" + struct.pack(">BBHQI", kind, 0, 0, identity, len(data)) + data
def request(identity, opcode, data, store=1, deadline_ms=10000,response_bytes=64*1024*1024):
    return frame(2, identity, struct.pack(">QIHIQB", 1, store, 1, deadline_ms, response_bytes, opcode)+data)
def read_exact(fd, count, end):
    output=bytearray()
    while len(output)<count:
        if not select.select([fd],[],[],max(0,end-time.monotonic()))[0]: raise TimeoutError("result deadline")
        part=os.read(fd,count-len(output))
        if not part: raise EOFError("terminal response missing")
        output.extend(part)
    return bytes(output)
def receive(process,timeout=12):
    end=time.monotonic()+timeout; digest=hashlib.sha256(); length=0
    while True:
        h=read_exact(process.stdout.fileno(),20,end)
        assert h[:4]==b"LFB1"
        kind,flags,reserved,identity,n=struct.unpack(">BBHQI",h[4:]); assert flags==reserved==0 and n<=32768
        body=read_exact(process.stdout.fileno(),n,end)
        if kind==5: digest.update(body);length+=len(body);continue
        assert kind in (6,7)
        return kind,body,length,digest.hexdigest()
def exchange(process,identity,opcode,metadata,body=b"",store=1):
    errors=[]
    def send():
        try:
            process.stdin.write(request(identity,opcode,metadata,store))
            for offset in range(0,len(body),16384):process.stdin.write(frame(3,identity,body[offset:offset+16384]))
            process.stdin.write(frame(4,identity,struct.pack(">Q",len(body))));process.stdin.flush()
        except (BrokenPipeError,OSError) as e:errors.append(type(e).__name__)
    writer=threading.Thread(target=send,daemon=True);writer.start()
    result=receive(process);writer.join(timeout=1);assert not writer.is_alive()
    return result
class Diagnostics:
    def __init__(self,stream,framed=False,native=True):
        self.stream=stream;self.framed=framed;self.native=native;self.records=[];self.bytes=0;self.overflow=0
        self.thread=threading.Thread(target=self.run,daemon=True);self.thread.start()
    def run(self):
        pending=bytearray()
        def parts():
            if not self.framed:
                while part:=os.read(self.stream.fileno(),4096):yield part
                return
            while True:
                header=bytearray()
                while len(header)<8:
                    part=os.read(self.stream.fileno(),8-len(header))
                    if not part:return
                    header.extend(part)
                if header[:4]!=b"\2\0\0\0":self.overflow+=1;return
                size=struct.unpack(">I",header[4:])[0]
                if size>65536:self.overflow+=1;return
                while size:
                    part=os.read(self.stream.fileno(),min(size,4096))
                    if not part:self.overflow+=1;return
                    size-=len(part);yield part
        for part in parts():
            pending.extend(part)
            if len(pending)>32768:self.overflow+=1;pending.clear();continue
            while b"\n" in pending:
                line,_,pending=pending.partition(b"\n")
                if self.bytes+len(line)<=65536 and len(self.records)<128:self.records.append(line.decode(errors="replace"));self.bytes+=len(line)
                else:self.overflow+=1
    def close(self):self.thread.join(timeout=1)
def start_daemon(command,env,diagnostics,drain=True):
    # Docker's ordinary attach multiplexes stdout/stderr on one socket. Give
    # product output and diagnostics separate attach connections so a blocked
    # diagnostic consumer cannot stall the CLI's product-output demultiplexer.
    child=subprocess.Popen(command[:2]+["--attach=stdin","--attach=stdout"]+command[2:],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    diagnostics.append(Diagnostics(child.stderr,native=False))
    endpoint=subprocess.check_output(["docker","context","inspect","--format","{{.Endpoints.docker.Host}}"],text=True).strip();assert endpoint.startswith("unix://")
    name=command[command.index("--name")+1];deadline=time.monotonic()+3
    while True:
        stream=socket.socket(socket.AF_UNIX);stream.settimeout(3);stream.connect(endpoint[7:]);stream.sendall(f"POST /containers/{name}/attach?stream=1&stdout=0&stderr=1 HTTP/1.1\r\nHost: docker\r\nConnection: Upgrade\r\nUpgrade: tcp\r\nContent-Length: 0\r\n\r\n".encode())
        header=bytearray()
        while not header.endswith(b"\r\n\r\n"):
            part=stream.recv(1);assert part and len(header)<8192;header.extend(part)
        if header.startswith(b"HTTP/1.1 101"):break
        stream.close()
        # Authentication/admission may refuse the daemon before its optional
        # stderr attachment exists. Keep the real process outcome, not a fake
        # harness failure or a log-file fallback.
        state=subprocess.run(["docker","inspect","--format","{{.State.Status}}",name],capture_output=True,text=True)
        if state.returncode==0 and state.stdout.strip() in ("exited","dead"):
            child.diagnostics_stream=None
            return child
        if not header.startswith(b"HTTP/1.1 404")or time.monotonic()>=deadline:raise RuntimeError("independent diagnostics attach failed: "+header.decode(errors="replace"))
        time.sleep(.01)
    stream.settimeout(None);child.diagnostics_stream=stream
    if drain:diagnostics.append(Diagnostics(stream,framed=True))
    return child
def public(private):
    env=os.environ.copy();env["LAYERFS_PRIVATE_KEY"]=private
    return subprocess.check_output([BIN/"examples/public_key"],env=env,text=True).strip()
def main():
    p=argparse.ArgumentParser();p.add_argument("--image",required=True);p.add_argument("--output",type=Path,required=True);p.add_argument("--telemetry",default="off");args=p.parse_args()
    args.output.mkdir(parents=True,exist_ok=False)
    evidence={"kind":"functional-deployment","performance":"NOT_RUN","image":args.image,"telemetry":args.telemetry,"cases":[],"cleanup":"INCOMPLETE"}
    evidence["artifacts"]=verify_artifacts(args.image)
    service=None;daemon=None;diagnostics=[];container=None
    try:
        with tempfile.TemporaryDirectory(prefix="layerfs-issue192-") as temp:
            path=Path(temp)/"store.sqlite"
            fixture=prepared_fixture(path);evidence["fixture"]=fixture
            direct_path=Path(temp)/"direct.sqlite";shutil.copyfile(path,direct_path);evidence["fixture_copy"]="closed independent byte copy"
            server_key=os.urandom(32).hex();client_key=os.urandom(32).hex();server_public=public(server_key);client_public=public(client_key)
            with socket.socket() as s:s.bind(("127.0.0.1",0));port=s.getsockname()[1]
            env=os.environ.copy();env.update(LAYERFS_PRIVATE_KEY=server_key,LAYERFS_PEERS=f"1,{client_public},{int(time.time())+3600},31",LAYERFS_STORE=str(path),LAYERFS_LISTEN=f"0.0.0.0:{port}",LAYERFS_TELEMETRY=args.telemetry,LAYERFS_RUN_ID="192",LAYERFS_NAMESPACE="1",LAYERFS_TELEMETRY_DIRECTORY=str(Path(temp)/"service-telemetry"))
            service=subprocess.Popen([BIN/"layerfs-server"],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            evidence["service_pid"]=service.pid
            line=service.stderr.readline();assert b"ready" in line,line
            diagnostics.append(Diagnostics(service.stderr));evidence["ready"]=line.decode().strip()
            name=f"layerfs-issue192-{os.getpid()}";container=name
            command=["docker","run","--name",name,"--label=io.layerfs.task=issue192","--log-driver=none","--read-only","--cap-drop=ALL","--security-opt=no-new-privileges","--cpus=1","--memory=128m","--memory-swap=128m","--pids-limit=16","-i"]
            if args.telemetry in ("local","both"):command.remove("--read-only")
            for key,value in {"LAYERFS_PRIVATE_KEY":client_key,"LAYERFS_SERVER_KEY":server_public,"LAYERFS_ENDPOINT":f"host.docker.internal:{port}","LAYERFS_SELECTOR":"1","LAYERFS_TELEMETRY":args.telemetry,"LAYERFS_RUN_ID":"192","LAYERFS_NAMESPACE":"2","LAYERFS_TELEMETRY_DIRECTORY":"/logs/run"}.items():
                env[key]=value;command += ["-e",key]
            command += [args.image]
            daemon=start_daemon(command,env,diagnostics)
            payload=bytes(i%251 for i in range(500000));result=exchange(daemon,1,3,struct.pack(">Q",len(payload)),payload)
            assert result[0]==6 and result[1][0]==2,result;root=result[1][1:33];file_counts=list(struct.unpack(">QQ",result[1][41:57]))
            evidence["cases"].append({"id":"V02","selection":"500000-byte construct","status":"PASS","root":root.hex()})
            result=exchange(daemon,2,1,root+struct.pack(">QQ",0,len(payload)))
            assert result[0]==6 and result[2]==len(payload) and result[3]==hashlib.sha256(payload).hexdigest(),result
            evidence["cases"].append({"id":"V03","selection":"exact full readback","status":"PASS"})
            result=exchange(daemon,3,4,root+struct.pack(">QHQQQ",len(payload),1,10,15,3),b"new")
            assert result[0]==6 and result[1][0]==2,result;edit_counts=list(struct.unpack(">QQ",result[1][41:57]));edited=result[1][1:33];expected=payload[:10]+b"new"+payload[15:]
            result=exchange(daemon,4,1,edited+struct.pack(">QQ",0,len(expected)))
            assert result[0]==6 and result[3]==hashlib.sha256(expected).hexdigest(),result
            evidence["cases"].append({"id":"V05","selection":"replace and readback","status":"PASS"})
            result=exchange(daemon,5,2,edited+b"\0");assert result[0]==6 and result[1][0]==3,result
            base=bytes.fromhex(fixture["root"]);scope=bytes.fromhex(fixture["scope"]);meta=bytes.fromhex(fixture["metadata"])
            update=base+scope+struct.pack(">QHHQB",1,0,1,2,1)+edited+meta
            result=exchange(daemon,6,5,update);assert result[0]==6 and result[1][0]==7,result;tree=result[1][1:33];tree_counts=list(struct.unpack(">QQ",result[1][33:49]))
            result=exchange(daemon,7,2,tree+b"\1"+struct.pack(">H",1)+b"f");assert result[0]==6 and result[1][0]==4,result
            assert result[1][18:50]==edited,result
            assert result[1][82:]==struct.pack(">IqI",0o777,1700000000,7),result
            evidence["cases"].append({"id":"V06","selection":"existing inode content attachment and stat","status":"PASS"})
            result=exchange(daemon,8,2,tree+b"\2"+struct.pack(">HHHI",0,0,1,16384));assert result[0]==6 and result[1][0]==5,result
            assert result[1][1:4]==b"\0\1f" and struct.unpack(">H",result[1][4:6])[0]==1,result
            result=exchange(daemon,9,2,tree+b"\2"+struct.pack(">HH",0,1)+b"f"+struct.pack(">HI",128,16384));assert result[0]==6 and result[1][0]==5,result
            assert result[1]==b"\5\0\0\0\2"+struct.pack(">H",1)+b"g"+struct.pack(">QH",4,4)+b"link"+struct.pack(">Q",3),result
            result=exchange(daemon,10,2,tree+b"\3"+struct.pack(">H",4)+b"link");assert result[0]==6 and result[1]==b"\6\0\1f",result
            evidence["cases"].append({"id":"V04","selection":"File, Stat, two paged List calls, Readlink","status":"PASS"})
            for identity,length in enumerate([0,131071,131072,131073],11):
                data=bytes(i%251 for i in range(length));result=exchange(daemon,identity,3,struct.pack(">Q",length),data);assert result[0]==6 and struct.unpack(">Q",result[1][33:41])[0]==length,result
            result=exchange(daemon,15,1,root+struct.pack(">QQ",500000,500000));assert result[0]==6 and result[2]==0,result
            result=exchange(daemon,16,4,root+struct.pack(">QH",len(payload),0));assert result[0]==6 and result[1][1:33]==root,result
            result=exchange(daemon,17,4,root+struct.pack(">QHQQQ",len(payload),1,0,0,3),b"add");assert result[0]==6,result
            inserted=result[1][1:33]
            result=exchange(daemon,18,4,inserted+struct.pack(">QHQQQ",len(payload)+3,1,0,3,0));assert result[0]==6,result
            result=exchange(daemon,19,1,result[1][1:33]+struct.pack(">QQ",0,len(payload)));assert result[3]==hashlib.sha256(payload).hexdigest(),result
            evidence["cases"].append({"id":"V02","selection":"empty and 131071/131072/131073 cutoff boundaries","status":"PASS"})
            evidence["cases"].append({"id":"V05","selection":"no-op, insert and delete","status":"PASS"})
            result=exchange(daemon,20,1,root+struct.pack(">QQ",0,len(payload)));assert result[3]==hashlib.sha256(payload).hexdigest()
            saved=[]
            for identity,data in [(21,b"first saved file"),(22,b"second saved file")]:
                value=exchange(daemon,identity,3,struct.pack(">Q",len(data)),data);assert value[0]==6,value;saved.append(value[1][1:33])
            update=tree+scope+struct.pack(">QHH",1,0,2)+b"".join(struct.pack(">QB",serial,1)+content+meta for serial,content in zip((2,4),saved))
            value=exchange(daemon,23,5,update);assert value[0]==6,value;attached=value[1][1:33]
            for identity,path,content in [(24,b"f",saved[0]),(25,b"g",saved[1])]:
                value=exchange(daemon,identity,2,attached+b"\1"+struct.pack(">H",len(path))+path);assert value[0]==6 and value[1][18:50]==content,value
            evidence["cases"].append({"id":"V07","selection":"two independent file saves, then one tree attachment; two retained roots","status":"PASS","logical_exchanges":3,"roots":[r.hex()for r in saved],"tree":attached.hex(),"atomic_composite":False})
            inspect=json.loads(subprocess.check_output(["docker","inspect",name],text=True))[0]
            evidence["container"]={"id":inspect["Id"],"image":inspect["Image"],"labels":inspect["Config"]["Labels"],"nano_cpus":inspect["HostConfig"]["NanoCpus"],"memory_limit":inspect["HostConfig"]["Memory"],"pids_limit":inspect["HostConfig"]["PidsLimit"],"mounts":inspect["Mounts"],"logging":inspect["HostConfig"]["LogConfig"],"readonly":inspect["HostConfig"]["ReadonlyRootfs"],"devices":inspect["HostConfig"]["Devices"],"pid":inspect["State"]["Pid"],"network":inspect["NetworkSettings"]["Networks"]}
            assert inspect["Config"]["Labels"]["io.layerfs.task"]=="issue192"
            assert inspect["HostConfig"]["NanoCpus"]==1000000000 and inspect["HostConfig"]["Memory"]==128*1024*1024 and inspect["HostConfig"]["PidsLimit"]==16
            assert not inspect["Mounts"] and not inspect["HostConfig"]["Devices"]
            if args.telemetry not in ("local","both"):assert inspect["HostConfig"]["ReadonlyRootfs"]
            evidence["diff"]=subprocess.check_output(["docker","diff",name],text=True)
            if args.telemetry not in ("local","both"):assert not evidence["diff"]
            else:assert all(line[2:].startswith("/logs")for line in evidence["diff"].splitlines())
            assert inspect["HostConfig"]["LogConfig"]["Type"]=="none"
            evidence["observed_host_sockets"]=subprocess.check_output(["/usr/sbin/lsof","-a","-p",str(service.pid),"-iTCP","-nP"],text=True)
            docker_endpoint=subprocess.check_output(["docker","context","inspect","--format","{{.Endpoints.docker.Host}}"],text=True).strip()
            assert docker_endpoint.startswith("unix://")
            stats=subprocess.check_output(["curl","-sS","--max-time","3","--max-filesize","65536","--unix-socket",docker_endpoint[7:],f"http://localhost/containers/{inspect['Id']}/stats?stream=false&one-shot=true"])
            assert len(stats)<=65536
            stats=json.loads(stats)
            evidence["container_memory_observation"]={"scope":"enclosing container cgroup; not additive with process RSS","source":"Docker Engine stats one-shot","read":stats.get("read"),"memory_stats":stats.get("memory_stats"),"phase_peak":"unavailable; lifetime counters are not reset phase peaks"}

            evidence["cases"].append({"id":"V16" if args.telemetry not in ("local","both") else "ENV04","status":"PASS","selection":"no mounts/devices; primary read-only/no files or explicitly selected owned local telemetry"})
            daemon.stdin.close();daemon.wait(timeout=7)
            service.stdin.write(b"q");service.stdin.flush();service.wait(timeout=4)
            evidence["service_exit"]=service.returncode;evidence["daemon_exit"]=daemon.returncode
            assert service.returncode==daemon.returncode==0
            # Reopen the Store in a new native process and read via a new Docker daemon.
            service=subprocess.Popen([BIN/"layerfs-server"],env={**env,"LAYERFS_PRIVATE_KEY":server_key,"LAYERFS_TELEMETRY_DIRECTORY":str(Path(temp)/"service-telemetry")},stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            assert b"ready" in service.stderr.readline();diagnostics.append(Diagnostics(service.stderr))
            subprocess.run(["docker","rm",container],check=True,stdout=subprocess.DEVNULL)
            daemon=start_daemon(command,env,diagnostics)
            for identity,content,data in [(1,root,payload),(2,edited,expected),(3,saved[0],b"first saved file"),(4,saved[1],b"second saved file")]:
                value=exchange(daemon,identity,1,content+struct.pack(">QQ",0,len(data)));assert value[0]==6 and value[3]==hashlib.sha256(data).hexdigest(),value
            value=exchange(daemon,5,2,attached+b"\1"+struct.pack(">H",1)+b"g");assert value[0]==6 and value[1][18:50]==saved[1],value
            daemon.stdin.close();daemon.wait(timeout=4);service.stdin.write(b"q");service.stdin.flush();service.wait(timeout=4);assert daemon.returncode==service.returncode==0
            evidence["cases"].append({"id":"V14","selection":"new native service and new Docker daemon read old/new roots after ordinary Store reopen","status":"PASS","service_pid":service.pid})
            direct=json.loads(subprocess.check_output([BIN/"examples/direct",direct_path,fixture["root"],fixture["scope"],fixture["metadata"]],env=env,text=True))
            assert direct=={"file":root.hex(),"edited":edited.hex(),"tree":tree.hex(),"file_counts":file_counts,"edit_counts":edit_counts,"tree_counts":tree_counts},direct
            evidence["direct_parity"]=direct
            evidence["cases"].append({"id":"V17","selection":"all five operations, identical roots and save counts from an independent pristine Store copy","status":"PASS"})
            if args.telemetry in ("local","both"):
                local=Path(temp)/"daemon-telemetry";subprocess.run(["docker","cp",name+":/logs/run",str(local)],check=True,stdout=subprocess.DEVNULL)
                evidence["local_records"]={}
                for role,folder in [("daemon",local),("service",Path(temp)/"service-telemetry")]:
                    rows=[]
                    assert folder.stat().st_mode&0o077==0
                    for segment in folder.glob("segment-*.jsonl"):
                        assert segment.stat().st_size<=16*1024*1024 and segment.stat().st_mode&0o077==0
                        for line in segment.read_text().splitlines():assert line.startswith("LFT1 ");rows.append(json.loads(line[5:]))
                    assert any(r["kind"]=="operation"for r in rows) and any(r["kind"]=="resource"for r in rows)
                    evidence["local_records"][role]=rows
            for d in diagnostics:d.close()
            if args.telemetry in ("forward","both"):
                resource_roles=set()
                for d in diagnostics:
                    if not d.native:continue
                    reports=[json.loads(line[5:]) for line in d.records if line.startswith("LFT1 ")]
                    assert any(r.get("kind")=="operation" for r in reports), d.records
                    for r in reports:
                        if r.get("kind")=="resource":
                            resource_roles.add(r["role"])
                            assert r["rss"]>0
                            assert r["source"] in ("getrusage+libproc","procfs+sysconf") and r["probe_ns"] is not None and r["incarnation"]>0
                    assert d.overflow==0
                assert resource_roles=={1,2},resource_roles
                evidence["cases"].append({"id":"ENV03","selection":"both native producers forwarded valid bounded diagnostics","status":"PASS"})
            evidence["status"]="PASS"
    except BaseException as error:
        evidence["status"]="FAIL";evidence["error"]=repr(error);raise
    finally:
        for process in [daemon,service]:
            if process and process.poll() is None:process.kill();process.wait(timeout=3)
        if container:subprocess.run(["docker","rm","-f",container],check=True,stdout=subprocess.DEVNULL)
        for d in diagnostics:d.close()
        evidence["diagnostics"]=[{"records":d.records,"overflow":d.overflow}for d in diagnostics]
        evidence["cleanup"]="PASS"
        evidence["source_head"]=subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip()
        evidence["lock_sha256"]=hashlib.sha256((ROOT/"core/Cargo.lock").read_bytes()).hexdigest()
        for name,path in {"service":BIN/"layerfs-server","daemon":TARGET/"aarch64-unknown-linux-musl/debug/layerfs-daemon"}.items():evidence[name+"_sha256"]=hashlib.sha256(path.read_bytes()).hexdigest()
        (args.output/"result.json").write_text(json.dumps(evidence,indent=2)+"\n")
if __name__=="__main__":main()
