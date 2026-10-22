// The trajectory page, drawn the way LayerFS's own History picture is:
// main is the green middle bough, every other branch a bough that splits
// off it, a checkpoint a leaf on its bough, and a publish a graft back
// onto main that adds a Layer band. The replay reveals the leaves in
// the order they were committed; click a name tag to focus one branch,
// click a leaf for its checkpoint.
//
// The page may read no live data (its sandbox allows no fetch), so the
// tools write the recorded trajectory beside this page as trajectory.js
// after every call; reopening the page shows the newest recording.
(async () => {
  const V = globalThis.LayerfsView;
  const $ = (id) => document.getElementById(id);
  const SVG = "http://www.w3.org/2000/svg";
  // main is always the leaf green; branches take the rest in lane order
  const PALETTE = ["#4ade80", "#7dd3fc", "#a5b4fc", "#fcd34d", "#f9a8d4", "#5eead4", "#fca5a5", "#c4b5fd", "#fdba74", "#94a3b8"];

  let theme = { lang: "en", dark: true };
  try { theme = (await magpie.ready) || theme; } catch { /* standalone preview */ }
  const lang = theme.lang || "en";
  const t = V.text(lang);
  document.querySelectorAll("[data-t]").forEach((e) => { e.textContent = t[e.dataset.t]; });

  const data = globalThis.LayerfsTrajectory;
  const projects = (data && data.projects || []).filter((p) => p.branches.some((b) => b.commits.length));
  if (!projects.length) {
    $("stage").hidden = true; $("controls").hidden = true; $("stats").hidden = true;
    $("empty").hidden = false; $("empty").textContent = t.noData;
    return;
  }
  $("stamp").textContent = data.sample ? t.sample : `${t.recorded} ${String(data.generatedAt || "").replace("T", " ").slice(0, 19)} · ${data.backend || ""}`;

  const picker = $("project");
  for (const p of projects) {
    const o = document.createElement("option");
    o.value = p.name; o.textContent = p.name;
    picker.append(o);
  }
  picker.hidden = projects.length < 2;
  picker.addEventListener("change", () => show(picker.value));

  let timer = null, revealed = 0, current = null, speed = 1, focus = null;

  function el(name, attrs = {}, parent) {
    const e = document.createElementNS(SVG, name);
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
    if (parent) parent.append(e);
    return e;
  }
  // a branch an agent worked on wears that agent's colour; the rest
  // take the palette by lane
  const color = (lane) => PALETTE[lane % PALETTE.length];
  const colorOf = (branch, lane) => (V.agentOf(branch) || {}).color || color(lane);
  const LEAF = "M 0 -10 C 8 -6, 10 4, 0 11 C -10 4, -8 -6, 0 -10 Z";
  const seg = (a, b) => {
    const mx = (a.x + b.x) / 2;
    return `M ${a.x} ${a.y} C ${mx} ${a.y}, ${mx} ${b.y}, ${b.x} ${b.y}`;
  };

  function show(name) {
    stop(); focus = null;
    const project = projects.find((p) => p.name === name) || projects[0];
    const stepsCount = V.steps(project).length;
    const wide = Math.max(320, ($("stage").clientWidth || 720) - 8);
    // never squish below a readable pitch: the stage scrolls instead
    const dx = stepsCount > 1 ? Math.max(46, Math.min(76, (wide - 68) / (stepsCount - 1))) : 54;
    const L = V.layout(project, data.events, { dx });
    current = { project, L };
    $("histLine").textContent = `${t.history} ${project.name} @ L${L.topLayer}`;
    $("histMeta").textContent = `${L.nodes.length} commits · ${L.laneCount} ${t.live} · ${L.topLayer + 1} ${t.layers}`;
    const svg = $("graph");
    svg.replaceChildren();
    svg.removeAttribute("data-focus");
    svg.setAttribute("viewBox", `0 0 ${L.width} ${L.height}`);
    svg.setAttribute("width", L.width); svg.setAttribute("height", L.height);
    const all = [...L.nodes, ...L.landings];

    // Layer bands on main: a stratum behind the bough per layer
    const mainY = (L.spines.get("main") || [])[0]?.y ?? 42;
    L.bands.forEach((band, i) => {
      const endNode = all.find((n) => n.branch === "main" && n.x === band.x1);
      const g = el("g", { class: "band", "data-step": endNode ? endNode.step : 0 }, svg);
      el("rect", {
        x: band.x0 - 16, y: mainY - 18, width: Math.max(30, band.x1 - band.x0 + 32), height: 36, rx: 18,
        class: "band-fill",
      }, g);
      const label = el("text", { y: mainY - 28, class: "band-label" }, g);
      // the band a head publish just opened has no width yet: anchor its
      // label at the publish point instead of past the edge
      if (i === L.bands.length - 1 && band.x1 - band.x0 < 60) {
        label.setAttribute("x", band.x1 + 14); label.setAttribute("text-anchor", "end");
      } else {
        label.setAttribute("x", band.x0 - 6);
      }
      label.textContent = `L${band.layer}`;
    });
    // grafts under everything: a publish joining a branch back to main
    for (const gr of L.grafts) {
      el("path", {
        d: seg(gr.from, gr.to), class: "graft",
        "data-step": gr.to.step, "data-branch": gr.from.branch, stroke: colorOf(gr.from.branch, gr.from.lane),
      }, svg);
    }
    // one group per branch: spine segments, leaves, name tag
    for (const [branch, pts] of L.spines) {
      const g = el("g", { class: "lane", "data-branch": branch }, svg);
      const laneIdx = L.laneOf(branch);
      for (let i = 1; i < pts.length; i++) {
        const a = pts[i - 1], b = pts[i];
        el("path", {
          d: seg(a, b), class: "seg" + (a.lane !== b.lane ? " split-seg" : ""),
          stroke: colorOf(branch, laneIdx), "data-step": b.step, "data-branch": branch,
        }, g);
      }
    }
    for (const n of all) {
      const g = el("g", { class: "lane", "data-branch": n.branch }, svg);
      const leaf = el("g", { class: "node" + (n.landing ? " landing" : ""), "data-step": n.step, "data-branch": n.branch, transform: `translate(${n.x} ${n.y})` }, g);
      el("path", { d: LEAF, class: "leaf", fill: colorOf(n.branch, n.lane), transform: "rotate(32) scale(1.3)" }, leaf);
      el("circle", { r: 2.1, class: "leaf-core" }, leaf);
      leaf.addEventListener("click", () => select(n, true));
    }
    // name tags at each branch's first own leaf; main needs none — the
    // HISTORY line and the Layer bands name it. An agent's branch gets
    // its monogram chip in the agent's colour, like magpie's own rows.
    for (const b of project.branches) {
      if (b.name === "main") continue;
      const first = L.nodes.find((n) => n.branch === b.name);
      if (!first) continue;
      const g = el("g", { class: "lane", "data-branch": b.name }, svg);
      const tag = el("g", { class: "tag", transform: `translate(${first.x - 4} ${first.y - 32})` }, g);
      const agent = V.agentOf(b.name);
      let nameX = 0;
      if (agent) {
        el("rect", { x: -2, y: -11, width: 23, height: 15, rx: 3.5, fill: agent.color, class: "chip" }, tag);
        const mark = el("text", { x: 9.5, y: 1, class: "chip-mark" }, tag);
        mark.textContent = agent.mark;
        nameX = 27;
      }
      const label = el("text", { x: nameX, y: 1, class: "tag-label", fill: colorOf(b.name, first.lane) }, tag);
      label.textContent = b.name + (!L.graftedBranches.has(b.name) ? " ·" : "");
      tag.addEventListener("click", () => setFocus(focus === b.name ? null : b.name));
    }
    // legend: which agents are in this picture, in their own colours
    const legend = $("legend");
    legend.replaceChildren();
    const seen = new Map();
    for (const b of project.branches) {
      const a = V.agentOf(b.name);
      if (a && !seen.has(a.id)) seen.set(a.id, a);
    }
    for (const a of seen.values()) {
      const item = document.createElement("span");
      item.className = "legend-item";
      const dot = document.createElement("i");
      dot.style.background = a.color;
      item.append(dot, document.createTextNode(a.name));
      legend.append(item);
    }
    legend.hidden = seen.size === 0;
    $("scrub").max = L.nodes.length;
    setRevealed(matchMedia("(prefers-reduced-motion: reduce)").matches ? L.nodes.length : 0);
    if (revealed === 0 && L.nodes.length) play();
  }

  function setFocus(branch) {
    focus = branch;
    const svg = $("graph");
    if (!branch) { svg.removeAttribute("data-focus"); return; }
    svg.setAttribute("data-focus", branch);
    document.querySelectorAll("#graph .lane, #graph .graft").forEach((e) => {
      e.classList.toggle("focus", e.dataset.branch === branch);
    });
  }

  function setRevealed(n) {
    if (!current) return;
    revealed = Math.max(0, Math.min(current.L.nodes.length, n));
    $("scrub").value = revealed;
    document.querySelectorAll("#graph .node, #graph .band, #graph .graft, #graph .seg").forEach((e) => {
      e.classList.toggle("on", Number(e.dataset.step) < revealed);
    });
    const last = current.L.nodes[revealed - 1];
    const total = current.L.nodes[current.L.nodes.length - 1];
    $("nCommits").textContent = `${revealed} / ${current.L.nodes.length} ${t.commits}`;
    const cumL = last ? last.cumLogical : 0, cumS = last ? last.cumStored : 0;
    $("barCopies").style.width = total && total.cumLogical ? `${Math.max(1.5, (cumL / total.cumLogical) * 100)}%` : "0%";
    $("barStored").style.width = total && total.cumLogical ? `${Math.max(1.5, (cumS / total.cumLogical) * 100)}%` : "0%";
    $("vCopies").textContent = V.bytes(cumL);
    $("vStored").textContent = V.bytes(cumS);
    $("vSaved").textContent = cumL > 0 ? `${t.saved} ${V.bytes(cumL - cumS)} (${Math.round((1 - cumS / cumL) * 100)}%)` : "";
    if (last) select(last, false);
  }

  function select(node, pin) {
    const d = V.detail(node.commit, lang);
    const graft = current.L.grafts.find((g) => g.to === node);
    $("dBranch").textContent = `${t.branch}: ${node.branch}` +
      (graft ? ` · ${t.graft} L${graft.layer}` : node.branch !== "main" && isTip(node) && !current.L.graftedBranches.has(node.branch) ? ` · ${t.twig}` : "");
    $("dMsg").textContent = d.message;
    $("dMeta").textContent = d.meta;
    $("dDelta").textContent = d.delta;
    $("dTime").textContent = String(node.commit.time || "").replace("T", " ").slice(0, 19);
    document.querySelectorAll("#graph .node").forEach((e) => e.classList.toggle("sel", Number(e.dataset.step) === node.step && e.dataset.branch === node.branch));
    if (pin) stop();
  }
  function isTip(node) {
    return !current.L.nodes.some((n) => n.branch === node.branch && n.step > node.step);
  }

  function play() {
    if (!current || timer) return;
    if (revealed >= current.L.nodes.length) setRevealed(0);
    $("play").textContent = t.pause;
    timer = setInterval(() => {
      setRevealed(revealed + 1);
      if (revealed >= current.L.nodes.length) stop();
    }, 620 / speed);
  }
  function stop() {
    if (timer) clearInterval(timer);
    timer = null;
    $("play").textContent = t.replay;
  }
  $("play").addEventListener("click", () => (timer ? stop() : play()));
  $("scrub").addEventListener("input", () => { stop(); setRevealed(Number($("scrub").value)); });
  $("speed").addEventListener("change", () => { speed = Number($("speed").value); if (timer) { stop(); play(); } });

  // the store.stats line, in LayerFS's own accounting words
  const totals = data.totals || {};
  $("statsLine").textContent =
    `${t.stats} · ${t.dedupe} ${totals.dedupRatio ?? "–"}× · ${t.stored} ${V.bytes(totals.storedBytes)} / ${V.bytes(totals.logicalBytes)}`;

  show(projects[0].name);
})();
