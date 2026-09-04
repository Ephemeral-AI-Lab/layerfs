#!/usr/bin/env python3
"""One 100k delete scaling sample; reused qualified input, no Phase 1 lock."""
import hashlib, json, pathlib, shutil, subprocess, time, uuid
ROOT=pathlib.Path(__file__).resolve().parents[2]
OUT=ROOT/'investigations/bulk-create/evidence/container-dense-delete-500-s1'
BINARY=ROOT/'investigations/bulk-create/evidence/container-dense-delete-10-s1-r2/fs-benchmark-pro'
MASTER=ROOT.parent/'layerfs/target/phase1-prepared/6b562e1ebe88e1dfa1a8f8aa6ec0a5140b275497d362d3b321518bb545947e91/store'
EXPECTED='7fa7c8c46f648b5555fd5bbe4947f713bd01dacd0048dfb249f252f43aad5677'
BASE='sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e'
OUT.mkdir();commands=[];active=None
def digest(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def run(label,argv,timeout=600):
    print('start '+label,flush=True);start=time.monotonic_ns()
    with (OUT/(label+'.stdout')).open('xb') as stdout,(OUT/(label+'.stderr')).open('xb') as stderr:
        p=subprocess.run([str(x) for x in argv],cwd=ROOT,stdout=stdout,stderr=stderr,timeout=timeout)
    commands.append(dict(label=label,argv=[str(x) for x in argv],elapsed_ns=time.monotonic_ns()-start,exit_code=p.returncode))
    (OUT/'commands.json').write_text(json.dumps(commands,indent=2)+'\n');p.check_returncode();print('finish '+label,flush=True)
try:
    source=subprocess.check_output(['git','rev-parse','b5dd2829'],cwd=ROOT,text=True).strip()
    (OUT/'scope.json').write_text(json.dumps(dict(binary_source=source,runner_source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),profile='exploratory-linux-colocated-2cpu-2g',shared_host_interference_allowed=True,phase1_lock_acquired=False,case='tiny-bulk-delete-500',seed=1,mode='verify',admission_eligible=False),indent=2)+'\n')
    start=time.monotonic_ns();prepared=OUT/'prepared';prepared.mkdir()
    shutil.copy2(MASTER/'store.sqlite',prepared/'store.sqlite');shutil.copy2(MASTER/'branch-id',prepared/'branch-id')
    assert digest(prepared/'store.sqlite')==EXPECTED
    (OUT/'preparation.json').write_text(json.dumps(dict(master=str(MASTER),copy_and_validation_ns=time.monotonic_ns()-start,store_sha256=EXPECTED,binary_sha256=digest(BINARY)),indent=2)+'\n')
    active='layerfs-dense-delete-'+uuid.uuid4().hex[:12]
    run('create',['docker','run','-d','--name',active,'--cpus','2','--memory','2g','--memory-swap','2g','--pids-limit','256','--device','/dev/fuse','--cap-add','SYS_ADMIN','--security-opt','apparmor=unconfined','--mount','type=volume,destination=/data','--entrypoint','sleep',BASE,'infinity'])
    run('environment',['docker','inspect',active]);run('concurrent-containers',['docker','ps','--format','{{.Names}}\t{{.Status}}'])
    run('copy-input',['docker','cp',prepared,active+':/data/sample'])
    run('copy-binary',['docker','cp',BINARY,active+':/usr/local/bin/fs-benchmark-pro'])
    run('identity',['docker','exec',active,'sh','-c','sha256sum /usr/local/bin/fs-benchmark-pro /usr/local/bin/fs-benchmark-workload /data/sample/store.sqlite'])
    stats='cat /sys/fs/cgroup/cpu.stat /sys/fs/cgroup/memory.peak /sys/fs/cgroup/memory.events /sys/fs/cgroup/memory.swap.current /sys/fs/cgroup/pids.current'
    run('before',['docker','exec',active,'sh','-c',stats])
    run('run',['docker','exec','-e','LAYERFS_EXPERIMENT_DENSE_DELETE=1',active,'fs-benchmark-pro','workspace-run','/data/sample','/unused-input','tiny-bulk-delete-500','1','verify','diagnostic-host-fuse'],1800)
    run('after',['docker','exec',active,'sh','-c',stats])
    run('proof-copy',['docker','cp',active+':/data/sample/canonical-verification',OUT/'canonical-verification'])
    run('cleanup-check',['docker','exec',active,'sh','-c','cat /proc/self/mountinfo; find /tmp/layerfs-runtime -type f 2>/dev/null; ps -eo pid,comm'])
finally:
    if active:
        run('terminal-environment',['docker','inspect',active])
        if not (OUT/'run.stdout').exists() or '"verification-complete","status":"pass"' not in (OUT/'run.stdout').read_text():
            run('failed-state-copy',['docker','cp',active+':/data/sample',OUT/'failed-state'])
        run('cleanup',['docker','rm','-fv',active])
        run('remaining',['docker','ps','-aq','--filter','name=^/'+active+'$'])
    files=[p for p in sorted(OUT.rglob('*')) if p.is_file() and 'prepared' not in p.parts and 'failed-state' not in p.parts and p.name!='evidence.sha256']
    (OUT/'evidence.sha256').write_text('\n'.join(digest(p)+'  '+str(p.relative_to(OUT)) for p in files)+'\n')
