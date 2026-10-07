from pathlib import Path
import datetime,hashlib,json,sys,time,subprocess
sys.path.insert(0,'core/benchmark/fs-bench-pro')
from shared import disposable_wal as policy
from families import phase7_sqlite
root=Path.cwd();base=root/'benchmark-results/fs-bench-pro';checks=root/'core/docs/issues/307/checks/disposable-wal-matrix-20261007'
started=time.monotonic_ns();rows=[];verified_files=0;seen_binaries={}
selected=list(policy.ROWS)
def digest(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1048576):h.update(b)
 return h.hexdigest()
for name in selected:
 d=base/f'disposable-wal-matrix-20261007-{name}-candidate'
 r=json.loads((d/'receipt.json').read_text());spec=policy.ROWS[name]
 invocation=base/'disposable-wal-matrix-20261007-wrapper'/(name+'-candidate.json')
 launch=json.loads(invocation.read_text())
 assert launch['exit_code']==0 and not launch['timed_out']
 manifest=json.loads((d/'manifest.json').read_text())
 for rel,pin in manifest['files'].items():
  p=d/rel;assert p.stat().st_size==pin['bytes'] and digest(p)==pin['sha256'],p
  verified_files+=1
 assert r['profile']==policy.PROFILE and r['requested_profile']=='disposable'
 assert r['command_budget_ns']==spec['command_budget_ns'] and r['verification_budget_ns']==spec['verification_budget_ns']
 assert r['identity']['source_dirty'] and len(r['declared_noninputs'])==3
 assert r['measured_source_commit'] in ('5ba8362cb82ce8f210f2cc0fcdb8affffcc1d72b','06dc05a7d6a8e77fb1c5ffa532225ad635920275')
 assert r['report_template_sha256']==digest(root/'benchmark_agent_report.md')
 for b in r['build']['binaries'].values():
  if b['path'] not in seen_binaries:seen_binaries[b['path']]=digest(b['path'])
  assert seen_binaries[b['path']]==b['sha256']
 item={'case':name,'source':r['measured_source_commit'],'raw_receipt':str(d/'receipt.json'),'raw_receipt_sha256':digest(d/'receipt.json'),'wrapper':str(invocation),'wrapper_sha256':digest(invocation),'source_tree':r['identity']['source_tree'],'build':r['build'],'profile':r['profile'],'status':r['status'],'sample_count':r['sample_count'],'original_wrapper_stdout_created_utc':datetime.datetime.fromtimestamp((invocation.with_suffix('.stdout')).stat().st_birthtime,datetime.timezone.utc).isoformat(),'timestamp_scope':'filesystem creation time of the original launcher stdout artifact','fixture':r['fixture'],'cache':r['cache_status'],'command_budget_ns':r['command_budget_ns'],'verification_budget_ns':r['verification_budget_ns'],'image':'N/A native macOS','competing_work':r['competing_work']}
 if r['status']=='INELIGIBLE':
  assert name.endswith('init-100000-owner-wal-v1') and r['sample_count']==0
  assert r['residency']['resident_after']==1626 and not (d/'driver.stdout').exists() and not (d/'store.sqlite').exists()
  item.update(reason='source-cold attestation: 1626 resident pages; no product attempt',verification='NOT_RUN',source_residency=r['residency'])
  rows.append(item);continue
 assert r['status']=='COMPLETE' and r['sample_count']==1 and r['cache_status']=='PASS'
 assert r['residency']['resident_after']==0
 assert r['verification_status']=='PASS' and r['verification']['exit_code']==0 and not r['verification']['timed_out']
 assert r['performance']['exit_code']==0 and not r['performance']['timed_out']
 assert r['command_wall_ns']<=spec['command_budget_ns'] and r['verification_wall_ns']<=spec['verification_budget_ns']
 assert r['effective_profile']['identity']==policy.PROFILE and r['effective_profile']['journal_mode']=='wal' and r['effective_profile']['synchronous']==0
 for ending in ('-wal','-shm','-journal'):assert not Path(str(d/'store.sqlite')+ending).exists()
 assert r['storage_bytes']==sum(f['allocated_bytes'] for f in r['storage']['files'])
 child=r['performance']['child'];proof=r['verification']['child']
 cleanup=r.get('cleanup_status',r.get('cleanup',{}).get('status'))
 assert cleanup=='PASS'
 ref=policy.reference(name)
 if spec['states']:
  n=spec['states'];assert child['states']==child['selected_states']==child['custody_states']==n
  assert proof['states']==proof['custody_states']==n and proof['independent_root_pins']=='CHECKED'
  assert child['roots']==ref['performance']['child']['roots']
  assert r['database_cold']['total']['checks']==n
  assert all(x['attestation']['resident_after']==0 for x in r['database_cold']['boundaries'])
  census=json.loads((d/'census.json').read_text())
  assert census['status']=='CHECKED' and census['owners'][0]['sha256_before']==digest(d/'store.sqlite')==census['owners'][0]['sha256_after']
  assert digest(d/'independent-proof.sqlite')==digest(d/'store.sqlite')
  item['canonical_census']=census
 else:
  assert child['public_api_call_count']==1 and child['root']==ref['performance']['child']['root']
  assert proof['workers']==4 and proof['discovered_files']==r['fixture']['files']
 post=[]
 for f in ('store.sqlite','independent-proof.sqlite'):
  p=d/f
  if p.exists():
   s=p.stat();post.append({'file':f,'length_bytes':s.st_size,'allocated_bytes':s.st_blocks*512,'allocation_minus_length':s.st_blocks*512-s.st_size})
 c=policy.comparison(r,name)
 item.update(product_ns=r['comparison_ns'],complete_performance_ns=r['command_wall_ns'],verification_ns=r['verification_wall_ns'],allocated_store_bytes_at_measurement=r['storage_bytes'],storage_files_at_measurement=r['storage']['files'],post_proof_file_stat=post,post_proof_file_stat_scope='later read-only observation; never replaces measured original allocation',verification='PASS',cleanup=cleanup,proof=proof,comparison=c,cpu_ns=r['performance']['cpu_ns'],lifetime_peak_rss_bytes=r['performance']['peak_rss_bytes'],rss_scope=r['performance']['rss_scope'])
 if 'incumbent' in c:
  old=c['incumbent'];item['versus_retained_incumbent']={'time_delta_percent':100*(r['comparison_ns']-old['comparison_ns'])/old['comparison_ns'],'allocated_delta_bytes':r['storage_bytes']-old['storage_bytes'],'allocated_delta_percent':100*(r['storage_bytes']-old['storage_bytes'])/old['storage_bytes']}
 rows.append(item)
completed=[r for r in rows if r['status']=='COMPLETE']
assert len(completed)==7
assert sum(r['comparison']['historical_speed_arithmetic']=='FAIL' for r in completed)==3
assert [r['case'] for r in completed if r['comparison'].get('allocation_gate')=='FAIL']==['phase7-sqlite-disposable-history-stride1-group-rows-indexed-wal-v1']
record={'schema':'disposable-wal-matrix-report-v1','timestamp_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'receipt_integrity':'PASS','completed_product_cases':7,'retained_ineligible_zero_sample_cases':1,'historical_speed_arithmetic_failures':3,'allocation_failures':1,'admission_eligible':False,'durable_execution':'NOT_RUN — disabled by owner until explicit reauthorization','rows':rows,'verified_manifest_files':verified_files,'verified_binary_hashes':seen_binaries,'validation_ns':time.monotonic_ns()-started,'retention':'seven original closed Stores, three independent proof copies, original and corrected source preparations, all receipts retained; no unknown product outcome'}
with (checks/'29-matrix-report.json').open('x') as f:json.dump(record,f,indent=2);f.write('\n')
print(json.dumps({k:v for k,v in record.items() if k not in ('rows','verified_binary_hashes')},indent=2))
