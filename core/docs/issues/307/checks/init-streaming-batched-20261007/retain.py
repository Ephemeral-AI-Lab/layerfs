from pathlib import Path
import json,hashlib,gzip,shutil,sqlite3,sys
root=Path.cwd(); wt=Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs')
label,diagnostic,source_commit=sys.argv[1:]
evidence=root/'core/docs/issues/307/checks'/label
retained=root/'benchmark-results/fs-bench-pro'/(label+'-retained');retained.mkdir()
copies=[];rows=[]
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
def copy(source,dest,raw):
 shutil.copytree(source,dest);raw.mkdir(parents=True)
 for p in source.iterdir():
  if not p.is_file():continue
  a=sha(p);assert a==sha(dest/p.name)
  copies.append(dict(original=str(p),copy=str(dest/p.name),bytes=p.stat().st_size,sha256=a))
  if p.suffix=='.json':shutil.copy2(p,raw/p.name)
  elif p.suffix in ['.stdout','.stderr']:(raw/(p.name+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
for item in json.loads((evidence/'selection.json').read_text())['cases']:
 name=item['id'];path=wt/'benchmark-results/fs-bench-pro'/(label+'-'+name+'-candidate')
 raw=evidence/'raw'/name;copy(path,retained/name,raw)
 outer=root/'benchmark-results/fs-bench-pro'/label/name
 for suffix in ['.json','.stdout','.stderr']:
  p=outer.with_suffix(suffix)
  if suffix=='.json':shutil.copy2(p,raw/'campaign-invocation.json')
  else:(raw/('campaign'+suffix+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
 r=json.loads((path/'receipt.json').read_text());assert r['identity']['source_commit']==source_commit
 ref=r['regression_reference']
 with sqlite3.connect(f'file:{path}/store.sqlite?mode=ro&immutable=1',uri=True) as db:
  acq={t:db.execute(f'SELECT count(*) FROM {t}').fetchone()[0] for t in ['init_operation','init_entry','init_native_file']}
  free=db.execute('PRAGMA freelist_count').fetchone()[0]
 rows.append(dict(case=name,profile=r['requested_profile'],control_ns=ref['comparison_ns'],candidate_ns=r['comparison_ns'],control_bytes=ref['storage_bytes'],candidate_bytes=r['storage_bytes'],speed_gate='PASS' if 10*r['comparison_ns']<=11*ref['comparison_ns'] else 'FAIL',allocation_gate='PASS' if r['storage_bytes']<=ref['storage_bytes'] else 'FAIL',complete=r['status'],proof=r['verification_status'],cache=r['cache_status'],cleanup=r['cleanup']['status'],freelist=free,acquisition_rows=acq,receipt=str((raw/'receipt.json').relative_to(root))))
path=wt/'benchmark-results/fs-bench-pro'/diagnostic;copy(path,retained/'diagnostic',evidence/'diagnostics')
r=json.loads((path/'receipt.json').read_text());units=r.get('units',[])
acq=[u for u in units if u['unit']!='complete_init' and not u['unit'].startswith('storage.')]
agg={k:sum(u[k] for u in acq) for k in ['calls','statements','vm_steps','write_commits','read_snapshots']}
for name in ['benchmark_init','verify_namespace','durable_init_costs']:
 p=wt/'core/target/release/examples'/name;d=retained/'binaries'/name;d.parent.mkdir(exist_ok=True);shutil.copy2(p,d);assert sha(p)==sha(d)
(evidence/'closed-copy-manifest.json').write_text(json.dumps(dict(method='independent ordinary closed-output byte copies; bothSHA checked; immutable readonly inventory; no replay',copies=copies),indent=2)+'\n')
(evidence/'ledger.json').write_text(json.dumps(dict(source=source_commit,rows=rows,diagnostic=dict(status=r['status'],receipt='diagnostics/receipt.json',whole=next((u for u in units if u['unit']=='complete_init'),None),acquisition=agg),interference='No own builds/tests/copies overlap timed operation in measurementworktree. Subagents edit/read small primary source and logs; no builds/measurements. Competing processes retained in receipts.'),indent=2)+'\n')
print(json.dumps(dict(rows=rows,acquisition=agg),indent=2))
