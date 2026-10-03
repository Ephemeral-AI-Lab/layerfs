"""Bounded, redacted observer using mc already shipped in the pinned MinIO image."""
import json
import subprocess
import threading
import time
from diagnostics.services import services
LIMIT = 16 * 1024 * 1024
HOST = 'export MC_HOST_diag="http://${MINIO_ROOT_USER}:${MINIO_ROOT_PASSWORD}@127.0.0.1:9000"; '

def redact(value):
    if isinstance(value, dict):
        return {k:redact(v) for k,v in value.items() if k.lower() not in ('authorization','x-amz-security-token','body')}
    if isinstance(value, list):return [redact(v) for v in value]
    return value

class Trace:
    def __init__(self, folder):
        self.phase='setup';self.bytes=0;self.omitted=0;self.invalid=0;self.records=0
        self.ready=threading.Event();self.pid=None
        self.output=(folder/'minio-trace.jsonl').open('x')
        self.err=(folder/'minio-trace.stderr').open('x')
        self.process=subprocess.Popen(['docker','exec',services.NAMES['minio'],'sh','-c',HOST+'echo $$ >&2; exec mc admin trace --json --all diag'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        self.reader=threading.Thread(target=self.read,daemon=True);self.reader.start()
        # The shell reports only its own PID; credentials stay in container env.
        line=self.process.stderr.readline().strip()
        if line.isdigit():self.pid=int(line)
        else:self.err.write(line+'\n')
        self.error_reader=threading.Thread(target=self.errors,daemon=True);self.error_reader.start()
        deadline=time.monotonic()+5
        while not self.ready.is_set() and self.process.poll() is None and time.monotonic()<deadline:
            services.docker('exec',services.NAMES['minio'],'sh','-c',HOST+'mc stat diag/layerfs/cluster1/_trace_ready',check=False)
            self.ready.wait(0.25)
        if not self.ready.is_set():
            self.stop()
            raise ValueError('MinIO trace readiness sentinel missing within 5 s; no measured child entered')
    def read(self):
        for line in self.process.stdout:
            try:record=redact(json.loads(line))
            except ValueError:self.invalid+=1;continue
            if '_trace_ready' in line:self.ready.set()
            record={'observer_phase':self.phase,'host_received_ns':time.monotonic_ns(),'event':record}
            text=json.dumps(record,separators=(',',':'))+'\n'
            if self.bytes+len(text.encode())>LIMIT:self.omitted+=1;continue
            self.output.write(text);self.bytes+=len(text.encode());self.records+=1
    def errors(self):
        for line in self.process.stderr:
            # mc diagnostics contain no request payloads; cap observer errors too.
            if self.err.tell()<65536:self.err.write(line)
    def start_product(self):self.phase='product'
    def stop(self):
        self.phase='post-product'
        if self.pid is not None:
            services.docker('exec',services.NAMES['minio'],'kill','-TERM',str(self.pid),check=False)
        try:self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:self.process.terminate();self.process.wait(timeout=3)
        self.reader.join(timeout=3);self.error_reader.join(timeout=3)
        self.output.close();self.err.close()
        return {'kind':'CAUSE_DIAGNOSTIC','scope':'mc --json --all; server profiler overhead present; setup sentinel excluded; stopped before verifier','output_limit_bytes':LIMIT,'bytes':self.bytes,'records':self.records,'omitted':self.omitted,'invalid':self.invalid,'ready':self.ready.is_set(),'exit_code':self.process.returncode}
