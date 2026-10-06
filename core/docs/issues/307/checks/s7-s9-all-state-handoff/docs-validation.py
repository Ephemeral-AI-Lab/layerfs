from pathlib import Path
import hashlib,json,re,struct,subprocess,unicodedata,urllib.parse,zipfile
root=Path.cwd();out=Path('core/docs/issues/307/checks/s7-s9-all-state-handoff')
names=set(json.loads((out/'initial-inventory.json').read_text())['files'])
names.update({'core/docs/issues/307/HANDOFF-S7-S9.md','core/docs/issues/307/LINUX-TIMESTAMP-DOCKER-20261006.md','core/docs/issues/307/PROGRESS.md','core/docs/issues/307/HANDOFF-FUSER-PATCH-S7-S9.md'})
docs=[Path(p) for p in sorted(names) if p.endswith('.md')]
errors=[];file_links=anchor_links=0

def headings(p):
 text=re.sub(r'```[^\n]*\n.*?```','',p.read_text(),flags=re.S);seen={};result=set()
 for line in text.splitlines():
  m=re.match(r'^#{1,6}\s+(.+?)\s*#*$',line)
  if not m:continue
  value=re.sub(r'\[([^\]]+)\]\([^)]+\)',r'\1',m[1]).casefold()
  value=''.join(c for c in value if unicodedata.category(c)[0] in 'LNM' or c in ' _-')
  value=value.replace(' ','-');suffix=seen.get(value,0);seen[value]=suffix+1
  result.add(value+('-'+str(suffix) if suffix else ''))
 return result

for p in docs:
 text=p.read_text()
 if 'Status:' not in text[:850] and '**Status:**' not in text[:850]:errors.append({'file':str(p),'error':'missing status banner'})
 stripped=re.sub(r'```[^\n]*\n.*?```','',text,flags=re.S)
 for target in re.findall(r'!?\[[^\]]*\]\(([^)\n]+)\)',stripped):
  target=target.split(' "',1)[0].strip('<>');url=urllib.parse.urlsplit(target)
  if url.scheme:continue
  resolved=(p.parent/urllib.parse.unquote(url.path)).resolve() if url.path else p.resolve()
  file_links+=1
  if not resolved.exists():errors.append({'file':str(p),'target':target,'error':'missing path'});continue
  if url.fragment and resolved.suffix=='.md':
   anchor_links+=1;anchor=urllib.parse.unquote(url.fragment)
   if anchor not in headings(resolved):errors.append({'file':str(p),'target':target,'error':'missing heading anchor'})
asset=Path('output/imagegen/cluster-one-x-20261005');images={}
for p in sorted(asset.glob('*.png')):
 data=p.read_bytes();assert data[:8]==b'\x89PNG\r\n\x1a\n'
 size=struct.unpack('>II',data[16:24]);assert size==(1254,1254)
 images[p.name]={'bytes':len(data),'dimensions':size,'sha256':hashlib.sha256(data).hexdigest()}
with zipfile.ZipFile(asset/'cluster-one-x-images.zip') as z:
 assert set(z.namelist())==set(images)
 for name in z.namelist():assert z.read(name)==(asset/name).read_bytes()
initial=json.loads((out/'initial-inventory.json').read_text());edited={'docs/README.md','docs/general/sandbox-cache-design.md','docs/general/workspace-filesystem-view.md','docs/general/workspace-queue-scheduling.md','output/imagegen/cluster-one-x-20261005/generation-notes.md','core/docs/issues/301/06-tables.md'}
for p,item in initial['files'].items():
 if p not in edited:assert hashlib.sha256(Path(p).read_bytes()).hexdigest()==item['sha256'],p
assert not subprocess.check_output(['git','diff','5cde942fc6b84abbfa6c12a11db83fc6689a2ad9','--','core/docs/issues/307/HANDOFF-S7-S13.md','core/crates','crates','core/vendor','core/Cargo.toml','core/Cargo.lock','core/benchmark','tools/production_loc.py'],text=True)
result={'documents':len(docs),'local_links':file_links,'anchors':anchor_links,'errors':errors,'images':images,'zip_matches_four_pngs':True,'initial_documents_and_binary_assets_preserved_except_declared_navigation_status_edits':True,'production_build_harness_counter_and_closed_S5_S6_handoff_unchanged':True}
(out/'docs-and-assets-validation-sealed.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2));raise SystemExit(bool(errors))
