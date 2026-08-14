import { createHash } from "node:crypto";
import { closeSync, mkdirSync, openSync, readFileSync, readSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { blake3, createBLAKE3 } from "hash-wasm";

export const ROOT = dirname(fileURLToPath(import.meta.url));
export const DATA = join(ROOT, "data");
export const PROFILE = Object.freeze({
  min: 8192,
  target: 16384,
  max: 32768,
  normalization: 2,
  seed: 0n,
  smallMask: 0x0000d90303537000n,
  largeMask: 0x0000d90103530000n,
  shiftedSmallMask: 0x0001b20606a6e000n,
  shiftedLargeMask: 0x0001b20206a60000n,
});

export const SCHEMA = `
CREATE TABLE IF NOT EXISTS objects(
  object_kind INTEGER NOT NULL,
  object_digest BLOB NOT NULL,
  object_length INTEGER NOT NULL,
  object_bytes BLOB NOT NULL,
  PRIMARY KEY(object_kind, object_digest),
  CHECK(length(object_bytes) = object_length)
) STRICT;
CREATE TABLE IF NOT EXISTS roots(
  root_id BLOB PRIMARY KEY,
  manifest_digest BLOB NOT NULL UNIQUE,
  logical_digest BLOB NOT NULL,
  logical_bytes INTEGER NOT NULL,
  manifest_bytes BLOB NOT NULL,
  object_count INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS root_members(
  root_id BLOB NOT NULL REFERENCES roots(root_id),
  ordinal INTEGER NOT NULL,
  object_kind INTEGER NOT NULL,
  object_digest BLOB NOT NULL,
  object_length INTEGER NOT NULL,
  PRIMARY KEY(root_id, ordinal),
  FOREIGN KEY(object_kind, object_digest) REFERENCES objects(object_kind, object_digest)
) STRICT;
CREATE TABLE IF NOT EXISTS current_head(
  slot INTEGER PRIMARY KEY CHECK(slot = 1),
  root_id BLOB NOT NULL REFERENCES roots(root_id),
  generation INTEGER NOT NULL
) STRICT;
`;

export const SQL = Object.freeze({
  insertObject: "INSERT INTO objects(object_kind,object_digest,object_length,object_bytes) VALUES(?1,?2,?3,?4)",
  selectObject: "SELECT object_length,object_bytes FROM objects WHERE object_kind=?1 AND object_digest=?2",
  insertRoot: "INSERT INTO roots(root_id,manifest_digest,logical_digest,logical_bytes,manifest_bytes,object_count) VALUES(?1,?2,?3,?4,?5,?6)",
  insertMember: "INSERT INTO root_members(root_id,ordinal,object_kind,object_digest,object_length) VALUES(?1,?2,?3,?4,?5)",
  selectHead: "SELECT root_id,generation FROM current_head WHERE slot=1",
  upsertHead: "INSERT INTO current_head(slot,root_id,generation) VALUES(1,?1,?2) ON CONFLICT(slot) DO UPDATE SET root_id=excluded.root_id,generation=excluded.generation",
  selectRoot: "SELECT manifest_digest,logical_digest,logical_bytes,manifest_bytes,object_count FROM roots WHERE root_id=?1",
  selectMembers: "SELECT ordinal,object_kind,object_digest,object_length FROM root_members WHERE root_id=?1 ORDER BY ordinal",
});

const GEAR = Array.from({ length: 256 }, (_, value) => {
  const bytes = Buffer.alloc(64, value);
  return createHash("md5").update(bytes).digest().readBigUInt64BE(0);
});
const GEAR_LS = GEAR.map((value) => BigInt.asUintN(64, value << 1n));
const GEAR_HI = GEAR.map((value) => Number(value >> 32n) >>> 0);
const GEAR_LO = GEAR.map((value) => Number(value & 0xffffffffn) >>> 0);
const GEAR_LS_HI = GEAR_LS.map((value) => Number(value >> 32n) >>> 0);
const GEAR_LS_LO = GEAR_LS.map((value) => Number(value & 0xffffffffn) >>> 0);
const MASKS = [
  [Number(PROFILE.shiftedSmallMask >> 32n) >>> 0, Number(PROFILE.shiftedSmallMask & 0xffffffffn) >>> 0],
  [Number(PROFILE.shiftedLargeMask >> 32n) >>> 0, Number(PROFILE.shiftedLargeMask & 0xffffffffn) >>> 0],
  [Number(PROFILE.smallMask >> 32n) >>> 0, Number(PROFILE.smallMask & 0xffffffffn) >>> 0],
  [Number(PROFILE.largeMask >> 32n) >>> 0, Number(PROFILE.largeMask & 0xffffffffn) >>> 0],
];
export const GEAR_TABLE = GEAR.map((value) => `0x${value.toString(16).padStart(16, "0")}`);

export function fastcdcCut(bytes) {
  const n = bytes.length;
  if (n <= PROFILE.min) return n;
  const retained = Math.min(n, PROFILE.max);
  let index = PROFILE.min;
  let high = 0;
  let low = 0;
  while (index < PROFILE.max && index + 1 < n) {
    high = ((high << 2) | (low >>> 30)) >>> 0;
    low = (low << 2) >>> 0;
    const even = bytes[index];
    let sum = low + GEAR_LS_LO[even];
    low = sum >>> 0;
    high = (high + GEAR_LS_HI[even] + (sum > 0xffffffff ? 1 : 0)) >>> 0;
    const shiftedMask = MASKS[index < PROFILE.target ? 0 : 1];
    if ((high & shiftedMask[0]) === 0 && (low & shiftedMask[1]) === 0) return index;
    const odd = bytes[index + 1];
    sum = low + GEAR_LO[odd];
    low = sum >>> 0;
    high = (high + GEAR_HI[odd] + (sum > 0xffffffff ? 1 : 0)) >>> 0;
    const mask = MASKS[index < PROFILE.target ? 2 : 3];
    if ((high & mask[0]) === 0 && (low & mask[1]) === 0) return index + 1;
    index += 2;
  }
  return retained;
}

export async function digest(bytes) {
  return Buffer.from(await blake3(bytes), "hex");
}

export async function digestDomain(domain, bytes) {
  return digest(Buffer.concat([Buffer.from(`${domain}\0`), bytes]));
}

export async function blake3Hasher() {
  return createBLAKE3();
}

export function hex(bytes) {
  return Buffer.from(bytes).toString("hex");
}

export function fromHex(value) {
  return Buffer.from(value, "hex");
}

export function* streamFileChunks(path) {
  const fd = openSync(path, "r");
  const buffer = Buffer.alloc(PROFILE.max);
  let filled = 0;
  let eof = false;
  try {
    while (!eof || filled > 0) {
      if (!eof && filled < buffer.length) {
        const n = readSync(fd, buffer, filled, buffer.length - filled, null);
        if (n === 0) eof = true;
        else filled += n;
      }
      if (filled === 0 && eof) break;
      const cut = fastcdcCut(buffer.subarray(0, filled));
      const chunk = Buffer.from(buffer.subarray(0, cut));
      buffer.copyWithin(0, cut, filled);
      filled -= cut;
      yield chunk;
    }
  } finally {
    closeSync(fd);
  }
}

export async function manifestForBytes(fixtureId, sourceBytes, chunks) {
  const objects = [];
  for (const chunk of chunks) objects.push({ kind: 1, digest: hex(await digestDomain("object-v1", chunk)), length: chunk.length });
  const manifestBytes = Buffer.from(JSON.stringify({ version: 1, fixture_id: fixtureId, objects }));
  const logicalDigest = await digestDomain("logical-v1", sourceBytes);
  const manifestDigest = await digestDomain("manifest-v1", manifestBytes);
  const rootId = await digestDomain("root-v1", Buffer.concat([manifestDigest, logicalDigest]));
  return {
    fixture_id: fixtureId,
    bytes: sourceBytes.length,
    logical_digest: hex(logicalDigest),
    manifest_digest: hex(manifestDigest),
    root_id: hex(rootId),
    object_count: objects.length,
    unique_object_count: new Set(objects.map((object) => object.digest)).size,
    objects,
    manifest_b64: manifestBytes.toString("base64"),
  };
}

export function loadFixture(fixtureId) {
  return JSON.parse(readFileSync(join(DATA, "fixtures", `${fixtureId}.json`), "utf8"));
}

export function dbSizes(path) {
  const size = (suffix) => {
    try { return statSync(path + suffix).size; } catch { return 0; }
  };
  return { db: size(""), wal: size("-wal") };
}

export function ensureParent(path) {
  mkdirSync(dirname(path), { recursive: true });
}

export function writeJson(path, value) {
  ensureParent(path);
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}
