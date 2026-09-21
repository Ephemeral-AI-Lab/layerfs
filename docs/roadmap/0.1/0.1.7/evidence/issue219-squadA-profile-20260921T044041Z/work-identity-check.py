import json
a=json.load(open('/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000/core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-pinned2-20260921T031259Z/pipeline-namespace-10000/receipt.json'))
b=json.load(open('/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000/core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-squadA-profile-20260921T044041Z/pipeline-namespace-10000/receipt.json'))
ca,cb=a['counters'],b['counters']
common=sorted(set(ca)&set(cb))
print('counters only in pinned2:',sorted(set(ca)-set(cb)))
print('counters only in diagnostic:',sorted(set(cb)-set(ca)))
diff=[(k,ca[k],cb[k]) for k in common if ca[k]!=cb[k]]
print('shared counters compared:',len(common))
print('shared counters that MOVED:',len(diff))
for k,x,y in diff: print('  MOVED',k,x,'->',y)
print('same_work_tree:', a['resources']['space']['canonical_bytes_total']==b['resources']['space']['canonical_bytes_total'])
print('canonical_bytes_total', a['resources']['space']['canonical_bytes_total'], b['resources']['space']['canonical_bytes_total'])
print('canonical_objects_total', a['resources']['space']['canonical_objects_total'], b['resources']['space']['canonical_objects_total'])
print('canonical_objects_equal:', a['resources']['space']['canonical_objects']==b['resources']['space']['canonical_objects'])
ga={g['id']+g['measured'][:0]:g['status'] for g in a['gates']}
print('pinned2 statuses:', sorted({g['status'] for g in a['gates']}))
print('diag statuses:', sorted({g['status'] for g in b['gates']}))
print('diag gates:', len(b['gates']))
print('identity diffs:')
ia,ib=a['identity'],b['identity']
for k in sorted(set(ia)|set(ib)):
    if ia.get(k)!=ib.get(k) and k!='source_dirty_files':
        print('  ',k,'|',str(ia.get(k))[:70],'->',str(ib.get(k))[:70])
print('pinned2 dirty files:', ia.get('source_dirty_files'))
print('diag dirty files:', ib.get('source_dirty_files'))