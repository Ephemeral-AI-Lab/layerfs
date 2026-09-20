#!/usr/bin/env python3
"""Read-only post-verification semantic inventory and physical-size comparison."""
import hashlib,json,sqlite3
from pathlib import Path
P=Path(__file__).resolve().parent
result={}
for case in ['history-stride10','history-stride3']:
 arms={}
 for arm in ['baseline','candidate']:
  run=P/'runs'/f'{arm}-{case}'
  if not (run/'verify-receipt.json').exists():continue
  path=run/'raw/sample.sqlite'
  db=sqlite3.connect(path.as_uri()+'?mode=ro',uri=True)
  rows=list(db.execute('SELECT hex(object_id),object_role,canonical_length FROM objects ORDER BY object_id'))
  serial=json.dumps(rows,separators=(',',':')).encode()
  arms[arm]={'canonical_object_count':len(rows),'canonical_bytes':sum(r[2] for r in rows),'canonical_inventory_sha256':hashlib.sha256(serial).hexdigest(),'packs':db.execute('SELECT count(*),sum(length(data)) FROM object_packs').fetchone(),'quick_check':db.execute('PRAGMA quick_check').fetchall(),'store_sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
  db.close()
 if len(arms)==2:
  assert arms['baseline']['canonical_inventory_sha256']==arms['candidate']['canonical_inventory_sha256']
  result[case]=arms
with (P/'store-comparison.json').open('x') as f:json.dump(result,f,indent=2);f.write('\n')
print(json.dumps(result,indent=2))
