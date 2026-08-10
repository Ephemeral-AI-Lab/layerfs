import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const seed = 0x5eedc0de;
let state = seed >>> 0;
const bytes = new Uint8Array(1024 * 1024);
for (let index = 0; index < bytes.length; index += 1) {
  state ^= state << 13;
  state ^= state >>> 17;
  state ^= state << 5;
  bytes[index] = state & 0xff;
}
const digest = createHash("sha256").update(bytes).digest("hex");
const directory = path.resolve(import.meta.dirname, "../tests/fixtures/generated");
await mkdir(directory, { recursive: true });
const metadata = `${JSON.stringify({ algorithm: "xorshift32", seed, bytes: bytes.length, sha256: digest }, null, 2)}\n`;
const binaryPath = path.join(directory, "seed-5eedc0de.bin");
const metadataPath = path.join(directory, "seed-5eedc0de.json");
let unchanged = false;
try {
  unchanged = Buffer.compare(await readFile(binaryPath), bytes) === 0 && await readFile(metadataPath, "utf8") === metadata;
} catch {}
if (process.argv.includes("--check") && !unchanged) {
  throw new Error("generated fixture is missing or differs from seed 0x5eedc0de");
}
if (!unchanged) {
  await writeFile(binaryPath, bytes);
  await writeFile(metadataPath, metadata);
}
console.log(JSON.stringify({ seed, bytes: bytes.length, sha256: digest, unchanged }));
