import { expect, test } from "bun:test"
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"

const dir = import.meta.dir
const page = join(dir, "page")
const read = (f) => readFileSync(join(page, f), "utf8")
const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"))

// view.js is a classic script, as the page loads it
const scope = {}
new Function("globalThis", read("view.js"))(scope)
const V = scope.LayerfsView

test("the package declares the page magpie shows", () => {
  const p = pkg.magpie.page
  expect(p.title).toBe("LayerFS trajectory")
  expect(p.titles.zh).toBe("LayerFS 轨迹")
  expect(p.file).toBe("page/index.html")
  expect(p.icon).toBe("page/icon.svg")
  expect(pkg.files).toContain("page")
  for (const f of ["index.html", "app.js", "view.js", "style.css", "icon.svg"])
    expect(existsSync(join(page, f))).toBe(true)
  const html = read("index.html")
  expect(html).toContain("_magpie/bridge.js") // the host serves the bridge beside every page
  expect(html).toContain('src="trajectory.js"')
  // the page sandbox forbids inline scripts and any fetch; the only data
  // script is the recorded trajectory the tools write
  expect(html).not.toMatch(/<script>(?!\s*src)/)
})

const C = (id, parent, time, over = {}) => ({
  id, parent, message: `commit ${id}`, time,
  files: 10, logicalBytes: 1000, newBytes: 100,
  delta: { added: 1, modified: 2, deleted: 0 }, ...over,
})
const project = {
  name: "demo",
  branches: [
    { name: "agent-b", commits: [C("b1", "m2", "2026-01-01T00:03:00.000Z"), C("b2", "b1", "2026-01-01T00:05:00.000Z")] },
    {
      name: "main", commits: [
        C("m1", null, "2026-01-01T00:01:00.000Z", { newBytes: 1000 }),
        C("m2", "m1", "2026-01-01T00:02:00.000Z"),
      ],
    },
    { name: "agent-a", commits: [C("a1", "m1", "2026-01-01T00:04:00.000Z")] },
  ],
}

test("lanes put main first, then branches by first checkpoint", () => {
  const lane = V.lanes(project)
  expect(lane.get("main")).toBe(0)
  expect(lane.get("agent-b")).toBe(1) // first checkpoint 00:03, before agent-a's 00:04
  expect(lane.get("agent-a")).toBe(2)
})

test("steps are time-ordered and accumulate both curves", () => {
  const s = V.steps(project)
  expect(s.map((x) => x.commit.id)).toEqual(["m1", "m2", "b1", "a1", "b2"])
  expect(s[0].cumLogical).toBe(1000)
  expect(s[0].cumStored).toBe(1000) // the import stores everything once
  expect(s[4].cumLogical).toBe(5000) // five checkpoints as copies
  expect(s[4].cumStored).toBe(1400) // but only the changed bytes were new
})

test("layout links each checkpoint to its parent, across lanes on a fork", () => {
  const L = V.layout(project, [])
  expect(L.nodes.length).toBe(5)
  expect(L.laneCount).toBe(3)
  const edge = (id) => L.edges.find((e) => e.to.id === id)
  expect(edge("m2").fork).toBe(false)
  expect(edge("b1").fork).toBe(true) // agent-b forked from main's m2
  expect(edge("b1").from.id).toBe("m2")
  expect(edge("a1").from.id).toBe("m1")
  expect(L.nodes[4].x).toBeGreaterThan(L.nodes[0].x)
})

test("a publish grafts its branch back onto main and adds a Layer band", () => {
  const events = [{ kind: "publish", project: "demo", branch: "agent-b", commit: "b2", layer: 1, t: "2026-01-01T00:06:00.000Z" }]
  expect(V.publishes(project, events)).toEqual([{ layer: 1, commit: "b2", branch: "agent-b", time: "2026-01-01T00:06:00.000Z", overwrote: false }])
  const L = V.layout(project, events)
  expect(L.topLayer).toBe(1)
  expect(L.grafts.length).toBe(1)
  expect(L.grafts[0].from.branch).toBe("agent-b")
  expect(L.grafts[0].to.branch).toBe("main")
  expect(L.grafts[0].to.landing).toBe(true) // main gains a landing leaf, not a copy
  expect(L.graftedBranches.has("agent-b")).toBe(true)
  expect(L.graftedBranches.has("agent-a")).toBe(false) // still a bare twig
  expect(L.bands.map((b) => b.layer)).toEqual([0, 1]) // L0 up to the publish, L1 after
  expect(L.bands[1].x0).toBe(L.bands[0].x1)
})

test("a branch's spine grows out of its fork point on the parent's lane", () => {
  const L = V.layout(project, [])
  const spine = L.spines.get("agent-b")
  expect(spine[0].id).toBe("m2") // the fork point, drawn on main's lane
  expect(spine[0].lane).toBe(0)
  expect(spine.slice(1).map((n) => n.id)).toEqual(["b1", "b2"])
  // and each commit is counted once, however many branches descend from it
  const total = L.nodes[L.nodes.length - 1]
  expect(total.cumLogical).toBe(5000)
  expect(L.nodes.length).toBe(5)
})

test("branches name their agent by token, in that agent's colour", () => {
  expect(V.agentOf("claude").name).toBe("Claude Code")
  expect(V.agentOf("claude").color).toBe("#d97757")
  expect(V.agentOf("claude-fix").id).toBe("claude")
  expect(V.agentOf("codex/feat").id).toBe("codex")
  expect(V.agentOf("pi").mark).toBe("Pi")
  expect(V.agentOf("pipeline")).toBeNull() // a token match, not a substring
  expect(V.agentOf("release")).toBeNull()
  expect(V.agentOf("agent-1")).toBeNull()
})

test("a checkpoint's detail line carries its delta and byte split", () => {
  const d = V.detail(project.branches[1].commits[1], "en")
  expect(d.meta).toContain("10 files")
  expect(d.meta).toContain("100 B / 1.0 KB")
  expect(d.delta).toContain("+1 added")
  expect(d.delta).toContain("~2 modified")
  expect(V.detail(project.branches[1].commits[1], "zh").delta).toContain("新增")
})
