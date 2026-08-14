import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { DATA, GEAR_TABLE, PROFILE, ROOT, digest, hex, manifestForBytes, writeJson } from "./common.mjs";
import { fastcdcCut } from "./common.mjs";

const FIXTURES = join(DATA, "fixtures");
const OBJECTS = join(DATA, "objects");
const FINAL_100M = process.argv.includes("--100m");
mkdirSync(FIXTURES, { recursive: true });
mkdirSync(OBJECTS, { recursive: true });
writeFileSync(join(DATA, "gear.txt"), `${GEAR_TABLE.join("\n")}\n`);

function bytes(size, seed, repeated = false) {
  const output = Buffer.alloc(size);
  let state = BigInt.asUintN(64, BigInt(seed));
  const pattern = Buffer.alloc(65536);
  for (let i = 0; i < pattern.length; i++) {
    state ^= state << 13n;
    state ^= state >> 7n;
    state ^= state << 17n;
    state = BigInt.asUintN(64, state);
    pattern[i] = Number(state & 255n);
  }
  for (let offset = 0; offset < size; offset += pattern.length) {
    pattern.copy(output, offset, 0, Math.min(pattern.length, size - offset));
    if (!repeated) {
      state ^= state << 13n;
      state ^= state >> 7n;
      state ^= state << 17n;
      state = BigInt.asUintN(64, state);
      const twist = Number(state & 255n);
      output[offset] ^= twist;
    }
  }
  return output;
}

function chunks(source) {
  const output = [];
  let offset = 0;
  while (offset < source.length) {
    const cut = fastcdcCut(source.subarray(offset));
    output.push(source.subarray(offset, offset + cut));
    offset += cut;
  }
  return output;
}

async function saveFixture(id, source, mode = "file") {
  const sourcePath = join(FIXTURES, `${id}.bin`);
  writeFileSync(sourcePath, source);
  const chunkList = chunks(source);
  const fixture = { ...(await manifestForBytes(id, source, chunkList)), source_path: sourcePath, mode };
  for (const object of fixture.objects) {
    const objectPath = join(OBJECTS, `${object.kind}-${object.digest}.bin`);
    writeFileSync(objectPath, chunkList[fixture.objects.indexOf(object)]);
    object.path = objectPath;
  }
  writeJson(join(FIXTURES, `${id}.json`), fixture);
  return fixture;
}

const random = await saveFixture("random-10m", bytes(10 * 1024 * 1024, 0x51f15e));
const duplicate = await saveFixture("duplicate-10m", bytes(10 * 1024 * 1024, 0x51f15e, true));
const baseBytes = bytes(10 * 1024 * 1024, 0x71a5e);
const base = await saveFixture("edit-base-10m", baseBytes);
const editedBytes = Buffer.from(baseBytes);
for (let i = 0; i < 32; i++) editedBytes[5 * 1024 * 1024 + 123 + i] ^= (i * 17 + 3) & 255;
const edited = await saveFixture("edit-small-10m", editedBytes);
const cal = await saveFixture("cal-sql-10m", bytes(10 * 1024 * 1024, 0x1234));

const finalFixtures = [];
if (FINAL_100M) {
  finalFixtures.push(await saveFixture("random-100m", bytes(100 * 1024 * 1024, 0x51f15e)));
  const largeBaseBytes = bytes(100 * 1024 * 1024, 0x71a5e);
  finalFixtures.push(await saveFixture("edit-base-100m", largeBaseBytes));
  const largeEditedBytes = Buffer.from(largeBaseBytes);
  for (let i = 0; i < 32; i++) largeEditedBytes[50 * 1024 * 1024 + 123 + i] ^= (i * 17 + 3) & 255;
  finalFixtures.push(await saveFixture("edit-small-100m", largeEditedBytes));
}

const manyFiles = [];
for (let i = 0; i < 100; i++) manyFiles.push(await saveFixture(`many-files-${String(i).padStart(3, "0")}`, bytes(100 * 1024, 0x9000 + i)));
const directoryEntries = [];
for (let i = 0; i < 10000; i++) directoryEntries.push({ name: `entry-${String(i).padStart(5, "0")}`, digest: hex(await digest(Buffer.from(`entry-${i}`))) });
writeJson(join(FIXTURES, "directory-10k.json"), {
  fixture_id: "directory-10k",
  entry_count: directoryEntries.length,
  manifest_digest: hex(await digest(Buffer.from(JSON.stringify(directoryEntries)))),
  root_id: hex(await digest(Buffer.from(`root-directory-10k\0${JSON.stringify(directoryEntries)}`))),
  object_count: directoryEntries.length,
});

writeJson(join(DATA, "fixture-summary.json"), {
  generated_by: ROOT,
  fastcdc: { ...PROFILE, seed: PROFILE.seed.toString(), smallMask: PROFILE.smallMask.toString(16), largeMask: PROFILE.largeMask.toString(16), shiftedSmallMask: PROFILE.shiftedSmallMask.toString(16), shiftedLargeMask: PROFILE.shiftedLargeMask.toString(16) },
  fixtures: [random, duplicate, base, edited, cal, ...finalFixtures],
  many_files: { count: manyFiles.length, bytes_each: 100 * 1024, object_count: manyFiles.reduce((sum, fixture) => sum + fixture.unique_object_count, 0) },
  directory_10k: JSON.parse(readFileSync(join(FIXTURES, "directory-10k.json"), "utf8")),
});

console.log(JSON.stringify({ generated: true, bytes: random.bytes, random_root: random.root_id, random_objects: random.unique_object_count }));
