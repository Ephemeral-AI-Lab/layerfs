#!/usr/bin/env python3
"""Focused immutable-CAS retention check, bounded private container, no campaign lock."""
import hashlib,json,pathlib,subprocess,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[2]
OUT=ROOT/'investigations/bulk-create/evidence/container-immutable-cas-r2'
BASE='sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e'
OUT.mkdir();commands=[];active=None
def run(label,argv,timeout=600):
    print('start '+label,flush=True);started=time.monotonic_ns()
    with (OUT/(label+'.stdout')).open('xb') as out,(OUT/(label+'.stderr')).open('xb') as err:
        p=subprocess.run([str(x) for x in argv],cwd=ROOT,stdout=out,stderr=err,timeout=timeout)
    commands.append(dict(label=label,argv=[str(x) for x in argv],elapsed_ns=time.monotonic_ns()-started,exit_code=p.returncode))
    (OUT/'commands.json').write_text(json.dumps(commands,indent=2)+'\n');p.check_returncode();print('finish '+label,flush=True)
try:
    source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    (OUT/'source.json').write_text(json.dumps(dict(source=source,profile='private-2cpu-2g-container',purpose='verification-only; no performance claim',phase1_lock=False))+'\n')
    run('archive',['git','archive','--format=tar','--output',OUT/'source.tar',source,'Cargo.toml','Cargo.lock','crates','tools','benchmark/fs-bench-pro','investigations/bulk-create/sync-source.py'])
    active='layerfs-immutable-'+uuid.uuid4().hex[:12]
    run('create',['docker','run','-d','--name',active,'--cpus','2','--memory','2g','--memory-swap','2g','--pids-limit','256','--mount','type=volume,destination=/data','--entrypoint','sleep',BASE,'infinity'])
    run('environment',['docker','inspect',active])
    run('reuse-private-build',['docker','cp',str(ROOT/'investigations/bulk-create/evidence/container-dense-delete-10-s1/failed-state')+'/.',active+':/data'])
    run('copy-source',['docker','cp',OUT/'source.tar',active+':/data/new-source.tar'])
    run('focused-test',['docker','exec','-e','CARGO_HOME=/data/cargo','-e','CARGO_TARGET_DIR=/data/target','-e','CARGO_BUILD_JOBS=2','-e','LAYERFS_EXPERIMENT_DENSE_DELETE=1','-e','LAYERFS_EXPERIMENT_FINAL_NEW_REFS=1',active,'sh','-c','mkdir /data/update && tar -xf /data/new-source.tar -C /data/update && python3 /data/update/investigations/bulk-create/sync-source.py /data/update /data/source && cd /data/source && cargo test --offline --locked -p layerfs-workspace --features test-instrumentation --lib dense_delete_preserves_aliases_open_unlinked_and_old_root -- --nocapture'],1200)
    run('trace-regression',['docker','exec','-e','CARGO_HOME=/data/cargo','-e','CARGO_TARGET_DIR=/data/target','-e','CARGO_BUILD_JOBS=2',active,'sh','-c','cd /data/source && cargo test --offline --locked -p layerfs-layerstack-store --features test-instrumentation --lib snapshot_cache_reads_only_requested_authenticated_objects_and_reuses_them'],1200)
    run('resources',['docker','exec',active,'sh','-c','cat /sys/fs/cgroup/memory.peak /sys/fs/cgroup/memory.events /sys/fs/cgroup/memory.swap.current'])
finally:
    if active:
        run('cleanup',['docker','rm','-fv',active])
        run('remaining',['docker','ps','-aq','--filter','name=^/'+active+'$'])
    files=[p for p in sorted(OUT.iterdir()) if p.is_file() and p.name not in ['source.tar','evidence.sha256']]
    (OUT/'evidence.sha256').write_text('\n'.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name for p in files)+'\n')
