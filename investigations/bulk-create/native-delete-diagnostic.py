#!/usr/bin/env python3
"""One native Linux delete comparator, identical sealed workload, private volume."""
import hashlib,json,pathlib,subprocess,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[2]
OUT=ROOT/'investigations/bulk-create/evidence/native-delete-500-s1'
IMAGE='sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e'
FIXTURE=OUT.parent/'native-500-s1/fixture'
OUT.mkdir();commands=[];active=None
(OUT/'scope.json').write_text(json.dumps(dict(profile='native-linux-volume',case='tiny-bulk-delete-500',seed=1,source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),limits=dict(cpus=2,memory_bytes=2147483648,memory_swap_bytes=2147483648,pids=256),phase1_lock=False,shared_host_interference_allowed=True,scope='Identical POSIX workload only; no LayerFS SDK, FUSE, Commit, visibility or End. Creation and qualification are input preparation.'),indent=2)+'\n')
def run(label,argv,timeout=600):
    print('start '+label,flush=True);start=time.monotonic_ns()
    with (OUT/(label+'.stdout')).open('xb') as out,(OUT/(label+'.stderr')).open('xb') as err:
        p=subprocess.run([str(x) for x in argv],cwd=ROOT,stdout=out,stderr=err,timeout=timeout)
    commands.append(dict(label=label,argv=[str(x) for x in argv],elapsed_ns=time.monotonic_ns()-start,exit_code=p.returncode))
    (OUT/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
    print('finish '+label+' '+str(p.returncode),flush=True);p.check_returncode()
try:
    active='layerfs-native-delete-'+uuid.uuid4().hex[:12]
    run('create',['docker','run','-d','--name',active,'--cpus','2','--memory','2g','--memory-swap','2g','--pids-limit','256','--mount','type=volume,destination=/native','--entrypoint','sleep',IMAGE,'infinity'])
    run('environment',['docker','inspect',active])
    run('copy-witness',['docker','cp',str(FIXTURE)+'/.',active+':/native'])
    run('prepare-root',['docker','exec',active,'sh','-c','chmod 0750 /native && touch -d @1700000000 /native'])
    run('helper-identity',['docker','exec',active,'sha256sum','/usr/local/bin/fs-benchmark-workload'])
    run('prepare-input',['docker','exec','-w','/native',active,'fs-benchmark-workload','workspace-apply','tiny-bulk-create-500','1','0','performance'])
    run('qualify-delete-input',['docker','exec','-w','/native',active,'fs-benchmark-workload','workspace-verify-tree','tiny-bulk-delete-500','1','0'])
    stats='cat /sys/fs/cgroup/cpu.stat /sys/fs/cgroup/memory.peak /sys/fs/cgroup/memory.events /sys/fs/cgroup/memory.swap.current; stat -f -c %T /native'
    run('before',['docker','exec',active,'sh','-c',stats])
    run('performance',['docker','exec','-w','/native',active,'fs-benchmark-workload','workspace-apply','tiny-bulk-delete-500','1','0','performance'])
    run('after',['docker','exec',active,'sh','-c',stats])
    run('verify',['docker','exec','-w','/native',active,'fs-benchmark-workload','workspace-verify-tree','tiny-bulk-delete-500','1','1'])
finally:
    if active:
        if not any(c['label']=='verify' and c['exit_code']==0 for c in commands):
            run('failed-state-copy',['docker','cp',active+':/native',OUT/'failed-state'])
        run('cleanup',['docker','rm','-fv',active])
        run('remaining',['docker','ps','-aq','--filter','name=^/'+active+'$'])
    files=[p for p in sorted(OUT.iterdir()) if p.is_file() and p.name!='evidence.sha256']
    (OUT/'evidence.sha256').write_text('\n'.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name for p in files)+'\n')
