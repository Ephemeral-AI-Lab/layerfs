// The profile table, the generated page and the tool schemas stay equal to the
// call table: every call in exactly one bucket, every schema a valid MCP tool.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { table } from "@layerfs/node";
import { ToolSchema } from "@modelcontextprotocol/sdk/types.js";
import { bucketOf, check, tools } from "../src/profiles.js";
import { toolsOf } from "../src/tools.js";
import { document, render, splice } from "../scripts/gen-profiles.mjs";

const nameOf = (spec) => `${spec.family}.${spec.call}`;

test("every call is in exactly one bucket, and admin adds the work bucket", () => {
  assert.deepEqual(check(table), []);
  assert.equal(table.length, 83);
  const buckets = table.map(bucketOf);
  assert.equal(buckets.filter((bucket) => bucket === "work").length, 17);
  assert.equal(buckets.filter((bucket) => bucket === "admin").length, 66);
  assert.ok(buckets.every((bucket) => bucket === "work" || bucket === "admin"));
  // every entry names its own profile (call surface 2.0); the monitoring calls
  // a UI polls (machine.stats, files, log) are `full` rows
  for (const name of ["machine.stats", "machine.files", "machine.log"]) assert.equal(bucketOf(table.find((spec) => nameOf(spec) === name)), "admin", name);
  const work = tools(table, "work").map(nameOf);
  const admin = tools(table, "admin").map(nameOf);
  assert.equal(work.length, 17);
  assert.equal(admin.length, 83);
  for (const name of work) assert.ok(admin.includes(name), `${name} is in work but not admin`);
  assert.ok(!admin.some((name) => name.startsWith("doctor.")), "a doctor call is a tool");
  assert.throws(() => tools(table, "root"), /unknown profile/);
});

test("an entry's own profile decides its bucket, whatever its family", () => {
  // `branch` has work calls, but an entry marked `full` is administration
  const retire = { family: "branch", call: "x", args: [], options: [], form: { profile: "full" } };
  assert.equal(bucketOf(retire), "admin");
  assert.ok(!tools([...table, retire], "work").includes(retire));
  assert.ok(tools([...table, retire], "admin").includes(retire));
  const agent = { family: "store", call: "x", args: [], options: [], form: { profile: "agent" } };
  assert.equal(bucketOf(agent), "work");
  assert.ok(tools([...table, agent], "work").includes(agent));
});

test("every generated tool is a valid MCP tool with the table's arguments and options", () => {
  const generated = toolsOf(table);
  assert.equal(generated.length, table.length);
  assert.equal(new Set(generated.map((tool) => tool.name)).size, table.length);
  for (const [index, tool] of generated.entries()) {
    const spec = table[index];
    assert.equal(tool.name, `${spec.family}_${spec.call}`);
    ToolSchema.parse(tool);
    assert.equal(tool.inputSchema.type, "object");
    assert.equal(tool.inputSchema.additionalProperties, false);
    const properties = Object.keys(tool.inputSchema.properties);
    const visible = spec.options.filter((option) => option.kind !== "function" && option.name !== "stream");
    // a migrated call (call surface 2.0) also takes `detail`, and `with` when its entry lists sections
    const sections = Object.keys(spec.form?.with ?? {});
    const globals = spec.form ? ["detail", ...(sections.length > 0 ? ["with"] : [])] : [];
    assert.deepEqual(properties, [...spec.args.map((arg) => arg.name), ...visible.map((option) => option.name), ...globals]);
    if (sections.length > 0) assert.deepEqual(tool.inputSchema.properties.with.items.enum, sections);
    assert.deepEqual(
      tool.inputSchema.required,
      // required positionals, then the options the table marks required
      [...spec.args.filter((arg) => !arg.optional), ...visible.filter((option) => option.required)].map((arg) => arg.name),
    );
    for (const hidden of ["on_progress", "stdout", "stderr", "stream"]) {
      assert.ok(!properties.includes(hidden), `${tool.name} exposes ${hidden}`);
    }
    for (const property of Object.values(tool.inputSchema.properties)) {
      const any = property.type === undefined && property.oneOf === undefined;
      assert.ok(any ? /any JSON value/.test(property.description ?? "") : true, JSON.stringify(property));
    }
  }
});

test("the page's generated profile table is what the table and buckets give", () => {
  const current = readFileSync(document, "utf8");
  const { block, problems } = render(table);
  assert.deepEqual(problems, []);
  assert.equal(splice(current, block), current, "run node api/mcp/scripts/gen-profiles.mjs");
});

test("a call with no bucket is a problem, not a silent admin tool", () => {
  const future = { family: "future", call: "x", args: [], options: [], help: "" };
  assert.match(check([...table, future]).join("\n"), /future\.x: no profile bucket/);
  assert.throws(() => tools([...table, future], "admin"), /future\.x: no profile bucket/);
  assert.equal(bucketOf(future), null);
});
