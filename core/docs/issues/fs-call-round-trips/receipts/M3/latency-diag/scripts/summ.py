import json,re,subprocess,sys
D=sys.argv[1]; S=sys.argv[2]
def load(arm,grp,netem):
    out=subprocess.run(['python3','-I',S+'/cases.py',f'{S}/{arm}-{grp}.json',f'{D}/{arm}-{grp}.log',str(netem),'__none__'],capture_output=True,text=True).stdout
    summ=json.loads(out.strip().splitlines()[-1])
    log=open(f'{D}/{arm}-{grp}.log').read()
    ms={m.group(1):float(m.group(2)) for m in re.finditer(r'"case":"([a-z0-9_.]+)","method":"[^"]*","outcome":"[^"]*","ms":([0-9.]+),"client":\{"connects":\d+,"round_trips":(\d+)',log)}
    trips={m.group(1):int(m.group(2)) for m in re.finditer(r'"case":"([a-z0-9_.]+)","method":"[^"]*","outcome":"[^"]*","ms":[0-9.]+,"client":\{"connects":\d+,"round_trips":(\d+)',log)}
    cal=[int(m.group(1)) for m in re.finditer(r'"delay_ms":10,"min_us":\d+,"median_us":(\d+)',log)]
    return summ,ms,trips,cal
for grp in ('history','writes'):
    a,ams,at,_=load('pg',grp,0); b,bms,bt,cal=load('pg10',grp,10)
    c=cal[0]/1000
    print(f'## {grp}: calibration median {c:.2f} ms')
    print('case                 trips wire-exch  local_ms pg10_ms  added/trip  x_cal | per-trip: overshoot local->pg10  gap local->pg10   (ms)')
    for k in b:
        n=bt[k]; e=b[k]['exchanges']; ea=a[k]['exchanges']
        add=(bms[k]-ams[k])/n
        print(f"{k:20s} {n:5d} {ea:4d}/{e:<4d} {ams[k]:8.2f} {bms[k]:8.2f} {add:9.2f} {add/c:6.2f} |   {a[k]['server_ms']/ea:5.2f} -> {b[k]['server_ms']/e:5.2f}     {a[k]['gaps_ms']/max(ea-1,1):5.2f} -> {b[k]['gaps_ms']/max(e-1,1):5.2f}  xfer {a[k]['xfer_ms']:.2f}->{b[k]['xfer_ms']:.2f}")
