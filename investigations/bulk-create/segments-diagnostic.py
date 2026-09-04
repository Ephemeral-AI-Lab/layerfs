#!/usr/bin/env python3
import fcntl, hashlib, json, pathlib, subprocess, time
ROOT=pathlib.Path(__file__).resolve().parents[2]
OUT=ROOT/'investigations/bulk-create/evidence/segments-500-s1'
with (ROOT.parent/'layerfs/benchmark-results/fs-bench-pro/phase1-v013/measurement.lock').open('r') as lock:
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    OUT.mkdir()
    rows=[]
    for label,argv in [
        ('source',['git','rev-parse','HEAD']),
        ('build',['rustc','--edition=2021','-O','investigations/bulk-create/segments.rs','-o',OUT/'segments']),
        ('performance',['/usr/bin/time','-l',OUT/'segments',OUT/'scratch'])]:
        start=time.monotonic_ns()
        p=subprocess.run([str(x) for x in argv],cwd=ROOT,capture_output=True,timeout=600)
        (OUT/(label+'.stdout')).write_bytes(p.stdout);(OUT/(label+'.stderr')).write_bytes(p.stderr)
        rows.append(dict(label=label,argv=[str(x) for x in argv],elapsed_ns=time.monotonic_ns()-start,exit_code=p.returncode))
        (OUT/'commands.json').write_text(json.dumps(rows,indent=2)+'\n')
        if p.returncode: break
    (OUT/'evidence.sha256').write_text('\n'.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name for p in sorted(OUT.iterdir()) if p.is_file() and p.name!='evidence.sha256')+'\n')
    assert p.returncode==0
