#!/usr/bin/env python3
"""Exploratory container-only frontier A/B: create and delete, no Phase 1 lock."""
import hashlib, json, pathlib, subprocess, tarfile, time, tomllib, uuid
ROOT=pathlib.Path(__file__).resolve().parents[2]
OUT=ROOT/'investigations/bulk-create/evidence/container-final-create-10-s1'
BASE='sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e'
PREPARED=ROOT/'investigations/bulk-create/evidence/colocated-r1/prepared'
OUT.mkdir();commands=[];active=None
(OUT/'scope.json').write_text(json.dumps(dict(profile='exploratory-linux-colocated-host-fuse',coordination='user-authorized concurrent containers; no shared campaign lock',limits=dict(cpus=2,memory_bytes=2147483648,memory_swap_bytes=2147483648,pids=256),cases=['tiny-bulk-create-10'],seed=1,mode='verify',warning='shared physical hardware; not frozen-profile or release evidence'),indent=2)+'\n')
def run(label,argv,timeout=600):
    print('start '+label,flush=True);start=time.monotonic_ns()
    with (OUT/(label+'.stdout')).open('xb') as stdout,(OUT/(label+'.stderr')).open('xb') as stderr:
        p=subprocess.run([str(x) for x in argv],cwd=ROOT,stdout=stdout,stderr=stderr,timeout=timeout)
    commands.append(dict(label=label,argv=[str(x) for x in argv],elapsed_ns=time.monotonic_ns()-start,exit_code=p.returncode))
    (OUT/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
    print('finish '+label+' '+str(p.returncode),flush=True);p.check_returncode()
    return (OUT/(label+'.stdout')).read_text().strip()
def start(label):
    global active
    active='layerfs-explore-'+uuid.uuid4().hex[:12]
    run(label+'-create',['docker','run','-d','--name',active,'--cpus','2','--memory','2g','--memory-swap','2g','--pids-limit','256','--device','/dev/fuse','--cap-add','SYS_ADMIN','--security-opt','apparmor=unconfined','--mount','type=volume,destination=/data','--entrypoint','sleep',BASE,'infinity'])
    run(label+'-environment',['docker','inspect',active])
try:
    source=run('source',['git','rev-parse','HEAD'])
    import shutil
    prior=OUT.parent/'container-frontier-10-s1-r5'
    run('archive',['git','archive','--format=tar','--output',OUT/'source.tar',source,'Cargo.toml','Cargo.lock','crates','tools','benchmark/fs-bench-pro','investigations/bulk-create/sync-source.py'])
    start('build')
    run('reuse-private-build',['docker','cp',str(OUT.parent/'container-dense-delete-10-s1/failed-state')+'/.',active+':/data'])
    run('copy-new-source',['docker','cp',OUT/'source.tar',active+':/data/new-source.tar'])
    run('build',['docker','exec','-e','CARGO_HOME=/data/cargo','-e','CARGO_TARGET_DIR=/data/target','-e','CARGO_BUILD_JOBS=2',active,'sh','-c','mkdir /data/update && tar -xf /data/new-source.tar -C /data/update && python3 /data/update/investigations/bulk-create/sync-source.py /data/update /data/source && cd /data/source && cargo clean -p layerfs-workspace -p fs-benchmark-pro && cargo build --offline --locked --release -p fs-benchmark-pro'],1200)
    run('focused-regression',['docker','exec','-e','CARGO_HOME=/data/cargo','-e','CARGO_TARGET_DIR=/data/target','-e','CARGO_BUILD_JOBS=2','-e','LAYERFS_EXPERIMENT_DENSE_DELETE=1','-e','LAYERFS_EXPERIMENT_FINAL_NEW_REFS=1',active,'sh','-c','cd /data/source && cargo test --offline --locked -p layerfs-workspace --features test-instrumentation --lib dense_delete_preserves_aliases_open_unlinked_and_old_root'],1200)
    run('binary-copy',['docker','cp',active+':/data/target/release/fs-benchmark-pro',OUT/'fs-benchmark-pro'])
    run('build-cleanup',['docker','rm','-fv',active]);active=None
    (OUT/'input-identity.json').write_text(json.dumps(dict(prepared_source=str(PREPARED),store_sha256=hashlib.sha256((PREPARED/'store.sqlite').read_bytes()).hexdigest(),benchmark_sha256=hashlib.sha256((OUT/'fs-benchmark-pro').read_bytes()).hexdigest()),indent=2)+'\n')
    for kind in ['create']:
      for treatment,batch in [('control','0'),('final-refs','1')]:
        arm=kind+'-'+treatment
        prepared=PREPARED if kind=='create' else OUT/'delete-prepared'
        start(arm)
        run(arm+'-copy-input',['docker','cp',prepared,active+':/data/sample'])
        run(arm+'-copy-binary',['docker','cp',OUT/'fs-benchmark-pro',active+':/usr/local/bin/fs-benchmark-pro'])
        run(arm+'-identity',['docker','exec',active,'sh','-c','sha256sum /usr/local/bin/fs-benchmark-pro /usr/local/bin/fs-benchmark-workload /data/sample/store.sqlite'])
        stats='cat /sys/fs/cgroup/cpu.stat /sys/fs/cgroup/memory.peak /sys/fs/cgroup/memory.events /sys/fs/cgroup/memory.swap.current /sys/fs/cgroup/pids.current'
        run(arm+'-before',['docker','exec',active,'sh','-c',stats])
        run(arm+'-run',['docker','exec','-e','LAYERFS_EXPERIMENT_FINAL_NEW_REFS='+batch,active,'fs-benchmark-pro','workspace-run','/data/sample','/unused-input','tiny-bulk-'+kind+'-10','1','verify','diagnostic-host-fuse'])
        run(arm+'-after',['docker','exec',active,'sh','-c',stats])
        run(arm+'-proof-copy',['docker','cp',active+':/data/sample/canonical-verification',OUT/(arm+'-canonical-verification')])
        run(arm+'-cleanup-check',['docker','exec',active,'sh','-c','cat /proc/self/mountinfo; find /tmp/layerfs-runtime -type f 2>/dev/null; ps -eo pid,comm'])
        run(arm+'-cleanup',['docker','rm','-fv',active]);active=None
finally:
    if active:
        # Keep every failed result/source; stop owned processes even after an error.
        run('failed-environment',['docker','inspect',active])
        try: run('failed-state-copy',['docker','cp',active+':/data',OUT/'failed-state'])
        finally: run('failed-cleanup',['docker','rm','-fv',active])
    files=[p for p in sorted(OUT.rglob('*')) if p.is_file() and 'failed-state' not in p.parts and p.name not in ['evidence.sha256','source.tar','dependencies.tar','fs-benchmark-pro']]
    (OUT/'evidence.sha256').write_text('\n'.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(OUT)) for p in files)+'\n')
