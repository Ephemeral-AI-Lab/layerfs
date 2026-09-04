import copy, hashlib, importlib.util, json, os, pathlib, sys, tempfile
from unittest.mock import patch
source=pathlib.Path('benchmark/fs-bench-pro/workspace-runner.py').resolve()
spec=importlib.util.spec_from_file_location('producer_runner',source);r=importlib.util.module_from_spec(spec);sys.modules[spec.name]=r;spec.loader.exec_module(r)
root=pathlib.Path('benchmark-results/fs-bench-pro/phase1-v013').resolve(); old=root/'assets-b8c2ad4b'; new=root/'assets-34224330'
a=json.loads((new/'evidence/build.json').read_text());b=json.loads((old/'evidence/build.json').read_text())
# Sealed metadata/system comparison is real; binary validation is mocked in this
# product-free selection model and remains mandatory in actual runner execution.
a['workspace_preparation_compatibility']=b['workspace_preparation_compatibility']='qualified-compatibility'
registry=[{'scenario_id':'git-tool-10','operation':'git-tool','family_id':'git_tool_workflow','tier':10}]
assert r.git_system_identity(old,b)==r.git_system_identity(new,a)
with patch.object(r,'sealed_build',return_value=copy.deepcopy(b)),patch.object(r,'command',return_value=json.dumps(registry[0])):
 selected=r.select_preparation(new,a,old,registry,registry)
 assert selected['revision']==b['revision'] and a['revision']!=selected['revision']
 assert selected['git_system_identity_sha256']
 for kind in ['compatibility','contract','registry','system']:
  changed=copy.deepcopy(b); expected_registry=registry
  if kind=='compatibility':changed['workspace_preparation_compatibility']='different'
  if kind=='contract':changed['phase1_contract_files']['docs/roadmap/0.1/0.1.3/git-tool-workflow.md']='different'
  if kind=='registry':expected_registry=[dict(registry[0],tier=11)]
  with patch.object(r,'sealed_build',return_value=changed):
   try:
    if kind=='system':
     with patch.object(r,'git_system_identity',side_effect=[{'system':'old'},{'system':'different'}]):r.select_preparation(new,a,old,registry,registry)
    else:r.select_preparation(new,a,old,expected_registry,registry)
   except ValueError:pass
   else:raise AssertionError('accepted '+kind+' mismatch')
# Explicit producer uses old binary/image/plan on both Store and oracle paths;
# runtime info is checked with that same image and never becomes the producer.
with tempfile.TemporaryDirectory() as tmp:
 t=pathlib.Path(tmp); run=t/'one';run.mkdir(); again=t/'two';again.mkdir(); ref=t/'ref';ref.mkdir()
 producer_binary=str(old/'fs-benchmark-pro'); runtime_binary=str(new/'fs-benchmark-pro')
 calls=[]; publishes=[]; acquisitions={}; plans={False:'producer-store-plan',True:'producer-reference-plan'}
 original={'revision':'earlier-compatible-master','image_id':b['image_id'],'status':'pass'}
 def info(argv,**kwargs):
  calls.append((list(argv),os.environ.get('LAYERFS_V013_IMAGE')))
  is_ref=argv[1]=='workspace-reference-info'
  return json.dumps({'input_plan_sha256':plans[is_ref],'input_mode':'directory' if is_ref else 'store'})
 def acquire(cache,binary,size,path,evidence,**kwargs):
  publishes.append((binary,evidence,dict(kwargs),os.environ.get('LAYERFS_V013_IMAGE')))
  r.custody.write_json(path,{'producer':original,'fixture':kwargs['workspace_expected'],'key':'original-key','cache_disposition':'hit','prepared_path':'original-master'})
 with patch.dict(os.environ,{'LAYERFS_V013_IMAGE':b['image_id']}),patch.object(r,'command',side_effect=info),patch.object(r.custody,'acquire_prepared',side_effect=acquire):
  first=r.acquire(registry[0],1,producer_binary,t,run,old/'evidence',acquisitions,b,runtime_binary=runtime_binary)
  second=r.acquire(registry[0],1,producer_binary,t,again,old/'evidence',acquisitions,b,runtime_binary=runtime_binary)
  oracle=r.acquire(registry[0],1,producer_binary,t,ref,old/'evidence',acquisitions,b,reference=True,runtime_binary=runtime_binary)
  assert first['producer']==second['producer']==oracle['producer']==original
  assert second['run_acquisition_reused'] and len(publishes)==2
  assert all(binary==producer_binary and image==b['image_id'] for binary,evidence,kw,image in publishes)
  assert [p[2]['workspace_reference'] for p in publishes]==[False,True]
  assert all(image==b['image_id'] for argv,image in calls)
  assert {argv[0] for argv,image in calls}=={producer_binary,runtime_binary}
 with patch.object(r,'command',side_effect=[json.dumps({'plan':'producer'}),json.dumps({'plan':'runtime'})]),patch.object(r.custody,'acquire_prepared') as publish:
  try:r.acquire(registry[0],1,producer_binary,t,run,old/'evidence',{},b,runtime_binary=runtime_binary)
  except ValueError:pass
  else:raise AssertionError('fixture mismatch accepted')
  publish.assert_not_called()
# Runtime creation/measurement stays wired exclusively to runtime assets.
body=source.read_text();assert 'LAYERFS_V013_IMAGE=assets["image_id"]' in body
assert 'name, assets["image_id"]], env=env)' in body
assert 'argv = [binary, "workspace-run"' in body
assert 'os.environ["LAYERFS_V013_IMAGE"] = producer["image_id"]' in body
result={'status':'pass','source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'checks':['actual b8/342 immutable Git system identity match','producer/runtime selection separated','compatibility/contract/registry/system mismatch rejected','Store and reference use producer image/binary/plan','same-run acquisition reused','original master provenance unchanged','runtime fixture mismatch rejects before acquisition','runtime execution image/binary preserved'],'scope':'Mocked selection/acquisition model plus sealed image metadata comparison; no benchmark binary, Docker container, product execution or build.'}
out=pathlib.Path(__file__).resolve().parent;(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
