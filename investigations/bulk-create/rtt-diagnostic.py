#!/usr/bin/env python3
import fcntl, hashlib, json, pathlib, subprocess, sys, time, uuid
ROOT=pathlib.Path(__file__).resolve().parents[2]
OUT=ROOT/'investigations/bulk-create/evidence/rtt-10k'
IMAGE='sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e'
with (ROOT.parent/'layerfs/benchmark-results/fs-bench-pro/phase1-v013/measurement.lock').open('r') as lock:
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB);OUT.mkdir();rows=[]
    def run(label,argv):
        start=time.monotonic_ns();p=subprocess.run([str(x) for x in argv],cwd=ROOT,capture_output=True,timeout=60)
        (OUT/(label+'.stdout')).write_bytes(p.stdout);(OUT/(label+'.stderr')).write_bytes(p.stderr)
        rows.append(dict(label=label,argv=[str(x) for x in argv],elapsed_ns=time.monotonic_ns()-start,exit_code=p.returncode))
        (OUT/'commands.json').write_text(json.dumps(rows,indent=2)+'\n');p.check_returncode()
    name='layerfs-feasibility-rtt-'+uuid.uuid4().hex[:8]
    script=ROOT/'investigations/bulk-create/rtt.py'
    server=None
    try:
        run('source',['git','rev-parse','HEAD'])
        run('create',['docker','run','-d','--name',name,'--cpus','2','--memory','2g','--memory-swap','2g','--pids-limit','256','--entrypoint','sleep',IMAGE,'infinity'])
        run('copy',['docker','cp',script,name+':/rtt.py'])
        run('environment',['docker','inspect',name])
        server=subprocess.Popen([sys.executable,str(script),'server'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        port=server.stdout.readline().strip()
        run('host-boundary',['docker','exec',name,'python3','/rtt.py','host.docker.internal',port])
        stdout,stderr=server.communicate(timeout=30)
        (OUT/'server.json').write_text(json.dumps(dict(exit_code=server.returncode,stdout=stdout,stderr=stderr))+'\n');assert server.returncode==0
        run('linux-loopback',['docker','exec',name,'python3','/rtt.py','local'])
    finally:
        if server and server.poll() is None: server.kill();server.communicate()
        run('cleanup',['docker','rm','-fv',name])
        run('remaining',['docker','ps','-aq','--filter','name='+name])
        (OUT/'evidence.sha256').write_text('\n'.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name for p in sorted(OUT.iterdir()) if p.is_file() and p.name!='evidence.sha256')+'\n')
