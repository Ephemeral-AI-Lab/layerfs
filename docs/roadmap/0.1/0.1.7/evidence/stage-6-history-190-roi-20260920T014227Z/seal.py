#!/usr/bin/env python3
"""Pin a compiled diagnostic arm and all first-party compilation inputs."""
import hashlib,json,shutil,subprocess,sys
from pathlib import Path
p=Path(__file__).resolve().parent
repo=p.parents[5]
arm=sys.argv[1]
sha=lambda f: hashlib.sha256(f.read_bytes()).hexdigest()
def files(base):
    return {str(f.relative_to(repo)):sha(f) for f in sorted(base.rglob("*")) if f.is_file() and f.suffix in (".rs",".sql",".toml",".lock",".py") and "target" not in f.parts and "__pycache__" not in f.parts}
product={str(f.relative_to(repo)):sha(f) for f in sorted((repo/"core/crates").rglob("*")) if f.is_file() and ("src" in f.parts or "sql" in f.parts or f.name=="Cargo.toml") and f.name!=".DS_Store"}
harness=files(repo/"core/benchmark/fs-bench-pro-storage-content")
binary_source=Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content")
binary=p/"binaries"/arm
binary.parent.mkdir(exist_ok=True)
assert not binary.exists()
shutil.copy2(binary_source,binary)
identity={"source_commit":subprocess.check_output(["git","rev-parse","HEAD"],cwd=repo,text=True).strip(),"source_tree":subprocess.check_output(["git","rev-parse","HEAD^{tree}"],cwd=repo,text=True).strip(),"source_dirty":True,"product_files":product,"harness_files":harness,"locks":{str(f.relative_to(repo)):sha(f) for f in [repo/"core/Cargo.lock",repo/"core/benchmark/fs-bench-pro-storage-content/Cargo.lock"]},"binary":str(binary),"binary_sha256":sha(binary),"build_reuse":"incremental shared Cargo target; immutable executable copy","cache_contract":"fresh-growing-store; uncontrolled OS/intra-chain residency; admission INELIGIBLE"}
with (p/(arm+"-identity.json")).open("x") as f: json.dump(identity,f,indent=2)
with (p/(arm+"-source.patch")).open("x") as f: subprocess.run(["git","diff","--","core/crates","core/benchmark"],cwd=repo,stdout=f,check=True)
with (p/(arm+"-loc.json")).open("x") as f: subprocess.run(["python3","tools/production_loc.py","--json"],cwd=repo,stdout=f,check=True)
print(json.dumps({"arm":arm,"binary_sha256":identity["binary_sha256"],"source_commit":identity["source_commit"]}))
