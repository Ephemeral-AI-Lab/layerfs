// One VM provider CLI call, for the two providers in this folder: the command
// is spawned with pipes, its output stays apart from its error stream, and its
// exit code comes back. Nothing is retried, nothing is buffered beyond the
// call, and a call that runs past its timeout is killed and reported.
import { spawn } from "node:child_process";
import { run } from "../shared.js";

/**
 * `command args` with `input` on stdin, answered as { code, stdout, stderr }.
 * `detach` marks a call whose started work outlives it and is not waited for.
 */
export function capture(command, args, { input = undefined, timeout = 60_000 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: ["pipe", "pipe", "pipe"] });
    const out = [], err = [];
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`${command} ${args.slice(0, 2).join(" ")} did not end within ${timeout / 1000} s`));
    }, timeout);
    child.stdout.on("data", (chunk) => out.push(chunk));
    child.stderr.on("data", (chunk) => err.push(chunk));
    child.on("error", (error) => { clearTimeout(timer); reject(error); });
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout: Buffer.concat(out).toString(), stderr: Buffer.concat(err).toString() });
    });
    if (input === undefined) child.stdin.end(); else child.stdin.end(input);
  });
}

/** One call whose stdout, trimmed, is the command's answer. */
export const answer = (command, args, options = {}) =>
  run(command, args, { timeout: 30_000, ...options }).then((result) => result.stdout.trim());

/** A guest file's text, or null when it does not exist. */
export async function readFile(command, name, path, exec) {
  const { code, stdout } = await exec(name, ["sh", "-c", "test -f \"$1\" && cat \"$1\"", "sh", path]);
  return code === 0 ? stdout : null;
}

/** Writes a guest file from stdin: the text never appears in a command line. */
export async function writeFile(command, name, path, text, exec) {
  const { code, stderr } = await exec(name, ["sh", "-c", "umask 077 && mkdir -p \"$(dirname \"$1\")\" && cat > \"$1\" && chmod 600 \"$1\"", "sh", path], { input: text });
  if (code !== 0) throw new Error(`could not write ${path} in ${name}: ${stderr.trim()}`);
}
