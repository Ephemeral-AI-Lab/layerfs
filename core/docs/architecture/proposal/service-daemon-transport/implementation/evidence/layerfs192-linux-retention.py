import json,hashlib,os,shutil,subprocess,tempfile
from pathlib import Path
proof=Path('core/docs/architecture/proposal/service-daemon-transport/implementation/evidence')
rows=[json.loads(line)for line in(proof/'linux-native-retention-build-20260920/command.log').read_text().splitlines()if line.startswith('{')]
binary=Path(next(r['executable']for r in rows if r.get('executable')and r.get('target',{}).get('name')=='native'));sha=hashlib.sha256(binary.read_bytes()).hexdigest();tag='layerfs-issue192-native:'+sha[:16];name='layerfs-issue192-retention-'+str(os.getpid())
with tempfile.TemporaryDirectory(prefix='layerfs192-native-image-')as directory:
 p=Path(directory);shutil.copyfile(binary,p/'tests');(p/'tests').chmod(0o755);(p/'tmp').mkdir();(p/'tmp/.keep').touch();(p/'Dockerfile').write_text('FROM scratch\nCOPY tests /tests\nCOPY --chown=65532:65532 --chmod=1777 tmp /tmp\nUSER 65532:65532\nENTRYPOINT ["/tests"]\n')
 subprocess.run(['docker','build','-t',tag,directory],check=True)
 image=subprocess.check_output(['docker','image','inspect',tag,'--format','{{.Id}}'],text=True).strip()
 try:
  command=['docker','run','--name',name,'--label=io.layerfs.task=issue192','--log-driver=none','--cpus=1','--memory=32m','--memory-swap=32m','--pids-limit=16','--cap-drop=ALL',image,'retention','--nocapture'];subprocess.run(command,check=True,timeout=10)
  item=json.loads(subprocess.check_output(['docker','inspect','--size',name],text=True))[0]
  record={'status':'PASS','image':image,'test_binary_sha256':sha,'command':command,'scope':'two external Linux retention tests; optional local output configuration, not forward mode','container_id':item['Id'],'mounts':item['Mounts'],'labels':item['Config']['Labels'],'memory_limit':item['HostConfig']['Memory'],'exit':item['State']['ExitCode'],'logical_writable_layer_bytes':item.get('SizeRw'),'diff':subprocess.check_output(['docker','diff',name],text=True),'phase_memory_peak':'unavailable; not claimed'}
  assert not item['Mounts'];(proof/'linux-native-retention-identities.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record))
 finally:subprocess.run(['docker','rm','-f',name],check=True,stdout=subprocess.DEVNULL)
