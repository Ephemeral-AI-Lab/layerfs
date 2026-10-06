"""Frozen, qualified cluster-one Project observations; no reference execution."""
import hashlib,json
from pathlib import Path
SOURCE='197d2fb7d0a141d7a9350852022febeec3255bf2'
TREE='dbbe49212b26294024be28986e868c27f4de4825'
VALUES={
    'durable':{100:(79759708,5255168),1000:(201566000,20545536),10000:(2492429625,305070080),100000:(7724523333,514965504)},
    'disposable':{100:(38747750,5222400),1000:(129258375,20537344),10000:(1645276292,305074176),100000:(5558569958,514940928)},
}
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def reference(root,profile,fixture):
    """Read the original candidate row, preserving its name, seals and verdict."""
    root=Path(root);n=int(fixture.split('-')[1])
    path=root/f'core/docs/issues/302/checks/shared-allocation1/{profile}-init{n}.json'
    pair=json.loads(path.read_text());row=next(r for r in pair['rows'] if r['arm']=='candidate')
    audit_path=root/'core/docs/issues/307/checks/monolithic-restoration-20261006/cluster-one-end-baseline-audit.json'
    audit=json.loads(audit_path.read_text())
    if audit['status']!='PASS' or audit['source']!=SOURCE or audit['tree']!=TREE:raise ValueError('qualified cluster-one control audit missing')
    checked=next(r for r in audit['rows'] if r['profile']==profile and r['files']==n)
    if digest(path)!=checked['compact_sha256'] or row['measured_source_commit']!=SOURCE or row['identity']['source_tree']!=TREE or row['identity']['source_dirty']:raise ValueError('cluster-one control source/custody changed')
    if (row['comparison_ns'],row['storage_bytes'])!=VALUES[profile][n]:raise ValueError('wrong regression control operands')
    if row['fixture']['case']!=fixture or row['requested_profile']!=profile or row['status']!='COMPLETE':raise ValueError('wrong control workload/profile/operation')
    if any(row[k]!='PASS' for k in ['cache_status','verification_status']) or row['cleanup']['status']!='PASS' or row['residency']['resident_after']!=0:raise ValueError('control correctness/cache/cleanup gap')
    if row['command_wall_ns']>30_000_000_000 or row['verification_wall_ns']>19_000_000_000:raise ValueError('control bound miss')
    if row['performance']['child']['root']!=checked['root']:raise ValueError('control root changed')
    return {'source_commit':SOURCE,'source_tree':TREE,'original_case':row['case'],
        'original_arm':row['arm'],'original_pair_verdict':pair['status'],
        'original_row_status':row['status'],'reuse_role':'same-profile cluster-one-end public Project Init regression control',
        'comparison_ns':row['comparison_ns'],'storage_bytes':row['storage_bytes'],
        'compact_receipt':str(path),'compact_sha256':digest(path),
        'raw_receipt_sha256':checked['receipt_sha256'],'raw_manifest_sha256':checked['manifest_sha256'],
        'audit':str(audit_path),'audit_sha256':digest(audit_path),
        'fixture':row['fixture'],'root':checked['root'],'effective_profile':row['effective_profile'],
        'product_seal':row['product_seal'],'compilation_seal':row['compilation_seal'],
        'dependency_seal':row['dependency_seal'],'binaries':row['build']['binaries'],
        'old_competitive_reference':checked['older_competitive_reference']}
