"""R4 per-commit production LOC: first parent vs staged tree, pinned counter."""
from pathlib import Path
import hashlib,importlib.util,io,json,subprocess,sys,tarfile,tempfile,tomllib
counter=Path('tools/production_loc.py');digest=hashlib.sha256(counter.read_bytes()).hexdigest()
assert digest=='c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb'
spec=importlib.util.spec_from_file_location('counter',counter);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
def git(*args):return subprocess.check_output(['git',*args],text=True).strip()
def count(ref):
 with tempfile.TemporaryDirectory(prefix='layerfs-r4-loc-') as directory:
  root=Path(directory)
  data=subprocess.check_output(['git','archive',ref,'core/crates','crates','core/Cargo.toml'])
  with tarfile.open(fileobj=io.BytesIO(data)) as archive:archive.extractall(root,filter='data')
  totals=module.scan(root);perfile=module.per_file(root)['files'];members=tomllib.loads((root/'core/Cargo.toml').read_text())['workspace']['members']
  active=['core/'+m+'/' for m in members];sub={'active':0,'excluded_predecessors':0,'excluded_integration':0,'reference':0}
  for entry in perfile:
   path=entry['path']
   category='reference' if entry['scope']=='reference' else 'active' if any(path.startswith(prefix) for prefix in active) else 'excluded_predecessors' if '-legacy/' in path else 'excluded_integration'
   sub[category]+=entry['production_loc'];entry['classification']=category
  assert sum(sub.values())==totals['combined']
  return {'combined':totals['combined'],'core':totals['scopes']['core']['lines'],'subtotals':sub,'product_trees':{p:git('rev-parse',ref+':'+p) for p in ['core/crates','crates']}},perfile
mode=sys.argv[1]
if mode=='staged':parent=git('rev-parse','HEAD');final=git('write-tree')
else:parent=git('rev-parse','HEAD^');final=git('rev-parse','HEAD^{tree}')
before,bfiles=count(parent);after,afiles=count(final)
record={'first_parent':parent,'counted_tree':final,'mode':mode,'counter_sha256':digest,'method':'Independent parent/final git archive extractions; pinned tools/production_loc.py scan/per_file; first-party Rust and shipped SQL, excluding comments/blanks/inline/transitive tests/docs/tools/third-party; exact Cargo member path classification','before':before,'after':after,'delta':after['combined']-before['combined']}
if len(sys.argv)>2:Path(sys.argv[2]).write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
