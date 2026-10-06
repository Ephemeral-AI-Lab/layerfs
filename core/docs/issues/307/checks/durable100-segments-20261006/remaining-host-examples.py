import pathlib,json,subprocess,os,signal,time
checks=pathlib.Path('core/docs/issues/307/checks/durable100-segments-20261006')
selection=json.loads((checks/'remaining-host-examples-selection.json').read_text())['tests']
for path in selection:
 label='host-example-'+pathlib.Path(path).name.rsplit('-',1)[0]
 child=subprocess.run(['python3','/tmp/durable100-segments-command.py',label,'30',path,'--test-threads=1'],capture_output=True,text=True)
 print(label,child.returncode,flush=True)
 if child.returncode:print(child.stdout,child.stderr);raise SystemExit(child.returncode)
