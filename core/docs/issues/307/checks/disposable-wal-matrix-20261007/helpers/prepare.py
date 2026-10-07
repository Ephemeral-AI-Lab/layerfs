import fcntl, hashlib, json, os, sys, time
from pathlib import Path
sys.path.insert(0,'core/benchmark/fs-bench-pro')
import runner
from families import init_namespace, history_retention
from shared import disposable_wal
lock_path=runner.RESULTS/'phase7-sqlite.lock'
with lock_path.open('a+b') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 rows=[]
 for name,item in disposable_wal.ROWS.items():
  if item['fixture'] is None: continue
  fixture=init_namespace.prepare(init_namespace.CASES[item['fixture']],runner.RESULTS/'sdk-prepared')
  assert fixture['manifest_sha256']==disposable_wal.reference(name)['fixture']['manifest_sha256']
  rows.append(fixture)
  print(json.dumps({'prepared':name,'reused':fixture['reused'],'wall_ns':fixture['preparation_wall_ns']}),flush=True)
 record={'namespace':rows,'history_corpus':history_retention.corpus.identity(),'setup':'prepared source reuse only; fresh Store creation remains measured','construction_workers':os.environ['LAYERFS_CONSTRUCTION_WORKERS']}
 with Path('core/docs/issues/307/checks/disposable-wal-matrix-20261007/10-prepared-inputs.json').open('x') as f:json.dump(record,f,indent=2);f.write('\n')
