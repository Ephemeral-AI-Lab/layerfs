// Reproduce #417: JS SDK can write a binary file but fs.read refuses it as Binary.
import { client, LayerFSError } from "/root/lfs/wt-clidist/api/node/src/index.js";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

async function main() {
  const base = "/root/lfs-tmp/issue417";
  mkdirSync(base, { recursive: true });
  const home = mkdtempSync(join(base, "home-"));
  const src = mkdtempSync(join(base, "src-"));
  writeFileSync(join(src, "README.md"), "hello\n");

  const lfs = client({ home });
  await lfs.store.create("s");
  await lfs.project.init("s/p", src);

  // A PNG-like binary payload with NUL bytes
  await lfs.branch.fork("s/p", "dev", "main");
  const binPayload = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x00, 0x01, 0x02, 0xff, 0xfe]);
  console.log("1. Writing binary payload (length " + binPayload.length + " bytes):", binPayload.toString("hex"));
  await lfs.file.write("s/p", "dev", "/image.png", binPayload, { message: "add png" });
  console.log("   -> file.write succeeded.");

  const st = await lfs.file.stat("s/p", "dev", "/image.png");
  console.log("2. file.stat result:", JSON.stringify(st));

  console.log("3. Attempting file.read on binary file...");
  try {
    const r = await lfs.file.read("s/p", "dev", "/image.png");
    console.log("   -> UNEXPECTED success:", r);
  } catch (err) {
    console.log("   -> REFUSED as expected! Error details:");
    console.log("      error instanceof LayerFSError:", err instanceof LayerFSError);
    console.log("      error.code   :", err.code);
    console.log("      error.message:", err.message);
  }

  // Cleanup
  rmSync(home, { recursive: true, force: true });
  rmSync(src, { recursive: true, force: true });
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
