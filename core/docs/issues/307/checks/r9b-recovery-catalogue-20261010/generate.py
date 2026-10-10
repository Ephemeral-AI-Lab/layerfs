#!/usr/bin/env python3
"""Generate the appended recovery catalogue for the root reference tree.

Read-only against the repository: every byte is read from the object database
at one pinned revision through git plumbing (`ls-tree`, `cat-file --batch`,
`cat-file --batch-check`, `log`, `rev-list`, `rev-parse`, `diff-tree`). The
working tree and the index are never read, so uncommitted edits cannot change
the result. No historical receipt is opened for writing.

Outputs (written only into --output-dir):
  reference-recovery-catalogue.jsonl   header object, then one object per row
  missing-pins.txt                     distinct MISSING_OBJECT pin values
  README.md                            method, identities, counts, limits

The output is a pure function of (this script, the pinned revision, the local
object database, and the local remote-tracking/tag refs). Rows are sorted by
path; there are no timestamps and no absolute paths.
"""

import argparse
import collections
import hashlib
import json
import os
import posixpath
import re
import subprocess
import sys
import threading

RECOVERY_COMMIT = "f8a0a5ff1cd5f93b90bf16f964705b4c7d773615"
PUSHED_REF = "origin/main"
TAG = "v0.1.6"
EXCLUDED_TREES = ("crates/", "core/crates/", "core/vendor/", "core/reference-tests/")
OWN_DIR = "core/docs/issues/307/checks/r9b-recovery-catalogue-20261010/"
DOC_TREES = ("core/docs/", "docs/", "release-notes/")
PATH_CAP = 8
CATALOGUE = "reference-recovery-catalogue.jsonl"
MISSING = "missing-pins.txt"
README = "README.md"

# --- pin keys -------------------------------------------------------------
# Tier "lead_order": the preference order decided by the lead.
# Tier "tree_like": tree-like keys listed in audit A6 section C.2. The lead did
# not order them; the two that state the reference tree itself come first.
# Tier "audit_listed": other source-identity key listed in audit C.2.
# Tier "extended": keys the audit did not list. They are consulted only after
# every key above, so they never displace a listed key in the same file.
PIN_KEY_TIERS = (
    ("lead_order", ("layerfs_source_commit", "source_commit", "commit", "source_tip",
                    "head", "git_head", "revision")),
    ("tree_like", ("reference_tree", "crates", "layerfs_source_tree", "source_tree",
                   "staged_tree")),
    ("audit_listed", ("org.opencontainers.image.revision",)),
    ("extended", ("reference", "reference_source", "root_reference_tree", "tag_commit",
                  "source", "source_head", "measured_source_commit")),
)
# Keys that state the reference identity itself. Reported beside the pin so a
# disagreement with the selected pin is visible; they do not change selection.
EXPLICIT_REFERENCE_KEYS = ("reference", "reference_source", "reference_commit",
                           "reference_tree", "root_reference_tree", "tag_commit")
# Commit-like keys named by the audit that are deliberately NOT pins: they name
# a parent, a contract document, the reporting tool or a per-file digest.
NON_PIN_AUDIT_KEYS = ("source_parent", "contract_commit", "reporter_head", "source_sha",
                      "parent", "first_parent", "implementation_parent")

STRONG_SIGNALS = ("root_link", "root_only_package", "root_only_package_historical",
                  "root_path", "root_path_historical", "root_only_name",
                  "fs-benchmark-pro", "layerfs-eval", "reference-tests")
REVIEW_SIGNALS = ("ambiguous_shared_path", "unknown_package_path",
                  "historical_root_only_name", "v0.1.6_substantive", "reference_arm",
                  "reference_phrase", "path_v0.1.6")
SIGNAL_ORDER = STRONG_SIGNALS + REVIEW_SIGNALS

MENTION = re.compile(rb"crates/layerfs-([a-z0-9]+(?:-[a-z0-9]+)*)((?:/[A-Za-z0-9_.@+\-]+)*)")
PREFIX_BYTES = frozenset(
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.@~+-/")
NAME_BYTES = frozenset(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_")
PACKAGE_BYTES = NAME_BYTES | {0x2D}
HASH_SUFFIX = re.compile(rb"-[0-9a-f]{16}(?![0-9A-Za-z_])")
V016_DOTTED = re.compile(rb"(?<![A-Za-z0-9.])v0\.1\.6(?![0-9])")
V016_PLAIN = re.compile(rb"(?<![A-Za-z0-9])v016(?![A-Za-z0-9])")
REF_ARM = re.compile(rb"reference[ _-]arm", re.I)
REF_PHRASE = re.compile(
    rb"(?:root|legacy)[ _-]reference|reference[ _-](?:workspace|implementation|product|tree)",
    re.I)
EVAL = re.compile(rb"layerfs[-_]eval(?![A-Za-z0-9])")
REFTESTS = re.compile(
    rb"(?<![A-Za-z0-9\-])((?:core/)?reference-tests)(?![A-Za-z0-9])((?:/[A-Za-z0-9_.@+\-]+)*)")
HEX40 = re.compile(rb"(?<![0-9a-fA-F])[0-9a-f]{40}(?![0-9a-fA-F])")
KEY_BEFORE = re.compile(rb"([A-Za-z0-9_.\-/]+)[\"'`]?\s*[:=]\s*[\"'`]?$")
PATH_V016 = re.compile(r"(?<![A-Za-z0-9.])(?:v0\.1\.6(?![0-9])|v016(?![A-Za-z0-9]))")


def git(*args, stdin=None, check=True):
    proc = subprocess.run(("git",) + args, input=stdin, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE)
    if check and proc.returncode != 0:
        raise SystemExit("git %s failed: %s" % (" ".join(args), proc.stderr.decode()))
    return proc.stdout


def rev_parse(spec):
    out = git("rev-parse", "--verify", "--quiet", spec, check=False).decode().strip()
    return out or None


def batch_check(specs):
    """Return {spec: (object id, type)} for specs that resolve; others absent."""
    specs = sorted(set(specs))
    if not specs:
        return {}
    out = git("cat-file", "--batch-check", stdin=("\n".join(specs) + "\n").encode())
    result = {}
    for spec, line in zip(specs, out.decode().split("\n")):
        parts = line.split()
        if len(parts) == 3 and parts[1] in ("commit", "tree", "blob", "tag"):
            result[spec] = (parts[0], parts[1])
    return result


def with_dirs(paths):
    full = set()
    for path in paths:
        full.add(path)
        while "/" in path:
            path = path.rsplit("/", 1)[0]
            full.add(path)
    return full


def tree_paths(treeish):
    out = git("ls-tree", "-r", "-z", "--name-only", treeish)
    return [p.decode("utf-8", "surrogateescape") for p in out.split(b"\0") if p]


def history_adds(rev):
    """Earliest commit (topological order) that added each path, in one pass."""
    out = git("log", "--topo-order", "--reverse", "-m", "--no-renames", "--diff-filter=A",
              "--name-only", "-z", "--format=%x01%H", rev)
    introduced = {}
    for chunk in out.split(b"\x01")[1:]:
        fields = chunk.split(b"\0")
        commit = fields[0].strip().decode()
        for field in fields[1:]:
            field = field.lstrip(b"\n")
            if field:
                introduced.setdefault(field.decode("utf-8", "surrogateescape"), commit)
    return introduced


def package_names(rev, base):
    """Every `name = "layerfs-…"` ever added to a Cargo.toml under base."""
    out = git("log", "--format=", "-p", "--no-renames", rev, "--",
              ":(glob)%s/**/Cargo.toml" % base)
    names = set()
    for match in re.finditer(rb'^\+name\s*=\s*"layerfs[-_]([A-Za-z0-9_\-]+)"', out, re.M):
        names.add(match.group(1).decode().replace("_", "-"))
    return names


class Names:
    """Root and core path/package name sets, all derived from git."""

    def __init__(self, rev, recovery, introduced):
        hist_root = [p[len("crates/"):] for p in introduced if p.startswith("crates/")]
        hist_core = [p[len("core/crates/"):] for p in introduced
                     if p.startswith("core/crates/")]
        self.root_recovery = with_dirs(tree_paths(recovery + ":crates"))
        self.root_history = with_dirs(hist_root) | self.root_recovery
        self.core_any = with_dirs(hist_core) | with_dirs(tree_paths(rev + ":core/crates"))

        def packages(paths):
            return {p[len("layerfs-"):] for p in paths
                    if "/" not in p and p.startswith("layerfs-")}

        self.root_dirs_recovery = packages(self.root_recovery)
        self.root_dirs = packages(self.root_history)
        self.core_dirs = packages(self.core_any)
        core_names = self.core_dirs | package_names(rev, "core/crates")
        root_names = self.root_dirs | package_names(rev, "crates")
        self.bare_root_only = self.root_dirs_recovery - core_names
        self.bare_root_only_historical = (root_names - core_names) - self.bare_root_only
        alternatives = sorted(self.bare_root_only | self.bare_root_only_historical,
                              key=lambda n: (-len(n), n))
        self.bare_regex = re.compile(
            ("layerfs[-_](%s)" % "|".join(re.escape(n).replace(r"\-", "[-_]")
                                          for n in alternatives)).encode())


def classify_mention(names, file_dir, in_doc_tree, prefix, pkg, rest, is_link):
    """Return (signal, named_path). signal 'core' means not reference evidence."""
    if prefix and not prefix.endswith("/"):
        return None, None  # "xcrates/…": not the crates directory
    segments = prefix.split("/")[:-1] if prefix else []
    if segments and segments[-1] == "core":
        return "core", None
    rest = rest.rstrip(".-")
    tail = "crates/layerfs-" + pkg + rest
    relative = bool(segments) and all(s in (".", "..") for s in segments)
    if is_link and (relative or not segments) or relative and not in_doc_tree:
        resolved = posixpath.normpath(posixpath.join(file_dir, prefix + tail))
        if resolved.startswith("core/crates/"):
            return "core", None
        if resolved.startswith("crates/"):
            return "root_link", resolved
    in_root, in_core = pkg in names.root_dirs, pkg in names.core_dirs
    if in_root and not in_core:
        if pkg in names.root_dirs_recovery:
            return "root_only_package", tail
        return "root_only_package_historical", tail
    if in_core and not in_root:
        return "core", None
    if not in_root:
        return "unknown_package_path", None
    if rest:
        key = "layerfs-" + pkg + rest
        root_now, root_ever = key in names.root_recovery, key in names.root_history
        core_ever = key in names.core_any
        if root_ever and not core_ever:
            return ("root_path" if root_now else "root_path_historical"), tail
        if core_ever and not root_ever:
            return "core", None
    return "ambiguous_shared_path", None


def scan_signals(names, path, data, binary):
    """Return (signals, named_paths, has_core_mention, cargo_version_only)."""
    signals, named = set(), set()
    has_core = False
    file_dir = posixpath.dirname(path)
    in_doc_tree = path.startswith(DOC_TREES)
    if b"crates/layerfs-" in data:
        cache = {}
        for match in MENTION.finditer(data):
            start = match.start()
            low = max(0, start - 512)
            begin = start
            while begin > low and data[begin - 1] in PREFIX_BYTES:
                begin -= 1
            lead = data[max(0, begin - 6):begin]
            is_link = lead.endswith((b"](", b"]: ", b'href="'))
            key = (data[begin:start], match.group(1), match.group(2), is_link)
            if key not in cache:
                cache[key] = classify_mention(
                    names, file_dir, in_doc_tree, key[0].decode("latin-1"),
                    key[1].decode(), key[2].decode(), is_link)
        for signal, named_path in cache.values():
            if signal == "core":
                has_core = True
            elif signal:
                signals.add(signal)
                if named_path:
                    named.add(named_path)
    if b"layerfs" in data:
        for match in names.bare_regex.finditer(data):
            start, end = match.start(), match.end()
            if data[max(0, start - 7):start] == b"crates/":
                continue  # path form, classified above
            if end < len(data):
                nxt = data[end]
                if nxt in NAME_BYTES:
                    continue  # a longer identifier, e.g. layerfs-client
                if nxt == 0x2D and not HASH_SUFFIX.match(data, end):
                    if end + 1 < len(data) and data[end + 1] in NAME_BYTES:
                        continue  # a longer hyphenated name
            name = match.group(1).decode().replace("_", "-")
            if name in names.bare_root_only:
                signals.add("root_only_name")
            else:
                signals.add("historical_root_only_name")
        if EVAL.search(data):
            signals.add("layerfs-eval")
    if b"fs-benchmark-pro" in data or b"fs_benchmark_pro" in data:
        signals.add("fs-benchmark-pro")
    if b"reference-tests" in data:
        for match in REFTESTS.finditer(data):
            signals.add("reference-tests")
            named.add("core/reference-tests" + match.group(2).decode().rstrip(".-"))
    cargo_only = False
    if not binary:
        cargo_form = substantive = 0
        if b"v0.1.6" in data:
            for match in V016_DOTTED.finditer(data):
                start, end = match.start(), match.end()
                if (data[end:end + 2] == b" (" and start >= 2
                        and data[start - 1] == 0x20 and data[start - 2] in PACKAGE_BYTES):
                    cargo_form += 1
                else:
                    substantive += 1
        if b"v016" in data and V016_PLAIN.search(data):
            substantive += 1
        if substantive:
            signals.add("v0.1.6_substantive")
        cargo_only = bool(cargo_form) and not substantive
        if b"eference" in data or b"EFERENCE" in data:
            if REF_ARM.search(data):
                signals.add("reference_arm")
            if REF_PHRASE.search(data):
                signals.add("reference_phrase")
    return signals, named, has_core, cargo_only


def scan_pins(data, key_rank):
    """Return (pin candidates, unlisted keys, explicit reference pairs)."""
    candidates, unlisted, explicit = {}, set(), {}
    for match in HEX40.finditer(data):
        start = match.start()
        key_match = KEY_BEFORE.search(data, max(0, start - 96), start)
        if not key_match:
            continue
        raw = key_match.group(1)
        key_start = key_match.start(1)
        if raw[:1] == b"n" and data[key_start - 1:key_start] == b"\\":
            raw = raw[1:]  # an escaped newline in a quoted string, e.g. "\ngit_head="
        key = raw.decode("latin-1").lower().lstrip("-/.").replace("-", "_")
        if not key:
            continue
        value = match.group(0).decode()
        if key in key_rank:
            candidates.setdefault((key, value), start)
        else:
            unlisted.add(key)
        if key in EXPLICIT_REFERENCE_KEYS:
            explicit.setdefault((key, value), start)
    ordered = sorted((key_rank[k], off, k, v) for (k, v), off in candidates.items())
    explicit_pairs = [kv for kv, _ in sorted(explicit.items(), key=lambda i: i[1])]
    return ordered, unlisted, explicit_pairs


def top_directory(path):
    parts = path.split("/")[:-1]
    if not parts:
        return "(repository root)"
    if parts[:2] in (["docs", "roadmap"], ["core", "docs"]):
        depth = 4
    elif parts[0] in ("docs", "release-notes"):
        depth = 3
    else:
        depth = 2
    return "/".join(parts[:depth])


def table(headers, rows):
    lines = ["| " + " | ".join(headers) + " |", "| " + " | ".join("---" for _ in headers) + " |"]
    for row in rows:
        lines.append("| " + " | ".join(str(cell) for cell in row) + " |")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--rev", default="HEAD",
                        help="revision whose tracked files are catalogued (default HEAD)")
    parser.add_argument("--recovery-commit", default=RECOVERY_COMMIT)
    parser.add_argument("--exclude-pin-key", action="append", default=[],
                        help="drop one key from the pin tables (repeatable)")
    args = parser.parse_args()

    top = git("rev-parse", "--show-toplevel").decode().strip()
    os.chdir(top)
    rev = rev_parse(args.rev + "^{commit}")
    recovery = rev_parse(args.recovery_commit + "^{commit}")
    if not rev or not recovery:
        raise SystemExit("cannot resolve --rev or --recovery-commit")
    with open(os.path.abspath(__file__), "rb") as handle:
        script_sha256 = hashlib.sha256(handle.read()).hexdigest()

    key_rank, key_tier = {}, {}
    for tier, keys in PIN_KEY_TIERS:
        for key in keys:
            normalised = key.replace("-", "_")
            if key in args.exclude_pin_key or normalised in args.exclude_pin_key:
                continue
            key_rank[normalised] = len(key_rank)
            key_tier[normalised] = tier

    # --- identities ---------------------------------------------------------
    reference_tree = rev_parse(recovery + ":crates")
    reference_tests_tree = rev_parse(recovery + ":core/reference-tests")
    pushed_commit = rev_parse(PUSHED_REF + "^{commit}")
    pushed = None
    if pushed_commit:
        pushed = {
            "ref": PUSHED_REF,
            "commit": pushed_commit,
            "crates_tree": rev_parse(pushed_commit + ":crates"),
            "reference_tests_tree": rev_parse(pushed_commit + ":core/reference-tests"),
        }
        pushed["same_trees"] = (pushed["crates_tree"] == reference_tree
                                and pushed["reference_tests_tree"] == reference_tests_tree)
    tag_commit = rev_parse(TAG + "^{commit}")
    tag = None
    if tag_commit:
        tag_tree = rev_parse(tag_commit + ":crates")
        status = collections.Counter()
        changed = []
        for line in git("diff-tree", "-r", "--no-renames", "--name-status", tag_tree,
                        reference_tree).decode().splitlines():
            code, name = line.split("\t", 1)
            status[code] += 1
            changed.append(code + " crates/" + name)
        tag = {"commit": tag_commit, "crates_tree": tag_tree,
               "differs_from_reference_tree_by": dict(sorted(status.items())),
               "changed_paths": changed}

    remote_set = set(git("rev-list", "--remotes", "--tags").decode().split())
    ancestry = git("rev-list", "--topo-order", "--reverse", rev).decode().split()
    ancestry_set = set(ancestry)
    known = ancestry + sorted(remote_set - ancestry_set)
    order = {commit: index for index, commit in enumerate(known)}
    crates_of = batch_check(c + ":crates" for c in known)
    carriers = {}
    for commit in known:
        entry = crates_of.get(commit + ":crates")
        if entry:
            rank = (commit not in remote_set, order[commit])
            if entry[0] not in carriers or rank < carriers[entry[0]][0]:
                carriers[entry[0]] = (rank, commit)

    def reachability(commit):
        if commit in remote_set:
            return "remote_tracking_or_tag"
        if commit in ancestry_set:
            return "catalogue_commit_ancestry_only"
        return "other_local_object"

    introduced = history_adds(rev)
    names = Names(rev, recovery, introduced)

    # --- one streaming pass over every in-scope blob ----------------------
    entries = []
    for record in git("ls-tree", "-r", "-z", rev).split(b"\0"):
        if not record:
            continue
        meta, raw_path = record.split(b"\t", 1)
        mode, kind, blob = meta.split()
        path = raw_path.decode("utf-8", "surrogateescape")
        if kind != b"blob" or path.startswith(EXCLUDED_TREES) or path.startswith(OWN_DIR):
            continue
        entries.append((path, blob.decode()))
    entries.sort()

    reader = subprocess.Popen(("git", "cat-file", "--batch"), stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE)

    def feed():
        for _, blob in entries:
            reader.stdin.write(blob.encode() + b"\n")
        reader.stdin.close()

    threading.Thread(target=feed, daemon=True).start()

    scanned = {}       # path -> (blob, signals, named paths, binary)
    pins = {}          # path -> ordered candidates
    extras = {}        # path -> (unlisted keys, explicit pairs)
    excluded = collections.Counter()
    text_count = binary_count = 0
    for path, blob in entries:
        header = reader.stdout.readline().split()
        if len(header) != 3 or header[0].decode() != blob:
            raise SystemExit("cat-file --batch desynchronised at " + path)
        data = reader.stdout.read(int(header[2]))
        reader.stdout.read(1)
        binary = b"\0" in data[:8000]
        if binary:
            binary_count += 1
        else:
            text_count += 1
        signals, named, has_core, cargo_only = scan_signals(names, path, data, binary)
        if PATH_V016.search(path):
            signals.add("path_v0.1.6")
        if not binary:
            ordered, unlisted, explicit = scan_pins(data, key_rank)
            if ordered:
                pins[path] = ordered
            if unlisted or explicit:
                extras[path] = (unlisted, explicit)
        if signals:
            scanned[path] = (blob, signals, named, binary)
        elif has_core and cargo_only:
            excluded["core_only_mentions_and_cargo_version_only"] += 1
        elif has_core:
            excluded["core_only_mentions"] += 1
        elif cargo_only:
            excluded["cargo_version_only"] += 1
    if reader.wait() != 0:
        raise SystemExit("git cat-file --batch failed")

    # --- resolve every candidate value once --------------------------------
    values = {v for ordered in pins.values() for _, _, _, v in ordered}
    values |= {v for _, explicit in extras.values() for _, v in explicit}
    objects = batch_check(values)
    crates_at = batch_check(v + ":crates" for v in objects if objects[v][1] != "blob")
    crates_tree_ids = set(carriers)

    def resolve(value):
        """Return (status, kind, reference tree) for one recorded value."""
        entry = objects.get(value)
        if not entry:
            return "MISSING_OBJECT", "unknown", None
        if entry[1] == "blob":
            return None, "blob", None
        sub = crates_at.get(value + ":crates")
        if entry[1] in ("commit", "tag"):
            return "LOCAL_COMMIT", entry[1], sub[0] if sub and sub[1] == "tree" else None
        if value in crates_tree_ids:
            return "LOCAL_TREE", "tree", value  # the value is itself a crates tree
        return "LOCAL_TREE", "tree", sub[0] if sub and sub[1] == "tree" else None

    usable = {}
    for path, ordered in pins.items():
        kept = [c for c in ordered if resolve(c[3])[0] is not None]
        if kept:
            usable[path] = kept
    by_dir = collections.defaultdict(list)
    for path in sorted(usable):
        by_dir[posixpath.dirname(path)].append(path)

    def find_pin(path):
        if path in usable:
            return usable[path][0], path, "self"
        directory = posixpath.dirname(path)
        for association in ("sibling", "ancestor_1", "ancestor_2"):
            best = None
            for other in by_dir.get(directory, ()):
                if other == path:
                    continue
                rank, offset, key, value = usable[other][0]
                choice = (rank, other, offset, key, value)
                if best is None or choice < best:
                    best = choice
            if best:
                return (best[0], best[2], best[3], best[4]), best[1], association
            if not directory:
                break
            directory = posixpath.dirname(directory)
        return None, None, "none"

    # --- rows ---------------------------------------------------------------
    rows = []
    for path in sorted(scanned):
        blob, signals, named, binary = scanned[path]
        ordered_signals = [s for s in SIGNAL_ORDER if s in signals]
        strong = any(s in STRONG_SIGNALS for s in ordered_signals)
        named_sorted = sorted(named)
        pin, found_path, association = find_pin(path)
        pin_object = None
        status, tree_at_pin, matches, reach = "NOT_RECORDED", None, None, None
        candidate_values, candidate_missing, candidate_trees = [], 0, []
        if pin:
            _, _, key, value = pin
            status, kind, tree_at_pin = resolve(value)
            pin_object = {
                "value": value, "key": key, "kind": "commit" if kind == "tag" else kind,
                "found_in": "self" if association == "self" else found_path,
                "association": association, "key_tier": key_tier[key],
            }
            if kind == "tag":
                pin_object["object_type"] = "tag"
            if status == "LOCAL_COMMIT":
                peeled = rev_parse(value + "^{commit}") if kind == "tag" else value
                reach = reachability(peeled)
            if tree_at_pin:
                matches = tree_at_pin == reference_tree
            candidate_values = sorted({c[3] for c in usable[found_path]})
            resolved = [resolve(v) for v in candidate_values]
            candidate_missing = sum(1 for r in resolved if r[0] == "MISSING_OBJECT")
            candidate_trees = sorted({r[2] for r in resolved if r[2]})

        if status == "NOT_RECORDED":
            basis, recover_at, as_executed = (
                "recovery_commit_current_tree_not_execution_pin", recovery, False)
        elif status == "MISSING_OBJECT":
            basis, recover_at, as_executed = "unrecoverable_missing_pin", None, False
        elif matches:
            basis, recover_at, as_executed = "recovery_commit_same_tree", recovery, True
        elif tree_at_pin:
            basis, recover_at, as_executed = "original_pin", pin_object["value"], True
        else:
            # The pin resolves locally but carries no root `crates` tree.
            basis, recover_at, as_executed = (
                "recovery_commit_current_tree_not_execution_pin", recovery, False)

        # "As executed" is claimed only for a pin the row's own file records.
        by_association = as_executed and association != "self"
        as_executed = as_executed and association == "self"

        carrier = None
        if tree_at_pin and not matches and tree_at_pin in carriers:
            commit = carriers[tree_at_pin][1]
            carrier = {"commit": commit, "reachability": reachability(commit)}

        unlisted, explicit = extras.get(path, (set(), []))
        explicit_rows = []
        for key, value in explicit[:4]:
            e_status, _, e_tree = resolve(value)
            explicit_rows.append({"key": key, "value": value,
                                  "status": e_status or "LOCAL_BLOB",
                                  "reference_tree": e_tree})
        agrees = None
        explicit_trees = {e["reference_tree"] for e in explicit_rows if e["reference_tree"]}
        if explicit_trees and tree_at_pin:
            agrees = explicit_trees == {tree_at_pin}
        unlisted_sorted = sorted(unlisted)

        rows.append(collections.OrderedDict((
            ("evidence_path", path),
            ("evidence_blob", blob),
            ("class", "STRONG" if strong else "REVIEW"),
            ("signals", ordered_signals),
            ("binary_blob", binary),
            ("named_reference_paths", named_sorted[:PATH_CAP]),
            ("named_reference_path_count", len(named_sorted)),
            ("original_pin", pin_object),
            ("original_pin_status", status),
            ("original_pin_reachability", reach),
            ("candidate_pin_count", len(candidate_values)),
            ("candidate_pin_missing_count", candidate_missing),
            ("candidate_reference_trees", candidate_trees[:PATH_CAP]),
            ("candidate_reference_tree_count", len(candidate_trees)),
            ("reference_tree_at_pin", tree_at_pin),
            ("matches_recovery_tree", matches),
            ("reference_tree_carrier", carrier),
            ("explicit_reference_pins", explicit_rows),
            ("explicit_reference_pin_count", len(explicit)),
            ("explicit_reference_agrees_with_pin", agrees),
            ("unlisted_hex40_keys", unlisted_sorted[:PATH_CAP]),
            ("unlisted_hex40_key_count", len(unlisted_sorted)),
            ("introduced_commit", introduced.get(path)),
            ("recover_reference_at", recover_at),
            ("recovery_basis", basis),
            ("recoverable_as_executed", as_executed),
            ("recoverable_by_association", by_association),
            ("recover_reference_tests_at",
             recovery if "reference-tests" in signals else None),
            ("original_verdict_touched", False),
        )))

    # --- summaries ----------------------------------------------------------
    def count(field, subset=None):
        counter = collections.Counter()
        for row in rows if subset is None else subset:
            value = field(row)
            counter[value if value is not None else "null"] += 1
        return counter

    by_class = count(lambda r: r["class"])
    by_status = count(lambda r: r["original_pin_status"])
    by_basis = count(lambda r: r["recovery_basis"])
    missing_rows = [r for r in rows if r["original_pin_status"] == "MISSING_OBJECT"]
    missing = collections.defaultdict(lambda: {"keys": set(), "STRONG": 0, "REVIEW": 0,
                                               "self": 0, "example": None})
    for row in missing_rows:
        entry = missing[row["original_pin"]["value"]]
        entry["keys"].add(row["original_pin"]["key"])
        entry[row["class"]] += 1
        entry["self"] += row["original_pin"]["association"] == "self"
        entry["example"] = entry["example"] or row["evidence_path"]
    other_trees = collections.defaultdict(lambda: {"STRONG": 0, "REVIEW": 0, "carrier": None})
    for row in rows:
        if row["matches_recovery_tree"] is False:
            entry = other_trees[row["reference_tree_at_pin"]]
            entry[row["class"]] += 1
            entry["carrier"] = row["reference_tree_carrier"]

    header = collections.OrderedDict((
        ("record_type", "header"),
        ("status", "Appended recovery catalogue; not release evidence."),
        ("catalogue_source_commit", rev),
        ("script_sha256", script_sha256),
        ("recovery_commit", recovery),
        ("recovery_commit_reachability", reachability(recovery)),
        ("reference_tree", reference_tree),
        ("reference_tests_tree", reference_tests_tree),
        ("pushed_equivalent", pushed),
        ("v0.1.6_tag", tag),
        ("excluded_trees", list(EXCLUDED_TREES) + [OWN_DIR]),
        ("pin_key_tiers", [[tier, [k for k in keys if k.replace("-", "_") in key_rank]]
                           for tier, keys in PIN_KEY_TIERS]),
        ("excluded_pin_keys", sorted(args.exclude_pin_key)),
        ("row_count", len(rows)),
        ("rows_by_class", dict(sorted(by_class.items()))),
        ("rows_by_original_pin_status", dict(sorted(by_status.items()))),
        ("rows_by_recovery_basis", dict(sorted(by_basis.items()))),
        ("distinct_missing_pin_values", len(missing)),
        ("original_verdict_touched", False),
    ))

    os.makedirs(args.output_dir, exist_ok=True)

    def write(name, text):
        with open(os.path.join(args.output_dir, name), "w", encoding="utf-8",
                  newline="\n") as handle:
            handle.write(text)

    dump = lambda obj: json.dumps(obj, ensure_ascii=True, separators=(",", ":"))
    catalogue_text = "".join(dump(obj) + "\n" for obj in [header] + rows)
    write(CATALOGUE, catalogue_text)

    missing_lines = ["# value\tkeys\trows\tSTRONG\tREVIEW\trows_where_pin_is_in_the_file_itself"
                     "\texample_evidence_path"]
    for value in sorted(missing):
        entry = missing[value]
        missing_lines.append("\t".join((
            value, ",".join(sorted(entry["keys"])), str(entry["STRONG"] + entry["REVIEW"]),
            str(entry["STRONG"]), str(entry["REVIEW"]), str(entry["self"]),
            entry["example"])))
    missing_text = "\n".join(missing_lines) + "\n"
    write(MISSING, missing_text)

    # --- README -------------------------------------------------------------
    def class_table(field, title, subset=None, limit=None):
        counter = collections.defaultdict(collections.Counter)
        for row in rows if subset is None else subset:
            value = field(row)
            counter[value if value is not None else "null"][row["class"]] += 1
        items = sorted(counter.items(), key=lambda i: (-sum(i[1].values()), str(i[0])))
        shown = items if limit is None else items[:limit]
        body = [(("`%s`" % k), v["STRONG"], v["REVIEW"], sum(v.values())) for k, v in shown]
        if limit is not None and len(items) > limit:
            rest = items[limit:]
            body.append(("(%d further values)" % len(rest),
                         sum(v["STRONG"] for _, v in rest),
                         sum(v["REVIEW"] for _, v in rest),
                         sum(sum(v.values()) for _, v in rest)))
        return table((title, "STRONG", "REVIEW", "rows"), body)

    signal_rows = []
    for signal in SIGNAL_ORDER:
        having = [r for r in rows if signal in r["signals"]]
        sole = [r for r in having if r["signals"] == [signal]]
        signal_rows.append(("`%s`" % signal,
                            "STRONG" if signal in STRONG_SIGNALS else "REVIEW",
                            len(having), len(sole)))
    pinned = [r for r in rows if r["original_pin"]]
    executed = [r for r in rows if r["recoverable_as_executed"]]
    associated = [r for r in rows if r["recoverable_by_association"]]
    missing_with_tree = [r for r in missing_rows if r["candidate_reference_trees"]]
    disagree = [r for r in rows if r["explicit_reference_agrees_with_pin"] is False]
    no_crates = [r for r in rows if r["original_pin_status"] in ("LOCAL_COMMIT", "LOCAL_TREE")
                 and not r["reference_tree_at_pin"]]
    multi_tree = [r for r in rows if r["candidate_reference_tree_count"] > 1]
    not_recorded = [r for r in rows if r["original_pin_status"] == "NOT_RECORDED"]
    nr_unlisted = [r for r in not_recorded if r["unlisted_hex40_key_count"]]
    binary_rows = [r for r in rows if r["binary_blob"]]
    no_intro = [r for r in rows if not r["introduced_commit"]]
    tree_rows = []
    for tree in sorted(other_trees, key=lambda t: (-(other_trees[t]["STRONG"]
                                                     + other_trees[t]["REVIEW"]), t)):
        entry = other_trees[tree]
        carrier = entry["carrier"]
        tree_rows.append(("`%s`" % tree, entry["STRONG"], entry["REVIEW"],
                          entry["STRONG"] + entry["REVIEW"],
                          "`%s`" % carrier["commit"] if carrier else "none found",
                          carrier["reachability"] if carrier else "-",
                          "yes" if tag and tree == tag["crates_tree"] else ""))
    missing_table = []
    for value in sorted(missing, key=lambda v: (-(missing[v]["STRONG"] + missing[v]["REVIEW"]),
                                                v)):
        entry = missing[value]
        missing_table.append(("`%s`" % value, ", ".join("`%s`" % k for k in sorted(entry["keys"])),
                              entry["STRONG"], entry["REVIEW"],
                              entry["STRONG"] + entry["REVIEW"]))

    sha = lambda text: hashlib.sha256(text.encode("utf-8")).hexdigest()
    lines = []
    add = lines.append
    add("# Root reference recovery catalogue (R9b)")
    add("")
    add("> **Status:** Appended recovery catalogue; not release evidence.")
    add("")
    add("This directory is appended beside the historical receipts. It records, for every")
    add("tracked file outside the excluded trees that names the root reference (`crates/`")
    add("at the repository root, and `core/reference-tests`), where that reference can be")
    add("recovered after the trees are retired. It deletes nothing, rewrites no receipt and")
    add("changes no verdict; `original_verdict_touched` is `false` in every row. The scope,")
    add("the two classes, the pin search order and the four recovery bases are lead decisions")
    add("recorded for owner review, not owner rulings. \"Choices made by the generator\" lists")
    add("what no lead decision covers.")
    add("")
    add("## Files")
    add("")
    add(table(("File", "Content", "SHA-256"), (
        ("`generate.py`", "generator (this README is its output)", "`%s`" % script_sha256),
        ("`%s`" % CATALOGUE, "header line + %d rows" % len(rows),
         "`%s`" % sha(catalogue_text)),
        ("`%s`" % MISSING, "%d distinct `MISSING_OBJECT` pin values" % len(missing),
         "`%s`" % sha(missing_text)),
        ("`selfcheck.py`, `selfcheck.txt`", "reproducibility and blob-identity check and its "
         "output", "not embedded here; see the hand-back"),
    )))
    add("")
    add("## Header")
    add("")
    header_rows = [
        ("catalogue source commit (every blob is read at this commit)", "`%s`" % rev),
        ("recovery commit", "`%s` (%s)" % (recovery, reachability(recovery))),
        ("reference tree (`<recovery>:crates`)", "`%s`" % reference_tree),
        ("reference-tests tree (`<recovery>:core/reference-tests`)",
         "`%s`" % reference_tests_tree),
    ]
    if pushed:
        header_rows.append(("pushed equivalent `%s`" % PUSHED_REF,
                            "`%s`; `crates` `%s`; `core/reference-tests` `%s`; same trees: **%s**"
                            % (pushed["commit"], pushed["crates_tree"],
                               pushed["reference_tests_tree"],
                               "yes" if pushed["same_trees"] else "NO")))
    else:
        header_rows.append(("pushed equivalent `%s`" % PUSHED_REF, "ref not present locally"))
    if tag:
        header_rows.append(("tag `%s`" % TAG, "commit `%s`; `crates` `%s`; reference tree differs "
                            "from it by %s" % (tag["commit"], tag["crates_tree"], ", ".join(
                                "%d %s" % (n, c) for c, n in
                                sorted(tag["differs_from_reference_tree_by"].items())) or "nothing")))
    add(table(("Item", "Value"), header_rows))
    add("")
    add("`%s` is a local remote-tracking ref; this script performs no network access and" % PUSHED_REF)
    add("does not show that the remote still holds that commit. Reachability labels in rows")
    add("(`remote_tracking_or_tag`, `catalogue_commit_ancestry_only`, `other_local_object`)")
    add("describe the local ref state at generation time in the same sense.")
    if tag and tag["changed_paths"]:
        add("")
        add("Paths by which the reference tree differs from tag `%s`:" % TAG)
        add("")
        for changed in tag["changed_paths"]:
            add("- `%s`" % changed)
    add("")
    add("## Method")
    add("")
    add("`python3 generate.py --output-dir <dir>` (optional `--rev`, `--recovery-commit`,")
    add("`--exclude-pin-key`). It runs git plumbing and text processing only: `ls-tree`,")
    add("one `cat-file --batch` stream over every in-scope blob, `cat-file --batch-check`,")
    add("one `log --diff-filter=A` pass, `rev-list`, `rev-parse`, `diff-tree`. It does not read")
    add("the working tree or the index, so uncommitted edits cannot change the result.")
    add("")
    add("1. **Scope.** Every blob tracked at the catalogue source commit outside `%s` and"
        % "`, `".join(EXCLUDED_TREES))
    add("   outside this directory: %d blobs (%d text, %d binary; binary means a NUL byte"
        % (len(entries), text_count, binary_count))
    add("   in the first 8,000 bytes).")
    add("2. **Name sets, from git only.** Root package directories ever present under")
    add("   `crates/` (%d) and core package directories ever present under `core/crates/`"
        % len(names.root_dirs))
    add("   (%d); every path ever added under either tree; the recovery tree's paths." %
        len(names.core_dirs))
    add("   Root-only bare names in the recovery tree: %s. Historical root-only bare names:"
        % ", ".join("`layerfs-%s`" % n for n in sorted(names.bare_root_only)))
    add("   %s." % ", ".join("`layerfs-%s`" % n for n in sorted(names.bare_root_only_historical)))
    add("3. **Signals.** Each `crates/layerfs-<pkg>[/path]` mention is resolved: a `core/`")
    add("   prefix is core; a Markdown link (and, outside `core/docs`, `docs`,")
    add("   `release-notes`, a `../` path) is resolved against the file's directory; otherwise")
    add("   a root-only directory name is the reference, a core-only one is core, and a")
    add("   shared name (%s) is decided by whether the named path ever existed"
        % ", ".join("`%s`" % n for n in sorted(names.root_dirs & names.core_dirs)))
    add("   under only one of the two trees. A path that existed under both or neither is")
    add("   `ambiguous_shared_path`. `v0.1.6` immediately between a package name and")
    add("   ` (path)` (cargo's `name vX (path)` form) is not a signal. Binary blobs are")
    add("   searched for path and name signals only.")
    add("4. **Class.** `STRONG` if any of: %s. Otherwise `REVIEW` if any of: %s."
        % (", ".join("`%s`" % s for s in STRONG_SIGNALS),
           ", ".join("`%s`" % s for s in REVIEW_SIGNALS)))
    add("   Files with neither are not rows.")
    add("5. **Pin.** A pin is a 40-hex value directly after a recognised key and `:`/`=`")
    add("   (quotes or backticks allowed). 40-hex prefixes of longer hex strings are ignored,")
    add("   as are prose and table mentions with no key, and short hashes. Search order: the")
    add("   file itself, then files directly in the same directory, then files directly in")
    add("   the parent and grandparent directories; `original_pin.found_in` and")
    add("   `original_pin.association` record which. Within a file the first key in this")
    add("   order wins, then the earliest occurrence:")
    for tier, keys in header["pin_key_tiers"]:
        add("   - `%s`: %s" % (tier, ", ".join("`%s`" % k for k in keys) or "(none)"))
    add("   Among neighbouring files the best key rank wins, then the path order. A value")
    add("   that resolves to a blob is not a pin. Deliberately not pins: %s."
        % ", ".join("`%s`" % k for k in NON_PIN_AUDIT_KEYS))
    add("6. **Resolution.** `git cat-file --batch-check` gives the object type; the reference")
    add("   tree at a pin is `<pin>:crates`, or the pin itself when it is a tree that some")
    add("   known commit carries as `crates`. Nothing is inferred for a value with no local")
    add("   object: the row is `MISSING_OBJECT`, `recover_reference_at` is null and")
    add("   `recoverable_as_executed` is false. `recoverable_as_executed` is true only for a")
    add("   pin found in the row's own file; a resolving pin found in a neighbouring file sets")
    add("   `recoverable_by_association` instead.")
    add("7. **Recovery basis.** `recovery_commit_same_tree`: the pin's reference tree equals")
    add("   the recovery tree. `original_pin`: it resolves to a different reference tree, so")
    add("   the row points at its own pin. `recovery_commit_current_tree_not_execution_pin`:")
    add("   no pin was recorded, or the pin resolves but carries no root `crates` tree; the")
    add("   recovery commit is the final reference tree, not the tree that was executed.")
    add("   `unrecoverable_missing_pin`: the recorded pin is not in this clone.")
    add("8. **`introduced_commit`.** The earliest commit in topological order that added the")
    add("   path (`git log --topo-order --reverse -m --no-renames --diff-filter=A`). It is")
    add("   an upper bound on when the file was produced, never an execution pin.")
    add("")
    add("### Choices made by the generator")
    add("")
    add("These go beyond the lead decisions and are open to review like the rest.")
    add("")
    add("- Shared package names are disambiguated against every path that ever existed under")
    add("  either tree, not only the paths present now, so a core path that was later renamed")
    add("  counts as core and is not a row.")
    add("- `path_v0.1.6` makes a file a `REVIEW` row when only its own path carries `v0.1.6`")
    add("  or `v016`; this is the only way a binary blob with no path string becomes a row.")
    add("- Within the tree-like keys, `reference_tree` and `crates` come first because they")
    add("  state the reference tree itself. A key spelled `core/crates` is not `crates`.")
    add("- The `extended` tier is not in the audit's key list. It is used only when a file")
    add("  carries no listed key; `original_pin.key_tier` marks every such row.")
    add("- Parent, contract, reporter and digest keys are not pins (step 5).")
    add("- `recoverable_as_executed` is withheld from pins found in a neighbouring file;")
    add("  those rows carry `recoverable_by_association` instead.")
    add("- `fs-benchmark-pro` and `layerfs-eval` give `STRONG`, following the audit.")
    add("")
    add("## Counts")
    add("")
    add("Rows: **%d** (`STRONG` %d, `REVIEW` %d). Rows that are binary blobs: %d."
        % (len(rows), by_class["STRONG"], by_class["REVIEW"], len(binary_rows)))
    add("")
    add("### By signal")
    add("")
    add(table(("Signal", "Class it gives", "Rows carrying it", "Rows where it is the only signal"),
              signal_rows))
    add("")
    add("### By top directory")
    add("")
    add(class_table(lambda r: top_directory(r["evidence_path"]), "Directory", limit=40))
    add("")
    add("### By `original_pin_status`")
    add("")
    add(class_table(lambda r: r["original_pin_status"], "Status"))
    add("")
    add("### By `recovery_basis`")
    add("")
    add(class_table(lambda r: r["recovery_basis"], "Basis"))
    add("")
    add("### By recoverability")
    add("")
    add("`recoverable_as_executed` is true only when the row's own file records a pin that")
    add("resolves to a reference tree in this clone. `recoverable_by_association` is true")
    add("when such a pin was found only in a neighbouring file: that is an association by")
    add("directory, not a statement made by the row's own file, so it is counted apart.")
    add("")
    add(class_table(lambda r: ("as_executed (own pin)" if r["recoverable_as_executed"] else
                               "by_association (neighbouring pin)"
                               if r["recoverable_by_association"] else "neither"), "Value"))
    add("")
    add("### Where the pin was found, and under which key")
    add("")
    add(class_table(lambda r: r["original_pin"]["association"] if r["original_pin"] else "none",
                    "Association"))
    add("")
    add(class_table(lambda r: r["original_pin"]["key"], "Pin key", subset=pinned))
    add("")
    add(class_table(lambda r: r["original_pin"]["key_tier"], "Key tier", subset=pinned))
    add("")
    add(class_table(lambda r: r["original_pin_reachability"], "Reachability of a `LOCAL_COMMIT` pin",
                    subset=[r for r in rows if r["original_pin_status"] == "LOCAL_COMMIT"]))
    add("")
    add("### `MISSING_OBJECT`")
    add("")
    add("%d rows depend on %d distinct pin values that resolve to no object in this clone"
        % (len(missing_rows), len(missing)))
    add("(`%s` lists each with its keys, row counts and one example path)." % MISSING)
    add("%d of those rows also carry, in the file where the pin was found, another recorded"
        % len(missing_with_tree))
    add("identity that does resolve to a reference tree (`candidate_reference_trees`); the")
    add("catalogue reports it but does not substitute it for the missing pin.")
    add("")
    add(table(("Missing pin value", "Keys", "STRONG", "REVIEW", "rows"), missing_table)
        if missing_table else "None.")
    add("")
    add("### `NOT_RECORDED`")
    add("")
    add("%d rows have no recognised pin in the file, its directory, or two levels up"
        % len(not_recorded))
    add("(`STRONG` %d, `REVIEW` %d). %d of them carry a 40-hex value under a key this"
        % (sum(1 for r in not_recorded if r["class"] == "STRONG"),
           sum(1 for r in not_recorded if r["class"] == "REVIEW"), len(nr_unlisted)))
    add("script does not treat as a pin (`unlisted_hex40_keys`).")
    add("")
    add(class_table(lambda r: top_directory(r["evidence_path"]), "Directory",
                    subset=not_recorded, limit=25))
    add("")
    add("### Reference trees other than the recovery tree")
    add("")
    add("%d rows resolve to %d distinct reference trees that differ from `%s`. For these"
        % (sum(v["STRONG"] + v["REVIEW"] for v in other_trees.values()), len(other_trees),
           reference_tree))
    add("rows the recovery commit does not hold the reference as executed;")
    add("`recover_reference_at` is the row's own pin. The carrier is one known commit whose")
    add("`crates` tree is that tree (a remote-tracking or tagged one when there is one).")
    add("")
    add(table(("Reference tree", "STRONG", "REVIEW", "rows", "Carrier commit",
               "Carrier reachability", "Is tag `%s`" % TAG), tree_rows) if tree_rows else "None.")
    add("")
    add("### Other counts")
    add("")
    add(table(("Count", "Rows"), (
        ("pin resolves locally but has no root `crates` tree (basis falls back to the "
         "recovery commit, `recoverable_as_executed` false)", len(no_crates)),
        ("file where the pin was found records identities resolving to more than one "
         "reference tree (`candidate_reference_tree_count` > 1)", len(multi_tree)),
        ("an explicit reference key in the row's own file resolves to a reference tree "
         "different from the selected pin's (`explicit_reference_agrees_with_pin` false)",
         len(disagree)),
        ("rows naming `core/reference-tests` (`recover_reference_tests_at` set)",
         sum(1 for r in rows if r["recover_reference_tests_at"])),
        ("rows with no `introduced_commit`", len(no_intro)),
    )))
    add("")
    add("### Files that are not rows")
    add("")
    add(table(("Excluded class", "Files"), (
        ("names `crates/layerfs-…` only as core paths, no other signal",
         excluded["core_only_mentions"]),
        ("`v0.1.6` only as a cargo package version, no other signal",
         excluded["cargo_version_only"]),
        ("both of the above, no other signal",
         excluded["core_only_mentions_and_cargo_version_only"]),
    )))
    add("")
    add("## What this catalogue does not establish")
    add("")
    add("- **`REVIEW` rows are unreviewed.** They carry only a weak signal (a shared-name")
    add("  path, a `v0.1.6`/`v016` mention, a reference phrase, or a path component). Nobody")
    add("  has decided whether each names the root reference.")
    add("- **`STRONG` is a text rule, not a reading.** It means a root-only name or path")
    add("  occurs in the file. It does not show the file is a receipt, or that the reference")
    add("  was executed to produce it. `fs-benchmark-pro` and `layerfs-eval` are the root")
    add("  benchmark and evaluation packages that build against the reference; a file naming")
    add("  only them is `STRONG` by this rule.")
    add("- **Missing pins cannot be shown equal to the recovery tree.** For a")
    add("  `MISSING_OBJECT` row nothing in this clone shows which reference tree was executed.")
    add("- **A 40-hex token under a commit-like key may not be a commit.** A missing value")
    add("  may be a commit never fetched here or an identifier of something else, such as a")
    add("  workload corpus revision. This script does not classify missing values.")
    add("- **A neighbouring pin is an association.** When `original_pin.found_in` is another")
    add("  file, the row's own file did not state that pin. In a large shared directory the")
    add("  neighbour is chosen by key rank and then path order, and may concern other work.")
    add("  %d rows rest on such a pin (`recoverable_by_association`)." % len(associated))
    add("- **A selected pin is one of possibly several.** `candidate_pin_count` and")
    add("  `candidate_reference_trees` show the others recorded in the same file; the script")
    add("  does not decide which a multi-arm receipt means. Pins written in prose or tables")
    add("  without a key, and short hashes, are not read at all.")
    add("- **`NOT_RECORDED` rows point at the final reference tree, not the executed one.**")
    add("- **`introduced_commit` is an upper bound**, and is the earliest add of that path")
    add("  even if the path was later removed and added again.")
    add("- **Recoverability is local.** A `LOCAL_COMMIT` or `LOCAL_TREE` exists in this")
    add("  clone's object database; rows whose reachability is not `remote_tracking_or_tag`")
    add("  depend on local history, and tree pins may be unreferenced objects.")
    add("- **No build, test or measurement ran.** Nothing here shows that a recovered tree")
    add("  builds or reproduces any recorded number.")
    add("")
    write(README, "\n".join(lines))

    sys.stderr.write("rows %d (STRONG %d, REVIEW %d); missing pin values %d\n"
                     % (len(rows), by_class["STRONG"], by_class["REVIEW"], len(missing)))


if __name__ == "__main__":
    main()
