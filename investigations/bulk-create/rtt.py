#!/usr/bin/env python3
"""Transport-only 10k serial echo screen. No LayerFS work or timing claim."""
import json, socket, sys, threading, time
N=10000
FRAME=bytes(range(32))
def exact(s,n):
    b=b''
    while len(b)<n:
        part=s.recv(n-len(b))
        if not part: raise EOFError()
        b+=part
    return b
def server(s):
    c,_=s.accept()
    with c:
        c.setsockopt(socket.IPPROTO_TCP,socket.TCP_NODELAY,1)
        for _ in range(N):
            assert exact(c,len(FRAME))==FRAME
            c.sendall(FRAME)
    s.close()
def listener():
    s=socket.socket();s.bind(('0.0.0.0',0));s.listen(1);return s
def client(host,port):
    samples=[];cpu=time.process_time_ns()
    with socket.create_connection((host,port),timeout=30) as s:
        s.setsockopt(socket.IPPROTO_TCP,socket.TCP_NODELAY,1)
        start=time.monotonic_ns()
        for _ in range(N):
            at=time.monotonic_ns();s.sendall(FRAME)
            assert exact(s,len(FRAME))==FRAME
            samples.append(time.monotonic_ns()-at)
        wall=time.monotonic_ns()-start
    samples.sort()
    print(json.dumps(dict(requests=N,frame_bytes=len(FRAME),wall_ns=wall,mean_ns=wall/N,p50_ns=samples[N//2],p95_ns=samples[N*95//100],cpu_ns=time.process_time_ns()-cpu)))
if sys.argv[1]=='server':
    s=listener();print(s.getsockname()[1],flush=True);server(s)
elif sys.argv[1]=='local':
    s=listener();t=threading.Thread(target=server,args=(s,));t.start();client('127.0.0.1',s.getsockname()[1]);t.join()
else: client(sys.argv[1],int(sys.argv[2]))
