import fcntl,hashlib,io,json,os,subprocess,sys,tarfile,tempfile
from pathlib import Path
root=Path('/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs');os.chdir(root);handles=[]
for p in dict.fromkeys([(Path(os.environ.get('TMPDIR','/tmp'))/'layerfs-infra-measurement.lock').resolve(),Path('/tmp/layerfs-infra-measurement.lock').resolve()]):
 h=p.open('a');fcntl.flock(h,fcntl.LOCK_EX|fcntl.LOCK_NB);handles.append(h)
parent=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();tree=subprocess.check_output(['git','write-tree'],text=True).strip()
result={'parent':parent,'staged_tree':tree,'counter':'tools/production_loc.py','counter_sha256':hashlib.sha256(Path('tools/production_loc.py').read_bytes()).hexdigest(),'scope':'reference and core first-party runtime Rust and SQL; external application adapters=0; new bridge/service/daemon source counted in core; tests/examples/benchmarks/tooling/docs/manifests/Docker deployment configuration excluded; reference inline tests excluded by counter'}
for label,revision in [('before',parent),('after',tree)]:
 paths=subprocess.check_output(['git','ls-tree','-rz','--name-only',revision]).decode().split('\0');selected=[]
 for name in paths:
  parts=Path(name).parts
  if not parts:continue
  product=parts[0]=='crates' or (len(parts)>1 and parts[:2]==('core','crates'))
  if product and ((name.endswith('.rs') and 'src'in parts)or(name.endswith('.sql')and 'sql'in parts)):selected.append(name)
 with tempfile.TemporaryDirectory(prefix='layerfs192-loc-')as folder:
  data=subprocess.check_output(['git','archive','--format=tar',revision,'--',*selected]);tarfile.open(fileobj=io.BytesIO(data)).extractall(folder,filter='data')
  counted=json.loads(subprocess.check_output([sys.executable,str(root/'tools/production_loc.py'),'--root',folder,'--json'],text=True));counted.pop('root',None);result[label]=counted
result['delta']=result['after']['combined']-result['before']['combined'];Path(sys.argv[1]).write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
