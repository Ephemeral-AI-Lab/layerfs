import fcntl,hashlib,json,os,pathlib,subprocess,time,uuid
root=pathlib.Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs');out=root/'docs/roadmap/0.1/0.1.6/evidence/phase3-host-operations';name='mounted-host-attempt01';receipt=out/(name+'.json');assert not receipt.exists()
native=root/'target/debug/deps/layerfs_workspace-7a70d4db173d6a6f';helper=root/'target/aarch64-unknown-linux-musl/debug/layerfs-fuse'
expected={native:'0869ef5dcda8f36013d124129753dd125bd1d3a25101f13d51c9477cebcfb4a7',helper:'ed690820d4437aad700e4d7a2b1974206c2ccd69dc6e1c5deca7be6b77c23e2c'}
for p,h in expected.items():assert hashlib.sha256(p.read_bytes()).hexdigest()==h
image='sha256:b9d3d2c3090596364d2d70ee304a5b7316b8dc51b9d672b8a0b3857e1de940f3';container=None
r={'identity':'host_runtime::tests::mounted_host_owner_preserves_sdk_mapping_handles_and_owned_commit_input','source':'../phase4-candidate/capacity-compile-attempt03-source.json','linux_source':'mount-authority-linux-attempt01-source.json','binaries':{str(p.relative_to(root)):h for p,h in expected.items()},'image':image,'status':'NOT_RUN','performance':'N/A correctness component, no benchmark case','scope':'macOS Store/HostOverlay/SDK/coordinator/spool; Linux actual FUSE/helper/workload; explicit owned input supplied to construction, V1 acquisition remains OPEN'}
lockpath=pathlib.Path(os.environ.get('TMPDIR','/tmp'))/'layerfs-infra-measurement.lock'
with lockpath.open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 try:
  run=['docker','run','--detach','--rm','--name','layerfs-v124-'+uuid.uuid4().hex[:12],'--device','/dev/fuse','--cap-add','SYS_ADMIN','--security-opt','apparmor=unconfined','--cpus','2','--memory','2g','--memory-swap','2g','--pids-limit','256','--entrypoint','/bin/sleep',image,'3600'];r['container_command']=run
  container=subprocess.check_output(run,text=True).strip();r['container_id']=container
  inspection=json.loads(subprocess.check_output(['docker','inspect',container],text=True))[0];h=inspection['HostConfig'];assert inspection['Image']==image and not inspection['Mounts'] and not h['Privileged'] and h['Memory']==2147483648 and h['MemorySwap']==2147483648 and h['NanoCpus']==2000000000 and h['PidsLimit']==256
  (out/(name+'-container.json')).write_text(json.dumps({k:inspection[k] for k in ('Id','Image','Created','State','HostConfig','Mounts')},indent=2)+'\n')
  r['kernel']=subprocess.check_output(['docker','exec',container,'uname','-r'],text=True).strip()
  subprocess.check_call(['docker','exec',container,'python3','--version'])
  env=os.environ.copy();env['LAYERFS_HOST_OVERLAY_CONTAINER']=container;env['LAYERFS_FUSE_HELPER']=str(helper);env.pop('LAYERFS_CONTAINER_FUSE_HELPER',None)
  cmd=[str(native),r['identity'],'--exact','--ignored','--nocapture'];r['command']=cmd;t=time.monotonic()
  with (out/(name+'.log')).open('w') as log:
   try:
    result=subprocess.run(cmd,env=env,cwd=root,stdout=log,stderr=subprocess.STDOUT,timeout=180);r['exit_code']=result.returncode
    raw=(out/(name+'.log')).read_text();r['status']='PASS' if result.returncode==0 and '1 passed; 0 failed' in raw else 'FAIL' if result.returncode else 'NOT_RUN'
   except subprocess.TimeoutExpired:r['status']='TIMEOUT';r['deadline_seconds']=180
  r['command_wall_seconds']=time.monotonic()-t
  r['state_before_owner_container_cleanup']=json.loads(subprocess.check_output(['docker','inspect',container],text=True))[0]['State']
 except Exception as e:
  r['error']=repr(e)
  if r['status']=='NOT_RUN':r['status']='SETUP_FAIL'
 finally:
  if container:
   cleanup=subprocess.run(['docker','rm','--force',container],text=True,capture_output=True);r['container_cleanup']={'exit_code':cleanup.returncode,'stdout':cleanup.stdout,'stderr':cleanup.stderr}
   verified=subprocess.run(['docker','inspect',container],text=True,capture_output=True);r['container_cleanup']['verified_absent']=verified.returncode!=0 and 'No such object' in verified.stderr
  r['artifacts_unchanged']=all(hashlib.sha256(p.read_bytes()).hexdigest()==h for p,h in expected.items());receipt.write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps(r,indent=2));log=out/(name+'.log');print(log.read_text()[-8000:] if log.exists() else '')
