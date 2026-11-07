// docs/architecture/api/call-table/coverage.md holds a matrix generated from the three front ends'
// own call lists (api/scripts/gen-coverage.mjs): this fails when the committed block is not what
// they give now, or when a front end lacks a call or disagrees with the Node table (#32).
// Needs the built addon, LAYERFS_PYTHON (default python3) able to import layerfs, and the built
// command; without them the tests are skipped with the reason, never passed.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { collect, document, locate, render, splice } from "../../scripts/gen-coverage.mjs";

let view = null;
let skip = false;
try {
  view = await collect(locate());
} catch (error) {
  skip = `cannot collect the three front ends: ${error.message}`;
}

test("coverage.md is what the front ends' own call lists give", { skip }, () => {
  const { block, problems } = render(view);
  assert.deepEqual(problems, []);
  const current = readFileSync(document, "utf8");
  assert.equal(splice(current, block), current, "run node api/scripts/gen-coverage.mjs");
});

test("a call missing from a front end, or one that differs, is a problem and changes the block", { skip }, () => {
  const { block } = render(view);
  const nodeSpec = view.node.table[0];
  const name = `${nodeSpec.family}.${nodeSpec.call}`;

  const noCli = structuredClone(view);
  delete noCli.cli[name];
  assert.match(render(noCli).problems.join("\n"), new RegExp(`${name}: no command`));
  assert.notEqual(render(noCli).block, block);

  const noAio = structuredClone(view);
  noAio.python.aio = noAio.python.aio.filter((entry) => entry !== name);
  assert.match(render(noAio).problems.join("\n"), new RegExp(`${name}: no aio coroutine`));

  const otherArgument = structuredClone(view);
  otherArgument.python.table[0].args[0].name = "renamed";
  assert.match(render(otherArgument).problems.join("\n"), new RegExp(`${name}: the Python table differs`));

  const otherFlag = structuredClone(view);
  otherFlag.cli[name].options.push("bogus");
  assert.match(render(otherFlag).problems.join("\n"), new RegExp(`${name}: the command line differs`));

  const extra = structuredClone(view);
  extra.cli["file.bogus"] = { positionals: [], options: [], from: false };
  assert.match(render(extra).problems.join("\n"), /file\.bogus: a call of a front end that the Node table does not have/);
});
