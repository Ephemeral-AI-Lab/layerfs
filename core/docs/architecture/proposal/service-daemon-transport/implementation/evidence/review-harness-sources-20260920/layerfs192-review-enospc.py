import json,os,plistlib,subprocess,tempfile
from pathlib import Path
proof=Path('core/docs/architecture/proposal/service-daemon-transport/implementation/evidence')
with tempfile.TemporaryDirectory(prefix='layerfs192-enospc-')as directory:
 root=Path(directory);volume=root/'volume';volume.mkdir();image=root/'disk.dmg'
 create=['hdiutil','create','-size','32m','-fs','HFS+','-volname','layerfs192','-type','UDIF',str(image)]
 subprocess.run(create,check=True,timeout=10)
 attach=['hdiutil','attach','-nobrowse','-mountpoint',str(volume),'-plist',str(image)]
 attachment=plistlib.loads(subprocess.check_output(attach,timeout=10));env=os.environ.copy();env['LAYERFS_TEST_DISK_ROOT']=str(volume)
 command=['cargo','+1.85.1','test','--manifest-path','core/Cargo.toml','--locked','-p','layerfs-telemetry','--features','native','--test','full_disk','--','--ignored','--nocapture']
 try:
  result=subprocess.run(command,env=env,timeout=15);code=result.returncode
 finally:
  detach=['hdiutil','detach',str(volume)];cleanup=subprocess.run(detach,check=True,timeout=10,capture_output=True,text=True)
 record={'status':'PASS'if code==0 else'FAIL','command':command,'create':create,'attach':attach,'attachment':attachment,'detach':detach,'cleanup':cleanup.stdout.strip(),'source_scope':'current source and dependency selection, with full test filesystem guard','returncode':code}
 with(proof/'review-enospc-identities-20260920.json').open('x')as f:json.dump(record,f,indent=2)
 print(json.dumps(record));assert code==0
