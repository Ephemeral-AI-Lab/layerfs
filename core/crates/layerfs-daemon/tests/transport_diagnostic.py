#!/usr/bin/env python3
"""One frozen transport-only diagnostic selection; macOS owns coordination."""
import argparse, hashlib, json, os, select, signal, struct, subprocess, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
EVIDENCE=ROOT/'core/docs/architecture/proposal/service-daemon-transport/implementation/evidence'
CASES={f'tdx1-{carrier}-{direction}-{streams}':(carrier,direction,streams) for carrier,streams in [('tcp',1),('noise',1),('noise',2)] for direction in ['upload','download']}
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--case',choices=CASES,required=True);parser.add_argument('--mode',choices=['perf','verify'],required=True);parser.add_argument('--output',type=Path,required=True)
    # The frozen selection keeps its original artifact manifest by default; a new
    # source/profile identity set is selected explicitly instead of silently.
    parser.add_argument('--manifest',type=Path,default=EVIDENCE/'transport-probe-artifacts-v3-20260920.json');args=parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=False)
    started=time.monotonic_ns();record={'case':args.case,'mode':args.mode,'admission_eligible':False,'status':'INCOMPLETE','cleanup':'INCOMPLETE','target_payload_bytes_per_second':2000000000,'cache_contract':'generated distinct input; every transmitted block filled inside transfer; no file-backed payload/cache/spool','scope':'bridge transport component; C1/C2 and daemon stdin/stdout excluded'}
    server=None;children=[];names=[]
    def expire(*_):raise TimeoutError('complete-command work allowance')
    signal.signal(signal.SIGALRM,expire);signal.alarm(13 if args.mode=='perf'else 50)
    try:
        manifest=json.loads(args.manifest.read_text())
        for name,digest in manifest['files'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest,name
        record['identities']=manifest
        probe=ROOT/'core/target/release/examples/transport_probe'
        # Independent small Python oracle, outside the transfer timer.
        oracle=struct.pack('<4Q',*(0x1922026092000000+i for i in range(4))).hex()
        assert subprocess.check_output([probe,'self-check'],text=True).strip()==oracle
        record['oracle_and_corruption_self_check']='PASS'
        private=os.urandom(32).hex();client=os.urandom(32).hex()
        def public(secret):return subprocess.check_output([ROOT/'core/target/release/examples/public_key'],env=os.environ|{'LAYERFS_PRIVATE_KEY':secret},text=True).strip()
        server_public=public(private);client_public=public(client)
        carrier,direction,streams=CASES[args.case]
        env=os.environ|{'LAYERFS_PRIVATE_KEY':private,'LAYERFS_PEER_KEY':client_public}
        with (args.output/'server.log').open('w')as log:
            server=subprocess.Popen([probe,'server',carrier,direction,str(streams),args.mode],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log)
            def line():
                assert select.select([server.stdout],[],[],5)[0],'endpoint readiness'
                value=server.stdout.readline().decode().strip();assert value,value;return value
            ready=line();assert ready.startswith('LISTEN '),ready;port=int(ready.split()[1])
            for index in range(streams):
                name=f'layerfs-issue192-tdx-{os.getpid()}-{index}';names.append(name)
                child_env=os.environ|{'LAYERFS_PRIVATE_KEY':client,'LAYERFS_SERVER_KEY':server_public,'LAYERFS_ENDPOINT':f'host.docker.internal:{port}'}
                command=['docker','run','--name',name,'--label=io.layerfs.task=issue192','--log-driver=none','--read-only','--cap-drop=ALL','--security-opt=no-new-privileges','--cpus=1','--memory=128m','--memory-swap=128m','--pids-limit=16']
                for key in ['LAYERFS_PRIVATE_KEY','LAYERFS_SERVER_KEY','LAYERFS_ENDPOINT']:command+=['-e',key]
                output=(args.output/f'workload-{index}.log').open('w')
                child=subprocess.Popen(command+[manifest['image_id'],'client',carrier,direction,str(index),args.mode],env=child_env,stdout=output,stderr=subprocess.STDOUT);children.append(child);output.close()
                assert line()==f'READY {index}'
            record['setup_ns']=time.monotonic_ns()-started
            server.stdin.write(b'go\n');server.stdin.flush()
            raw=server.stdout.readline();assert raw,'missing transfer result'
            result=json.loads(raw);record['transfer']=result
            assert result['payload_bytes']==streams*1073741824 and result['payload_chunks']==streams*65536
            assert result['verified_bytes']==(streams*1073741824 if args.mode=='verify'else 0)
            server.wait(timeout=1);assert server.returncode==0
            for child in children:child.wait(timeout=2);assert child.returncode==0
            if args.mode=='perf':
                rate=result['payload_bytes']*1000000000/result['transfer_ns']
                record.update(received_payload_bytes_per_second=rate,decimal_GB_per_second=rate/1e9,gigabits_per_second=rate*8/1e9,n=1,target_met=rate>=2000000000,verification='REQUIRES_SEPARATE_MATCHING_RECEIPT')
            configs=json.loads(subprocess.check_output(['docker','inspect',*names],text=True))
            record['containers']=[{'id':c['Id'],'image':c['Image'],'memory_limit':c['HostConfig']['Memory'],'memory_swap_limit':c['HostConfig']['MemorySwap'],'nano_cpus':c['HostConfig']['NanoCpus'],'pids_limit':c['HostConfig']['PidsLimit'],'mounts':c['Mounts'],'readonly':c['HostConfig']['ReadonlyRootfs'],'log_driver':c['HostConfig']['LogConfig']['Type']}for c in configs]
            assert all(c['memory_limit']==134217728 and c['memory_swap_limit']==134217728 and c['nano_cpus']==1000000000 and c['pids_limit']==16 and c['readonly'] and c['log_driver']=='none' and not c['mounts']for c in record['containers'])
            record['status']='PASS'
    except BaseException as error:
        record.update(status='FAIL',error=repr(error));raise
    finally:
        signal.alarm(0);cleanup=time.monotonic_ns()
        for child in children+([server]if server else[]):
            if child.poll()is None:child.kill();child.wait(timeout=2)
        for name in names:subprocess.run(['docker','rm','-f',name],check=True,stdout=subprocess.DEVNULL,timeout=2)
        record['cleanup']='PASS';record['cleanup_ns']=time.monotonic_ns()-cleanup;record['complete_command_ns']=time.monotonic_ns()-started
        (args.output/'result.json').write_text(json.dumps(record,indent=2)+'\n')
        print(json.dumps({k:v for k,v in record.items()if k not in ('identities','containers')},indent=2))
if __name__=='__main__':main()
