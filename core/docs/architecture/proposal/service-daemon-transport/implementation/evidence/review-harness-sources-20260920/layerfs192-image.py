import hashlib,json,shutil,subprocess,tempfile
from pathlib import Path
binary=Path('core/target/aarch64-unknown-linux-musl/debug/layerfs-daemon');sha=hashlib.sha256(binary.read_bytes()).hexdigest();tag='layerfs-issue192:'+sha[:16]
with tempfile.TemporaryDirectory(prefix='layerfs192-image-')as directory:
 p=Path(directory);shutil.copyfile(binary,p/'layerfs-daemon');(p/'layerfs-daemon').chmod(0o755);shutil.copyfile('core/crates/layerfs-daemon/Dockerfile',p/'Dockerfile');(p/'logs').mkdir();(p/'logs/.keep').touch()
 subprocess.run(['docker','build','--build-arg','BINARY_SHA256='+sha,'-t',tag,directory],check=True)
 result=json.loads(subprocess.check_output(['docker','image','inspect',tag],text=True))[0]
 data={'image':result['Id'],'tag':tag,'daemon_sha256':sha,'architecture':result['Architecture'],'os':result['Os'],'labels':result['Config']['Labels']};Path('/tmp/layerfs-issue192-current-image.json').write_text(json.dumps(data));print(json.dumps(data))
