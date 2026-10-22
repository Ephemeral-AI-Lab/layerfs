// The trajectory page's working parts, with no DOM in them, so a test can
// check them. The model follows LayerFS's own History picture: main is
// the middle bough, each branch a bough of its own, a commit a leaf on
// its bough, a fork a split, a publish a graft back onto main that adds
// a Layer band. A classic script: it sets globalThis.LayerfsView.
(() => {
  const TEXT = {
    en: {
      title: "LayerFS trajectory", replay: "Replay", pause: "Pause",
      commits: "checkpoints", stored: "stored", asCopies: "as copies",
      saved: "saved by dedup", added: "added", modified: "modified", deleted: "deleted",
      files: "files", newBytes: "new bytes", noData: "No trajectory recorded yet. When an agent provisions, forks or checkpoints, the path appears here.",
      sample: "Sample trajectory from the plugin's benchmark run.",
      recorded: "Recorded", branch: "branch", workspace: "workspace",
      live: "live", layers: "layers", history: "HISTORY", stats: "store.stats · read-only",
      dedupe: "dedupe", graft: "published", twig: "bare twig",
    },
    zh: {
      title: "LayerFS 轨迹", replay: "回放", pause: "暂停",
      commits: "个检查点", stored: "实际存储", asCopies: "若逐份复制",
      saved: "去重省下", added: "新增", modified: "修改", deleted: "删除",
      files: "个文件", newBytes: "新增字节", noData: "还没有记录到轨迹。agent 一旦 provision、fork 或 checkpoint，它的路径就会出现在这里。",
      sample: "示例轨迹，来自插件的基准运行。",
      recorded: "记录于", branch: "分支", workspace: "工作区",
      live: "条存活分支", layers: "层", history: "HISTORY", stats: "store.stats · 只读",
      dedupe: "去重", graft: "已发布", twig: "未发布的枝条",
    },
  };

  function text(lang) {
    return String(lang || "").toLowerCase().startsWith("zh") ? TEXT.zh : TEXT.en;
  }

  // The agents magpie drives, in the colours their own products wear, so
  // a branch an agent worked on carries that agent's identity in the
  // picture: its colour on the bough and leaves, its monogram on the
  // name tag. Matching is by token: "claude", "claude-fix" and
  // "codex/feat" name their agent; anything else keeps the palette.
  const AGENTS = {
    claude: { name: "Claude Code", color: "#d97757", mark: "Cl" },
    codex: { name: "Codex", color: "#10a37f", mark: "Cx" },
    opencode: { name: "OpenCode", color: "#e7e5e4", mark: "Oc" },
    cursor: { name: "Cursor", color: "#94a3b8", mark: "Cu" },
    gemini: { name: "Gemini", color: "#5b8def", mark: "Gm" },
    pi: { name: "Pi", color: "#f59e0b", mark: "Pi" },
    dsh: { name: "DeepSeek Harness", color: "#4d6bfe", mark: "Ds" },
    qoder: { name: "Qoder", color: "#6366f1", mark: "Qd" },
    cline: { name: "Cline", color: "#38bdf8", mark: "Cn" },
    grok: { name: "Grok", color: "#d4d4d8", mark: "Gk" },
    kimi: { name: "Kimi", color: "#a78bfa", mark: "Km" },
    droid: { name: "Droid", color: "#34d399", mark: "Dr" },
    hermes: { name: "Hermes", color: "#f472b6", mark: "Hm" },
    goose: { name: "Goose", color: "#fbbf24", mark: "Gs" },
    zcode: { name: "ZCode", color: "#22d3ee", mark: "Zc" },
  };

  function agentOf(branch) {
    const tokens = String(branch || "").toLowerCase().split(/[-_/ ]+/).filter(Boolean);
    for (const tok of tokens) if (AGENTS[tok]) return { id: tok, ...AGENTS[tok] };
    return null;
  }

  function bytes(n) {
    n = Number(n) || 0;
    if (n >= 1e9) return (n / 1e9).toFixed(2) + " GB";
    if (n >= 1e6) return (n / 1e6).toFixed(1) + " MB";
    if (n >= 1e3) return (n / 1e3).toFixed(1) + " KB";
    return n + " B";
  }

  // Lane 0 is main, the middle bough; other branches follow in the order
  // their first checkpoint appears.
  function lanes(project) {
    const firstAt = new Map();
    for (const b of project.branches) {
      if (b.commits.length) firstAt.set(b.name, b.commits[0].time);
    }
    const names = project.branches.map((b) => b.name).sort((a, b) => {
      if (a === "main") return -1;
      if (b === "main") return 1;
      return String(firstAt.get(a) ?? "").localeCompare(String(firstAt.get(b) ?? "")) || a.localeCompare(b);
    });
    return new Map(names.map((name, i) => [name, i]));
  }

  // Every commit of the project in the order the replay reveals them:
  // by time, ties broken by lane then id so a fork's first checkpoint
  // never precedes the checkpoint it forked from. A branch's list holds
  // only its own checkpoints (ancestors belong to their own bough), so
  // each commit appears exactly once and the curves count it once.
  function steps(project) {
    const lane = lanes(project);
    const all = [];
    for (const b of project.branches) {
      for (const c of b.commits) all.push({ branch: b.name, lane: lane.get(b.name), commit: c });
    }
    all.sort((x, y) =>
      String(x.commit.time).localeCompare(String(y.commit.time)) ||
      x.lane - y.lane || String(x.commit.id).localeCompare(String(y.commit.id)));
    let cumLogical = 0, cumStored = 0;
    for (const s of all) {
      cumLogical += s.commit.logicalBytes || 0;
      cumStored += s.commit.newBytes || 0;
      s.cumLogical = cumLogical;
      s.cumStored = cumStored;
    }
    return all;
  }

  // The publishes that landed on this project, oldest first, each one a
  // Layer on main. Falls back to numbering by order when an older
  // recording has no layer field.
  function publishes(project, events) {
    const pubs = (events || [])
      .filter((e) => e.kind === "publish" && e.project === project.name)
      .sort((a, b) => String(a.t).localeCompare(String(b.t)));
    return pubs.map((e, i) => ({
      layer: Number.isInteger(e.layer) ? e.layer : i + 1,
      commit: e.commit, branch: e.branch, time: e.t, overwrote: !!e.overwrote,
    }));
  }

  // The picture, worked out: one node per checkpoint on the lane of the
  // branch that made it; a publish adds a landing node on main's lane at
  // the same step and a graft from the branch's leaf to it; each branch's
  // spine runs from its fork point (a node on another lane) through its
  // own leaves; main's Layer bands run between consecutive landings.
  function layout(project, events, { dx = 54, laneH = 64, padX = 34, padTop = 42 } = {}) {
    const lane = lanes(project);
    const ordered = steps(project);
    const nodes = ordered.map((s, i) => ({
      id: s.commit.id, branch: s.branch, lane: s.lane, step: i,
      x: padX + i * dx, y: padTop + s.lane * laneH, commit: s.commit,
      cumLogical: s.cumLogical, cumStored: s.cumStored, landing: false,
    }));
    const byId = new Map(nodes.map((n) => [n.id, n]));
    const pubs = publishes(project, events);
    const mainLane = lane.get("main") ?? 0;
    const landings = [];
    const grafts = [];
    for (const pub of pubs) {
      const from = byId.get(pub.commit);
      if (!from || from.branch === "main") continue;
      const to = {
        id: pub.commit, branch: "main", lane: mainLane, step: from.step,
        x: from.x, y: padTop + mainLane * laneH, commit: from.commit,
        cumLogical: from.cumLogical, cumStored: from.cumStored, landing: true,
      };
      landings.push(to);
      grafts.push({ from, to, layer: pub.layer });
    }
    // parent edges: within a lane along the bough, or a fork split when
    // the parent leaf sits on another lane
    const edges = [];
    for (const n of nodes) {
      const p = n.commit.parent && byId.get(n.commit.parent);
      if (p && p !== n) edges.push({ from: p, to: n, fork: p.lane !== n.lane });
    }
    // spines: per branch, the fork point first (when it lives on another
    // lane), then the branch's own leaves in order. Main's spine also
    // runs through its landing leaves.
    const spines = new Map();
    for (const b of project.branches) {
      const own = nodes.filter((n) => n.branch === b.name);
      const pts = [];
      if (b.name !== "main" && own.length) {
        const parent = byId.get(own[0].commit.parent);
        if (parent && parent.lane !== own[0].lane) pts.push({ ...parent, spineStep: own[0].step });
      }
      pts.push(...own);
      if (b.name === "main") {
        const merged = [...pts, ...landings].sort((a, z) => a.x - z.x || a.step - z.step);
        spines.set(b.name, merged);
      } else {
        spines.set(b.name, pts);
      }
    }
    // A branch whose head was published is grafted; any other branch is
    // still a bare twig at its tip.
    const grafted = new Set(grafts.map((g) => g.from.branch));
    const bands = [];
    const mainExtent = spines.get("main") || [];
    if (mainExtent.length) {
      let start = mainExtent[0].x, layer = 0;
      for (const g of grafts) {
        bands.push({ layer, x0: start, x1: g.to.x });
        start = g.to.x; layer = g.layer;
      }
      bands.push({ layer, x0: start, x1: mainExtent[mainExtent.length - 1].x });
    }
    const laneCount = Math.max(1, lane.size);
    return {
      nodes, landings, edges, grafts, spines, bands,
      topLayer: pubs.length ? pubs[pubs.length - 1].layer : 0,
      graftedBranches: grafted,
      width: padX * 2 + Math.max(0, nodes.length - 1) * dx,
      height: padTop * 2 + (laneCount - 1) * laneH + 18,
      laneOf: (name) => lane.get(name), laneCount,
    };
  }

  // One line of a checkpoint's detail card.
  function detail(commit, lang) {
    const t = text(lang);
    const d = commit.delta || { added: 0, modified: 0, deleted: 0 };
    const parts = [];
    if (d.added) parts.push(`+${d.added} ${t.added}`);
    if (d.modified) parts.push(`~${d.modified} ${t.modified}`);
    if (d.deleted) parts.push(`-${d.deleted} ${t.deleted}`);
    return {
      message: commit.message || "",
      meta: `${commit.files == null ? "" : `${commit.files} ${t.files} · `}${t.newBytes} ${bytes(commit.newBytes)} / ${bytes(commit.logicalBytes)}`,
      delta: parts.join(" · "),
    };
  }

  globalThis.LayerfsView = { TEXT, text, bytes, lanes, steps, publishes, layout, detail, AGENTS, agentOf };
})();
