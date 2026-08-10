import { spawn } from "node:child_process";
import path from "node:path";
import workerd from "workerd";

const root = path.resolve(import.meta.dirname, "..");
const config = path.join(root, "tests", "workerd", "algorithms.capnp");
const port = 18799;
const workerdPath = workerd.default;
const child = spawn(
  workerdPath,
  ["serve", config, `--socket-addr=http=127.0.0.1:${port}`],
  { cwd: root, stdio: ["ignore", "pipe", "pipe"], windowsHide: true },
);
let diagnostic = "";
child.stdout.on("data", (chunk) => {
  diagnostic += chunk;
});
child.stderr.on("data", (chunk) => {
  diagnostic += chunk;
});

const delay = (milliseconds) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds));
try {
  let response;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (child.exitCode !== null)
      throw new Error(`workerd exited before serving:\n${diagnostic}`);
    try {
      response = await fetch(`http://127.0.0.1:${port}/`);
      break;
    } catch {
      await delay(25);
    }
  }
  if (!response) throw new Error(`workerd did not become ready:\n${diagnostic}`);
  const body = await response.text();
  if (!response.ok)
    throw new Error(
      `workerd algorithm gate failed (${response.status}): ${body}\n${diagnostic}`,
    );
  const result = JSON.parse(body);
  if (result.runtime !== "workerd" || result.passed !== 5)
    throw new Error(`unexpected workerd result: ${body}`);
  console.log(JSON.stringify(result));
} finally {
  child.kill();
}
