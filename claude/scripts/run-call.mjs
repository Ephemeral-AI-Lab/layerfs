// Pass the original command as one bash argument, never interpolate its contents.
import { spawn } from 'node:child_process';
import { read, update, requireThat } from './state.mjs';

const file = process.argv[2];
const call = read(file);
await update(file, (state) => {
  requireThat(state.status === 'prepared', 'call has already started; do not replay it');
  state.status = 'running'; state.pid = process.pid; state.started = new Date().toISOString();
});
const child = spawn('/bin/bash', ['--noprofile', '--norc', '-c', call.command], {
  cwd: call.cwd,
  env: { ...Object.fromEntries(Object.entries(process.env).filter(([name]) => !/^(LAYERFS_|DSH_LAYERFS_|CC_LAYERFS_)/.test(name) && !['BASH_ENV','ENV'].includes(name))), ...call.env },
  stdio: 'inherit', detached: true
});
const finished = new Promise((resolve) => {
  child.on('error', (error) => resolve({ error: error.message, code: 127 }));
  child.on('exit', (code, signal) => resolve({ code, signal }));
});
let interrupted = null;
for (const signal of ['SIGTERM','SIGINT','SIGHUP']) process.on(signal, () => {
  interrupted = signal;
  try { process.kill(-child.pid, signal); } catch (error) { if (error.code !== 'ESRCH') throw error; }
});
await update(file, (state) => { state.child = child.pid; });
const result = await finished;
if (result.error) console.error(`LayerFS Bash: ${result.error}`);
await update(file, (state) => { Object.assign(state, result, { status: interrupted || result.signal ? 'interrupted' : 'exited', ended: new Date().toISOString() }); });
if (result.signal) {
  // Restore normal signal exit semantics after saving the receipt.
  process.removeAllListeners(result.signal);
  process.kill(process.pid, result.signal);
} else process.exitCode = result.code ?? 1;
