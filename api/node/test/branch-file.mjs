// Mountless conformance through real JS, Python sync/aio and CLI on one Store.
// Run separately for native SQLite, fresh PostgreSQL and worker SQLite:
// LAYERFS_BRANCH_FILE_ROUTE=native|postgresql|worker node api/node/test/branch-file.mjs
// Build all bindings first. See branch-file/fixture.mjs for route prerequisites.
// Missing required front ends fail this selected suite, rather than count as parity.
import { test } from "node:test";
import assert from "node:assert/strict";
import { close } from "../src/index.js";
import { fixture } from "./branch-file/fixture.mjs";
import { parity, requireFrontends, semantics } from "./branch-file/frontends.mjs";

const saved = (ref, id, kind) => {
  assert.ok(ref, "resolved saved reference");
  assert.equal(ref.id, id);
  assert.equal(ref.kind, kind);
  assert.match(ref.id, /^[0-9a-f]{66}$/);
  assert.match(ref.root, /^[0-9a-f]{64}$/);
};
const step = (name, call, args, opts = {}, check) => ({ name, call, args, opts, check });
const bad = (name, call, args, opts = {}, error = "InvalidInput") => ({ name, call, args, opts, error });

test("branch/file saved history, comparison and provenance agree across real front ends", { timeout: 100_000 }, async () => {
  requireFrontends();
  const f = await fixture();
  const { project: p, lfs } = f;
  const run = cases => parity(f.home, cases);
  try {
    // Establish a saved starting point before moving the mutable branch head.
    const [first] = await run([
      step("saved first history page", "branch.commits", [p, "left"], { at_commit: f.head, page: 1, page_size: 1 }, v => {
        saved(v.at_commit, f.head, "Commit");
        assert.equal(v.items.length, 1);
        assert.equal(v.items[0].id, f.head);
        assert.equal(v.has_more, true);
        assert.equal(v.page, 1);
      }),
    ]);
    const nextId = first.items[0].parent;
    assert.ok(nextId);
    const moved = await lfs.file.write(p, "left", "/later.txt", "head moved after first page\n");
    await run([
      step("main typed saved head", "branch.get", [p, "main"], {}, v => saved(v.saved_head, f.publication, "Layer")),
      step("side branch typed saved head", "branch.get", [p, "left"], {}, v => saved(v.saved_head, moved.head, "Commit")),
      step("unwritten branch resolves its saved base", "branch.get", [p, "unwritten"], {}, v => saved(v.saved_head, f.init, "Layer")),
      step("paged branches include main and typed heads", "branch.list", [p], { page: 1, page_size: 50 }, v => {
        assert.ok(v.items.some(b => b.name === "main"));
        for (const b of v.items) assert.ok(b.saved_head?.id);
      }),
      step("pinned second history page survives head movement", "branch.commits", [p, "left"], { at_commit: f.head, page: 2, page_size: 1 }, v => {
        saved(v.at_commit, f.head, "Commit");
        assert.equal(v.items[0].id, nextId);
      }),
      step("main publication history", "branch.commits", [p, "main"], { at_commit: f.publication, page: 1, page_size: 1 }, v => {
        saved(v.at_commit, f.publication, "Layer");
        saved(v.items[0].ref, f.publication, "Layer");
        assert.equal(v.items[0].time, null, "no persisted publication time is fabricated");
        assert.equal(v.items[0].time_status, "unavailable");
        assert.equal(v.items[0].source_commit, f.source);
        assert.equal(v.items[0].parent, f.init);
        assert.equal(v.items[0].details_omitted, true);
        assert.equal(typeof v.items[0].source_time, "number");
      }),
      step("main Init history page", "branch.commits", [p, "main"], { at_commit: f.publication, page: 2, page_size: 1 }, v => {
        assert.equal(v.items[0].id, f.init);
        assert.equal(v.has_more, false);
      }),
      step("publication details preserve relationships", "branch.get_commit", [p, "main", f.publication], {}, v => {
        saved(v.ref, f.publication, "Layer");
        saved(v.parent_ref, f.init, "Layer");
        saved(v.source_commit_ref, f.source, "Commit");
        assert.notEqual(v.parent_ref.id, v.source_commit_ref.id);
      }),
      step("first checkpoint retains base Layer", "branch.get_commit", [p, "left", f.source], {}, v => {
        saved(v.ref, f.source, "Commit");
        assert.equal(v.parent_ref, null);
        saved(v.base_layer, f.init, "Layer");
      }),
      step("legacy commit get remains available", "commit.get", [p, "left", f.source], {}, v => assert.equal(v.id, f.source)),
      step("legacy commit list remains available", "commit.list", [p, "left"], { page_size: 1 }, v => assert.equal(v.items.length, 1)),
      bad("page zero", "branch.commits", [p, "left"], { page: 0 }),
      bad("oversized page size", "branch.commits", [p, "left"], { page_size: 1001 }),
      bad("conflicting page modes", "branch.diff", [p, "main", "left"], { page: 1, page_pos: 0 }),
    ]);

    // Four Commit/Layer combinations, including the first-Commit boundary.
    const pairs = [[f.init, f.source, "Layer", "Commit", "main", "left"], [f.source, f.publication, "Commit", "Layer", "left", "main"],
      [f.init, f.publication, "Layer", "Layer", "main", "main"], [f.source, f.other, "Commit", "Commit", "left", "right"]];
    await run(pairs.map(([from, to, leftKind, rightKind, leftBranch, rightBranch]) => step(
      `${leftKind} to ${rightKind}`, "branch.diff", [p, leftBranch, rightBranch],
      { from_commit: from, to_commit: to, page: 1, page_size: 50 }, v => {
        saved(v.from, from, leftKind);
        saved(v.to, to, rightKind);
        assert.equal(typeof v.has_more, "boolean");
      },
    )));

    await run([
      ...["read", "stat", "list"].map(call => step(`pinned file.${call}`, `file.${call}`,
        [p, "left", call === "list" ? "/" : "/base.txt", f.head], call === "list" ? { page: 1, page_size: 2 } : {},
        v => saved(v.at, f.head, "Commit"))),
      step("pinned glob", "file.glob", [p, "left", "/", "**/*.txt", f.head], { page: 1, page_size: 2 },
        v => saved(v.at, f.head, "Commit")),
      step("pinned grep", "file.grep", [p, "left", "/", "alpha", f.head], { page: 1, page_size: 2 },
        v => saved(v.at, f.head, "Commit")),
      step("equal text with independently changed ownership remains discoverable", "branch.diff", [p, "main", "right"],
        { from_commit: f.publication, to_commit: f.other, include_attribution: true, page: 1, page_size: 50 }, v => {
          const file = v.items.find(i => i.path === "/base.txt");
          assert.ok(file, "attribution-only path is not pruned on equal text");
          assert.equal(file.content, false);
          assert.equal(file.attribution, true);
          assert.ok(file.changed_owner_lines > 0);
        }),
      step("file attribution aligns unchanged text and exact owners", "file.diff", [p, "main", "/base.txt", "right", "/base.txt"],
        { left_commit: f.publication, right_commit: f.other, include_attribution: true, page: 1, page_size: 50 }, v => {
          saved(v.left_ref, f.publication, "Layer");
          saved(v.right_ref, f.other, "Commit");
          assert.equal(v.changes.content, false);
          assert.equal(v.changes.attribution, true);
          const changed = v.rows.filter(r => r.owner_changed === true);
          assert.equal(changed.length, 1);
          assert.equal(changed[0].op, " ");
          assert.equal(changed[0].before_owner.ref.id, f.source);
          assert.equal(changed[0].after_owner.ref.id, f.other);
          assert.equal(v.attribution_coverage.status, "complete");
        }),
      step("publication preserves source line ownership", "file.diff", [p, "left", "/base.txt", "main", "/base.txt"],
        { left_commit: f.source, right_commit: f.publication, include_attribution: true, page: 1, page_size: 50 }, v => {
          assert.equal(v.changes.attribution, false);
          assert.ok(v.rows.every(r => r.owner_changed === false));
        }),
      step("main blame owns changed and inherited lines correctly", "file.blame", [p, "main", "/base.txt", f.publication],
        {}, v => {
          assert.equal(v.lines[0].owner.ref.id, f.source);
          assert.equal(v.lines[1].owner.ref.id, f.init);
        }),
      step("metadata-only change is independent from bytes", "file.diff", [p, "left", "/base.txt", "left", "/base.txt"],
        { left_commit: f.source, right_commit: f.head, page: 1, page_size: 50 }, v => {
          assert.equal(v.changes.content, false);
          assert.equal(v.changes.metadata, true);
        }),
      step("addition with opted-in missing side", "file.diff", [p, "main", "/new.txt", "left", "/new.txt"],
        { left_commit: f.init, right_commit: f.head, allow_missing: true, page: 1, page_size: 50 }, v => {
          assert.equal(v.before, null);
          assert.ok(v.after);
        }),
      step("deletion with opted-in missing side", "file.diff", [p, "main", "/remove.txt", "left", "/remove.txt"],
        { left_commit: f.init, right_commit: f.head, allow_missing: true, page: 1, page_size: 50 }, v => {
          assert.ok(v.before);
          assert.equal(v.after, null);
        }),
      bad("legacy missing-side refusal remains", "file.diff", [p, "main", "/new.txt", "left", "/new.txt"],
        { left_commit: f.init, right_commit: f.head }, "NoSuchPath"),
      step("binary attribution is unavailable, not equal ownership", "file.diff", [p, "left", "/blob.bin", "left", "/blob.bin"],
        { left_commit: f.binary, right_commit: f.head, include_attribution: true, page: 1, page_size: 50 }, v => {
          assert.equal(v.changes.attribution, null);
          assert.equal(v.attribution_coverage.status, "unavailable");
        }),
      step("identical saved binary endpoints prove no owner change", "file.diff", [p, "left", "/blob.bin", "left", "/blob.bin"],
        { left_commit: f.head, right_commit: f.head, include_attribution: true, page: 1, page_size: 50 }, v => {
          assert.equal(v.changes.attribution, false);
        }),
      bad("oversized mandatory diff row refuses without advancing a partial page", "file.diff",
        [p, "left", "/long.txt", "left", "/long.txt"],
        { left_commit: f.long, right_commit: f.head, include_attribution: true, page: 1, page_size: 1 }, "Capacity"),
      step("clipped blame cannot claim complete attribution", "file.blame", [p, "left", "/long.txt", f.head], {}, v => {
        assert.equal(v.coverage.status, "partial");
        assert.ok(v.lines[0].clipped);
      }),
      step("file revision includes metadata and inherited history", "file.revision", [p, "left", "/base.txt"],
        { at_commit: f.head, page: 1, page_size: 50 }, v => {
          saved(v.at, f.head, "Commit");
          assert.ok(v.items.some(i => i.ref.id === f.head && i.changes.metadata));
          assert.ok(v.items.some(i => i.ref.id === f.source && i.changes.content));
          assert.equal(typeof v.has_more, "boolean");
        }),
      bad("file revision invalid page", "file.revision", [p, "left", "/base.txt"], { page: 0 }),
      bad("saved versions cannot cross an existing project", "file.read", ["sdk/other", "main", "/base.txt", f.head], {}),
      bad("foreign Layer cannot enter branch comparison", "branch.diff", [p, "main", "left"], { from_commit: f.foreign, to_commit: f.head }),
      bad("foreign saved details cannot enter branch context", "branch.get_commit", [p, "main", f.foreign]),
      bad("foreign file comparison endpoint", "file.diff", [p, "main", "/base.txt", "left", "/base.txt"],
        { left_commit: f.foreign, right_commit: f.head, include_attribution: true }),
    ]);

    // Scope checks go through the bindings and CLI switches, not just engine calls.
    await parity(f.home, [
      bad("folder scope cannot read project history", "branch.commits", [p, "left"], {}),
      bad("folder scope cannot inspect saved record", "branch.get_commit", [p, "left", f.source], {}),
      bad("folder scope cannot compare project trees", "branch.diff", [p, "main", "left"], { include_attribution: true }),
      bad("folder scope cannot expose full revision metadata", "file.revision", [p, "left", "/inside/a.txt"], {}),
      bad("folder scope cannot escape file path", "file.read", [p, "main", "/base.txt", f.publication], {}),
    ], { project: p, folder: "/inside", branch: "agent" });
    const cachedQuery = [step("cache-stable comparison page", "branch.diff", [p, "main", "left"],
      { from_commit: f.publication, to_commit: f.head, include_attribution: true, page: 2, page_size: 2 })];
    const cold = await run(cachedQuery);
    const warm = await run(cachedQuery);
    assert.deepEqual(semantics(warm), semantics(cold), "cache state changes work counters only, not membership/coverage");
    await f.cleanup();
  } finally {
    close();
  }
});
