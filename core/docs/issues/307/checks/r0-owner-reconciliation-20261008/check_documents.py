"""R0 scoped documentation/hash check; no product or runtime invocation."""
from pathlib import Path
import hashlib,json,re,subprocess,unicodedata
ROOT=Path(__file__).resolve().parents[6]
R=ROOT/'core/docs/issues/307/checks/r0-owner-reconciliation-20261008'
files=[ROOT/x for x in subprocess.check_output(['git','diff','--name-only'],cwd=ROOT,text=True).splitlines() if x.endswith('.md')]
files += [ROOT/'core/docs/issues/307/ROLLOUT-LEDGER-20261008.md'] + list(R.glob('*.md'))
files=list(dict.fromkeys(files))
def slug(h):
 h=re.sub(r'\[([^]]+)\]\([^)]*\)',r'\1',h)
 h=re.sub(r'<[^>]*>','',h).lower().strip()
 return ''.join(c for c in h if c.isalnum() or c in ' _-').replace(' ','-')
def anchors(text):
 out=set(re.findall(r'<a\s+id="([^"]+)"',text));counts={}
 for h in re.findall(r'^#{1,6}\s+(.+)$',text,re.M):
  k=slug(h);n=counts.get(k,0);out.add(k+(f'-{n}' if n else ''));counts[k]=n+1
 return out
bad=[];checked=0;external=0
for p in files:
 s=p.read_text()
 for m in re.finditer(r'\[[^\]\n]*\]\(([^)\n]*)\)',s):
  raw=m.group(1).strip();link=raw.split(' "')[0].strip('<>')
  if re.match(r'^[A-Za-z][\w+.-]*:',link):external+=1;continue
  path,_,anchor=link.partition('#');q=(p.parent/path).resolve() if path else p
  checked+=1
  if not q.exists():bad.append({'file':str(p.relative_to(ROOT)),'line':s[:m.start()].count('\n')+1,'link':link,'error':'missing path'})
  elif anchor and q.suffix=='.md' and anchor not in anchors(q.read_text()):bad.append({'file':str(p.relative_to(ROOT)),'line':s[:m.start()].count('\n')+1,'link':link,'error':'missing anchor'})
original=json.loads((R/'02-original-document-hashes.json').read_text())
changed=set(str(p.relative_to(ROOT)) for p in files)
immutable=[];immutable_checked=0
for name,digest in original.items():
 if name in changed:continue
 immutable_checked+=1
 p=ROOT/name
 if not p.exists() or hashlib.sha256(p.read_bytes()).hexdigest()!=digest:immutable.append(name)
snapshot=json.loads((R/'00-dispatch-snapshot.json').read_text());protected=[]
for row in snapshot['files']:
 if row['initial_status']=='untracked/protected':
  p=ROOT/row['path'];actual=hashlib.sha256(p.read_bytes()).hexdigest();protected.append({'path':row['path'],'sha256':actual,'matches':actual==row['sha256']})
report={'scope':'R0 documents only, no Cargo/tests/mount/measurement','checked_local_links':checked,'external_links_not_fetched':external,'link_errors':bad,'immutable_original_files_checked':immutable_checked,'immutable_errors':immutable,'protected_notes':protected,'source_trees':{x:subprocess.check_output(['git','rev-parse','HEAD:'+x],cwd=ROOT,text=True).strip() for x in ['core/crates','crates']}}
sequence=1
while (R/f'05-document-check-{sequence:02}.json').exists(): sequence+=1
(R/f'05-document-check-{sequence:02}.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='link_errors'},indent=2))
print('link errors',len(bad));print(json.dumps(bad[:35],indent=2))
raise SystemExit(bool(bad or immutable or not all(x['matches'] for x in protected)))
