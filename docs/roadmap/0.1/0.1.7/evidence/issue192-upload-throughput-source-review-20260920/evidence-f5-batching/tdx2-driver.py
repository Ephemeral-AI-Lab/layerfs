import json, os, select, subprocess, time, sys
ROOT="/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs/core"
PROBE=f"{ROOT}/target/release/examples/transport_probe"
PUB=f"{ROOT}/target/release/examples/public_key"
IMAGE="layerfs-tdx2-probe"
LIMITS=["--rm","--label=io.layerfs.task=issue192","--log-driver=none","--read-only",
        "--cap-drop=ALL","--security-opt=no-new-privileges","--cpus=1","--memory=128m",
        "--memory-swap=128m","--pids-limit=16"]

def public(private):
    return subprocess.check_output([PUB], env=os.environ|{"LAYERFS_PRIVATE_KEY":private}, text=True).strip()

def case(carrier, direction, streams):
    server_priv=os.urandom(32).hex(); client_priv=os.urandom(32).hex()
    spub, cpub = public(server_priv), public(client_priv)
    env=os.environ|{"LAYERFS_PRIVATE_KEY":server_priv,"LAYERFS_PEER_KEY":cpub}
    log=open(f"/tmp/tdx2/server-{carrier}-{direction}-{streams}.log","w")
    server=subprocess.Popen([PROBE,"server",carrier,direction,str(streams),"perf"],env=env,
                            stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True)
    def line(timeout=10):
        r,_,_=select.select([server.stdout],[],[],timeout)
        assert r,"server readiness"
        v=server.stdout.readline().strip(); assert v,v; return v
    listen=line(); port=listen.split()[1]
    names=[]; procs=[]
    for index in range(streams):
        name=f"tdx2-{carrier}-{direction}-{streams}-{index}"
        names.append(name)
        envs=[]
        for key,value in (("LAYERFS_PRIVATE_KEY",client_priv),("LAYERFS_SERVER_KEY",spub),
                          ("LAYERFS_ENDPOINT",f"host.docker.internal:{port}")):
            envs += ["-e",f"{key}={value}"]
        procs.append(subprocess.Popen(["docker","run","--name",name,*LIMITS,*envs,IMAGE,
                     "client",carrier,direction,str(index),"perf"],
                     stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True))
    # the server accepts and handshakes before it prints READY, so the clients
    # must already be connecting when READY is expected
    for _ in range(streams): line()
    time.sleep(0.2)
    t0=time.monotonic()
    server.stdin.write("go\n"); server.stdin.flush()
    raw=server.stdout.readline().strip()
    wall=time.monotonic()-t0
    for p in procs:
        out=p.communicate(timeout=60)[0]
        if p.returncode!=0: print("  client failed:",out[:200],file=sys.stderr)
    server.wait(timeout=5)
    for n in names: subprocess.run(["docker","rm","-f",n],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    log.close()
    r=json.loads(raw)
    return {"case":f"{carrier}-{direction}-{streams}","payload_bytes":r["payload_bytes"],
            "transfer_seconds":round(r["transfer_ns"]/1e9,4),
            "GBps":round((r["payload_bytes"]/1e9)/(r["transfer_ns"]/1e9),4),
            "us_per_16KiB_frame":round(r["transfer_ns"]/1e3/r["payload_chunks"],3),
            "wall_seconds":round(wall,3)}

print("case                        payload        sec     GB/s   us/frame")
for carrier,direction,streams in [("tcp","upload",1),("noise","upload",1),("noise","upload",2)]:
    row=case(carrier,direction,streams)
    print(f"{row['case']:<27} {row['payload_bytes']:>12} {row['transfer_seconds']:>8.3f} {row['GBps']:>8.3f} {row['us_per_16KiB_frame']:>10.2f}")
