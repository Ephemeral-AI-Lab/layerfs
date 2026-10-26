#!/usr/bin/env node
// The layerfs command's entry: process streams in, exit code out. Writes
// wait for their callback, so nothing is lost when the process exits (pipes
// are asynchronous on macOS).
import { main } from "./main.js";
import { UsageError } from "./argv.js";

const write = (stream) => (text) => new Promise((resolve) => (text ? stream.write(text, () => resolve()) : resolve()));

async function input() {
  if (process.stdin.isTTY) throw new UsageError("this command reads its content from stdin (or --from <file>), and stdin is a terminal");
  const parts = [];
  for await (const chunk of process.stdin) parts.push(chunk);
  return Buffer.concat(parts);
}

const code = await main(process.argv.slice(2), {
  out: write(process.stdout),
  err: write(process.stderr),
  input,
  stdout: process.stdout,
  stderr: process.stderr,
});
process.exit(code);
