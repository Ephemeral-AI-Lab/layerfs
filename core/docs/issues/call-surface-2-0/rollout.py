"""Builds rollout.html: the call surface page's head and styles, plus rollout-body.html."""
import pathlib, sys
S = pathlib.Path(sys.argv[1])
page = (S / "layerfs-call-surface.html").read_text()
head = page[: page.index('<div class="wrap">')]
assert head.count("<title>LayerFS 2.0 Call Surface</title>") == 1
head = head.replace("<title>LayerFS 2.0 Call Surface</title>", "<title>LayerFS 2.0 Rollout</title>")
extra = """
/* rollout page: trees, fan-out, phase lanes */
:root { --t-api: #e0eefc; --t-api-fg: #1f4f86; --t-doctor: #dcf3e6; --t-doctor-fg: #17613a; --t-ui: #ece8fb; --t-ui-fg: #4a3a99; --t-doc: #eceff0; --t-doc-fg: #4e5a5f; }
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --t-api: #17304a; --t-api-fg: #9cc8f5; --t-doctor: #143523; --t-doctor-fg: #87dba9; --t-ui: #2b2546; --t-ui-fg: #c3b6ff; --t-doc: #232c2b; --t-doc-fg: #a9b6b3; } }
:root[data-theme="dark"] { --t-api: #17304a; --t-api-fg: #9cc8f5; --t-doctor: #143523; --t-doctor-fg: #87dba9; --t-ui: #2b2546; --t-ui-fg: #c3b6ff; --t-doc: #232c2b; --t-doc-fg: #a9b6b3; }
section h3 { margin-top: 22px; }
.dia { background: var(--sheet); border: 1px solid var(--rule); border-radius: 10px; padding: 16px 18px; overflow-x: auto; }
.dia .small { margin: 12px 0 0; }
.tree, .tree ul { list-style: none; margin: 0; padding: 0; }
.tree ul { padding-left: 22px; }
.tree li { position: relative; padding: 5px 0 5px 16px; border-left: 1px solid var(--rule); }
.tree li::before { content: ""; position: absolute; left: 0; top: 17px; width: 11px; border-top: 1px solid var(--rule); }
.tree li:last-child { border-left-color: transparent; }
.tree li:last-child::after { content: ""; position: absolute; left: -1px; top: 0; height: 17px; border-left: 1px solid var(--rule); }
.tree > li { border-left: 0; padding-left: 0; }
.tree > li::before, .tree > li::after { display: none; }
.node { display: inline-flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px; min-width: 0; }
.node b { font-family: var(--font-display); font-weight: 600; }
.node i { font-style: normal; color: var(--muted); font-size: 13.5px; }
.node em { font-style: normal; font-size: 12px; padding: 1px 8px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); }
.node .dot { align-self: center; }
.fan { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0 18px; align-items: center; }
.fan-src { font-family: var(--font-display); font-weight: 600; padding: 8px 14px; border: 1px solid var(--accent); color: var(--accent); border-radius: 8px; white-space: nowrap; }
.fan-rows { display: grid; gap: 8px; border-left: 1px solid var(--rule); padding-left: 18px; }
.fan-row { display: flex; flex-wrap: wrap; gap: 4px 12px; align-items: baseline; }
.fan-row span { color: var(--muted); font-size: 13.5px; }
.fan-row b { font-weight: 600; font-size: 13px; }
@media (max-width: 560px) { .fan { grid-template-columns: minmax(0, 1fr); gap: 10px; } .fan-src { justify-self: start; } }
.sigrow { display: flex; flex-wrap: wrap; gap: 8px 22px; padding: 12px 16px; background: var(--sheet); border: 1px solid var(--rule); border-radius: 10px; }
.sigword { display: inline-flex; align-items: center; gap: 8px; font-weight: 500; }
.lanes2 { display: grid; grid-template-columns: 96px repeat(6, minmax(132px, 1fr)); min-width: 940px; border-top: 1px solid var(--rule); border-left: 1px solid var(--rule); }
.lanes2 > div { border-right: 1px solid var(--rule); border-bottom: 1px solid var(--rule); padding: 8px; min-width: 0; }
.lanes2 .lh { font-size: 12px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; display: flex; align-items: center; }
.lanes2 .ph { font-family: var(--font-display); font-weight: 700; font-size: 20px; line-height: 1.1; }
.lanes2 .ph small { display: block; font-family: var(--font-body); font-weight: 500; font-size: 12.5px; color: var(--muted); margin-top: 3px; }
.lanes2 .cell { display: flex; flex-wrap: wrap; gap: 5px; align-content: flex-start; }
.lanes2 .ag { font-variant-numeric: tabular-nums; font-weight: 600; font-size: 13px; }
.pk { display: inline-block; font-size: 12.5px; line-height: 1.25; padding: 4px 8px; border-radius: 6px; font-weight: 500; }
.pk small { display: block; font-weight: 400; font-size: 11px; opacity: 0.85; }
.t-api { background: var(--t-api); color: var(--t-api-fg); }
.t-doctor { background: var(--t-doctor); color: var(--t-doctor-fg); }
.t-ui { background: var(--t-ui); color: var(--t-ui-fg); }
.t-doc { background: var(--t-doc); color: var(--t-doc-fg); }
td.chg .badge { white-space: nowrap; }
"""
assert head.count("</style>") == 1
head = head.replace("</style>", extra + "</style>")
body = (S / "rollout-body.html").read_text()
out = head + body
(S / "rollout.html").write_text(out)
import re
opens = len(re.findall(r"<(div|section|table|ul|li|span|tr|td|th|tbody|thead|main|nav|header|p|h2|h3|code|b|i|em|small)\b", body))
closes = len(re.findall(r"</(div|section|table|ul|li|span|tr|td|th|tbody|thead|main|nav|header|p|h2|h3|code|b|i|em|small)>", body))
print("bytes", len(out), "tags open/close", opens, closes)
