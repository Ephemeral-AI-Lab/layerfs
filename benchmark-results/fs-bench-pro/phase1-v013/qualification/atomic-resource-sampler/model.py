import gzip, hashlib, json, os, pathlib, subprocess, threading
here=pathlib.Path(__file__).resolve().parent
source=pathlib.Path('benchmark/fs-bench-pro/workspace_registry.rs')
text=source.read_text(); body=text[text.index('fn sample_resources()'):]
fields=['memory.current','memory.peak','memory.stat','memory.events','memory.swap.current','pids.current','cpu.stat']
assert body.count('output.write_all(row.as_bytes())?;')==1
assert body.count('output.flush()?;')==1
assert body.index('if row.len() > PIPE_BUF')<body.index('output.write_all')
assert 'const PIPE_BUF: usize = 4096;' in body
assert 'print!(' not in body and 'println!(' not in body
assert 'from_millis(10)' in body
assert 'open("/proc/self/fd/1")?' in body and '.file_type().is_fifo()' in body
before=subprocess.check_output(['git','show','HEAD:'+str(source)]).decode()
assert before[:before.index('fn sample_resources()')]==text[:text.index('fn sample_resources()')]
artifact=pathlib.Path('benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s1-performance-2babc4ee0210/cgroup-samples.tsv.gz')
artifact_sha=hashlib.sha256(artifact.read_bytes()).hexdigest()
original=gzip.decompress(artifact.read_bytes()); rows=original.splitlines(keepends=True)
complete=[row for row in rows if row.endswith(b'\n')]
assert len(complete)==8 and len(rows)==9 and rows[-1].endswith(b'memory.stat:')
def assemble(stamp, values):
    row='sample_ns='+str(stamp)
    for name in fields:
        for line in values[name].splitlines(): row+='\t'+name+':'+line.replace(' ','=')
    row+='\n'
    encoded=row.encode()
    if len(encoded)>4096: raise ValueError('atomic pipe bound')
    return encoded
for row in complete:
    tokens=row.decode().rstrip('\n').split('\t'); stamp=int(tokens[0].split('=')[1]); values={field:[] for field in fields}
    for token in tokens[1:]:
        category,value=token.split(':',1); values[category].append(value.replace('=',' '))
    assert assemble(stamp,{key:'\n'.join(value)+'\n' for key,value in values.items()})==row
empty={field:'' for field in fields}
base=len(assemble(0,empty)); empty[fields[0]]='x'*(4096-base-len(fields[0])-2)
assert len(assemble(0,empty))==4096
empty[fields[0]]+='x'
try: assemble(0,empty)
except ValueError: pass
else: raise AssertionError('oversized row silently accepted')
r,w=os.pipe(); pipe_buf=os.fpathconf(w,'PC_PIPE_BUF'); width=min(pipe_buf,4096)
# Portable host-pipe model only: Darwin advertises512, Linux target advertises4096.
# This verifies atomic single-write framing within the actual host guarantee.
count=1024; observed=bytearray(); errors=[]
def read():
    while True:
        data=os.read(r,733)
        if not data: break
        observed.extend(data)
def write(tag):
    row=tag*(width-1)+b'\n'
    try:
        for _ in range(count): assert os.write(w,row)==len(row)
    except BaseException as e: errors.append(repr(e))
reader=threading.Thread(target=read); reader.start()
writers=[threading.Thread(target=write,args=(tag,)) for tag in (b'A',b'B')]
for writer in writers: writer.start()
for writer in writers: writer.join()
os.close(w);reader.join();os.close(r)
assert not errors,errors
emitted=bytes(observed).splitlines(keepends=True)
assert len(emitted)==2*count
assert all(row in (b'A'*(width-1)+b'\n',b'B'*(width-1)+b'\n') for row in emitted)
assert hashlib.sha256(artifact.read_bytes()).hexdigest()==artifact_sha
result={'status':'pass','source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'sampler_only_delta':True,'real_complete_rows_reconstructed_exactly':len(complete),'max_real_row_bytes':max(map(len,complete)),'linux_atomic_bound_bytes':4096,'boundary_4096_accepted_4097_rejected':True,'local_pipe_buf':pipe_buf,'local_atomic_model_rows':len(emitted),'local_model_row_bytes':width,'original_invalid_artifact_sha256':artifact_sha,'original_partial_row_preserved':True,'scope':'Product-free source/format/host-pipe model; no Rust build, Docker runtime, product execution, or performance collection.'}
(here/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
