#!/usr/bin/env python3
"""Disjoint self-sample attribution from native sample's main-thread call tree."""
import collections,json,re
from pathlib import Path
p=Path(__file__).resolve().parent
lines=(p/"profile/sample.txt").read_text().splitlines()
start=lines.index("Call graph:")+1
end=next(i for i,s in enumerate(lines) if s.startswith("Total number in stack"))
nodes=[];stack=[]
for line_number,line in enumerate(lines[start:end],start+1):
    m=re.match(r"^([ +!:|]*)([0-9]+) (.+)$",line)
    if not m:continue
    depth=m.start(2)
    while stack and nodes[stack[-1]]["depth"]>=depth:stack.pop()
    parent=stack[-1] if stack else None
    node={"depth":depth,"inclusive":int(m[2]),"symbol":m[3],"parent":parent,"children":[],"line":line_number}
    if parent is not None:nodes[parent]["children"].append(len(nodes))
    nodes.append(node);stack.append(len(nodes)-1)
roots=[i for i,n in enumerate(nodes) if n["parent"] is None]
assert len(roots)==1 and 'com.apple.main-thread' in nodes[roots[0]]["symbol"]
all_work=collections.Counter();provider=collections.Counter();examples=collections.defaultdict(list)
def classify(path):
    prep=any(x in path for x in ("::prepare_with_flags", "InnerConnection::prepare", "sqlite3Prepare", "sqlite3LockAndPrepare", "sqlite3RunParser"))
    if 'sqlite::lookup::pack_bytes' in path:return 'pack.prepare' if prep else 'pack.execute_copy_other'
    if 'sqlite::pool::group_for' in path:return 'catalogue.prepare' if prep else 'catalogue.execute_decode_other'
    if 'PoolReader::load_group' in path:
        if 'DecompressionWorkspace::decompress_group' in path or 'ZSTD_' in path:return 'value.decompress'
        if 'value_group::authenticate' in path or 'blake3' in path:return 'value.authenticate'
        return 'value.decode_allocate_other'
    if 'PoolReader::record' in path:return 'physical.record_decode_other'
    if 'PoolReader::leaf_canonical_with_groups' in path or 'PoolReader::leaf_body_with_groups' in path:return 'leaf.reconstruct_other'
    return 'provider.other'
for i,n in enumerate(nodes):
    count=n['inclusive']-sum(nodes[c]['inclusive'] for c in n['children'])
    assert count>=0,(n,count)
    if not count:continue
    ancestry=[];cursor=i
    while cursor is not None:
        ancestry.append(nodes[cursor]['symbol']);cursor=nodes[cursor]['parent']
    path='\n'.join(reversed(ancestry))
    kind=classify(path)
    all_work[kind]+=count
    if 'StoreProvider' in path:
        provider[kind]+=count
        if len(examples[kind])<3:examples[kind].append({'line':n['line'],'self_samples':count,'ancestry':list(reversed(ancestry))})
assert sum(all_work.values())==nodes[roots[0]]['inclusive']
result={'kind':'stack-residence samples, not exact CPU or phase nanoseconds','main_thread_samples':sum(all_work.values()),'provider_samples':sum(provider.values()),'provider_disjoint':dict(provider),'all_main_disjoint':dict(all_work),'examples':dict(examples),'coverage':'one native main-thread call tree; report start88ms after reported process launch; no exact first-sample bound'}
print(json.dumps(result,indent=2))
