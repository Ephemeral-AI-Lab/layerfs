"""Independent expected scratch-budget arithmetic; no candidate imports."""
from pathlib import Path
import json,hashlib
MIN=16777216
MAX=1099511623680
CASES=[
 ('zero',0,False,0,0,0),
 ('below_minimum',MIN-4096,False,0,0,0),
 ('unaligned',MIN+1,False,0,0,0),
 ('default16',16777216,True,65536,4128768,4096),
 ('configured32',33554432,True,131072,8257536,8192),
 ('configured48',50331648,True,196608,12386304,12288),
 ('configured64',67108864,True,262144,16515072,16384),
 ('format_maximum_unqualified',1099511623680,True,4294967280,270582938640,268435455),
 ('above_format_range',1099511627776,False,0,0,0),
]
if __name__=='__main__':
 for name,size,valid,records,logical,pages in CASES:
  actual=MIN<=size<=MAX and size%4096==0
  assert actual==valid,name
  if valid:assert (size//256,(size//256)*63,size//4096)==(records,logical,pages),name
 here=Path(__file__).parent
 data={'scope':'prospective format/admission arithmetic only, not provider/file-fit/healthy progress qualification','cases':[dict(zip(['name','native_bytes','valid','records','logical_bytes','pages'],case)) for case in CASES]}
 text=json.dumps(data,indent=2)+'\n';(here/'budget_vectors.json').write_text(text)
 rs='// Independent prospective budget literals; see budget_vectors.py.\n'
 rs+='pub const BUDGET_CASES: &[(&str,u64,bool,u64,u64,u64)] = &[\n'
 for name,size,valid,records,logical,pages in CASES:rs+=f'("{name}",{size},{str(valid).lower()},{records},{logical},{pages}),\n'
 rs+='];\n';(here/'budget_manifest.rs').write_text(rs)
 print(json.dumps({'cases':len(CASES),'json_sha256':hashlib.sha256(text.encode()).hexdigest(),'rust_sha256':hashlib.sha256(rs.encode()).hexdigest()}))
