"""SQLite-only prospective gate and cold-source contract; no service imports."""
from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'core/benchmark/fs-bench-pro-storage-content/shared'))
import residency
CACHE='phase7-cold-content-fresh-database-complete-lifecycle-v1'
PROFILE='sqlite-wal-full-macos-fullfsync-v1'

def dewarm_tree(root):
    root=Path(root)
    if root.is_symlink() or not root.is_dir(): raise ValueError('ordinary source directory required')
    paths=[]
    for p in sorted(root.rglob('*')):
        if p.is_symlink(): raise ValueError('symlink in source inventory')
        if p.is_file(): paths.append(p)
    if not paths: raise ValueError('empty input inventory')
    first=invalidated=pages=length=after=0
    for p in paths:
        r=residency.de_warm(p);first+=r.resident_first;invalidated+=r.invalidated
    for p in paths:
        r=residency.residency(p);after+=r.resident_pages;pages+=r.total_pages;length+=r.length_bytes
    return {'status':'PASS' if after==0 else 'INELIGIBLE','files':len(paths),'length_bytes':length,
            'resident_first':first,'resident_after':after,'total_pages':pages,'invalidated_files':invalidated,
            'method':'msync-invalidate followed by whole-input mincore; no payload reads',
            'scope':'regular-file content pages; filesystem metadata residency not directly observed'}

def gate(candidate,baseline,storage_limit,exclusive_storage=False):
    for row in (candidate,baseline):
        if row.get('cache_status')!='PASS': return 'INELIGIBLE'
        if row.get('status')!='COMPLETE' or row.get('verification_status')!='PASS' or row.get('cleanup',{}).get('status')!='PASS': return 'INCOMPLETE'
        if row['command_wall_ns']>row['command_budget_ns'] or row['verification_wall_ns']>row['verification_budget_ns']:return 'FAIL'
        if type(row.get('comparison_ns')) is not int or row['comparison_ns']<=0:return 'INCOMPLETE'
    if 10*candidate['comparison_ns']>11*baseline['comparison_ns']:return 'FAIL'
    if storage_limit is None or type(candidate.get('storage_bytes')) is not int:return 'INCOMPLETE'
    fits=candidate['storage_bytes']<storage_limit if exclusive_storage else candidate['storage_bytes']<=storage_limit
    return 'PASS' if fits else 'FAIL'

def allocations(paths):
    items=[]
    for p in map(Path,paths):
        for suffix in ('','-wal','-shm'):
            f=Path(str(p)+suffix)
            if f.exists():
                s=f.stat();items.append({'path':str(f),'length_bytes':s.st_size,'allocated_bytes':s.st_blocks*512})
    return {'files':items,'total_bytes':sum(i['allocated_bytes'] for i in items),
            'scope':'database/WAL/SHM at this observation; not an unobserved peak'}
