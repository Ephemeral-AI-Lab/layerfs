#!/usr/bin/env python3
"""One native Linux diagnostic, same sealed workload; never a LayerFS result."""
import fcntl, hashlib, json, pathlib, subprocess, time, uuid
ROOT = pathlib.Path(__file__).resolve().parents[2]
ORIGINAL = ROOT.parent / 'layerfs'
OUT = ROOT / 'investigations/bulk-create/evidence/native-500-s1-r2'
IMAGE = 'sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e'
with (ORIGINAL / 'benchmark-results/fs-bench-pro/phase1-v013/measurement.lock').open('r') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    OUT.mkdir()
    records = []
    def run(label, argv, timeout=600):
        start = time.monotonic_ns()
        p = subprocess.run([str(x) for x in argv], cwd=ROOT, capture_output=True, timeout=timeout)
        (OUT / (label+'.stdout')).write_bytes(p.stdout)
        (OUT / (label+'.stderr')).write_bytes(p.stderr)
        records.append(dict(label=label, argv=[str(x) for x in argv], elapsed_ns=time.monotonic_ns()-start, exit_code=p.returncode))
        (OUT/'commands.json').write_text(json.dumps(records, indent=2)+'\n')
        p.check_returncode()
        return p.stdout.decode().strip()
    name = 'layerfs-feasibility-'+uuid.uuid4().hex[:12]
    try:
        run('source', ['git','rev-parse','HEAD'])
        fixture = OUT.parent / 'native-500-s1/fixture'
        run('reused-preparation', ['shasum','-a','256',OUT.parent/'native-500-s1/prepare.stdout'])
        run('create',['docker','run','-d','--name',name,'--cpus','2','--memory','2g','--memory-swap','2g','--pids-limit','256','--mount','type=volume,destination=/native','--entrypoint','sleep',IMAGE,'infinity'])
        run('environment',['docker','inspect',name])
        run('copy',['docker','cp',str(fixture)+'/.',name+':/native'])
        run('prepare-root',['docker','exec',name,'sh','-c','chmod 0750 /native && touch -d @1700000000 /native'])
        run('helper-identity',['docker','exec',name,'sha256sum','/usr/local/bin/fs-benchmark-workload'])
        run('qualify',['docker','exec','-w','/native',name,'fs-benchmark-workload','workspace-verify-tree','tiny-bulk-create-500','1','0'])
        def stats(label):
            run(label,['docker','exec',name,'sh','-c','cat /sys/fs/cgroup/cpu.stat /sys/fs/cgroup/memory.peak /sys/fs/cgroup/memory.events /sys/fs/cgroup/memory.swap.current; stat -f -c %T /native'])
        stats('before')
        run('performance',['docker','exec','-w','/native',name,'fs-benchmark-workload','workspace-apply','tiny-bulk-create-500','1','0','performance'])
        stats('after')
        run('verify',['docker','exec','-w','/native',name,'fs-benchmark-workload','workspace-verify-tree','tiny-bulk-create-500','1','1'])
    finally:
        run('cleanup',['docker','rm','-fv',name])
        run('remaining',['docker','ps','-aq','--filter','name='+name])
        manifest=[]
        for p in sorted(OUT.iterdir()):
            if p.is_file() and p.name != 'evidence.sha256':
                manifest.append(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name)
        (OUT/'evidence.sha256').write_text('\n'.join(manifest)+'\n')
