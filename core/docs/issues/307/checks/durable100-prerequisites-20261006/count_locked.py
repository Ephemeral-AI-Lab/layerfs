import fcntl,pathlib,subprocess,sys,json,hashlib

lock=pathlib.Path('benchmark-results/fs-bench-pro/phase7-sqlite.lock')
lock.parent.mkdir(parents=True,exist_ok=True)
with lock.open('a+b') as h:
    fcntl.flock(h,fcntl.LOCK_EX|fcntl.LOCK_NB)
    b=pathlib.Path('core/target/cluster2-runtime-tests/release/examples/durable_init_costs')
    before=hashlib.sha256(b.read_bytes()).hexdigest()
    out=pathlib.Path(sys.argv[1]);assert not out.exists();out.mkdir()
    child=subprocess.run([str(b),'benchmark-results/fs-bench-pro/sdk-prepared/namespace-100-compact-v3-47c14612ce845b9b/payload',str(out/'store.sqlite'),'durable'],check=False)
    after=hashlib.sha256(b.read_bytes()).hexdigest();assert before==after
    (out/'binary-seal.json').write_text(json.dumps({'before':before,'after':after,'lock':str(lock),'source_parent':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()},indent=2)+'\n')
    sys.exit(child.returncode)
