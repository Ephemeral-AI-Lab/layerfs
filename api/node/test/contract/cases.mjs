// The case list that api/node/test/contract.mjs runs, in order, through Node, Python (sync and
// aio) and the command, each against its own fresh Disposable home. Only calls that need no
// daemon, mount, bucket or other machine: store.create on native, project.init, file.*,
// branch.*, commit.*.
//
// A step is `{ id, group, call, args, opts, expect, ... }`:
//   args      positionals, or `(ctx) => positionals` (`ctx.folder` is the front end's own folder);
//             `null` skips an optional one
//   opts      the call's options
//   expect    "ok", or the refusal's `code`
//   cli       "usage": the command refuses it as a wrong command line (exit 2, no JSON) where the
//             packages refuse it with `InvalidInput`; "skip": the command cannot say it
//   loose     compare only the refusal's `code`: the wording is the front end's own
//   differs   `{ node, python, cli }`: each front end's own expectation, no cross comparison
//   sameAs    this answer equals the answer of the step with this id, in the same front end
//   check     asserts on the normalized value (or refusal) of every front end
//   pythonOpts  the options as Python takes them, where a JavaScript value cannot say it
//             (`{ $float: "2.0" }` is a Python float)
import assert from "node:assert/strict";

export const P = "shop/api";
const B = "agent";
const UNICODE = "héllo ☃ 𝄞 é \r\n\ttab   {\"a\":1}\n";
const NESTED = '{"a":[1,{"b":null,"c":"","d":[[],{}]}],"n":9007199254740993,"s":"é☃"}';
const BYTES = Buffer.from(Array.from({ length: 256 }, (_, i) => i));
const MAX_SAFE = BigInt(Number.MAX_SAFE_INTEGER);
const deep = (depth) => Array.from({ length: depth }).reduce((inner) => [inner], []);

const ok = (id, group, call, args, opts = {}, more = {}) => ({ id, group, call, args, opts, expect: "ok", ...more });
const bad = (id, group, call, args, expect, opts = {}, more = {}) => ({ id, group, call, args, opts, expect, ...more });

export const steps = [
  // --- setup: a Store, a project from a folder, a Branch
  ok("store", "setup", "store.create", ["shop"], {}, { check: (v) => assert.equal(v.profile, "disposable") }),
  ok("init", "setup", "project.init", (ctx) => [P, ctx.folder], {}, { check: (v) => assert.equal(v.project, P) }),
  ok("fork", "setup", "branch.fork", [P, B, "main"], {}, { check: (v) => assert.equal(v.kind, "Forked") }),

  // --- required and optional arguments, defaults, omitted and null
  bad("missing path", "args", "file.read", [P, B], "InvalidInput", {}, { cli: "usage", loose: true, check: (v) => assert.match(v.message ?? v.usage, /path|positional/) }),
  bad("missing all", "args", "file.read", [], "InvalidInput", {}, { cli: "usage", loose: true }),
  ok("read default", "args", "file.read", [P, B, "/README.md"], {}, {
    check: (v) => assert.deepEqual([v.text, v.start_line, v.end_line, v.truncated, v.next_line], ["hello\nsecond\n", 1, 2, false, null]),
  }),
  ok("read null commit", "args", "file.read", [P, B, "/README.md", null], {}, { cli: "skip", sameAs: "read default" }),
  ok("read null options", "args", "file.read", [P, B, "/README.md"], { start_line: null, end_line: null }, { cli: "skip", sameAs: "read default" }),
  ok("read window", "args", "file.read", [P, B, "/README.md"], { start_line: 2 }, { check: (v) => assert.deepEqual([v.text, v.start_line], ["second\n", 2]) }),
  ok("branches default", "args", "branch.list", [P], {}, { check: (v) => assert.deepEqual([v.items.length, v.has_more], [2, false]) }),
  ok("branches page", "args", "branch.list", [P], { page_size: 1 }, { check: (v) => assert.deepEqual([v.items.length, v.has_more], [1, true]) }),
  bad("branches page 0", "args", "branch.list", [P], "InvalidInput", { page_size: 0 }, { check: (v) => assert.match(v.message, /pageSize/) }),
  ok("write default message", "args", "file.write", [P, B, "/dflt.txt", "x\n"], {}, { check: (v) => assert.equal(typeof v.commit.message, "string") }),
  ok("write message", "args", "file.write", [P, B, "/msg.txt", "x\n"], { message: "say so" }, { check: (v) => assert.equal(v.commit.message, "say so") }),
  ok("write mode", "args", "file.write", [P, B, "/mode.txt", "x\n"], { mode: 0o600 }),
  ok("stat mode", "args", "file.stat", [P, B, "/mode.txt"], {}, { check: (v) => assert.equal(v.mode, 0o600) }),
  bad("write no content", "args", "file.write", [P, B, "/nc.txt"], "InvalidInput", {}, { cli: "skip", check: (v) => assert.match(v.message, /content/) }),

  // --- unknown options and extra arguments are refused before any effect
  ok("state commits", "unknown", "commit.list", [P, B]),
  ok("state branches", "unknown", "branch.list", [P]),
  ok("state stores", "unknown", "store.list", []),
  ok("state projects", "unknown", "project.list", []),
  bad("write unknown option", "unknown", "file.write", [P, B, "/refused.txt", "x"], "InvalidInput", { bogus: 1 }, { cli: "usage", loose: true, check: (v) => assert.match(v.message ?? v.usage, /bogus/) }),
  bad("write null unknown option", "unknown", "file.write", [P, B, "/refused.txt", "x"], "InvalidInput", { bogus: null }, { cli: "skip", check: (v) => assert.match(v.message, /bogus/) }),
  bad("fork unknown option", "unknown", "branch.fork", [P, "other", "main"], "InvalidInput", { bogus: true }, { cli: "usage", loose: true }),
  bad("store.create unknown option", "unknown", "store.create", ["other"], "InvalidInput", { bogus: "x" }, { cli: "usage", loose: true }),
  bad("stat extra positional", "unknown", "file.stat", [P, B, "/README.md", "c1", "extra"], "InvalidInput", {}, { cli: "usage", loose: true }),
  bad("init function option", "unknown", "project.init", (ctx) => ["shop/other", ctx.folder], "InvalidInput", { on_progress: 5 }, { cli: "skip", check: (v) => assert.match(v.message, /function/) }),
  bad("never written", "unknown", "file.stat", [P, B, "/refused.txt"], "NoSuchPath"),
  bad("never forked", "unknown", "branch.get", [P, "other"], "NoSuchBranch"),
  bad("never stored", "unknown", "store.get", ["other"], "NotFound"),
  ok("state commits after", "unknown", "commit.list", [P, B], {}, { sameAs: "state commits" }),
  ok("state branches after", "unknown", "branch.list", [P], {}, { sameAs: "state branches" }),
  ok("state stores after", "unknown", "store.list", [], {}, { sameAs: "state stores" }),
  ok("state projects after", "unknown", "project.list", [], {}, { sameAs: "state projects" }),

  // --- validation errors are structured, and name what is wrong
  bad("no such project", "validation", "file.stat", ["nosuch/x", B, "/README.md"], "NoSuchStore"),
  bad("project without store", "validation", "file.stat", ["shop", B, "/README.md"], "InvalidInput", {}, { check: (v) => assert.match(v.message, /<store>\/<project>/) }),
  bad("empty project", "validation", "file.stat", ["", B, "/README.md"], "InvalidInput"),
  bad("null project", "validation", "file.stat", [null, B, "/README.md"], "InvalidInput", {}, { cli: "skip", check: (v) => assert.match(v.message, /`project` is required/) }),
  bad("no such branch", "validation", "file.stat", [P, "nosuch", "/README.md"], "NoSuchBranch"),
  bad("bad branch name", "validation", "branch.fork", [P, "Bad!", "main"], "InvalidInput"),
  bad("relative path", "validation", "file.stat", [P, B, "README.md"], "InvalidInput"),
  bad("dotdot path", "validation", "file.stat", [P, B, "/a/../README.md"], "InvalidInput"),
  bad("empty path", "validation", "file.stat", [P, B, ""], "InvalidInput"),
  bad("no such path", "validation", "file.stat", [P, B, "/nope"], "NoSuchPath", {}, { check: (v) => assert.deepEqual(v.detail, { path: "/nope" }) }),
  bad("write to main", "validation", "file.write", [P, "main", "/x.txt", "x"], "ReadOnly"),
  bad("fork exists", "validation", "branch.fork", [P, B, "main"], "NameTaken"),
  bad("wrong kind text for int", "validation", "branch.list", [P], "InvalidInput", { page_size: "10" }, { cli: "skip", check: (v) => assert.match(v.message, /whole number/) }),
  bad("wrong kind number for text", "validation", "file.stat", [P, B, 7], "InvalidInput", {}, { cli: "skip", check: (v) => assert.match(v.message, /text/) }),
  bad("wrong kind text for bool", "validation", "workspace.unmount", ["x"], "InvalidInput", { force: "yes" }, { cli: "usage", loose: true }),
  bad("bad page_pos", "validation", "branch.list", [P], "InvalidInput", { page_pos: "zzz" }, { cli: "skip" }),
  bad("apply not a list", "validation", "file.apply", [P, B, { op: "mkdir", path: "/z" }], "InvalidInput", {}, { check: (v) => assert.match(v.message, /list/) }),
  bad("apply unknown op", "validation", "file.apply", [P, B, [{ op: "frobnicate", path: "/z" }]], "InvalidInput", {}, { check: (v) => assert.deepEqual(v.detail, { index: 0 }) }),
  bad("apply second change refused", "validation", "file.apply", [P, B, [{ op: "mkdir", path: "/zz" }, { op: "rm", path: "/nope" }]], "NoSuchPath", {}, { check: (v) => assert.deepEqual(v.detail?.index, 1) }),
  bad("apply refused nothing committed", "validation", "file.stat", [P, B, "/zz"], "NoSuchPath"),
  bad("deep JSON", "validation", "file.apply", [P, B, deep(200)], "InvalidInput", {}, { cli: "usage", loose: true, check: (v) => assert.match(v.message ?? v.usage, /deeply/) }),

  // --- binary data, Unicode, empty values and nested JSON, stored and read back
  ok("write bytes", "values", "file.write", [P, B, "/all.bin", BYTES]),
  ok("stat bytes", "values", "file.stat", [P, B, "/all.bin"], {}, { check: (v) => assert.equal(v.size, 256) }),
  bad("read bytes", "values", "file.read", [P, B, "/all.bin"], "Binary"),
  ok("write utf8 bytes", "values", "file.write", [P, B, "/utf8.txt", Buffer.from(UNICODE, "utf8")]),
  ok("read utf8 bytes", "values", "file.read", [P, B, "/utf8.txt"], {}, { check: (v) => assert.equal(v.text, UNICODE) }),
  ok("write unicode", "values", "file.write", [P, B, "/é☃𝄞.txt", UNICODE]),
  ok("read unicode", "values", "file.read", [P, B, "/é☃𝄞.txt"], {}, { check: (v) => assert.equal(v.text, UNICODE) }),
  ok("write NFC", "values", "file.write", [P, B, "/nfc.txt", "é"]),
  ok("write NFD", "values", "file.write", [P, B, "/nfd.txt", "é"]),
  ok("stat NFC", "values", "file.stat", [P, B, "/nfc.txt"], {}, { check: (v) => assert.equal(v.size, 2) }),
  ok("stat NFD", "values", "file.stat", [P, B, "/nfd.txt"], {}, { check: (v) => assert.equal(v.size, 3) }),
  ok("write empty", "values", "file.write", [P, B, "/empty.txt", ""]),
  ok("read empty", "values", "file.read", [P, B, "/empty.txt"], {}, { check: (v) => assert.deepEqual([v.text, v.size, v.end_line], ["", 0, null]) }),
  ok("write empty message", "values", "file.write", [P, B, "/emptymsg.txt", "x"], { message: "" }),
  bad("edit empty old", "values", "file.edit", [P, B, "/utf8.txt"], "InvalidInput", { old: "", new: "x" }),
  ok("apply nested", "values", "file.apply", [P, B, [
    { op: "mkdir", path: "/d" },
    { op: "write", path: "/d/é☃.json", content: NESTED },
    { op: "write", path: "/d/raw.bin", content: Buffer.from([0, 255, 1]) },
    { op: "write", path: "/d/abc.txt", content: { base64: "QUJD" } },
    { op: "write", path: "/d/empty.txt", content: "" },
    { op: "edit", path: "/d/é☃.json", old: '"s":"é☃"', new: '"s":""' },
    { op: "cp", from_path: "/d/abc.txt", to_path: "/d/abc2.txt" },
    { op: "mv", from_path: "/d/abc2.txt", to_path: "/d/moved.txt" },
    { op: "chmod", path: "/d/moved.txt", mode: 0o755 },
  ]], { message: "nested" }, { check: (v) => assert.deepEqual([v.kind, v.commit.message], ["Committed", "nested"]) }),
  ok("read nested", "values", "file.read", [P, B, "/d/é☃.json"], {}, { check: (v) => assert.equal(v.text, NESTED.replace('"s":"é☃"', '"s":""')) }),
  ok("read base64 wrapper", "values", "file.read", [P, B, "/d/abc.txt"], {}, { check: (v) => assert.equal(v.text, "ABC") }),
  ok("read moved", "values", "file.read", [P, B, "/d/moved.txt"], {}, { check: (v) => assert.equal(v.text, "ABC") }),
  ok("stat moved", "values", "file.stat", [P, B, "/d/moved.txt"], {}, { check: (v) => assert.equal(v.mode, 0o755) }),
  bad("read raw", "values", "file.read", [P, B, "/d/raw.bin"], "Binary"),
  bad("apply base64 not base64", "values", "file.apply", [P, B, [{ op: "write", path: "/q.txt", content: { base64: "not base64!" } }]], "InvalidInput", {}, { check: (v) => assert.deepEqual(v.detail, { index: 0 }) }),
  ok("apply empty list", "values", "file.apply", [P, B, []], {}, { check: (v) => assert.equal(v.kind, "UpToDate") }),
  bad("lone surrogate text", "values", "file.write", [P, B, "/s.txt", "a\ud800b"], "InvalidInput", {}, { cli: "skip", loose: true }),
  bad("lone surrogate path", "values", "file.stat", [P, B, "/a\ud800"], "InvalidInput", {}, { cli: "skip", loose: true }),
  ok("listing", "values", "file.list", [P, B, "/d"], {}, { check: (v) => assert.equal(v.items.length, 5) }),
  ok("grep unicode", "values", "file.grep", [P, B, "/", "☃"], {}),
  ok("glob unicode", "values", "file.glob", [P, B, "/", "**/*.json"], {}),
  bad("whole numbers only", "values", "file.read", [P, B, "/README.md"], "InvalidInput", { start_line: 1.5 }, { cli: "usage", loose: true }),
  ok("whole float", "values", "file.read", [P, B, "/README.md"], { start_line: 2.0 }, {
    // JavaScript has no 2.0 apart from 2; a Python float is not a whole number, even 2.0
    pythonOpts: { start_line: { $float: "2.0" } },
    differs: { node: "ok", python: "InvalidInput", aio: "InvalidInput", cli: "ok" },
  }),

  // --- large integers: exact where an argument takes them, else refused outright
  ok("lines", "ints", "file.write", [P, B, "/lines.txt", "l1\nl2\nl3\n"]),
  ok("line 3", "ints", "file.read", [P, B, "/lines.txt"], { start_line: 3 }, { check: (v) => assert.deepEqual([v.text, v.start_line], ["l3\n", 3]) }),
  ok("line safe max", "ints", "file.read", [P, B, "/lines.txt"], { start_line: MAX_SAFE }, { check: (v) => assert.equal(v.start_line, Number.MAX_SAFE_INTEGER) }),
  bad("line 2^53", "ints", "file.read", [P, B, "/lines.txt"], "InvalidInput", { start_line: MAX_SAFE + 1n }, { check: (v) => assert.match(v.message, /safe integer/) }),
  bad("line 2^62+1", "ints", "file.read", [P, B, "/lines.txt"], "InvalidInput", { start_line: 2n ** 62n + 1n }, { check: (v) => assert.match(v.message, /safe integer/) }),
  // 2^63-1 reaches the library unchanged (its answer is the engine's, not "out of range"): a
  // conversion through a double would round it up to 2^63
  bad("line i64 max", "ints", "file.read", [P, B, "/lines.txt"], "InvalidInput", { start_line: 2n ** 63n - 1n }, { check: (v) => assert.match(v.message, /safe integer/) }),
  bad("line i64 max + 1", "ints", "file.read", [P, B, "/lines.txt"], "InvalidInput", { start_line: 2n ** 63n }, { cli: "usage", loose: true, check: (v) => assert.match(v.message ?? v.usage, /out of range|whole number/) }),
  bad("line unsafe number", "ints", "file.read", [P, B, "/lines.txt"], "InvalidInput", { start_line: Number.MAX_SAFE_INTEGER + 2 }, { cli: "skip", loose: true, check: (v) => assert.match(v.message, /safe integer/) }),
  bad("page_size 2^62", "ints", "branch.list", [P], "InvalidInput", { page_size: 2n ** 62n }, { check: (v) => assert.match(v.message, /safe integer/) }),
  bad("mode 2^62", "ints", "file.write", [P, B, "/big.txt", "x"], "InvalidInput", { mode: 2n ** 62n }, { check: (v) => assert.match(v.message, /mode/) }),
  bad("mode i64 max + 1", "ints", "file.write", [P, B, "/big.txt", "x"], "InvalidInput", { mode: 2n ** 63n }, { cli: "usage", loose: true }),
  bad("apply mode 2^63", "ints", "file.apply", [P, B, [{ op: "chmod", path: "/lines.txt", mode: 2n ** 63n }]], "InvalidInput", {}, { cli: "usage", loose: true }),
  bad("apply mode 2^62", "ints", "file.apply", [P, B, [{ op: "chmod", path: "/lines.txt", mode: 2n ** 62n }]], "InvalidInput", {}, { check: (v) => assert.deepEqual(v.detail, { index: 0 }) }),
  bad("big written nothing", "ints", "file.stat", [P, B, "/big.txt"], "NoSuchPath"),

  // --- a real flow across Branches, then the same answers from every front end
  ok("fork b2", "flow", "branch.fork", [P, "b2", "main"]),
  ok("b2 write", "flow", "file.write", [P, "b2", "/from-b2.txt", "b2\n"], { message: "on b2" }),
  ok("merge", "flow", "branch.merge", [P, "merged", B, "b2"], { message: "merge" }),
  ok("merged read", "flow", "file.read", [P, "merged", "/from-b2.txt"], {}, { check: (v) => assert.equal(v.text, "b2\n") }),
  ok("branch diff", "flow", "branch.diff", [P, "main", "merged"], { paths_only: true }),
  ok("commits", "flow", "commit.list", [P, "merged"]),
  ok("branch get", "flow", "branch.get", [P, "merged"]),
  ok("project get", "flow", "project.get", [P]),
  ok("projects", "flow", "project.list", []),
  ok("stores", "flow", "store.list", []),
  ok("doctor", "flow", "doctor", [], {}, { check: (v) => assert.equal(typeof v.ok, "boolean") }),
];
