// Real native SDK regression. Explicit binaries + isolated root are required.
// node api/binding/tests/unmount-concurrency.mjs /abs/root /abs/addon.node /abs/daemon
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, writeFile, readFile, access, readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const [base, addon, daemon, childMode, serialized] = process.argv.slice(2);
assert.ok(base?.startsWith('/') && addon?.startsWith('/') && daemon?.startsWith('/'), 'explicit absolute fixture paths');
const repo = fileURLToPath(new URL('../../..', import.meta.url));
Object.assign(process.env, {
  LAYERFS_SANDBOX: 'native', LAYERFS_NATIVE: addon, LAYERFS_DAEMON: daemon,
  LAYERFS_HOME: base+'/home', LAYERFS_LOCAL_DIR: base+'/local', LAYERFS_STORE_PATH: base+'/store/store.sqlite',
  LAYERFS_NFS_UNMOUNT: `sh ${base}/helper.sh {dir}`,
});
const { LayerFS } = await import(repo+'/api/agent/sdk/src/index.js');
const { LayerFSBackend } = await import(repo+'/api/backend/src/index.js');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function deadline(promise, ms) {
  let timer;
  try { return await Promise.race([promise, new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('deadline '+ms)),ms);})]); }
  finally { clearTimeout(timer); }
}
async function waitFile(path) {
  const end=Date.now()+5000;
  while(Date.now()<end){try{await access(path);return;}catch{} await sleep(20);}
  throw Error('helper did not start');
}
async function fast(label, call, expected) {
  const at=Date.now();
  const value=await deadline(call(),900);
  assert.ok(Date.now()-at<1000,label);
  if(expected)expected(value);
  console.log(JSON.stringify({label,elapsedMs:Date.now()-at}));
  return value;
}
async function refused(call,state='Unmounting') {
  try{await call();assert.fail('expected refusal');}
  catch(error){assert.equal(error.code,state==='Unmounting'?'Busy':'Unknown');assert.equal(error.detail?.state,state);assert.ok(error.detail.elapsedMs>=0);return error.detail;}
}
function child(mode, ws) {
  const p=spawn(process.execPath,[fileURLToPath(import.meta.url),base,addon,daemon,mode,JSON.stringify(ws)],{env:process.env,stdio:['ignore','pipe','pipe']});
  let output='';p.stdout.on('data',d=>output+=d);p.stderr.on('data',d=>output+=d);
  const ended=new Promise(resolve=>p.once('exit',(code,signal)=>resolve({code,signal,output})));
  return {p,ended};
}
if(childMode){
  const lf=await LayerFS.open('unmount');const ws=JSON.parse(serialized);
  if(childMode==='probe')await fast('cross-process-same-workspace',()=>refused(()=>lf.workspace.unmount(ws,true)));
  if(childMode==='owner')await lf.workspace.unmount(ws,true);
  await lf.close();
}else{
  assert.equal(process.platform,'darwin','this fixture uses host mount_nfs');
  await mkdir(base+'/local',{recursive:true});await mkdir(base+'/seed',{recursive:true});await mkdir(base+'/store',{recursive:true,mode:0o700});
  await writeFile(base+'/seed/file','base\n');
  await writeFile(base+'/helper.sh',`#!/bin/sh\nprintf '%s' "$1" > '${base}/started'\ni=0\nwhile [ ! -f '${base}/release' ] && [ "$i" -lt 200 ]; do sleep 0.1; i=$((i+1)); done\nif [ -f '${base}/fail' ]; then echo injected-helper-failure >&2; exit 1; fi\nexec /sbin/umount "$1"\n`);
  const lf=await LayerFS.init('unmount',base+'/seed');let owner;let active;
  try{
    await lf.branch.fork('a');await lf.branch.fork('b');
    const sb=await lf.sandbox.create();const a=await lf.workspace.mount(sb.sandbox,'a');const b=await lf.workspace.mount(sb.sandbox,'b');
    active=lf.workspace.unmount(a,true);active.catch(()=>{});
    await waitFile(base+'/started');
    await fast('sandbox-log-needs-discovery-lock',()=>LayerFSBackend.of(lf).daemon.sandbox.get_log(sb.sandbox,{lines:2}));
    await fast('other-workspace-status',()=>LayerFSBackend.of(lf).daemon.workspace.status(b));
    await fast('other-workspace-exec',()=>lf.workspace.exec(b,'printf ready'),r=>assert.equal(r.stdout,'ready'));
    await fast('same-workspace-status',()=>refused(()=>LayerFSBackend.of(lf).daemon.workspace.status(a)));
    await fast('same-workspace-exec',()=>refused(()=>lf.workspace.exec(a,'true')));
    await fast('same-workspace-commit',()=>refused(()=>lf.workspace.commit(a,'must not run')));
    await fast('same-workspace-list-changes',()=>refused(()=>LayerFSBackend.of(lf).daemon.workspace.list_changes(a)));
    await fast('repeat-unmount',()=>refused(()=>lf.workspace.unmount(a,true)));
    await fast('sandbox-close',()=>refused(()=>lf.sandbox.close(sb.sandbox)));
    const probe=child('probe',a);assert.equal((await deadline(probe.ended,5000)).code,0);
    const another=await fast('new-workspace-mount',()=>lf.workspace.mount(sb.sandbox,'b'));
    const locker=spawn('python3',['-c','import fcntl,sys; f=open(sys.argv[1],"r+"); fcntl.flock(f,fcntl.LOCK_EX); print("LOCKED",flush=True); sys.stdin.read(1)',base+'/home/unmount/sandboxes/'+sb.sandbox+'/ownership.lock'],{stdio:['pipe','pipe','pipe']});
    try {
      await deadline(new Promise((resolve,reject)=>{locker.stdout.once('data',resolve);locker.once('error',reject);}),2000);
      await writeFile(base+'/release','go');await sleep(200);
      // The terminal result is visible before cleanup can reacquire this lock.
      await fast('published-success-before-cleanup-lock',()=>lf.workspace.unmount(a,true));
    } finally { locker.stdin.end('x');await deadline(new Promise(r=>locker.once('exit',r)),2000); }
    await deadline(active,5000);active=null;
    await fast('repeat-completed-unmount',()=>lf.workspace.unmount(a,true));
    // Busy and Dirty are reversible: release only this attempt's intent.
    await lf.workspace.exec(b,'printf dirty > dirty');
    await assert.rejects(lf.workspace.unmount(b),e=>e.code==='Dirty');
    await lf.workspace.exec(b,'printf still-usable');
    const holder=spawn('/bin/sleep',['5'],{cwd:b.path,stdio:'ignore'});
    try { await assert.rejects(lf.workspace.unmount(b,true),e=>e.code==='Busy'); }
    finally { holder.kill('SIGTERM');await deadline(new Promise(r=>holder.once('exit',r)),2000); }
    await lf.workspace.unmount(b,true);await lf.workspace.unmount(another,true);await lf.sandbox.close(sb.sandbox);
    // A killed binding loses its lease, not its intent. No subsequent caller
    // may send a second unmount when the daemon may still own the first.
    const {unlink}=await import('node:fs/promises');await unlink(base+'/release');await unlink(base+'/started');
    const sb2=await lf.sandbox.create();const lost=await lf.workspace.mount(sb2.sandbox,'a');
    owner=child('owner',lost);await waitFile(base+'/started');owner.p.kill('SIGKILL');await deadline(owner.ended,5000);
    await fast('lost-owner-no-replay',()=>refused(()=>lf.workspace.unmount(lost,true),'Retained'));
    await writeFile(base+'/release','go');
    await sleep(300);
    // Explicit close takes over the orphan while its daemon is healthy;
    // it never resends the unknown Workspace unmount.
    await fast('lost-owner-close-takeover',()=>lf.sandbox.close(sb2.sandbox));
    // A failed helper leaves daemon custody and the original SDK error.
    // Repeated calls read that terminal result rather than launching again.
    const sb3=await lf.sandbox.create();const kept=await lf.workspace.mount(sb3.sandbox,'a');
    await writeFile(base+'/fail','fail');
    const error=await lf.workspace.unmount(kept,true).then(()=>null,e=>e);assert.ok(error);
    await fast('repeat-retained-result',async()=>{
      const repeated=await lf.workspace.unmount(kept,true).then(()=>null,e=>e);
      assert.equal(repeated.code,error.code);assert.equal(repeated.message,error.message);assert.equal(repeated.detail.state,'Retained');
    });
    await fast('retained-close-takeover',()=>lf.sandbox.close(sb3.sandbox));
    console.log('UNMOUNT_CONCURRENCY PASS');
  }finally{
    await writeFile(base+'/release','cleanup');
    if(owner?.p.exitCode===null&&owner?.p.signalCode===null)owner.p.kill('SIGKILL');
    if(active)await deadline(active.catch(()=>{}),5000);
    // Always give the fixture helpers permission to finish. A failed test
    // must not leave a stopped helper or a held client request behind.
    await lf.close();
    // Failure cleanup is scoped by the explicit fixture root. Detach while
    // its daemon is still alive; never remove directories through mounts.
    const {spawnSync}=await import('node:child_process');
    const table=spawnSync('/sbin/mount',[],{encoding:'utf8',timeout:5000});
    for(const line of (table.stdout||'').split('\n')){
      const point=line.match(/ on (.*) \(/)?.[1];
      if(point?.startsWith(base+'/local/')){
        const off=spawnSync('/sbin/umount',['-f',point],{encoding:'utf8',timeout:15000,killSignal:'SIGKILL'});
        assert.equal(off.status,0,'fixture detach: '+off.stderr);
      }
    }
    const processes=spawnSync('/bin/ps',['-axo','pid=,command='],{encoding:'utf8',timeout:5000});
    for(const line of (processes.stdout||'').split('\n')){
      if(line.includes(daemon+' --config '+base+'/local/')){
        const pid=Number(line.trim().split(/\s+/)[0]);
        try{process.kill(pid,'SIGTERM');}catch(e){if(e.code!=='ESRCH')throw e;}
      }
    }
  }
}
