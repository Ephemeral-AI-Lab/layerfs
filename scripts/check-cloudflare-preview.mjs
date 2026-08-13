import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url);
const wranglerBin = join(
  dirname(require.resolve("wrangler/package.json")),
  "bin",
  "wrangler.js",
);
const configPath = join(root, "examples", "durable-object-workspace", "wrangler.jsonc");
const outdirArgument = process.argv
  .slice(2)
  .find((argument) => argument.startsWith("--outdir="));
if (process.argv.slice(2).some((argument) => argument !== outdirArgument))
  throw new Error(
    "usage: node scripts/check-cloudflare-preview.mjs [--outdir=<directory>]",
  );

function parseJsonc(source) {
  let json = "";
  let string = false;
  let escaped = false;
  for (let offset = 0; offset < source.length; offset += 1) {
    const character = source[offset];
    const next = source[offset + 1];
    if (string) {
      json += character;
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') string = false;
      continue;
    }
    if (character === '"') {
      string = true;
      json += character;
      continue;
    }
    if (character === "/" && next === "/") {
      const end = source.indexOf("\n", offset + 2);
      offset = end < 0 ? source.length : end - 1;
      continue;
    }
    if (character === "/" && next === "*") {
      const end = source.indexOf("*/", offset + 2);
      if (end < 0) throw new Error("unterminated comment in wrangler.jsonc");
      offset = end + 1;
      continue;
    }
    json += character;
  }
  return JSON.parse(json.replace(/,\s*([}\]])/gu, "$1"));
}

function assertExactConfig(config) {
  if (config.name !== "ephemeral-ai-fs-preview")
    throw new Error("unexpected preview Worker name");
  if (config.main !== "src/index.ts")
    throw new Error("preview Worker must bundle its reviewed TypeScript entry point");
  if (config.compatibility_date !== "2026-08-10")
    throw new Error("unexpected preview Worker compatibility date");
  if (
    JSON.stringify(config.durable_objects?.bindings) !==
    JSON.stringify([{ name: "FILESYSTEM", class_name: "FilesystemObject" }])
  )
    throw new Error("unexpected preview Durable Object binding");
  if (
    JSON.stringify(config.migrations) !==
    JSON.stringify([{ tag: "v1", new_sqlite_classes: ["FilesystemObject"] }])
  )
    throw new Error("unexpected preview SQLite Durable Object migration");
}

const config = parseJsonc(await readFile(configPath, "utf8"));
assertExactConfig(config);

const retainedOutputDirectory = outdirArgument?.slice("--outdir=".length);
const outputDirectory = retainedOutputDirectory
  ? resolve(retainedOutputDirectory)
  : await mkdtemp(join(tmpdir(), "efs-m6-preview-"));
if (retainedOutputDirectory) await mkdir(outputDirectory, { recursive: true });
try {
  const environment = { ...process.env, CI: "1", WRANGLER_SEND_METRICS: "false" };
  delete environment.CLOUDFLARE_API_TOKEN;
  delete environment.CLOUDFLARE_ACCOUNT_ID;
  delete environment.CLOUDFLARE_EMAIL;
  delete environment.CLOUDFLARE_API_KEY;
  const result = spawnSync(
    process.execPath,
    [
      wranglerBin,
      "deploy",
      "--dry-run",
      "--config",
      configPath,
      "--outdir",
      outputDirectory,
      "--metafile",
    ],
    {
      cwd: root,
      encoding: "utf8",
      env: environment,
      shell: false,
      timeout: 120_000,
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(
      `Wrangler preview dry-run failed (${result.status}):\n${result.stdout}\n${result.stderr}`,
    );
  const combinedOutput = `${result.stdout}\n${result.stderr}`;
  if (!combinedOutput.includes("--dry-run: exiting now."))
    throw new Error("Wrangler did not confirm a dry-run-only preview build");
  if (!combinedOutput.includes("env.FILESYSTEM (FilesystemObject)"))
    throw new Error("Wrangler did not report the reviewed Durable Object binding");

  const bundlePath = join(outputDirectory, "index.js");
  const metadataPath = join(outputDirectory, "bundle-meta.json");
  const bundle = await readFile(bundlePath);
  const metadata = JSON.parse(await readFile(metadataPath, "utf8"));
  if (
    (await stat(bundlePath)).size <= 0 ||
    Object.keys(metadata.outputs ?? {}).length === 0
  )
    throw new Error("Wrangler did not emit a deployable Worker bundle and metadata");
  const digest = createHash("sha256").update(bundle).digest("hex");
  console.log(
    JSON.stringify({
      status: "pass",
      dryRun: true,
      bundleBytes: bundle.byteLength,
      bundleSha256: digest,
      compatibilityDate: config.compatibility_date,
      binding: config.durable_objects.bindings[0],
      migration: config.migrations[0],
      bundlePath,
    }),
  );
} finally {
  if (!retainedOutputDirectory)
    await rm(outputDirectory, { recursive: true, force: true });
}
