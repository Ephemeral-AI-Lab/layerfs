#!/usr/bin/env python3
"""One ownership experiment: existing public Host/FUSE route, small proof then 100k."""
import fcntl, hashlib, importlib.util, json, pathlib, subprocess, time, uuid
ROOT=pathlib.Path(__file__).resolve().parents[2]
ORIGINAL=ROOT.parent/'layerfs'
OUT=ROOT/'investigations/bulk-create/evidence/colocated-r1'
BASE='sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e'
CACHE=ORIGINAL/'target/phase1-prepared/03638fe317044f1e9c30324e80e38d8da9ea76a348697b596900839bd015ee98'
spec=importlib.util.spec_from_file_location('custody',ROOT/'benchmark/fs-bench-pro/sdk-edit-custody.py')
custody=importlib.util.module_from_spec(spec);spec.loader.exec_module(custody)
with (ORIGINAL/'benchmark-results/fs-bench-pro/phase1-v013/measurement.lock').open('r') as lock:
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    OUT.mkdir(); commands=[]
    def run(label,argv,timeout=600,stdin=None):
        print('start '+label,flush=True);start=time.monotonic_ns()
        with (OUT/(label+'.stdout')).open('xb') as stdout,(OUT/(label+'.stderr')).open('xb') as stderr:
            p=subprocess.run([str(x) for x in argv],cwd=ROOT,stdin=stdin,stdout=stdout,stderr=stderr,timeout=timeout)
        commands.append(dict(label=label,argv=[str(x) for x in argv],elapsed_ns=time.monotonic_ns()-start,exit_code=p.returncode))
        (OUT/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        print('finish '+label+' '+str(p.returncode),flush=True);p.check_returncode()
        return (OUT/(label+'.stdout')).read_text().strip()
    active=None
    try:
        source=run('source',['git','rev-parse','HEAD'])
        (OUT/'source-identity.json').write_text(json.dumps(custody.workspace_identity(source),indent=2)+'\n')
        base=run('base',['docker','image','inspect','layerfs-v013:fbf32e84662d'])
        assert json.loads(base)[0]['Id']==BASE
        run('archive',['git','archive','--format=tar','--output',OUT/'source.tar',source,'Cargo.toml','Cargo.lock','crates','tools','benchmark/fs-bench-pro','investigations/bulk-create/Dockerfile.colocated'])
        image='layerfs-colocated:'+source[:12]
        with (OUT/'source.tar').open('rb') as archive:
            run('build',['docker','build','--progress=plain','-f','investigations/bulk-create/Dockerfile.colocated','--build-arg','INVESTIGATION_SOURCE='+source,'-t',image,'-'],1200,archive)
        run('image',['docker','image','inspect',image])
        start=time.monotonic_ns();custody.verify_manifest(CACHE)
        metadata=json.loads((CACHE/'cache.json').read_text());prepared=OUT/'prepared';prepared.mkdir()
        clone=custody.clone_prepared(CACHE/'store/store.sqlite',prepared/'store.sqlite',metadata['store_sha256'])
        (prepared/'branch-id').write_bytes((CACHE/'store/branch-id').read_bytes())
        (OUT/'preparation.json').write_text(json.dumps(dict(cache=str(CACHE),validation_and_clone_ns=time.monotonic_ns()-start,clone=clone,cache_sha256=custody.sha(CACHE/'cache.json')),indent=2)+'\n')
        for tier,mode in [(1,'verify'),(500,'performance')]:
            label=f'tier-{tier}';active='layerfs-colocated-'+uuid.uuid4().hex[:12]
            run(label+'-create',['docker','run','-d','--name',active,'--cpus','2','--memory','2g','--memory-swap','2g','--pids-limit','256','--device','/dev/fuse','--cap-add','SYS_ADMIN','--security-opt','apparmor=unconfined','--mount','type=volume,destination=/data',image])
            run(label+'-environment',['docker','inspect',active])
            run(label+'-copy',['docker','cp',prepared,active+':/data/sample'])
            run(label+'-identity',['docker','exec',active,'sh','-c','sha256sum /usr/local/bin/fs-benchmark-pro /usr/local/bin/fs-benchmark-workload /data/sample/store.sqlite; rustc -Vv; stat -f -c %T /data; cat /proc/self/mountinfo'])
            stats='cat /sys/fs/cgroup/cpu.stat /sys/fs/cgroup/memory.peak /sys/fs/cgroup/memory.events /sys/fs/cgroup/memory.swap.current /sys/fs/cgroup/pids.current'
            run(label+'-before',['docker','exec',active,'sh','-c',stats])
            run(label+'-run',['docker','exec',active,'fs-benchmark-pro','workspace-run','/data/sample','/unused-input',f'tiny-bulk-create-{tier}','1',mode,'diagnostic-host-fuse'],600)
            run(label+'-after',['docker','exec',active,'sh','-c',stats])
            if tier==500:
                run(label+'-verify',['docker','exec',active,'fs-benchmark-pro','workspace-colocated-verify-existing','/data/sample',f'tiny-bulk-create-{tier}','1'],1800)
            run(label+'-proof-copy',['docker','cp',active+':/data/sample/canonical-verification',OUT/(label+'-canonical-verification')])
            run(label+'-cleanup-check',['docker','exec',active,'sh','-c','cat /proc/self/mountinfo; find /tmp/layerfs-runtime -type f 2>/dev/null; ps -eo pid,comm; du -sb /data/sample'])
            run(label+'-cleanup',['docker','rm','-fv',active]);active=None
    finally:
        if active:
            # Preserve reached state on failure; no workers survive diagnostic cleanup.
            try: run('failed-state-copy',['docker','cp',active+':/data/sample',OUT/'failed-state'])
            finally: run('failed-cleanup',['docker','rm','-fv',active])
        files=[p for p in sorted(OUT.rglob('*')) if p.is_file() and p.name!='evidence.sha256' and 'failed-state' not in p.parts and 'prepared' not in p.parts and p.name!='source.tar']
        (OUT/'evidence.sha256').write_text('\n'.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(OUT)) for p in files)+'\n')
