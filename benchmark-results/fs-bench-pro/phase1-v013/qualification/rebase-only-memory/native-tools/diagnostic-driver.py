from pathlib import Path
import subprocess,json,hashlib,os,signal,time,shutil
root=Path('benchmark-results/fs-bench-pro/phase1-v013/qualification/rebase-only-memory/native-tools').resolve();root.mkdir(exist_ok=False)
source=Path('crates/layerfs-workspace/src/lifecycle.rs')
cmd=['cargo','test','--release','-p','layerfs-workspace','--no-default-features','--features','test-instrumentation','--lib','--no-run','--message-format=json']
(root/'identity.json').write_text(json.dumps({'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'build_command':cmd,'runtime_deadline_seconds':180,'source_store':'workspace-dense-rewrite-500-s2-performance-d648880f3f73','memory_tools':'heap -s --noContent and vmmap -summary at after-load/after-rebase only'},indent=2)+'\n')
with (root/'build.jsonl').open('wb') as out,(root/'build.stderr.txt').open('wb') as err:
 r=subprocess.run(cmd,stdout=out,stderr=err)
(root/'build-result.json').write_text(json.dumps({'exit_code':r.returncode})+'\n')
if r.returncode:
 print((root/'build.stderr.txt').read_text())
 for line in (root/'build.jsonl').read_text().splitlines():
  row=json.loads(line)
  if row.get('reason')=='compiler-message' and row['message']['level']=='error':print(row['message']['rendered'])
 raise SystemExit(r.returncode)
artifacts=[json.loads(line) for line in (root/'build.jsonl').read_text().splitlines() if line.startswith('{')]
binary=next(row['executable'] for row in reversed(artifacts) if row.get('reason')=='compiler-artifact' and row.get('executable') and row['target']['name']=='layerfs_workspace')
retained=Path('benchmark-results/fs-bench-pro/phase1-v013/scratch/workspace-dense-rewrite-500-s2-performance-d648880f3f73')
subprocess.run(['/bin/cp','-c',str(retained/'store.sqlite'),str(root/'store.sqlite')],check=True)
assert (retained/'store.sqlite').stat().st_ino!=(root/'store.sqlite').stat().st_ino
shutil.copyfile(retained/'branch-id',root/'branch-id')
argv=[binary,'lifecycle::tests::diagnose_rebase_retained_committed_store','--ignored','--exact','--nocapture']
(root/'command.json').write_text(json.dumps({'argv':argv,'clone_method':'APFS cp -c; independent inode','input_bytes':(root/'store.sqlite').stat().st_size},indent=2)+'\n')
start=time.monotonic();timed_out=False
with (root/'output.log').open('wb') as log:
 p=subprocess.Popen(argv,stdout=log,stderr=subprocess.STDOUT,env=dict(os.environ,LAYERFS_REBASE_DIAGNOSTIC_ROOT=str(root)),start_new_session=True)
 print('diagnostic_pid='+str(p.pid),flush=True)
 try:p.wait(timeout=180)
 except subprocess.TimeoutExpired:
  timed_out=True;os.killpg(p.pid,signal.SIGKILL);p.wait()
(root/'result.json').write_text(json.dumps({'exit_code':p.returncode,'timeout':timed_out,'wall_seconds':time.monotonic()-start})+'\n')
print(root);print((root/'result.json').read_text());print((root/'output.log').read_text()[-1800:])
if (root/'memory.jsonl').exists():print((root/'memory.jsonl').read_text())
