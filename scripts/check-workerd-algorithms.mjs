import { spawn } from "node:child_process";
import { once } from "node:events";
import net from "node:net";
import path from "node:path";
import workerd from "workerd";

const root = path.resolve(import.meta.dirname, "..");
const config = path.join(root, "tests", "workerd", "algorithms.capnp");
const readinessDeadlineMs = 15_000;
const requestTimeoutMs = 10_000;
const processDeadlineMs = 20_000;
const requiredChecks = new Set([
  "sha256-golden",
  "fastcdc-boundary-goldens",
  "streaming-fastcdc",
  "manifest-diverse-grouping-root",
  "manifest-binary-goldens",
  "manifest-codec-cursor-corruption",
  "cow-pages",
  "structural-patches",
  "diagnostic-local-rebuild",
  "streamed-rebuild-sink-ownership",
  "runtime-progress-bound",
  "write-path-hashing",
]);

async function reserveEphemeralPort() {
  const server = net.createServer();
  server.unref();
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (!address || typeof address === "string")
    throw new Error("failed to reserve a workerd test port");
  const port = address.port;
  server.close();
  await once(server, "close");
  return port;
}

const port = await reserveEphemeralPort();
const workerdPath = workerd.default;
const child = spawn(
  workerdPath,
  ["serve", config, `--socket-addr=http=127.0.0.1:${port}`],
  { cwd: root, stdio: ["ignore", "pipe", "pipe"], windowsHide: true },
);
let diagnostic = "";
let spawnError;
let processTimedOut = false;
const processTimer = setTimeout(() => {
  processTimedOut = true;
  child.kill();
}, processDeadlineMs);
child.stdout.on("data", (chunk) => {
  diagnostic += chunk;
});
child.stderr.on("data", (chunk) => {
  diagnostic += chunk;
});
child.on("error", (error) => {
  spawnError = error;
});

const delay = (milliseconds) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds));
let failure;
let output;
try {
  let response;
  const deadline = Date.now() + readinessDeadlineMs;
  while (Date.now() < deadline) {
    if (processTimedOut)
      throw new Error(`workerd exceeded ${processDeadlineMs} ms:\n${diagnostic}`);
    if (spawnError) throw spawnError;
    if (child.exitCode !== null)
      throw new Error(`workerd exited before serving:\n${diagnostic}`);
    try {
      response = await fetch(`http://127.0.0.1:${port}/`, {
        signal: AbortSignal.timeout(requestTimeoutMs),
      });
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
  if (
    result.runtime !== "workerd" ||
    !Array.isArray(result.checks) ||
    result.passed !== result.checks.length
  )
    throw new Error(`unexpected workerd result: ${body}`);
  const observed = new Map();
  for (const check of result.checks) {
    if (
      !check ||
      typeof check.name !== "string" ||
      check.ok !== true ||
      observed.has(check.name)
    )
      throw new Error(`malformed or duplicate workerd check: ${body}`);
    observed.set(check.name, check.metrics ?? {});
    for (const [metricName, value] of Object.entries(check.metrics ?? {}))
      if (typeof value === "number" && (!Number.isFinite(value) || value < 0))
        throw new Error(
          `workerd metric ${check.name}.${metricName} is not finite and nonnegative: ${body}`,
        );
  }
  for (const name of requiredChecks)
    if (!observed.has(name))
      throw new Error(`workerd omitted required check ${name}: ${body}`);
  if (observed.size !== requiredChecks.size)
    throw new Error(`workerd returned an unreviewed check set: ${body}`);

  const streaming = observed.get("streaming-fastcdc");
  if (
    streaming.inputBytesCopied <= 0 ||
    streaming.outputBytesCopied !== streaming.inputBytesCopied ||
    streaming.boundaryBytesScanned > streaming.inputBytesCopied ||
    streaming.boundedPushOutputBytes !== 1025 ||
    streaming.boundedPushOutputCount !== 2
  )
    throw new Error(`workerd streaming metrics are not bounded: ${body}`);
  const grouping = observed.get("manifest-diverse-grouping-root");
  if (grouping.nodeCount !== 6 || grouping.groupingRecordCount !== 605)
    throw new Error(`workerd grouping metrics changed: ${body}`);
  const corruption = observed.get("manifest-codec-cursor-corruption");
  if (corruption.rootMutations !== 10 || corruption.nodeMutations !== 8)
    throw new Error(`workerd corruption matrix is incomplete: ${body}`);
  const binary = observed.get("manifest-binary-goldens");
  if (
    binary.emptyLeafBytes !== 32 ||
    binary.leafBytes !== 104 ||
    binary.fullLeafBytes !== 9248 ||
    binary.internalBytes !== 128 ||
    binary.rootBytes !== 68 ||
    binary.deepDepth !== 3 ||
    binary.deepNodeCount !== 146
  )
    throw new Error(`workerd binary-vector metrics are invalid: ${body}`);
  const cow = observed.get("cow-pages");
  if (cow.pageSizesTested !== 3 || cow.pages !== 6)
    throw new Error(`workerd COW coverage metrics are invalid: ${body}`);
  const local = observed.get("diagnostic-local-rebuild");
  if (local.sourceBytesRead > 524_288 || local.bytesHashed > 524_288)
    throw new Error(`workerd diagnostic local metrics are not window-bounded: ${body}`);
  const patches = observed.get("structural-patches");
  if (patches.copiedBytes !== 65_536 || patches.peakSegments > 65)
    throw new Error(`workerd patch metrics are not bounded: ${body}`);
  const fallback = observed.get("streamed-rebuild-sink-ownership");
  if (fallback.peakPendingEntries > 256)
    throw new Error(`workerd streamed-rebuild metadata is not bounded: ${body}`);
  const progress = observed.get("runtime-progress-bound");
  if (progress.requiredBytes !== 102_273_024)
    throw new Error(`workerd resource metric is invalid: ${body}`);
  const hashing = observed.get("write-path-hashing");
  if (
    hashing.hashedBytes < 32 * 1024 * 1024 ||
    hashing.mibPerSec < 300 ||
    hashing.baselineMibPerSec <= 0 ||
    hashing.speedup < 1.5
  )
    throw new Error(
      `workerd write-path hashing is below the M3.3 gates (${hashing.mibPerSec} MiB/s, ${hashing.speedup}x over pure-JS): ${body}`,
    );
  output = JSON.stringify(result);
} catch (error) {
  failure = error;
} finally {
  clearTimeout(processTimer);
  if (child.exitCode === null) child.kill();
  if (child.exitCode === null) {
    const closed = await Promise.race([
      once(child, "exit").then(() => true),
      delay(2_000).then(() => false),
    ]);
    if (!closed && !failure)
      failure = new Error(`workerd did not close promptly:\n${diagnostic}`);
  }
}
if (failure) throw failure;
console.log(output);
