#!/usr/bin/env python3
import argparse, csv, hashlib, json, os, pathlib, secrets, shutil, stat, subprocess, sys, tempfile, time

HERE=pathlib.Path(__file__).resolve().parent; REPO=HERE.parents[4]; TARGET=REPO/"target"
CONTRACT=HERE/"method/METHOD-CONTRACT-v2.json"; SCHEDULE=HERE/"method/SCHEDULE-v2.tsv"; EXPECTED=HERE/"method/EXPECTED-OUTCOMES-v2.tsv"; COVERAGE=HERE/"COVERAGE-MAP-v2.tsv"; REUSED=HERE/"REUSED-AUTHORITY-v2.json"
PRIMARY=HERE/"analyzers/primary.py"; INDEPENDENT=HERE/"analyzers/independent.py"; READINESS=HERE/"PREMEASUREMENT-READINESS-v2.json"; INPUT_MANIFEST=HERE/"method/INPUT-MANIFEST-v2.json"; PREPARATION_TERMINAL=HERE/"method/PREPARATION-TERMINAL-v2.json"; FREEZE=HERE/"method/SOURCE-FREEZE-v2.json"; FORECAST=HERE/"ZERO-ROW-FORECAST-v2.json"
FOCUSED=HERE/"FOCUSED-TEST-ATTEMPTS-v2.json"; DISPOSITION=HERE/"PREMEASUREMENT-REVISE-v2.json"; FOCUSED_ATTEMPT=HERE/"evidence/focused-attempt-1"
STATIC_CLOSURE=HERE/"POST-SCREEN-STATIC-CLOSURE-v2.json"; STATIC_EVIDENCE=HERE/"evidence/post-screen-static-closure"
LOCK=TARGET/"phase4-g5-history-integration"/"BENCHMARK_LOCK"; RESULTS={"screen":"phase4-g5-history-integration-v2-screen","gate":"phase4-g5-history-integration-v2-gate"}

compact=lambda value: json.dumps(value,sort_keys=True,separators=(",",":"))
def sha(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def write(path,value):
    path=pathlib.Path(path); path.parent.mkdir(parents=True,exist_ok=True); temporary=path.with_name(path.name+".tmp")
    with temporary.open("w") as handle: handle.write(compact(value)+"\n"); handle.flush(); os.fsync(handle.fileno())
    os.replace(temporary,path); fsync_dir(path.parent)
def write_text(path,value):
    path=pathlib.Path(path); path.parent.mkdir(parents=True,exist_ok=True)
    with path.open("w") as handle: handle.write(value); handle.flush(); os.fsync(handle.fileno())
def fsync_dir(path):
    descriptor=os.open(path,os.O_RDONLY)
    try: os.fsync(descriptor)
    finally: os.close(descriptor)
def load(path): return json.loads(pathlib.Path(path).read_text())

def verify_reused():
    reused=load(REUSED)
    if reused.get("status")!="PASS": raise RuntimeError("reused authority status")
    for milestone in reused["authorities"].values():
        for value in milestone.values():
            if isinstance(value,dict) and "path" in value and "sha256" in value:
                path=REPO/value["path"]
                if not path.is_file() or sha(path)!=value["sha256"]: raise RuntimeError("reused authority bytes changed")
    return reused

def verify_product_binding(ready):
    for item in ready["product"]["source_files"]:
        source=REPO/item["path"]
        if not source.is_file() or source.stat().st_size!=item["bytes"] or sha(source)!=item["sha256"]: raise RuntimeError("product source custody changed")
    executable=REPO/ready["product"]["executable_path"]
    if not executable.is_file() or executable.stat().st_size!=ready["product"]["executable_bytes"] or sha(executable)!=ready["product"]["executable_sha256"]: raise RuntimeError("product executable custody changed")
    return executable

def verify_readiness_method(ready):
    files={"coverage_map":COVERAGE,"expected_outcomes":EXPECTED,"focused_ledger":FOCUSED,"independent":INDEPENDENT,"limitations":HERE/"LIMITATIONS-v2.md","method_contract":CONTRACT,"premeasurement_revise":DISPOSITION,"preregistration":HERE/"PREREGISTRATION-v2.md","primary":PRIMARY,"reused_authority":REUSED,"runner":pathlib.Path(__file__),"schedule":SCHEDULE}
    if any(ready.get("method_hashes",{}).get(name)!=sha(path) for name,path in files.items()): raise RuntimeError("readiness method bytes changed")

def require_ready(action):
    ready=load(READINESS); contract=load(CONTRACT); verify_reused()
    if ready.get("status")!="PASS" or ready.get("authorization",{}).get(action) is not True or contract["product_cli"]["status"]!="SETTLED": raise RuntimeError("G5-3 remains PREMEASUREMENT_REVISE")
    verify_readiness_method(ready); executable=verify_product_binding(ready)
    return ready,contract,executable

def root(label): return hashlib.sha256(label.encode()).hexdigest()
def analyzer(script,raw,output,self_check=False): return subprocess.run([sys.executable,str(script),str(raw),str(output)]+(["--self-check-authority"] if self_check else []),capture_output=True,text=True)

def verify_external_operands(contract):
    observed={}
    for name,expected in contract["external_input_operands"].items():
        path=REPO/expected["path"]; stat=path.stat()
        if not path.is_file() or stat.st_size!=expected["bytes"] or sha(path)!=expected["sha256"]: raise RuntimeError(f"input operand {name} custody")
        if "mode" in expected and stat.st_mode&0o7777!=expected["mode"]: raise RuntimeError(f"input operand {name} mode")
        if "data_rows" in expected and sum(1 for _ in path.open())-1!=expected["data_rows"]: raise RuntimeError(f"input operand {name} rows")
        observed[name]={"path":str(path),"bytes":stat.st_size,"mode":stat.st_mode&0o7777,"sha256":expected["sha256"]}
    return observed

def owned_root(contract): return REPO/contract["owned_input"]["root"]
def make_owned_tree_writable(root):
    if root.exists():
        os.chmod(root,0o755)
        for path in root.iterdir():
            if path.is_file(): os.chmod(path,0o644)

def verify_owned_manifest(contract,executable):
    external=verify_external_operands(contract); manifest=load(INPUT_MANIFEST); root=owned_root(contract)
    root_status=root.lstat() if root.exists() else None; expected_names=set(contract["owned_input"]["files"].values())
    if manifest.get("status")!="PASS" or manifest.get("classification")!="ExactCopyFsyncSealReopenRehashNoRegeneration" or manifest.get("external_source_sha256")!={name:item["sha256"] for name,item in external.items()} or manifest.get("executable_sha256")!=sha(executable) or root_status is None or not stat.S_ISDIR(root_status.st_mode) or stat.S_ISLNK(root_status.st_mode) or root_status.st_mode&0o7777!=contract["owned_input"]["root_mode"] or {path.name for path in root.iterdir()}!=expected_names: raise RuntimeError("owned input manifest authority")
    observed={}
    if set(manifest.get("operands",{}))!=set(contract["external_input_operands"]): raise RuntimeError("owned operand names")
    for name,expected in contract["external_input_operands"].items():
        path=root/contract["owned_input"]["files"][name]; item=manifest["operands"][name]; status=path.lstat()
        if item.get("path")!=str(path) or not stat.S_ISREG(status.st_mode) or stat.S_ISLNK(status.st_mode) or status.st_mode&0o7777!=contract["owned_input"]["file_mode"] or status.st_size!=expected["bytes"] or sha(path)!=expected["sha256"] or item.get("bytes")!=expected["bytes"] or item.get("sha256")!=expected["sha256"] or item.get("mode")!=contract["owned_input"]["file_mode"]: raise RuntimeError(f"owned input {name} custody")
        if "data_rows" in expected and sum(1 for _ in path.open())-1!=expected["data_rows"]: raise RuntimeError("owned expected rows")
        observed[name]=item
    return observed

def self_check():
    verify_reused(); contract=load(CONTRACT)
    if contract["campaigns"]["gate"]["checkpoint_revisions"]!=[1,10,100,1000] or contract["full_reconstruction"]!={"gate_count":5,"required_at":"every pre-edit checkpoint and distinct terminal end after final N-to-N+1 edit","screen_count":3}: raise RuntimeError("method contract")
    focused_raw=FOCUSED_ATTEMPT/"RAW-ENVELOPE.json"
    if not focused_raw.is_file(): raise RuntimeError("durable focused v2 receipt required")
    observed=load(focused_raw)
    mutations=["child","edit","range","policy-root-route-constant","reconstruction","aba","historical","concurrency","terminal","rss","wall"]
    with tempfile.TemporaryDirectory() as directory:
        directory=pathlib.Path(directory)
        for label in ["valid","valid-final",*mutations]:
                row=json.loads(compact(observed))
                if label=="valid-final": row.update(analysis_stage="final",complete_wall_ns=1_000_000_000,lock_released=True,terminal_work_roots=0)
                if label=="child": row["children_reaped"]=0
                elif label=="edit": row["product"]["edits"][-1]["commits"]=0
                elif label=="range": row["product"]["checkpoints"][-1]["range_bytes"]=4095
                elif label=="policy-root-route-constant": row["product"]["checkpoints"][-1]["projection"]["exact_result_root"]=row["product"]["checkpoints"][-1]["projection"]["latest_result_root"]
                elif label=="reconstruction": row["product"]["reconstructions"].append(row["product"]["reconstructions"][-1])
                elif label=="aba": row["product"]["aba"]["final_root"]=root("wrong")
                elif label=="historical": row["product"]["historical_read"]["bytes"]=4095
                elif label=="concurrency": row["product"]["concurrency"]["writer_commits"]=0
                elif label=="terminal": row["product"]["terminal"]["q_terminal"]=1
                elif label=="rss": row["maximum_resident_set_size"]=40_000_000
                elif label=="wall": row.update(analysis_stage="final",complete_wall_ns=contract["campaigns"]["screen"]["complete_wall_hard_limit_ns"]+1,lock_released=True,terminal_work_roots=0)
                raw=directory/f"screen-{label}.json"; write(raw,row); normalized=[]
                for script,name in ((PRIMARY,"p"),(INDEPENDENT,"i")):
                    output=directory/f"screen-{label}-{name}.json"; result=analyzer(script,raw,output,True)
                    if result.returncode: raise RuntimeError(result.stderr)
                    normalized.append(load(output)["normalized"])
                if normalized[0]!=normalized[1] or normalized[0]["status"]!=("PASS" if label.startswith("valid") else "REVISE"): raise RuntimeError(f"analyzer mutation screen/{label}")
    result={"schema":"phase4-g5-3-runner-self-check-v2","status":"PASS","actual_receipt_sha256":sha(focused_raw),"mutation_cases":len(mutations),"analyzer_decisions":2*(2+len(mutations)),"product_processes":0,"product_rows":0}; print(compact(result)); return result

def prepare(executable,input_root):
    ready,contract,bound=require_ready("prepare"); executable=pathlib.Path(executable).resolve()
    root=owned_root(contract)
    if executable!=bound.resolve() or input_root is not None or INPUT_MANIFEST.exists() or PREPARATION_TERMINAL.exists() or root.exists(): raise RuntimeError("one-shot owned-input preparation custody")
    started=time.monotonic_ns(); external=verify_external_operands(contract); root.mkdir(parents=True); operands={}
    try:
        for name,source in external.items():
            destination=root/contract["owned_input"]["files"][name]; shutil.copyfile(source["path"],destination)
            descriptor=os.open(destination,os.O_RDONLY)
            try: os.fsync(descriptor); os.fchmod(descriptor,contract["owned_input"]["file_mode"]); os.fsync(descriptor)
            finally: os.close(descriptor)
        fsync_dir(root); os.chmod(root,contract["owned_input"]["root_mode"]); fsync_dir(root); fsync_dir(root.parent)
        for name,source in external.items():
            path=root/contract["owned_input"]["files"][name]; status=path.lstat()
            if not stat.S_ISREG(status.st_mode) or stat.S_ISLNK(status.st_mode) or status.st_mode&0o7777!=contract["owned_input"]["file_mode"] or status.st_size!=source["bytes"] or sha(path)!=source["sha256"]: raise RuntimeError(f"sealed owned input {name}")
            if name=="expected_roots" and sum(1 for _ in path.open())-1!=1001: raise RuntimeError("sealed expected roots rows")
            operands[name]={"path":str(path),"bytes":status.st_size,"mode":status.st_mode&0o7777,"sha256":source["sha256"]}
        phase_elapsed=time.monotonic_ns()-started
        manifest={"schema":"phase4-g5-3-input-manifest-v2","status":"PASS","classification":"ExactCopyFsyncSealReopenRehashNoRegeneration","copy_seal_rehash_elapsed_ns":phase_elapsed,"external_source_sha256":{name:item["sha256"] for name,item in external.items()},"owned_root":str(root),"owned_root_mode":root.stat().st_mode&0o7777,"operands":operands,"executable_sha256":sha(executable)}; write(INPUT_MANIFEST,manifest)
        complete=time.monotonic_ns()-started; status="PASS" if complete<contract["limits"]["preparation_wall_ns"] else "REVISE"
        terminal={"schema":"phase4-g5-3-preparation-terminal-v2","status":status,"classification":"CompleteWallThroughOwnedSealReopenRehashAndInputManifestFsync","complete_wall_ns":complete,"target_ns":contract["limits"]["preparation_target_ns"],"hard_limit_ns":contract["limits"]["preparation_wall_ns"],"within_target":complete<contract["limits"]["preparation_target_ns"],"owned_root":str(root),"owned_root_mode":root.stat().st_mode&0o7777}; write(PREPARATION_TERMINAL,terminal)
        if status!="PASS": raise RuntimeError("owned input preparation wall exceeded")
        verify_owned_manifest(contract,executable); return manifest
    except Exception:
        make_owned_tree_writable(root); shutil.rmtree(root,ignore_errors=True); raise

def freeze(executable):
    ready,contract,bound=require_ready("freeze"); executable=pathlib.Path(executable).resolve()
    if executable!=bound.resolve() or FREEZE.exists() or not INPUT_MANIFEST.is_file() or load(PREPARATION_TERMINAL).get("status")!="PASS": raise RuntimeError("freeze custody")
    external=verify_external_operands(contract); owned=verify_owned_manifest(contract,executable)
    if {name:(item["bytes"],item["sha256"]) for name,item in external.items()}!={name:(item["bytes"],item["sha256"]) for name,item in owned.items()}: raise RuntimeError("external/owned input equality")
    focused_files=sorted(path for path in FOCUSED_ATTEMPT.iterdir() if path.is_file()) if FOCUSED_ATTEMPT.is_dir() else []
    files=[pathlib.Path(__file__),PRIMARY,INDEPENDENT,CONTRACT,SCHEDULE,EXPECTED,COVERAGE,REUSED,READINESS,DISPOSITION,FOCUSED,HERE/"FREEZE-ATTEMPT-1-FAILURE-v2.json",*focused_files,INPUT_MANIFEST,PREPARATION_TERMINAL,HERE/"PREREGISTRATION-v2.md",HERE/"LIMITATIONS-v2.md",*(REPO/item["path"] for item in ready["product"]["source_files"]),*(REPO/item["path"] for item in contract["external_input_operands"].values()),*(pathlib.Path(item["path"]) for item in owned.values())]
    receipt={"schema":"phase4-g5-3-source-freeze-v2","status":"FROZEN_BEFORE_FORECAST","method_contract_sha256":sha(CONTRACT),"schedule_sha256":sha(SCHEDULE),"expected_outcomes_sha256":sha(EXPECTED),"coverage_map_sha256":sha(COVERAGE),"reused_authority_sha256":sha(REUSED),"input_manifest_sha256":sha(INPUT_MANIFEST),"preparation_terminal_sha256":sha(PREPARATION_TERMINAL),"external_owned_equality":True,"executable":str(executable),"executable_sha256":sha(executable),"authoritative_files":[{"path":str(path.relative_to(REPO)),"bytes":path.stat().st_size,"sha256":sha(path)} for path in files]}; write(FREEZE,receipt); return receipt

def verify_freeze():
    frozen=load(FREEZE)
    contract=load(CONTRACT); executable=pathlib.Path(frozen["executable"]); external=verify_external_operands(contract); owned=verify_owned_manifest(contract,executable)
    if frozen.get("status")!="FROZEN_BEFORE_FORECAST" or frozen.get("method_contract_sha256")!=sha(CONTRACT) or frozen.get("schedule_sha256")!=sha(SCHEDULE) or frozen.get("expected_outcomes_sha256")!=sha(EXPECTED) or frozen.get("coverage_map_sha256")!=sha(COVERAGE) or frozen.get("reused_authority_sha256")!=sha(REUSED) or frozen.get("input_manifest_sha256")!=sha(INPUT_MANIFEST) or frozen.get("preparation_terminal_sha256")!=sha(PREPARATION_TERMINAL) or frozen.get("external_owned_equality") is not True or {name:(item["bytes"],item["sha256"]) for name,item in external.items()}!={name:(item["bytes"],item["sha256"]) for name,item in owned.items()}: raise RuntimeError("freeze changed")
    for item in frozen["authoritative_files"]:
        path=REPO/item["path"]
        if path.stat().st_size!=item["bytes"] or sha(path)!=item["sha256"]: raise RuntimeError("authoritative bytes changed")
    if sha(frozen["executable"])!=frozen["executable_sha256"]: raise RuntimeError("executable changed")
    return frozen

def forecast():
    require_ready("forecast"); verify_freeze(); contract=load(CONTRACT)
    gate=1002*35_000_000+4*1_000_000_000+10_000_000_000+5_000_000_000; screen=12*35_000_000+2*1_000_000_000+10_000_000_000+3_000_000_000
    receipt={"schema":"phase4-g5-3-zero-row-forecast-v2","status":"PASS" if gate<=contract["limits"]["gate_wall_ns"] and screen<contract["limits"]["screen_wall_ns"] else "REVISE","product_processes":0,"product_rows":0,"screen_forecast_ns":screen,"gate_forecast_ns":gate,"screen_limit_ns":contract["limits"]["screen_wall_ns"],"gate_limit_ns":contract["limits"]["gate_wall_ns"],"gate_reserve_ns":contract["limits"]["gate_wall_ns"]-gate,"classification":"ProspectiveFeasibilityNotTimingEvidence"}; write(FORECAST,receipt); return receipt

def rss(text):
    for line in text.splitlines():
        if "maximum resident set size" in line: return int(line.split()[0])
    raise RuntimeError("RSS missing")

def adopt_focused():
    ready,contract=load(READINESS),load(CONTRACT); verify_reused(); verify_product_binding(ready)
    if FOCUSED_ATTEMPT.exists(): raise RuntimeError("one-shot focused evidence adoption")
    source=contract["focused_source_evidence"]; names={"command":"COMMAND.txt","return_code":"RETURN-CODE.txt","rss":"RSS.txt","stderr":"STDERR.txt","stdout":"STDOUT.txt"}; FOCUSED_ATTEMPT.mkdir(parents=True)
    for key,name in names.items():
        item=source[key]; path=REPO/item["path"]
        if not path.is_file() or path.stat().st_size!=item["bytes"] or sha(path)!=item["sha256"]: raise RuntimeError(f"focused source {key} custody")
        destination=FOCUSED_ATTEMPT/name; shutil.copyfile(path,destination); descriptor=os.open(destination,os.O_RDONLY)
        try: os.fsync(descriptor)
        finally: os.close(descriptor)
    if (FOCUSED_ATTEMPT/"RETURN-CODE.txt").read_text().strip()!="0": raise RuntimeError("focused return code")
    observed_rss=rss((FOCUSED_ATTEMPT/"RSS.txt").read_text()); product=json.loads((FOCUSED_ATTEMPT/"STDOUT.txt").read_text())
    envelope={"schema":contract["envelope_schema"],"status":"PASS","phase":"screen","analysis_stage":"preliminary","product_processes":1,"children_started":1,"children_reaped":1,"terminal_active_children":0,"product":product,"maximum_resident_set_size":observed_rss,"complete_wall_ns":None,"lock_released":None,"terminal_work_roots":None}; write(FOCUSED_ATTEMPT/"RAW-ENVELOPE.json",envelope); normalized=[]
    for script,name in ((PRIMARY,"PRIMARY.json"),(INDEPENDENT,"INDEPENDENT.json")):
        result=analyzer(script,FOCUSED_ATTEMPT/"RAW-ENVELOPE.json",FOCUSED_ATTEMPT/name,True)
        if result.returncode: raise RuntimeError(result.stderr)
        normalized.append(load(FOCUSED_ATTEMPT/name)["normalized"])
    status="PASS" if normalized[0]==normalized[1] and normalized[0]["status"]=="PASS" and observed_rss<=contract["limits"]["rss_bytes"] and product.get("terminal",{}).get("q_terminal")==0 else "REVISE"
    adoption={"schema":"phase4-g5-3-focused-evidence-adoption-v2","status":status,"classification":"DurableDirectReceiptReopenedAndAnalyzedNoProductRerun","source_artifacts":source,"adopted_artifacts":{name:artifact(FOCUSED_ATTEMPT/name) for name in names.values()},"raw_envelope":artifact(FOCUSED_ATTEMPT/"RAW-ENVELOPE.json"),"primary":artifact(FOCUSED_ATTEMPT/"PRIMARY.json"),"independent":artifact(FOCUSED_ATTEMPT/"INDEPENDENT.json"),"analyzer_agreement":normalized[0]==normalized[1],"maximum_resident_set_size":observed_rss,"rss_limit_bytes":contract["limits"]["rss_bytes"],"product_processes_executed_by_adoption":0,"measured_campaign_rows":0}; write(FOCUSED_ATTEMPT/"ADOPTION.json",adoption)
    if status!="PASS": raise RuntimeError("focused v2 evidence REVISE")
    return adoption

def lock():
    LOCK.parent.mkdir(parents=True,exist_ok=True); descriptor=os.open(LOCK,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o444); token=secrets.token_hex(32); os.write(descriptor,(token+"\n").encode()); os.fsync(descriptor); fsync_dir(LOCK.parent); stat=os.fstat(descriptor); return descriptor,stat.st_dev,stat.st_ino,token
def unlock(value):
    descriptor,device,inode,token=value; current=LOCK.stat(follow_symlinks=False); bound=os.fstat(descriptor)
    if (current.st_dev,current.st_ino)!=(device,inode) or (bound.st_dev,bound.st_ino)!=(device,inode): raise RuntimeError("lock identity")
    LOCK.unlink(); fsync_dir(LOCK.parent); os.close(descriptor); return {"device":device,"inode":inode,"token_sha256":hashlib.sha256(token.encode()).hexdigest(),"lock_absent":not LOCK.exists()}

def verify_screen_authority(contract,require_closure):
    result=TARGET/RESULTS["screen"]
    paths={name:result/name for name in ("TERMINAL-v2.json","RAW-v2.json","PRIMARY-v2.json","INDEPENDENT-v2.json","LOCK-RELEASE-v2.json","FINAL-DECISION-v2.json","FINAL-ARTIFACT-MANIFEST-v2.tsv")}
    if any(not path.is_file() for path in paths.values()): raise RuntimeError("accepted screen artifacts absent")
    terminal,raw,primary,independent,release,decision=(load(paths[name]) for name in ("TERMINAL-v2.json","RAW-v2.json","PRIMARY-v2.json","INDEPENDENT-v2.json","LOCK-RELEASE-v2.json","FINAL-DECISION-v2.json"))
    normalized_primary,normalized_independent=primary.get("normalized"),independent.get("normalized")
    if terminal.get("status")!="PASS" or terminal.get("phase")!="screen" or not (type(terminal.get("complete_wall_ns")) is int and 0<=terminal["complete_wall_ns"]<contract["limits"]["screen_wall_ns"]) or (terminal.get("children_started"),terminal.get("children_reaped"),terminal.get("product_processes"),terminal.get("product_rows"))!=(1,1,1,1) or terminal.get("maximum_resident_set_size",contract["limits"]["rss_bytes"]+1)>contract["limits"]["rss_bytes"] or terminal.get("lock_released") is not True or terminal.get("terminal_work_roots")!=0: raise RuntimeError("screen terminal authority")
    if raw.get("phase")!="screen" or raw.get("analysis_stage")!="final" or raw.get("status")!="PASS" or normalized_primary!=normalized_independent or normalized_primary.get("status")!="PASS" or normalized_primary.get("hard_failures")!=[] or release.get("lock_absent") is not True or decision.get("status")!="PASS" or not verify_final_manifest(result): raise RuntimeError("screen analyzer/lock authority")
    artifacts={name:{"path":str(path.relative_to(REPO)),"bytes":path.stat().st_size,"sha256":sha(path)} for name,path in paths.items()}
    if require_closure:
        closure=load(STATIC_CLOSURE)
        if closure.get("status")!="PASS" or closure.get("source_freeze_sha256")!=sha(FREEZE) or closure.get("executable_sha256")!=sha(verify_freeze()["executable"]) or closure.get("screen_artifacts")!=artifacts: raise RuntimeError("post-screen static closure authority")
        for item in closure.get("static_artifacts",[]):
            path=REPO/item["path"]
            if not path.is_file() or path.stat().st_size!=item["bytes"] or sha(path)!=item["sha256"]: raise RuntimeError("post-screen static artifact changed")
    return artifacts

def screen_closure():
    ready,contract,executable=require_ready("screen"); verify_freeze()
    if STATIC_CLOSURE.exists() or STATIC_EVIDENCE.exists(): raise RuntimeError("one-shot post-screen static closure")
    screen_artifacts=verify_screen_authority(contract,False); ledger=load(FOCUSED)
    tests=ledger.get("settled_source_focused_tests",[])
    if ledger.get("status")!="PASS" or len(tests)!=1 or any(test.get("result")!="PASS" for test in tests): raise RuntimeError("focused test not ledger-bound")
    STATIC_EVIDENCE.mkdir(parents=True); project=REPO/ready["product"]["project_path"]; source_paths=[item["path"] for item in ready["product"]["source_files"]]
    commands=(("fmt",["cargo","fmt","--","--check"],project),("clippy",["cargo","clippy","--locked","--","-D","warnings"],project),("diff",["git","diff","--check","--",*source_paths],REPO))
    static=[]
    for label,argv,cwd in commands:
        write(STATIC_EVIDENCE/f"{label}-COMMAND.json",{"argv":argv,"cwd":str(cwd)})
        with (STATIC_EVIDENCE/f"{label}-STDOUT.txt").open("w") as stdout_handle,(STATIC_EVIDENCE/f"{label}-STDERR.txt").open("w") as stderr_handle:
            completed=subprocess.run(argv,cwd=cwd,stdout=stdout_handle,stderr=stderr_handle,text=True)
            stdout_handle.flush(); os.fsync(stdout_handle.fileno()); stderr_handle.flush(); os.fsync(stderr_handle.fileno())
        write(STATIC_EVIDENCE/f"{label}-RETURN.json",{"returncode":completed.returncode})
        if completed.returncode: raise RuntimeError(f"post-screen {label} failed")
    for path in sorted(STATIC_EVIDENCE.iterdir()):
        static.append({"path":str(path.relative_to(REPO)),"bytes":path.stat().st_size,"sha256":sha(path)})
    closure={"schema":"phase4-g5-3-post-screen-static-closure-v2","status":"PASS","classification":"CachedProjectStaticClosureNoFocusedTestRerun","source_freeze_sha256":sha(FREEZE),"executable_sha256":sha(executable),"focused_ledger_sha256":sha(FOCUSED),"settled_source_focused_tests":tests,"screen_artifacts":screen_artifacts,"static_artifacts":static}
    write(STATIC_CLOSURE,closure); return closure

def artifact(path):
    path=pathlib.Path(path); return {"path":str(path.relative_to(REPO)),"bytes":path.stat().st_size,"sha256":sha(path)}

def process_evidence(process,return_record,maximum,product):
    names=("COMMAND.json","STDOUT.txt","STDERR.txt","RETURN.json","RSS.txt")
    evidence={"schema":"phase4-g5-3-process-evidence-v2","status":"CAPTURED_BEFORE_EVALUATION","classification":"LosslessCommandStdoutStderrReturnRssAndParsedReceipt","artifacts":{name:artifact(process/name) if (process/name).is_file() else None for name in names},"return":return_record,"maximum_resident_set_size":maximum,"parsed_receipt":artifact(process/"PARSED-RECEIPT.json") if product is not None else None,"receipt_status":product.get("status") if product is not None else None}
    write(process/"PROCESS-EVIDENCE-v2.json",evidence); return evidence

def final_manifest(result):
    manifest=result/"FINAL-ARTIFACT-MANIFEST-v2.tsv"; rows=[]
    for path in sorted((path for path in result.rglob("*") if path.is_file() and path!=manifest),key=lambda path:str(path.relative_to(result))): rows.append((str(path.relative_to(result)),path.stat().st_size,sha(path)))
    write_text(manifest,"path\tbytes\tsha256\n"+"".join(f"{path}\t{size}\t{digest}\n" for path,size,digest in rows)); fsync_dir(result); return manifest

def verify_final_manifest(result):
    manifest=result/"FINAL-ARTIFACT-MANIFEST-v2.tsv"
    if not manifest.is_file(): return False
    with manifest.open(newline="") as handle: rows=list(csv.DictReader(handle,delimiter="\t"))
    names=[row["path"] for row in rows]
    actual=sorted(str(path.relative_to(result)) for path in result.rglob("*") if path.is_file() and path!=manifest)
    return names==sorted(names)==actual and all((result/row["path"]).stat().st_size==int(row["bytes"]) and sha(result/row["path"])==row["sha256"] for row in rows)

def campaign(phase):
    ready,contract,executable=require_ready(phase); verify_freeze(); dry=load(FORECAST)
    if dry.get("status")!="PASS": raise RuntimeError("zero-row forecast missing")
    if phase=="gate": verify_screen_authority(contract,True)
    result=TARGET/RESULTS[phase]
    if result.exists(): raise RuntimeError("one-shot result exists")
    started=time.monotonic_ns(); failure=None; product=None; maximum=None; preliminary=None; work=None; process=None; child_started=0; child_reaped=0; ownership=lock()
    try:
        result.mkdir(parents=True); work=result/"work"; process=result/"PROCESS-01"; process.mkdir(); operands=verify_owned_manifest(contract,executable)
        command=["/usr/bin/time","-l","-o",str(process/"RSS.txt"),str(executable),contract["product_cli"]["run_flag"],operands["fixture_1m"]["path"],operands["expected_roots"]["path"],operands["concurrency_10m"]["path"],str(work),phase]; write(process/"COMMAND.json",{"argv":command})
        completed=None; launch_failure=None; return_record={"returncode":None,"timed_out":False,"error":None}
        with (process/"STDOUT.txt").open("w") as stdout_handle,(process/"STDERR.txt").open("w") as stderr_handle:
            try:
                child_started=1; completed=subprocess.run(command,stdout=stdout_handle,stderr=stderr_handle,text=True,timeout=contract["campaigns"][phase]["complete_wall_hard_limit_ns"]/1e9); child_reaped=1; return_record["returncode"]=completed.returncode
            except subprocess.TimeoutExpired as error:
                child_reaped=1; launch_failure=error; return_record.update(timed_out=True,error=f"TimeoutExpired: {error}")
            except Exception as error:
                launch_failure=error; return_record["error"]=f"{type(error).__name__}: {error}"
            finally:
                stdout_handle.flush(); os.fsync(stdout_handle.fileno()); stderr_handle.flush(); os.fsync(stderr_handle.fileno())
        write(process/"RETURN.json",return_record)
        if (process/"RSS.txt").is_file():
            descriptor=os.open(process/"RSS.txt",os.O_RDONLY)
            try: os.fsync(descriptor)
            finally: os.close(descriptor)
        parse_failure=None
        try:
            maximum=rss((process/"RSS.txt").read_text()); product=json.loads((process/"STDOUT.txt").read_text()); write(process/"PARSED-RECEIPT.json",product)
        except Exception as error: parse_failure=error
        process_evidence(process,return_record,maximum,product)
        if launch_failure is not None: raise launch_failure
        if parse_failure is not None: raise parse_failure
        if return_record["returncode"] or product.get("status")!="PASS": raise RuntimeError("product child failed")
        preliminary={"schema":contract["envelope_schema"],"status":"PASS","phase":phase,"analysis_stage":"preliminary","product_processes":1,"children_started":1,"children_reaped":1,"terminal_active_children":0,"product":product,"maximum_resident_set_size":maximum,"complete_wall_ns":None,"lock_released":None,"terminal_work_roots":None}; write(result/"RAW-PRELIMINARY-v2.json",preliminary)
        reports=[]
        for script,name in ((PRIMARY,"PRIMARY-PRELIMINARY-v2.json"),(INDEPENDENT,"INDEPENDENT-PRELIMINARY-v2.json")):
            analyzed=analyzer(script,result/"RAW-PRELIMINARY-v2.json",result/name)
            if analyzed.returncode: raise RuntimeError(analyzed.stderr)
            reports.append(load(result/name)["normalized"])
        if reports[0]!=reports[1] or reports[0]["status"]!="PASS": raise RuntimeError("preliminary analyzer failure")
    except Exception as error: failure=error
    finally:
        if work is not None: shutil.rmtree(work,ignore_errors=True)
        release=unlock(ownership)
        if result.exists(): write(result/"LOCK-RELEASE-v2.json",release)
    complete=time.monotonic_ns()-started; normalized=[]
    if product is not None:
        final={"schema":contract["envelope_schema"],"status":"PASS","phase":phase,"analysis_stage":"final","product_processes":1,"children_started":1,"children_reaped":1,"terminal_active_children":0,"product":product,"maximum_resident_set_size":maximum,"complete_wall_ns":complete,"lock_released":True,"terminal_work_roots":0}; write(result/"RAW-v2.json",final)
        for script,name in ((PRIMARY,"PRIMARY-v2.json"),(INDEPENDENT,"INDEPENDENT-v2.json")):
            analyzed=analyzer(script,result/"RAW-v2.json",result/name)
            if analyzed.returncode: failure=failure or RuntimeError(analyzed.stderr); break
            normalized.append(load(result/name)["normalized"])
        if len(normalized)!=2 or normalized[0]!=normalized[1] or normalized[0]["status"]!="PASS": failure=failure or RuntimeError("final analyzer failure")
    terminal={"schema":"phase4-g5-3-terminal-v2","status":"PASS" if failure is None else "REVISE","phase":phase,"complete_wall_ns":complete,"wall_scope":"ThroughPreliminaryAnalyzerPairCleanupAndOwnedLockReleaseFinalAnalyzerPairIsPostWallCustody","limit_ns":contract["campaigns"][phase]["complete_wall_hard_limit_ns"],"children_started":child_started,"children_reaped":child_reaped,"product_processes":child_started,"product_rows":1 if product else 0,"maximum_resident_set_size":maximum,"lock_released":not LOCK.exists(),"terminal_work_roots":0 if work is None or not work.exists() else 1}; write(result/"TERMINAL-v2.json",terminal)
    required=("TERMINAL-v2.json","RAW-v2.json","PRIMARY-v2.json","INDEPENDENT-v2.json","LOCK-RELEASE-v2.json")
    decision={"schema":"phase4-g5-3-final-decision-v2","status":"PASS" if failure is None else "REVISE","phase":phase,"classification":"TerminalRawTwoIndependentAnalyzersAndOwnedLockRelease","bindings":{name:artifact(result/name) for name in required if (result/name).is_file()},"process_evidence":artifact(process/"PROCESS-EVIDENCE-v2.json") if process is not None and (process/"PROCESS-EVIDENCE-v2.json").is_file() else None,"analyzer_agreement":len(normalized)==2 and normalized[0]==normalized[1],"hard_failures":normalized[0]["hard_failures"] if len(normalized)==2 and normalized[0]==normalized[1] else None}; write(result/"FINAL-DECISION-v2.json",decision)
    if failure: write(result/"FAILED-v2.json",{"status":"REVISE","error":f"{type(failure).__name__}: {failure}"})
    final_manifest(result)
    if not verify_final_manifest(result): raise RuntimeError("final artifact manifest")
    if failure: raise failure
    return terminal

def main():
    parser=argparse.ArgumentParser(); parser.add_argument("action",choices=("self-check","adopt-focused","prepare","freeze","forecast","screen","screen-closure","gate")); parser.add_argument("--executable"); parser.add_argument("--input-root"); args=parser.parse_args()
    if args.action=="self-check": self_check()
    elif args.action=="adopt-focused": print(compact(adopt_focused()))
    elif args.action=="prepare": print(compact(prepare(args.executable,args.input_root)))
    elif args.action=="freeze": print(compact(freeze(args.executable)))
    elif args.action=="forecast": print(compact(forecast()))
    elif args.action=="screen-closure": print(compact(screen_closure()))
    else: print(compact(campaign(args.action)))
if __name__=="__main__": main()
