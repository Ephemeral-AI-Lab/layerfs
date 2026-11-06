// One public refusal matrix shared with the CLI, Rust API and Python tests.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { createServer } from "node:net";
import { client, table } from "../src/index.js";
const fixture = readFileSync(new URL("../../test/disabled-platforms.json", import.meta.url), "utf8");
function snapshot(root, dir = root) {
  return readdirSync(dir, { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name)).flatMap(e => {
    const path = join(dir, e.name);
    return e.isDirectory() ? snapshot(root, path) : [[path.slice(root.length), readFileSync(path).toString("base64")]];
  });
}
for (const platform of ["firecracker", "ssh"]) test(`${platform}: Node refuses new work without modifying records or contacting workers`, async () => {
  const home = mkdtempSync(join(tmpdir(), "layerfs-disabled-node-"));
  const server = createServer(socket => { contacts++; socket.destroy(); });
  let contacts = 0;
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  try {
    const data = JSON.parse(fixture.replaceAll("$platform", platform).replaceAll("$address", `127.0.0.1:${server.address().port}`));
    for (const [file, value] of Object.entries(data.files)) {
      mkdirSync(dirname(join(home, file)), { recursive: true });
      writeFileSync(join(home, file), JSON.stringify(value));
    }
    const before = snapshot(home);
    const lfs = client({ home });
    for (const row of data.cases) {
      const [family, call] = row.call.split(".");
      const spec = table.find(s => s.family === family && s.call === call);
      const args = spec.args.map(a => row.args[a.name] ?? null);
      if (spec.options.length) args.push(Object.fromEntries(spec.options.filter(o => o.name in row.args).map(o => [o.name, row.args[o.name]])));
      const disabled = row.call === "machine.connect" ? "ssh" : platform;
      await assert.rejects(lfs[family][call](...args), e => e.code === "Unsupported" && e.message === `${disabled} is permanently disabled by product policy (layerfs-project#7)`, row.call);
      assert.deepEqual(snapshot(home), before, row.call);
    }
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(contacts, 0);
  } finally {
    await new Promise(resolve => server.close(resolve));
    rmSync(home, { recursive: true, force: true });
  }
});
