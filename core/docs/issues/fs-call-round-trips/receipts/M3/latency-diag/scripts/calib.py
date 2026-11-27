import json,sys,statistics as st
d=json.load(open(sys.argv[1])); netem=float(sys.argv[2])
ex=d['exchanges']; obs=set(d['observer_ports'])
o=[e for e in ex if e['conn'] in obs and e['msgs']=='Q' and any(s=='Q:SELECT 1' for s in e['sql'])]
print('calibration SELECT 1 exchanges', len(o))
# split into runs (calibration happens 2x in pg10: on and off)
runs=[]; cur=[]
for e in o:
    if cur and e['req_first']-cur[-1]['reply_last']>1.0: runs.append(cur); cur=[]
    cur.append(e)
if cur: runs.append(cur)
for r in runs:
    srv=[(e['reply_first']-e['req_last'])*1e3-netem for e in r]
    gaps=[(b['req_first']-a['reply_last'])*1e3 for a,b in zip(r,r[1:])]
    q=lambda v,p: sorted(v)[int(p*(len(v)-1))]
    print(f"n={len(r)} overshoot(reply_first-req-netem) min {min(srv):.2f} med {st.median(srv):.2f} p90 {q(srv,.9):.2f} mean {st.mean(srv):.2f} | wire gap (reply out->next req in) min {min(gaps):.2f} med {st.median(gaps):.2f} p90 {q(gaps,.9):.2f} mean {st.mean(gaps):.2f}")
