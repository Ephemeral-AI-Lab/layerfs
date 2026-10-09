"""Generate the per-crate coverage audit and check that it is complete and that
every active test or file it names exists."""
import collections, os, re, subprocess, sys
S = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, S)
import audit_data_small as sm, audit_data_large as lg
ROOT = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs"
OUT = ROOT + "/core/docs/issues/307/checks/r7-retire-20261010"
DIRS = {"D": "layerfs-daemon/tests", "W": "layerfs-workspace/tests", "O": "layerfs-overlay/tests",
        "F": "layerfs-fuse/tests", "B": "layerfs-bridge/tests", "S": "layerfs-sandbox/tests",
        "K": "layerfs-api/sdk/tests", "P": "layerfs-project/tests", "C": "layerfs-content/tests",
        "PE": "layerfs-persistence/tests", "ST": "layerfs-storage/tests"}

loc = {}
for line in open(S + "/loc-head.txt"):
    parts = line.split()
    if len(parts) >= 4 and parts[0].startswith("core/crates/"):
        loc[parts[0]] = int(parts[2])

def parse_tests(path):
    out, current = collections.OrderedDict(), None
    for line in open(path):
        if line.startswith("core/"):
            current = line.strip(); out[current] = []
        elif line.strip():
            out[current].append(line.strip())
    return out
legacy, active = parse_tests(S + "/legacy-tests.txt"), parse_tests(S + "/active-tests.txt")
tracked = subprocess.check_output(["git", "ls-files", "-co", "--exclude-standard", "core/crates"], cwd=ROOT, text=True).split()
errors = []

def check_refs(text, where):
    """Every `X \\`file.rs::test\\`` and `X \\`file.rs\\` (N` names something real."""
    for short, file, test in re.findall(r"\b(PE|ST|[DWOFBSKPC]) `([\w/]+\.rs)(?:::(\w+))?`", text):
        path = f"core/crates/{DIRS[short]}/{file}"
        if path not in tracked:
            errors.append(f"{where}: no such active file {path}")
        elif test and test not in active.get(path, []):
            errors.append(f"{where}: no test {test} in {path}")
    for short, file, count in re.findall(r"\b(PE|ST|[DWOFBSKPC]) `([\w/]+\.rs)` \((\d+)", text):
        path = f"core/crates/{DIRS[short]}/{file}"
        if path in active and len(active[path]) != int(count):
            errors.append(f"{where}: {path} has {len(active[path])} tests, text says {count}")
    for short, glob, count in re.findall(r"\b(PE|ST|[DWOFBSKPC]) `([\w/]+)\*\.rs` \((\d+)", text):
        total = sum(len(v) for k, v in active.items() if k.startswith(f"core/crates/{DIRS[short]}/{glob}"))
        if total != int(count):
            errors.append(f"{where}: {glob}*.rs has {total} tests, text says {count}")

def esc(text):
    return text.replace("|", "\\|")

def crate_doc(number, crate, title, verdict, intro, src, tests, other, src_dirs=None, all_c=None):
    base = f"core/crates/{crate}/"
    files = [p for p in tracked if p.startswith(base)]
    src_files = sorted(p for p in files if p[len(base):].startswith("src/"))
    lines = [f"# {title}", "", "> **Status:** Dated checkpoint receipt; coverage audit written before any removal.", "",
             intro, "", f"Verdict: **{verdict}**", "", "## Source", "",
             f"Production lines from the pinned counter at `cf5bd62ce` ([receipt 01](01-production-loc-head.txt)). "
             f"Total {sum(loc[p] for p in src_files)} in {len(src_files)} files.", "",
             "| File | Lines | Behaviour | Class | Covered by / recorded direction |", "| --- | ---: | --- | --- | --- |"]
    tally = collections.Counter()
    if src_dirs:
        for prefix, (cls, what, where) in src_dirs.items():
            members = [p for p in src_files if p[len(base):].startswith(prefix) and p[len(base):] not in src]
            lines.append(f"| `{prefix}` ({len(members)} files not listed singly) | {sum(loc[p] for p in members)} | {esc(what)} | {cls} | {esc(where)} |")
            tally[cls] += sum(loc[p] for p in members); check_refs(where, crate + " " + prefix)
    for p in src_files:
        rel = p[len(base):]
        if rel in src:
            cls, what, where = src[rel]
            lines.append(f"| `{rel}` | {loc[p]} | {esc(what)} | {cls} | {esc(where)} |")
            tally[cls] += loc[p]; check_refs(where, crate + " " + rel)
        elif not (src_dirs and any(rel.startswith(d) for d in src_dirs)):
            errors.append(f"{crate}: source {rel} is not classified")
    for rel in src:
        if base + rel not in src_files:
            errors.append(f"{crate}: classified source {rel} does not exist")
    assert sum(tally.values()) == sum(loc[p] for p in src_files), crate
    lines += ["", "Lines by class: " + ", ".join(f"{k} {v}" for k, v in sorted(tally.items())) + ".", "", "## Tests", ""]
    test_files = [p for p in legacy if p.startswith(base)]
    count = sum(len(legacy[p]) for p in test_files)
    lines += [f"{count} test functions in {sum(1 for p in test_files if legacy[p])} files. None can be built at HEAD "
              "([receipt 02](02-cargo-cannot-load.txt)), so none is coverage today; each is classified by the behaviour it asserted.", "",
              "| Test | Class | Covered by / recorded direction |", "| --- | --- | --- |"]
    ttally = collections.Counter()
    for p in test_files:
        rel = p[len(base):]
        names = legacy[p]
        if not names:
            continue
        table = tests.get(rel)
        lines.append(f"| **`{rel}`** ({len(names)}) | | |")
        if table == "ALL_C_D3":
            lines.append(f"| all {len(names)} tests | C | [D3] (page format, splice and cursor of the private extent sequence). File content semantics: W `payload.rs` (4), W `captured_namespace_fragments.rs` (9), O `payload_fragmentation.rs`, O `payload_runs.rs` (4) |")
            check_refs(lines[-1], crate + " " + rel); ttally["C"] += len(names); continue
        if table is None:
            errors.append(f"{crate}: test file {rel} is not classified"); continue
        for name in names:
            if name not in table:
                errors.append(f"{crate}: test {rel}::{name} is not classified"); continue
            cls, where = table[name]
            where = where.replace("r5_1_...", "r5_1_external_bash_changes_of_every_kind_are_committed_and_read_back_on_a_fresh_mount")
            lines.append(f"| `{name}` | {cls} | {esc(where)} |")
            ttally[cls] += 1; check_refs(where, f"{crate} {rel}::{name}")
        for name in table:
            if name not in names:
                errors.append(f"{crate}: classified test {rel}::{name} does not exist")
    for rel in tests:
        if base + rel not in legacy:
            errors.append(f"{crate}: classified test file {rel} does not exist")
    assert sum(ttally.values()) == count or errors, (crate, ttally, count)
    lines += ["", "Tests by class: " + ", ".join(f"{k} {v}" for k, v in sorted(ttally.items())) + ".", "",
              "## Other tracked files", "", "| File | Class | Note |", "| --- | --- | --- |"]
    for rel, (cls, note) in other.items():
        lines.append(f"| `{rel}` | {cls} | {esc(note)} |"); check_refs(note, crate + " " + rel)
    covered = set(src_files) | {p for p in test_files if legacy[p]}
    for p in files:
        rel = p[len(base):]
        if p in covered or rel in other or rel in tests:
            continue
        if any(k.endswith("/") and rel.startswith(k) for k in other):
            continue
        if rel.endswith(".py") and any(k.startswith("tests/*.py") for k in other):
            continue
        errors.append(f"{crate}: tracked file {rel} is not listed")
    lines.append("")
    open(f"{OUT}/{number}-{crate}.md", "w").write("\n".join(lines))
    return tally, ttally, len(files)

KEEP = ("NOT REMOVED in this stage. It carries pinned read-only view code, which no active crate covers and whose "
        "future is the unanswered owner question O-10. Everything else in it is class A or C; the removal is prepared "
        "and needs only the owner's answer.")
LEGEND = ("Classes and the directions `[D1]`–`[D13]` are defined in [00-scope-and-method.md](00-scope-and-method.md). "
          "Test directories: D daemon, W workspace, O overlay, F fuse, B bridge, S sandbox, K SDK, P project, C content, PE persistence.")
summary = {}
summary["layerfs-server"] = crate_doc("10", "layerfs-server", "Coverage audit: `layerfs-server`",
    "REMOVE. Retired by owner direction [D1]; every behaviour is class A or C.",
    "The host Server: authorized Service over Content, Storage and History, plus the operator process. " + LEGEND,
    sm.SERVER_SRC, sm.SERVER_TESTS, sm.SERVER_OTHER)
summary["layerfs-fuse-legacy"] = crate_doc("11", "layerfs-fuse-legacy", "Coverage audit: `layerfs-fuse-legacy`",
    "REMOVE after the one class B item is migrated (execution from the mount, `layerfs-daemon/tests/mounted_execute.rs`).",
    "The previous FUSE projection (direct-I/O profile) over the previous Workspace. Every test is `#[ignore]`d in source. " + LEGEND,
    sm.FUSE_SRC, sm.FUSE_TESTS, sm.FUSE_OTHER)
summary["layerfs-sandbox-legacy"] = crate_doc("12", "layerfs-sandbox-legacy", "Coverage audit: `layerfs-sandbox-legacy`",
    "REMOVE. Every behaviour is class A or C.",
    "The previous host Docker owner: a registry of sandboxes with checked routes to the host Server. " + LEGEND,
    sm.SANDBOX_SRC, sm.SANDBOX_TESTS, sm.SANDBOX_OTHER)
summary["layerfs-sdk-legacy"] = crate_doc("13", "layerfs-sdk-legacy", "Coverage audit: `layerfs-sdk-legacy`", KEEP,
    "The previous agent SDK over one composed host Server and its Sandbox owner. " + LEGEND,
    lg.SDK_SRC, lg.SDK_TESTS, lg.SDK_OTHER)
summary["layerfs-daemon-legacy"] = crate_doc("14", "layerfs-daemon-legacy", "Coverage audit: `layerfs-daemon-legacy`", KEEP,
    "The previous daemon: native bridge, mount assembly, managed Exec and headless delivery. Its Rust tests are three; "
    "the rest of its verification was Docker route scripts. " + LEGEND,
    lg.DAEMON_SRC, lg.DAEMON_TESTS, lg.DAEMON_OTHER)
summary["layerfs-bridge-legacy"] = crate_doc("15", "layerfs-bridge-legacy", "Coverage audit: `layerfs-bridge-legacy`", KEEP,
    "The previous Bridge: logical operation contract, deadlines, prepared construction and the authenticated native channel. " + LEGEND,
    lg.BRIDGE_SRC, lg.BRIDGE_TESTS, lg.BRIDGE_OTHER)
summary["layerfs-workspace-legacy"] = crate_doc("16", "layerfs-workspace-legacy", "Coverage audit: `layerfs-workspace-legacy`", KEEP,
    "The previous Workspace over private page files. Source is classified by directory, with the files that differ listed singly. " + LEGEND,
    lg.WS_FILE, lg.WS_TESTS, lg.WS_OTHER, src_dirs=lg.WS_DIR)
if errors:
    print("\n".join(errors)); sys.exit(f"{len(errors)} errors")
for crate, (tally, ttally, files) in summary.items():
    print(crate, "files", files, "src", dict(tally), sum(tally.values()), "tests", dict(ttally), sum(ttally.values()))
