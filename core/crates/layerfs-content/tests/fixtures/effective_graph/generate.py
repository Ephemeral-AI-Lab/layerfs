"""Independent bounded graph vectors; no LayerFS/candidate imports.

Exact nonzero transitive closure uses Floyd-Warshall over at most16 vertices.
The literal verdict is checked before emitting fixture code. This is test-data
preparation, not a benchmark runner or product graph implementation.
"""
from pathlib import Path
import hashlib,json

CASES = [
 ('no_seed_old_cycle','update',1,[1,2,3],[(2,3),(3,2)],[],[],[],'pass'),
 ('isolated_selected_child','update',1,[1,2],[(1,2)],[(1,2)],[],[],'pass'),
 ('selected_self_loop','update',1,[1,2],[(2,2)],[(2,2)],[],[],'cycle'),
 ('old_descendant_cycle_without_seed_return','update',1,[1,2,3,4],[(1,2),(2,3),(3,4),(4,3)],[(1,2)],[],[],'pass'),
 ('selected_descendant_in_old_cycle','update',1,[1,2,3,4],[(1,2),(2,3),(3,4),(4,3)],[(3,4)],[],[],'cycle'),
 ('selected_child_reaches_parent','update',1,[1,2,3],[(1,2),(2,3),(3,1)],[(1,2)],[],[],'cycle'),
 ('disconnected_selected_component_cycle','update',1,[1,2,9],[(9,2),(2,9)],[(9,2)],[],[],'cycle'),
 ('overlapping_seed_closures','update',1,[1,2,3,4,5],[(1,2),(2,3),(3,4),(4,5)],[(1,2),(2,3),(3,4)],[],[],'pass'),
 ('final_batch_two_edge_cycle','update',1,[1,2,3],[(1,2),(2,3),(3,2)],[(1,2),(2,3),(3,2)],[],[],'cycle'),
 ('acyclic_seed_plus_cyclic_seed','update',1,[1,2,3,4,5,6],[(1,2),(2,3),(4,5),(5,6),(6,5)],[(1,2),(4,5)],[],[],'cycle'),
 ('diamond_old_alias_dag','update',1,[1,2,3,4,5],[(1,2),(2,3),(2,4),(3,5),(4,5)],[(1,2)],[],[],'pass'),
 ('empty_fresh_root','fresh',1,[1],[],[],[1],[],'pass'),
 ('fresh_chain','fresh',1,[1,2,3,4],[(1,2),(2,3),(3,4)],[],[1,2,3,4],[],'pass'),
 ('fresh_disconnected_directory','fresh',1,[1,2],[],[],[1,2],[],'cycle'),
 ('fresh_disconnected_cycle','fresh',1,[1,2,3],[(2,3),(3,2)],[],[1,2,3],[],'cycle'),
 ('fresh_excluded_parent_does_not_reach_child','fresh',1,[1,2,3],[(2,3)],[],[1,2,3],[2],'cycle'),
 ('fresh_excluded_component','fresh',1,[1,2,3],[(2,3)],[],[1,2,3],[2,3],'pass'),
 ('fresh_reachable_multiple_parents','fresh',1,[1,2,3,4],[(1,2),(1,3),(2,4),(3,4)],[],[1,2,3,4],[],'multiple_parents'),
 ('fresh_repeated_parent_binding','fresh',1,[1,2],[(1,2),(1,2)],[],[1,2],[],'multiple_parents'),
 ('maximum_serial_selected_cycle','update',1,[1,9223372036854775806,9223372036854775807],[(1,9223372036854775806),(9223372036854775806,9223372036854775807),(9223372036854775807,9223372036854775806)],[(1,9223372036854775806),(9223372036854775806,9223372036854775807)],[],[],'cycle'),
]

def expected(case):
 _,mode,root,vertices,edges,seeds,declared,excluded,_=case
 assert len(vertices)<=16 and len(set(vertices))==len(vertices)
 index={v:i for i,v in enumerate(vertices)}; n=len(vertices)
 reach=[[False]*n for _ in range(n)]
 for u,v in edges:reach[index[u]][index[v]]=True
 for k in range(n):
  for i in range(n):
   for j in range(n):reach[i][j] = reach[i][j] or (reach[i][k] and reach[k][j])
 if mode=='update':
  for parent,child in seeds:
   assert (parent,child) in edges
   if reach[index[child]][index[child]] or reach[index[child]][index[parent]]:return 'cycle'
  return 'pass'
 visited={root}|{v for v in vertices if reach[index[root]][index[v]]}
 # Excluded parents are never expanded in the old fresh checker.
 if excluded:
  visited={root}; pending=[root]
  while pending:
   parent=pending.pop()
   if parent in excluded:continue
   for u,v in edges:
    if u==parent and v not in visited:visited.add(v);pending.append(v)
 counts={v:sum(1 for u,w in edges if w==v and u in visited and u not in excluded) for v in declared if v!=root and v not in excluded}
 if any(c>1 for c in counts.values()):return 'multiple_parents'
 if any(c==0 for c in counts.values()):return 'cycle'
 return 'pass'

if __name__=='__main__':
 for c in CASES:assert expected(c)==c[-1],(c[0],expected(c),c[-1])
 here=Path(__file__).parent
 data=[dict(zip(['name','mode','root','vertices','edges','seeds','declared','excluded','expected'],c)) for c in CASES]
 payload=json.dumps({'method':'nonzero Floyd-Warshall; fresh root walk and exact incoming observations; <=16 vertices','cases':data},indent=2)+'\n'
 (here/'vectors.json').write_text(payload)
 out='// Generated independent test fixtures; see generate.py and vectors.json.\n'
 out+='pub struct Case { pub name: &\'static str, pub fresh: bool, pub root: u64, pub vertices: &\'static [u64], pub edges: &\'static [(u64,u64)], pub seeds: &\'static [(u64,u64)], pub declared: &\'static [u64], pub excluded: &\'static [u64], pub expected: &\'static str }\n'
 out+='pub const CASES: &[Case] = &[\n'
 nums=lambda xs:'&['+','.join(map(str,xs))+']'
 pairs=lambda xs:'&['+','.join(f'({u},{v})' for u,v in xs)+']'
 for name,mode,root,vs,es,ss,ds,us,verdict in CASES:
  out+=f'Case{{name:"{name}",fresh:{str(mode=="fresh").lower()},root:{root},vertices:{nums(vs)},edges:{pairs(es)},seeds:{pairs(ss)},declared:{nums(ds)},excluded:{nums(us)},expected:"{verdict}"}},\n'
 out+='];\n';(here/'manifest.rs').write_text(out)
 print(json.dumps({'vectors':len(data),'vectors_sha256':hashlib.sha256(payload.encode()).hexdigest(),'manifest_sha256':hashlib.sha256(out.encode()).hexdigest()}))
