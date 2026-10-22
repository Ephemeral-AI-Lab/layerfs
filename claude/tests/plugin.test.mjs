import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { configPath, sdkHash, sha } from '../scripts/runtime.mjs';
import { write, read, sessionDir, agentPath, callPath, key, locked } from '../scripts/state.mjs';

const plugin = fileURLToPath(new URL('../',import.meta.url));
const sdk = fileURLToPath(new URL('../../sdk/js',import.meta.url));
const fixture = join(plugin,'tests/fixtures/native.cjs');
function setup(t) {
  const root = mkdtempSync(join(tmpdir(),"layerfs cc '$` test-"));
  t.after(() => rmSync(root,{ recursive:true, force:true }));
  const daemon = join(root,'daemon');
  writeFileSync(daemon,'#!/bin/sh\nexit 0\n',{ mode:0o700 });
  const config = { schema:1, profile:'Disposable', project:'shop', sdk, native:fixture, daemon, home:root, storePath:join(root,'store.sqlite'),
    docker:'/var/run/docker.sock', volume:'test', image:`sha256:${'0'.repeat(64)}`, build:{ sdk:sdkHash(sdk), native:sha(fixture), daemon:sha(daemon) } };
  write(configPath(root),config);
  const env = { ...process.env, CC_LAYERFS_CONTROL:root, LFS_TEST_STATE:join(root,'fake-state.json'),
    LAYERFS_STORE_PATH:'/wrong-store', DSH_LAYERFS_PROJECT:'wrong', SECRET_TEST_TOKEN:'do-not-persist' };
  const session = 'session-1';
  const dir = sessionDir(root,session);
  const hook = (event, more={}) => {
    const result = spawnSync(process.execPath,[join(plugin,'tests/fixtures/hook.mjs')],{ env, cwd:root,
      input:JSON.stringify({ session_id:session, cwd:root, hook_event_name:event, agent_type:'layerfs:lead', ...more }), encoding:'utf8', timeout:10000 });
    assert.equal(result.status,0,result.stderr);
    return JSON.parse(result.stdout);
  };
  const run = (command, more={}) => {
    const id = `tool-${Math.random()}`;
    const output = hook('PreToolUse',{ tool_name:'Bash', tool_use_id:id, tool_input:{command,timeout:10000}, ...more });
    assert(output.hookSpecificOutput.updatedInput,JSON.stringify(output));
    return spawnSync('/bin/bash',['-c',output.hookSpecificOutput.updatedInput.command],{ env, cwd:root, encoding:'utf8', timeout:10000 });
  };
  const script = (source,more) => run(`node --input-type=module <<'JS'\nconst api=await import(process.env.DSH_LAYERFS_SDK); const lf=new api.LayerFS(process.env.DSH_LAYERFS_PROJECT); try { ${source} } finally { await lf.close(); }\nJS`,more);
  const start = hook('SessionStart',{source:'startup'});
  assert(!start.hookSpecificOutput.additionalContext.includes('failed'),JSON.stringify(start));
  return {root,config,env,session,dir,hook,run,script};
}

test('main session, immutable binding, environment quoting and no secret snapshot', (t) => {
  const x=setup(t);
  const agent=read(agentPath(x.dir,'lead'));
  const result=x.run("printf '%s\\n' \"$CC_LAYERFS_BRANCH\" \"$DSH_LAYERFS_PROJECT\" \"$LAYERFS_STORE_PATH\"; printf '%s' 'literal $() and `ticks`'");
  assert.equal(result.status,0,result.stderr);
  assert(result.stdout.startsWith(`${agent.branch}\nshop\n\n`));
  assert(result.stdout.includes('literal $() and `ticks`'));
  for (const name of ['Read','Edit','EnterWorktree','PowerShell']) assert.equal(x.hook('PreToolUse',{tool_name:name}).hookSpecificOutput.permissionDecision,'deny');
  assert.equal(x.hook('PreToolUse',{tool_name:'Agent',tool_input:{subagent_type:'Explore'}}).hookSpecificOutput.permissionDecision,'deny');
  assert.equal(x.hook('PreToolUse',{tool_name:'Agent',tool_input:{subagent_type:'layerfs:worker',isolation:'worktree'}}).hookSpecificOutput.permissionDecision,'deny');
  assert.deepEqual(x.hook('PreToolUse',{tool_name:'Agent',tool_input:{subagent_type:'layerfs:worker'}}),{});
  x.hook('SessionStart',{source:'resume'});
  assert.equal(read(agentPath(x.dir,'lead')).branch,agent.branch);
  const forkSession='forked-session';
  x.hook('SessionStart',{source:'fork',session_id:forkSession});
  assert.notEqual(read(agentPath(sessionDir(x.root,forkSession),'lead')).branch,agent.branch);
  for (const name of readdirSync(join(x.dir,'calls'))) {
    const text=readFileSync(join(x.dir,'calls',name),'utf8');
    assert(!text.includes('do-not-persist'));
    assert(!text.includes('/wrong-store'));
  }
});

test('Workers fork committed Lead version, use separate Branches, and cannot publish/write elsewhere', (t) => {
  const x=setup(t);
  assert.equal(x.script(`await lf.fs.write(process.env.CC_LAYERFS_BRANCH,'/a','a');`).status,0);
  assert(readdirSync(join(x.dir,'traces')).length);
  for (const id of ['worker-a','worker-b']) {
    const r=x.hook('SubagentStart',{agent_type:'layerfs:worker',agent_id:id});
    assert(!r.hookSpecificOutput.additionalContext.includes('failed'));
  }
  const a=read(agentPath(x.dir,'worker-a')), b=read(agentPath(x.dir,'worker-b'));
  assert.notEqual(a.branch,b.branch); assert.equal(a.source,b.source); assert.match(a.source,/^h/);
  const worker={agent_type:'layerfs:worker',agent_id:'worker-a'};
  assert.equal(x.script(`await lf.fs.write(process.env.CC_LAYERFS_BRANCH,'/a','a');`,worker).status,0);
  const refusal=x.script(`await lf.branch.publish(process.env.CC_LAYERFS_BRANCH);`,worker);
  assert.notEqual(refusal.status,0); assert.match(refusal.stderr,/only the Lead/);
  assert.match(x.script(`await lf.fs.write('${b.branch}','/a','x');`,worker).stderr,/not owned/);
  assert.equal(x.hook('PreToolUse',{tool_name:'Bash',tool_use_id:'missing',tool_input:{command:'pwd'},agent_type:'layerfs:worker'}).hookSpecificOutput.permissionDecision,'deny');
  assert.equal(x.hook('PreToolUse',{tool_name:'Agent',tool_input:{subagent_type:'layerfs:worker'},...worker}).hookSpecificOutput.permissionDecision,'deny');
});

test('CLI commits nonzero exit, preserves Conflict, and supports retained background resources', (t) => {
  const x=setup(t);
  const failed=x.run('node "$DSH_LAYERFS_CLI" run "$CC_LAYERFS_BRANCH" fail');
  assert.equal(failed.status,7,failed.stderr);
  let agent=read(agentPath(x.dir,'lead'));
  assert(Object.values(agent.workspaces).every((w)=>w.status==='unmounted'));
  const conflict=x.run('node "$DSH_LAYERFS_CLI" run "$CC_LAYERFS_BRANCH" conflict');
  assert.equal(conflict.status,2); assert.match(conflict.stderr,/Conflict/);
  agent=read(agentPath(x.dir,'lead'));
  const kept=Object.values(agent.workspaces).find((w)=>w.status==='mounted');
  assert(kept);
  const state=read(join(x.root,'fake-state.json'));
  assert(!state.calls.some((c)=>c.method==='workspace.unmount' && c.args.path===kept.ws.path));
  assert.equal(x.hook('Stop',{stop_hook_active:false}).decision,'block');
  assert.deepEqual(x.hook('Stop',{stop_hook_active:true}),{});
  const jobs=x.script(`const {LayerFSOperator}=await import(process.env.DSH_LAYERFS_OPERATOR); const op=new LayerFSOperator(lf); const ws=await lf.workspace.mount(process.env.CC_LAYERFS_BRANCH); await api.retainWorkspace(ws,'dev server'); const job=await op.process.start(ws,'serve'); await op.process.cancel(job.process); await lf.workspace.unmount(ws);`);
  assert.equal(jobs.status,0,jobs.stderr);
});

test('merge selects its returned Branch; publish requires current main and validated head', (t) => {
  const x=setup(t);
  assert.equal(x.run('node "$DSH_LAYERFS_CLI" merge "$CC_LAYERFS_BRANCH" main integrated').status,0);
  assert.equal(read(agentPath(x.dir,'lead')).branch,'integrated');
  assert.match(x.run('node "$DSH_LAYERFS_CLI" publish "$CC_LAYERFS_BRANCH"').stderr,/validate/);
  const checked=x.run('node "$DSH_LAYERFS_CLI" run "$CC_LAYERFS_BRANCH" no-change --validate');
  assert.equal(checked.status,0,checked.stderr);
  assert.equal(x.run('node "$DSH_LAYERFS_CLI" publish "$CC_LAYERFS_BRANCH"').status,0);
  assert.match(x.run('node "$DSH_LAYERFS_CLI" publish "$CC_LAYERFS_BRANCH"').stderr,/merge current main/);
  assert.equal(x.run('node "$DSH_LAYERFS_CLI" merge "$CC_LAYERFS_BRANCH" main release2').status,0);
  const mergedDuringTest=x.run('node "$DSH_LAYERFS_CLI" run "$CC_LAYERFS_BRANCH" merge-during-test --validate');
  assert.equal(mergedDuringTest.status,2); assert.match(mergedDuringTest.stderr,/no concurrent merge/);
  assert(Object.values(read(agentPath(x.dir,'lead')).workspaces).every((w)=>w.status==='unmounted'));
  assert.equal(x.run('node "$DSH_LAYERFS_CLI" run "$CC_LAYERFS_BRANCH" no-change --validate').status,0);
  assert.equal(x.script(`await lf.fs.write(process.env.CC_LAYERFS_BRANCH,'/later','later');`).status,0);
  assert.match(x.run('node "$DSH_LAYERFS_CLI" publish "$CC_LAYERFS_BRANCH"').stderr,/validate/);
});

test('missing/wrong binding denies calls; duplicate call IDs never replay', (t) => {
  const x=setup(t);
  assert.equal(x.hook('PreToolUse',{agent_type:'general-purpose',tool_name:'Bash'}).hookSpecificOutput.permissionDecision,'deny');
  const event={tool_name:'Bash',tool_use_id:'same',tool_input:{command:'exit 0'}};
  assert(x.hook('PreToolUse',event).hookSpecificOutput.updatedInput);
  assert.equal(x.hook('PreToolUse',event).hookSpecificOutput.permissionDecision,'deny');
  const config=read(configPath(x.root)); config.project='other'; write(configPath(x.root),config);
  assert.equal(x.hook('PreToolUse',{...event,tool_use_id:'next'}).hookSpecificOutput.permissionDecision,'deny');
});

test('concurrent calls preserve actual Worker Branch, role and trace identity', async (t) => {
  const x=setup(t);
  const children=[];
  for (const id of ['a','b']) {
    const identity={agent_type:'layerfs:worker',agent_id:id};
    x.hook('SubagentStart',identity);
    const output=x.hook('PreToolUse',{...identity,tool_name:'Bash',tool_use_id:`parallel-${id}`,
      tool_input:{command:'printf "%s %s %s" "$CC_LAYERFS_BRANCH" "$CC_LAYERFS_ROLE" "$DSH_LAYERFS_TRACE_STEP"'}});
    const child=spawn('/bin/bash',['-c',output.hookSpecificOutput.updatedInput.command],{cwd:x.root,env:x.env,stdio:['ignore','pipe','pipe']});
    let stdout=''; child.stdout.on('data',(c)=>stdout+=c);
    children.push((async()=>{ const [code]=await once(child,'exit'); assert.equal(code,0); return stdout; })());
  }
  const outputs=await Promise.all(children);
  for (const [index,id] of ['a','b'].entries()) assert.equal(outputs[index],`${read(agentPath(x.dir,id)).branch} worker parallel-${id}`);
});

test('unknown and Dirty results preserve resources without replay/discard; explicit recovery checks ownership', (t) => {
  const x=setup(t);
  assert.equal(x.run('node "$DSH_LAYERFS_CLI" run "$CC_LAYERFS_BRANCH" unknown').status,2);
  let native=read(join(x.root,'fake-state.json'));
  assert.equal(native.calls.filter((c)=>c.method==='workspace.exec').length,1);
  assert.equal(native.calls.filter((c)=>c.method==='workspace.unmount').length,0);
  assert.equal(x.hook('Stop').decision,'block');
  const dirty=x.script(`const ws=await lf.workspace.mount(process.env.CC_LAYERFS_BRANCH); await lf.workspace.exec(ws,'write'); await lf.workspace.unmount(ws);`);
  assert.match(dirty.stderr,/Dirty/);
  const orphan=x.script(`const {LayerFS:Raw}=await import(${JSON.stringify(fileURLToPath(new URL('../../sdk/js/src/index.js',import.meta.url)))}); const raw=new Raw(process.env.DSH_LAYERFS_PROJECT); try { console.log(JSON.stringify(await raw.workspace.mount(process.env.CC_LAYERFS_BRANCH))); } finally {await raw.close();}`);
  assert.equal(orphan.status,0,orphan.stderr);
  const ws=JSON.parse(orphan.stdout);
  const recovered=x.run(`node "$DSH_LAYERFS_CLI" recover '${ws.sandbox}' '${ws.path}' 'known mount receipt'`);
  assert.equal(recovered.status,0,recovered.stderr);
  assert(read(agentPath(x.dir,'lead')).workspaces[`${ws.sandbox}:${ws.path}`].keep);
  const worker={agent_type:'layerfs:worker',agent_id:'recovery-worker'};
  x.hook('SubagentStart',worker);
  assert.match(x.run(`node "$DSH_LAYERFS_CLI" recover '${ws.sandbox}' '${ws.path}' evidence`,worker).stderr,/not owned/);
});

test('startup deployment failure blocks a previously ready session', (t) => {
  const x=setup(t);
  x.env.LFS_TEST_DEPLOY_FAIL='1';
  assert.match(x.hook('SessionStart',{source:'resume'}).hookSpecificOutput.additionalContext,/deployment unavailable/);
  assert.equal(x.hook('PreToolUse',{tool_name:'Bash',tool_use_id:'blocked',tool_input:{command:'true'}}).hookSpecificOutput.permissionDecision,'deny');
});

test('Lead cleanup respects Worker ownership, stopped state and retention', (t) => {
  const x=setup(t);
  const worker={agent_type:'layerfs:worker',agent_id:'cleanup-worker'};
  x.hook('SubagentStart',worker);
  const mounted=x.script(`const ws=await lf.workspace.mount(process.env.CC_LAYERFS_BRANCH); console.log(JSON.stringify(ws));`,worker);
  assert.equal(mounted.status,0,mounted.stderr);
  const ws=JSON.parse(mounted.stdout);
  const clean=`node "$DSH_LAYERFS_CLI" clean-worker cleanup-worker '${ws.sandbox}' '${ws.path}'`;
  assert.match(x.run(clean).stderr,/must have stopped/);
  x.hook('SubagentStop',worker);
  assert.equal(x.run(clean).status,0);
  assert.equal(read(agentPath(x.dir,'cleanup-worker')).workspaces[`${ws.sandbox}:${ws.path}`].status,'unmounted');
  const retained=x.script(`const ws=await lf.workspace.mount(process.env.CC_LAYERFS_BRANCH); await api.retainWorkspace(ws,'watcher'); console.log(JSON.stringify(ws));`,worker);
  assert.equal(retained.status,0,retained.stderr);
  const kept=JSON.parse(retained.stdout);
  x.hook('SubagentStop',worker);
  assert.match(x.run(`node "$DSH_LAYERFS_CLI" clean-worker cleanup-worker '${kept.sandbox}' '${kept.path}'`).stderr,/must not be retained/);
});

test('runner propagates signals, records interruption and refuses a second launch', async (t) => {
  const x=setup(t);
  const id='signal';
  const output=x.hook('PreToolUse',{tool_name:'Bash',tool_use_id:id,tool_input:{command:'exec sleep 30'}});
  const file=callPath(x.dir,id);
  const child=spawn(process.execPath,[join(plugin,'scripts/run-call.mjs'),file],{cwd:x.root,env:x.env,stdio:'ignore'});
  t.after(()=>{ if(child.exitCode===null && child.signalCode===null) child.kill('SIGKILL'); });
  const ended=once(child,'exit');
  const until=Date.now()+2000;
  while (!read(file).child) { assert(Date.now()<until,'runner never launched its child'); await delay(10); }
  child.kill('SIGTERM');
  const [code,signal]=await ended;
  assert.equal(signal,'SIGTERM');
  assert.equal(read(file).status,'interrupted');
  const duplicate=spawnSync(process.execPath,[join(plugin,'scripts/run-call.mjs'),file],{env:x.env,encoding:'utf8',timeout:5000});
  assert.notEqual(duplicate.status,0); assert.match(duplicate.stderr,/do not replay/);
});

test('local record lock serializes independent updates without retrying SDK calls', async (t) => {
  const root=mkdtempSync(join(tmpdir(),'layerfs-lock-')); t.after(()=>rmSync(root,{recursive:true,force:true}));
  const file=join(root,'record'); let value=0;
  await Promise.all([1,2,3].map(()=>locked(file,async()=>{ const before=value; await new Promise((r)=>setTimeout(r,5)); value=before+1; })));
  assert.equal(value,3);
});
