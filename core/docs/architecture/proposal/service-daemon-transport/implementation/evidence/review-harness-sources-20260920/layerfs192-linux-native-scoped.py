import json,os,subprocess
from pathlib import Path
image='sha256:a027c5b98643a5bba223de4e782545b436c1c106b8653dcd5d3d7c89040efa0e'
name='layerfs-issue192-native-'+str(os.getpid())
command=['docker','run','--name',name,'--label=io.layerfs.task=issue192','--log-driver=none','--cpus=1','--memory=32m','--memory-swap=32m','--pids-limit=16','--cap-drop=ALL',image,'--nocapture','--skip','output_failure_is_independent_and_encoding_is_valid']
try:
 result=subprocess.run(command,timeout=10)
 item=json.loads(subprocess.check_output(['docker','inspect','--size',name],text=True))[0]
 record={'status':'PASS'if result.returncode==0 else'FAIL','image':image,'command':command,'container_id':item['Id'],'mounts':item['Mounts'],'memory_limit':item['HostConfig']['Memory'],'exit':item['State']['ExitCode'],'diff':subprocess.check_output(['docker','diff',name],text=True),'omission':'One test requires a Python JSON parser absent from this scratch image. It passes on macOS; actual Linux forward/both output is independently JSON-parsed by the host deployment driver. Linux sink-failure coverage additionally comes from the maximum-envelope child.'}
 assert not item['Mounts']
finally:
 subprocess.run(['docker','rm','-f',name],check=True,stdout=subprocess.DEVNULL)
record['cleanup']='PASS'
p=Path('core/docs/architecture/proposal/service-daemon-transport/implementation/evidence/review-linux-native-scoped-identities-20260920.json')
with p.open('x')as f:json.dump(record,f,indent=2)
print(json.dumps(record));assert result.returncode==0
