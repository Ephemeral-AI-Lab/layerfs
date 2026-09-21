#!/usr/bin/env python3
"""Squad E -- INDEPENDENT RE-DERIVATION of the #219 campaign's headline arithmetic.

Reads ONLY raw files:
  * the two v0.1.7 receipts + run.json + timing.json/phases-perf.json,
  * counters.tsv / counters-packcounters.tsv / shares.tsv / work-identity-check.txt,
  * cadence-calibration.json, journal-calibration.json, cache-calibration.json,
    blob-calibration.json, packs / packcounters raw files,
  * the pinned row's own sample.sqlite (pack directories re-parsed from scratch),
  * Squad B's ranked_table.tsv, Squad D's raw/, the v0.1.6 perf.jsonl,
  * the harness and workload Rust sources, the campaign handoff and the
    stage-6-history-209 prior-art README.
Report prose is NEVER used as evidence: every reported number is recomputed from
raw and compared with what the report claims.

Exit code 0 = no FAIL. Exit code 1 = at least one FAIL. INCOMPLETE rows are
printed and do not by themselves set the exit code.
"""
import hashlib
import json
import os
import sqlite3
import struct
import sys

WT = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000"
EV = WT + "/docs/roadmap/0.1/0.1.7/evidence"
A = EV + "/issue219-squadA-profile-20260921T044041Z"
B = EV + "/issue219-squadB-sql-20260921T043911Z"
C = EV + "/issue219-squadC-cadence-20260921T044258Z"
D = EV + "/issue219-squadD-chunk-20260921T044229Z"
S6 = EV + "/stage-6-history-209-rca-20260920T191016Z"
PIN = WT + "/core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-pinned2-20260921T031259Z"
DIAG = WT + "/core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-squadA-profile-20260921T044041Z"
PCK = WT + "/core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-squadA-packcounters-20260921T044814Z"
FIN = WT + "/core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-final-20260921T031259Z"
V016 = WT + "/benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl"
HANDOFF = WT + "/docs/roadmap/0.1/0.1.7/issue219-namespace-3x8-gap-rca-handoff.md"
WORKLOAD = WT + "/benchmark/fs-bench-pro/workload/main.rs"
NSCONTENT = WT + "/core/benchmark/fs-bench-pro-storage-content/src/ops/namespace_content.rs"
MAINRS = WT + "/core/benchmark/fs-bench-pro-storage-content/src/main.rs"
REGISTRYRS = WT + "/core/benchmark/fs-bench-pro-storage-content/src/registry.rs"
LAYOUTRS = WT + "/core/crates/layerfs-storage/src/pack/layout.rs"

ROWS = []


def chk(cid, claim, raw, ok, note=""):
    ROWS.append({"id": cid, "claim": claim, "raw": raw,
                 "verdict": "PASS" if ok else "FAIL", "note": note})
    print("%-4s %-22s claim=%s raw=%s%s" % ("PASS" if ok else "FAIL", cid, claim, raw,
                                            ("  [" + note + "]") if note else ""))


def inc(cid, claim, reason):
    ROWS.append({"id": cid, "claim": claim, "raw": "-", "verdict": "INCOMPLETE",
                 "note": reason})
    print("%-4s %-22s claim=%s  [%s]" % ("INCOMPLETE", cid, claim, reason))


def note(cid, text):
    ROWS.append({"id": cid, "claim": text, "raw": "-", "verdict": "NOTE", "note": ""})
    print("NOTE %-22s %s" % (cid, text))


def p4(fraction):
    return "%.4f" % (100.0 * fraction)


def load_tsv(path):
    out = {}
    for line in open(path):
        line = line.rstrip("\n")
        if not line or "\t" not in line:
            continue
        k, v = line.split("\t", 1)
        try:
            out[k] = int(v)
        except ValueError:
            out[k] = v
    return out


def read(path):
    return open(path, encoding="utf-8", errors="replace").read()


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        while True:
            b = fh.read(1 << 22)
            if not b:
                break
            h.update(b)
    return h.hexdigest()


# --------------------------------------------------------------------------
# load the raw rows
# --------------------------------------------------------------------------
c1 = load_tsv(A + "/counters.tsv")                 # diagnostic row 1 counters
c2 = load_tsv(A + "/counters-packcounters.tsv")    # diagnostic row 2 counters
diag = json.load(open(A + "/diagnostic-receipt.json"))
diagrun = json.load(open(A + "/diagnostic-run.json"))
pin = json.load(open(PIN + "/pipeline-namespace-10000/receipt.json"))
pinrun = json.load(open(PIN + "/run.json"))
pck = json.load(open(PCK + "/pipeline-namespace-10000/receipt.json"))
pckrun = json.load(open(PCK + "/run.json"))
fin = json.load(open(FIN + "/pipeline-namespace-10000/receipt.json"))
cadence = json.load(open(A + "/cadence-calibration.json"))
journal = json.load(open(A + "/journal-calibration.json"))
cachecal = json.load(open(A + "/cache-calibration.json"))
blob = json.load(open(A + "/blob-calibration.json"))
synth = json.load(open(A + "/pack-append-synthesis.json"))
recon = json.load(open(A + "/reconciliation.json"))
v016_lines = read(V016).splitlines()
v016_hdr = json.loads(v016_lines[0])
v016 = json.loads(v016_lines[1])
v016_r0 = v016["records"][0]
handoff = read(HANDOFF)
workload = read(WORKLOAD)
mainrs = read(MAINRS)
registryrs = read(REGISTRYRS)
layoutrs = read(LAYOUTRS)
ranked = [l.split("\t") for l in read(B + "/ranked_table.tsv").splitlines() if l.strip()]
ranked = {row[0]: row for row in ranked}

OPERATION = diag["phases"]["operation_ns"]
SPAN = c1["pipeline.accept_span_ns"]

# --------------------------------------------------------------------------
# ROW 1 -- the seven SaveProfile buckets and the remainder
# --------------------------------------------------------------------------
BUCKETS = [
    ("commit_ns", "pipeline.profile_commit_ns"),
    ("sql_ns", "pipeline.profile_sql_ns"),
    ("full_ns", "pipeline.profile_full_ns"),
    ("place_ns", "pipeline.profile_place_ns"),
    ("group_ns", "pipeline.profile_group_ns"),
    ("resolve.pooled_ns", "pipeline.profile_resolve_pooled_ns"),
    ("delta_ns", "pipeline.profile_delta_ns"),
]
REPORT_BUCKETS = {  # README section 2 table: ns, share of span, share of operation
    "commit_ns": (1351360518, "38.2155", "38.1855"),
    "sql_ns": (781237539, "22.0928", "22.0755"),
    "full_ns": (245777362, "6.9504", "6.9450"),
    "place_ns": (79188910, "2.2394", "2.2376"),
    "group_ns": (18859112, "0.5333", "0.5329"),
    "resolve.pooled_ns": (5858368, "0.1657", "0.1655"),
    "delta_ns": (82960, "0.0023", "0.0023"),
}
seven = 0
for name, key in BUCKETS:
    got = c1[key]
    seven += got
    want_ns, want_span, want_op = REPORT_BUCKETS[name]
    chk("R1.ns." + name, str(want_ns), str(got), got == want_ns, "counters.tsv " + key)
    chk("R1.span." + name, want_span + "%", p4(got / SPAN) + "%",
        p4(got / SPAN) == want_span, "of pipeline.accept_span_ns=%d" % SPAN)
    chk("R1.op." + name, want_op + "%", p4(got / OPERATION) + "%",
        p4(got / OPERATION) == want_op, "of operation_ns=%d" % OPERATION)

chk("R1.seven_sum", "2482364769", str(seven), seven == 2482364769, "sum of the seven")
chk("R1.profile_total", "pipeline.profile_total_ns=2482364769",
    str(c1["pipeline.profile_total_ns"]), seven == c1["pipeline.profile_total_ns"],
    "instrument's own seven-bucket sum")
chk("R1.seven_span", "70.1995%", p4(seven / SPAN) + "%", p4(seven / SPAN) == "70.1995",
    "seven of accept span")
chk("R1.seven_op", "70.1444%", p4(seven / OPERATION) + "%",
    p4(seven / OPERATION) == "70.1444", "seven of operation")
rems = SPAN - seven
rem_op = OPERATION - seven
chk("R1.remainder_ns", "1053790981", str(rems), rems == 1053790981, "span - seven")
chk("R1.remainder_span", "29.8005%", p4(rems / SPAN) + "%",
    p4(rems / SPAN) == "29.8005", "remainder of accept span")
chk("R1.remainder_op", "29.7771%", p4(rems / OPERATION) + "%",
    p4(rems / OPERATION) == "29.7771", "remainder of operation")
chk("R1.span_vs_operation", "span is 99.92% of operation",
    p4(SPAN / OPERATION) + "%", p4(SPAN / OPERATION) == "99.9215", "span/operation")
chk("R1.zero_sub_buckets", "eligible/acquire/cost/reuse = 0",
    "eligible=%d acquire=%d cost=%d reuse=%d" % (
        c1["pipeline.profile_resolve_eligible_ns"], c1["pipeline.profile_resolve_acquire_ns"],
        c1["pipeline.profile_resolve_cost_ns"], c1["pipeline.profile_resolve_reuse_ns"]),
    all(c1[k] == 0 for k in ("pipeline.profile_resolve_eligible_ns",
                             "pipeline.profile_resolve_acquire_ns",
                             "pipeline.profile_resolve_cost_ns",
                             "pipeline.profile_resolve_reuse_ns")),
    "resolve is entirely pooled_ns")

# shares.tsv is itself a raw file produced beside the counters: recompute it.
shares_txt = read(A + "/shares.tsv")
for key, val in (("commit_ns", 1351360518), ("sql_ns", 781237539), ("full_ns", 245777362),
                 ("place_ns", 79188910), ("group_ns", 18859112),
                 ("resolve.pooled_ns", 5858368), ("delta_ns", 82960)):
    line = [l for l in shares_txt.splitlines() if l.startswith(key + "\t")]
    chk("R1.shares_tsv." + key, ("%s %s%% %s%%" % (val, p4(val / SPAN), p4(val / OPERATION))),
        line[0] if line else "<<missing>>",
        bool(line) and line[0] == "%s\t%d\t%s%%\t%s%%" % (key, val, p4(val / SPAN), p4(val / OPERATION)),
        "shares.tsv row recomputed from counters.tsv")

# --------------------------------------------------------------------------
# ROW 2 -- the database term and the decode term
# --------------------------------------------------------------------------
db_term = c1["pipeline.profile_sql_ns"] + c1["pipeline.profile_commit_ns"]
decode = (c1["pipeline.profile_resolve_ns"] + c1["pipeline.profile_delta_ns"]
          + c1["pipeline.profile_group_ns"])
chk("R2.db_term_ns", "2132598057", str(db_term), db_term == 2132598057, "sql_ns + commit_ns")
chk("R2.db_span", "60.3084%", p4(db_term / SPAN) + "%", p4(db_term / SPAN) == "60.3084",
    "of accept span")
chk("R2.db_op", "60.2610%", p4(db_term / OPERATION) + "%",
    p4(db_term / OPERATION) == "60.2610", "of operation")
chk("R2.db_claim", "the database term is 60.3% of the operation", p4(db_term / OPERATION) + "%",
    p4(db_term / OPERATION).startswith("60.2"), "report says 60.3%, exact 60.26% -- supported")
chk("R2.decode_ns", "24800440", str(decode), decode == 24800440, "resolve + delta + group")
chk("R2.decode_span", "0.7013%", p4(decode / SPAN) + "%", p4(decode / SPAN) == "0.7013",
    "of accept span")
chk("R2.decode_op", "0.7008%", p4(decode / OPERATION) + "%",
    p4(decode / OPERATION) == "0.7008", "of operation")
chk("R2.decode_claim", "decode is 0.70%", p4(decode / OPERATION) + "%",
    round(100.0 * decode / OPERATION, 2) == 0.70, "report says 0.70% -- supported")
chk("R2.ns_per_commit", "77762.7 ns/commit", "%.1f" % (c1["pipeline.profile_commit_ns"] / c1["pipeline.commits"]),
    abs(c1["pipeline.profile_commit_ns"] / c1["pipeline.commits"] - 77762.7) < 0.05,
    "commit_ns / commits")
chk("R2.objects_per_commit", "1.4527", "%.4f" % (c1["pipeline.inserted"] / c1["pipeline.commits"]),
    "%.4f" % (c1["pipeline.inserted"] / c1["pipeline.commits"]) == "1.4527", "inserted / commits")
chk("R2.bytes_per_commit", "17330.6", "%.1f" % (c1["pipeline.content_bytes"] / c1["pipeline.commits"]),
    abs(c1["pipeline.content_bytes"] / c1["pipeline.commits"] - 17330.6) < 0.05,
    "content_bytes / commits")
build_encode = (c1["pipeline.profile_full_ns"] + c1["pipeline.profile_delta_ns"]
                + c1["pipeline.profile_group_ns"] + c1["pipeline.profile_resolve_ns"]
                + c1["pipeline.profile_place_ns"])
chk("R2.build_encode_ms", "349.9 ms (README section 3 matrix cell)",
    "%.3f ms" % (build_encode / 1e6), abs(build_encode / 1e6 - 349.9) < 0.05,
    "full+delta+group+resolve+place = %.3f ms, report prints 349.9" % (build_encode / 1e6))

# --------------------------------------------------------------------------
# ROW 3 -- work identity between the pinned row and the diagnostic row
# --------------------------------------------------------------------------
ca, cb_ = pin["counters"], diag["counters"]
common = sorted(set(ca) & set(cb_))
moved = [k for k in common if ca[k] != cb_[k]]
chk("R3.shared", "15 counters published by both rows", str(len(common)), len(common) == 15,
    "set intersection of the two receipts' counters")
chk("R3.moved", "0 shared counters moved", str(len(moved)), len(moved) == 0,
    "value-by-value comparison")
chk("R3.only_diag", "15 counters only in the diagnostic",
    str(len(set(cb_) - set(ca))), len(set(cb_) - set(ca)) == 15,
    "all pipeline.profile_* plus pipeline.accept_span_ns")
chk("R3.only_pinned", "0 counters only in the pinned row",
    str(len(set(ca) - set(cb_))), len(set(ca) - set(cb_)) == 0, "")
chk("R3.canonical_bytes", "canonical_bytes_total = 302231057 in both",
    "%d / %d" % (pin["resources"]["space"]["canonical_bytes_total"],
                 diag["resources"]["space"]["canonical_bytes_total"]),
    pin["resources"]["space"]["canonical_bytes_total"]
    == diag["resources"]["space"]["canonical_bytes_total"] == 302231057,
    "resources.space in both receipts")
chk("R3.canonical_objects", "canonical_objects_total = 25245 in both",
    "%d / %d" % (pin["resources"]["space"]["canonical_objects_total"],
                 diag["resources"]["space"]["canonical_objects_total"]),
    pin["resources"]["space"]["canonical_objects_total"]
    == diag["resources"]["space"]["canonical_objects_total"] == 25245, "")
chk("R3.role_map", "canonical_objects by role identical",
    "identical" if pin["resources"]["space"]["canonical_objects"]
    == diag["resources"]["space"]["canonical_objects"] else "DIFFERENT",
    pin["resources"]["space"]["canonical_objects"]
    == diag["resources"]["space"]["canonical_objects"], "per-role object map")
chk("R3.gates", "13 gates each, all PASS, both rows status PASS",
    "pinned gates=%d diag gates=%d statuses=%s/%s" % (
        len(pin["gates"]), len(diag["gates"]),
        sorted({g["status"] for g in pin["gates"]}), sorted({g["status"] for g in diag["gates"]})),
    len(pin["gates"]) == 13 and len(diag["gates"]) == 13
    and all(g["status"] == "PASS" for g in pin["gates"] + diag["gates"])
    and pin["status"] == diag["status"] == "PASS", "")
ident_diff = sorted(k for k in set(pin["identity"]) | set(diag["identity"])
                    if pin["identity"].get(k) != diag["identity"].get(k))
chk("R3.identity_diffs", "exactly four identity differences",
    ",".join(ident_diff), ident_diff == ["harness_binary_sha256", "source_commit",
                                         "source_dirty_files", "started_utc"],
    "harness sha, source commit, started_utc, dirty-file list")
chk("R3.harness_sha", "cb21593d... -> 71d7c40c...",
    pin["identity"]["harness_binary_sha256"][:8] + " -> "
    + diag["identity"]["harness_binary_sha256"][:8],
    pin["identity"]["harness_binary_sha256"].startswith("cb21593d")
    and diag["identity"]["harness_binary_sha256"].startswith("71d7c40c"), "")
chk("R3.same_product_lock", "same product lock hash on both v0.1.7 rows",
    pin["identity"]["product_lock_sha256"][:16],
    pin["identity"]["product_lock_sha256"] == diag["identity"]["product_lock_sha256"], "")
chk("R3.work_identity_txt", "work-identity-check.txt reports 15 shared / 0 moved",
    read(A + "/work-identity-check.txt").splitlines()[3].strip(),
    "shared counters compared: 15" in read(A + "/work-identity-check.txt")
    and "shared counters that MOVED: 0" in read(A + "/work-identity-check.txt"),
    "raw output beside the report")
note("R3.scope", "R3 verifies the pinned-vs-diagnostic contrast (both v0.1.7 rows). It does NOT verify "
                 "the report's 'same product seal, same image' sentence: neither v0.1.7 row records a "
                 "product seal or an image (see R7.1b).")

# --------------------------------------------------------------------------
# ROW 4 -- the cadence calibration
# --------------------------------------------------------------------------
arms = cadence["arms"]
w1, w2, w3 = arms[0]["wall_ns"], arms[1]["wall_ns"], arms[2]["wall_ns"]
t1, t2, t3 = arms[0]["actual_txns"], arms[1]["actual_txns"], arms[2]["actual_txns"]
extreme = (w1 - w3) / (t1 - t3)
middle = (w2 - w3) / (t2 - t3)
chk("R4.ns_per_txn_arms", "67,214 / 332,916 / 6,005,525 ns per transaction",
    "%.1f / %.1f / %.1f" % (w1 / t1, w2 / t2, w3 / t3),
    abs(w1 / t1 - 67214) < 0.5 and abs(w2 / t2 - 332916) < 0.5 and abs(w3 / t3 - 6005525) < 0.5,
    "wall_ns / actual_txns per arm")
chk("R4.extremes", "13,084 ns from the extremes", "%.4f" % extreme,
    abs(extreme - 13084) < 1.0, "(550620792-444408875)/(8192-74) = %.4f (rounds to 13084)" % extreme)
chk("R4.middle", "7,959 ns from the middle segment", "%.4f" % middle, abs(middle - 7959) < 1.0,
    "(454763917-444408875)/(1366-74) = %.4f  <-- report and cadence-calibration.json both say 7959"
    % middle)
chk("R4.json_range", "cadence-calibration.json fixed_ns_per_txn_range == [7959, 13084]",
    str(cadence["derived"]["fixed_ns_per_txn_range"]),
    cadence["derived"]["fixed_ns_per_txn_range"] == [7959, 13084], "raw JSON's own derived block")
applied = cadence["derived"]["applied_to_17378_commits_ns"]
chk("R4.applied_low", "applied_to_17378_commits_ns[0] == 7959 x 17378 = 138311502",
    str(applied[0]), applied[0] == 7959 * 17378,
    "raw JSON says %d; 7959*17378 = %d (implied %0.2f ns/txn)" % (applied[0], 7959 * 17378, applied[0] / 17378))
chk("R4.applied_high", "applied_to_17378_commits_ns[1] == 13084 x 17378 = 227373752",
    str(applied[1]), applied[1] == 13084 * 17378,
    "raw JSON says %d; 13084*17378 = %d (implied %0.2f ns/txn)" % (applied[1], 13084 * 17378, applied[1] / 17378))
lo_share = 100.0 * applied[0] / OPERATION
hi_share = 100.0 * applied[1] / OPERATION
chk("R4.band", "3.9-6.4% of the 3,538,935,458 ns operation",
    "%.4f%% - %.4f%%" % (lo_share, hi_share),
    round(lo_share, 1) == 3.9 and round(hi_share, 1) == 6.4,
    "band holds with the JSON's own applied values")
lo2 = 100.0 * (middle * 17378) / OPERATION
chk("R4.band_recomputed", "band still 3.9-6.4% with the recomputed middle value",
    "%.4f%% - %.4f%%" % (lo2, hi_share), round(lo2, 1) == 3.9 and round(hi_share, 1) == 6.4,
    "8,014.74 ns/txn gives 3.936%, which still prints as 3.9%: the band survives the correction")
chk("R4.bound_is_commit_only", "the 3.9-6.4% bound prices only the fixed per-transaction cost",
    cadence["method"][:96] + "...", "insert identical 32 KiB blobs" in cadence["method"],
    "each blob is inserted once: the replica never rewrites a pack body")
chk("R4.correction_13_2", "section 13.2's correction (bound does not cover the pack-append "
    "amplification) is stated", "13.2 present",
    "is\nnot the total cadence lever" in read(A + "/README.md").replace("*", "")
    or "not the total cadence lever" in read(A + "/README.md"), "quoted from the README")
note("R4.correction_honesty", "Section 13.2 is explicit and self-limiting: it keeps the 3.9-6.4% number, "
     "says what it priced, and says it must not be quoted as the cadence lever. That is honest.")

# --------------------------------------------------------------------------
# ROW 5 -- the pack-append amplification
# --------------------------------------------------------------------------
appends = c2["pipeline.pack_appends"]
packs = c2["pipeline.packs_created"]
statements = c2["pipeline.statements"]
per_pack = appends / packs
chk("R5.counters", "pack_appends=15552 packs_created=1250 statements=16595",
    "appends=%d packs=%d statements=%d" % (appends, packs, statements),
    (appends, packs, statements) == (15552, 1250, 16595), "counters-packcounters.tsv")
chk("R5.appends_per_pack", "15,552 / 1,250 = 12.4416 appends per pack",
    "%.4f" % per_pack, "%.4f" % per_pack == "12.4416", "counters only")
chk("R5.factor_lower", "6.72x = (k+1)/2", "%.4f" % ((per_pack + 1) / 2.0),
    "%.4f" % ((per_pack + 1) / 2.0) == "6.7208", "equal-increment model, k = appends per pack")
chk("R5.factor_upper", "7.22x = (k+2)/2", "%.4f" % ((per_pack + 2) / 2.0),
    "%.4f" % ((per_pack + 2) / 2.0) == "7.2208", "equal-increment model, k = appends per pack")
LB = round(302231057 * 6.7208)
UB = round(302231057 * 7.2208)
chk("R5.bytes_lower", "2,031,234,488 bytes rewritten", str(LB), LB == 2031234488,
    "302,231,057 x 6.7208")
chk("R5.bytes_upper", "2,182,350,016 bytes rewritten", str(UB), UB == 2182350016,
    "302,231,057 x 7.2208")
chk("R5.section13_writes_per_pack", "section 13.1: '14.44 writes per pack'",
    "%.4f (16,802/1,250)" % (16802 / 1250), abs(16802 / 1250 - 14.44) < 0.005,
    "16,802/1,250 = 13.4416, not 14.44; 14.44 is in no raw file")
chk("R5.section13_factor", "section 13.1: 7.72x / ~2.34 GB",
    "%.4f (k+2)/2 at k=13.4416 and %.3f GB" % ((16802 / 1250 + 2) / 2, 302231057 * 7.72 / 1e9),
    False, "7.72x counts the 1,250 creates as extra rewrites; the measured factor is 7.5917x")
chk("R5.section13_label", "section 13.1's rank-2 line labels 16,802 as UPDATE calls alone",
    "ranked_table.tsv row 2 statement=%s" % ranked["2"][1][:58],
    not ("UPDATE object_packs" in ranked["2"][1] and "+ INSERT INTO object_packs" in ranked["2"][1]),
    "ranked_table.tsv row 2 is UPDATE **+ INSERT**; 16,802 = 1,250 INSERT + 15,552 UPDATE")
chk("R5.cross_check_calls", "Squad B's 16,802 == 1,250 + 15,552",
    "%s vs %d" % (ranked["2"][2], packs + appends),
    int(ranked["2"][2]) == packs + appends == 16802, "ranked_table.tsv row 2 vs the counters")
chk("R5.cross_check_statements", "Squad B's 16,595 == pipeline.statements",
    "%s vs %d" % (ranked["5"][2], statements),
    int(ranked["5"][2]) == statements == 16595, "ranked_table.tsv row 5 vs the counters")
chk("R5.ranked_label", "ranked_table.tsv row 2 carries Squad B's corrected DERIVED-EXACT label",
    ranked["2"][3][:40], ranked["2"][3].startswith("DERIVED-EXACT"),
    "the archived table still says EXACT while Squad B's README section 13.1 re-labels the count; "
    "Squad C section 6.1 quotes the stale table")

# --- independent re-parse of the pinned row's own pack directories ---------
sample = PIN + "/pipeline-namespace-10000/sample.sqlite"
man = json.load(open(PIN + "/manifest.json"))
man_sha = [f["sha256"] for f in man["files"] if f["path"].endswith("sample.sqlite")][0]
got_sha = sha256_file(sample)
chk("R5.pack_provenance", "the re-parsed store is the pinned row's sample.sqlite",
    got_sha[:16] + "...", got_sha == man_sha,
    "sha256 recomputed and matched against manifest.json")
LANES = {1: "Ordinary", 2: "Native", 4: "WholeFile", 6: "PooledMetadata", 7: "Singleton"}
con = sqlite3.connect("file:%s?mode=ro" % sample, uri=True)
con.execute("PRAGMA query_only = 1")
sum_data = con.execute("SELECT SUM(length(data)) FROM object_packs").fetchone()[0]
n_packs = con.execute("SELECT COUNT(*) FROM object_packs").fetchone()[0]
chk("R5.sum_data", "SUM(length(data)) == receipt pack_bodies_bytes 302023232",
    str(sum_data), sum_data == pin["resources"]["space"]["pack_bodies_bytes"] == 302023232,
    "SQL against the pinned store vs the receipt")
chk("R5.pack_rows", "object_packs holds 1,250 rows (== packs_created)", str(n_packs),
    n_packs == packs == 1250, "")
lane = {}
written_total = 0
bad = []
for pack_id, data in con.execute("SELECT pack_id, data FROM object_packs ORDER BY pack_id"):
    if data[:8] != b"LFPACK\x00\x00":
        bad.append((pack_id, "magic"))
        continue
    ver = struct.unpack_from("<I", data, 8)[0]
    gc = struct.unpack_from("<I", data, 12)[0]
    ln = LANES.get(ver)
    if ln is None:
        bad.append((pack_id, "version %d" % ver))
        continue
    if ln == "WholeFile":
        ent = [struct.unpack_from("<I", data, 16 + 4 * i)[0] for i in range(gc)]
        if ent[0] != 16 + 4 * gc:
            bad.append((pack_id, "wf directory start"))
        bodies = [ent[i + 1] - ent[i] for i in range(gc - 1)] + [len(data) - ent[-1]]
        entry = 4
    else:
        starts = [struct.unpack_from("<I", data, 16 + 16 * i)[0] for i in range(gc)]
        bodies = [struct.unpack_from("<I", data, 16 + 16 * i + 4)[0] for i in range(gc)]
        entry = 16
        off = 16 + 16 * gc
        for i, s in enumerate(starts):
            if s != off:
                bad.append((pack_id, "directory continuity"))
            off = s + bodies[i]
        if off != len(data):
            bad.append((pack_id, "trailing bytes"))
    acc = 16
    written = 0
    for b in bodies:
        acc += entry + b
        written += acc            # assembled length at each of the gc writes
    if acc != len(data):
        bad.append((pack_id, "assembled %d != stored %d" % (acc, len(data))))
    st = lane.setdefault(ln, {"packs": 0, "writes": 0, "written": 0, "final": 0, "max": 0})
    st["packs"] += 1
    st["writes"] += gc
    st["written"] += written
    st["final"] += len(data)
    st["max"] = max(st["max"], len(data))
    written_total += written
final_total = sum(v["final"] for v in lane.values())
factor = written_total / final_total
chk("R5.pack_format_clean", "0 format violations over all 1,250 pack directories",
    "%d violations" % len(bad), len(bad) == 0,
    "directory continuity, directory start, trailing bytes and assembled==stored, all packs")
chk("R5.measured_written", "2,292,865,337 bytes written", str(written_total),
    written_total == 2292865337, "Sigma of the assembled length at each of the 16,802 writes")
chk("R5.measured_final", "302,023,232 bytes persisted", str(final_total),
    final_total == 302023232, "Sigma of every pack body length")
chk("R5.measured_factor", "amplification 7.5917x", "%.4f" % factor,
    abs(factor - 7.5917) < 0.0001, "re-derived from the pinned row's own store")
chk("R5.writes_16802", "16,802 writes == groups in the store", str(sum(v["writes"] for v in lane.values())),
    sum(v["writes"] for v in lane.values()) == 16802, "one write per group")
chk("R5.max_pack", "largest pack 262,141 B", str(max(v["max"] for v in lane.values())),
    max(v["max"] for v in lane.values()) == 262141, "max body length over the store")
exp_lane = {"WholeFile": (102, 9444, 1260350294, 24613232, 51.21),
            "PooledMetadata": (3, 207, 25584656, 726096, 35.24),
            "Native": (1142, 7130, 1004612191, 276058548, 3.64),
            "Ordinary": (3, 21, 2318196, 625356, 3.71)}
for ln, (p_, w_, wr_, fi_, f_) in exp_lane.items():
    st = lane[ln]
    chk("R5.lane." + ln, "%d packs / %d writes / %d written / %d final / %.2fx" % (p_, w_, wr_, fi_, f_),
        "%d / %d / %d / %d / %.2fx" % (st["packs"], st["writes"], st["written"], st["final"],
                                       st["written"] / st["final"]),
        (st["packs"], st["writes"], st["written"], st["final"]) == (p_, w_, wr_, fi_)
        and abs(st["written"] / st["final"] - f_) < 0.005, "per-lane, re-derived")
chk("R5.skew_56", "WholeFile+PooledMetadata rewrite 56.1% of all rewritten bytes to persist 8.4% of final",
    "%.1f%% / %.1f%%" % (100.0 * (lane["WholeFile"]["written"] + lane["PooledMetadata"]["written"]) / written_total,
                         100.0 * (lane["WholeFile"]["final"] + lane["PooledMetadata"]["final"]) / final_total),
    abs(100.0 * (lane["WholeFile"]["written"] + lane["PooledMetadata"]["written"]) / written_total - 56.1) < 0.05
    and abs(100.0 * (lane["WholeFile"]["final"] + lane["PooledMetadata"]["final"]) / final_total - 8.4) < 0.05,
    "section 16.2")
pp = read(B + "/pack_parse.py")
wf_buggy = []
for pack_id in (5, 11, 26):
    data = con.execute("SELECT data FROM object_packs WHERE pack_id=?", (pack_id,)).fetchone()[0]
    gc = struct.unpack_from("<I", data, 12)[0]
    sizes = [struct.unpack_from("<I", data, 16 + 4 * i)[0] - 8 for i in range(gc)]
    acc = 16
    for s in sizes:
        acc += 4 + s
    wf_buggy.append((pack_id, acc, len(data)))
chk("R5.archived_parser", "the archived pack_parse.py reproduces the archived pack_stats.json",
    "packs %s -> recomputed %s vs stored %s" % ([p for p, _, _ in wf_buggy],
                                                [a for _, a, _ in wf_buggy], [l for _, _, l in wf_buggy]),
    all(a == l for _, a, l in wf_buggy),
    "its WholeFile branch computes size = directory_start - 8; the directory stores absolute starts, "
    "so the script's own assert fires on the first WholeFile pack")
chk("R5.dbstat_predecessor", "302,023,232 not 304,427,008 (dbstat reports page bytes)",
    "receipt pack_bodies_bytes=%d, SUM(length(data))=%d" % (pin["resources"]["space"]["pack_bodies_bytes"], sum_data),
    pin["resources"]["space"]["pack_bodies_bytes"] == sum_data == 302023232,
    "the 304,427,008 figure appears nowhere in the corrected raw files")
chk("R5.synthesis_corrected", "pack-append-synthesis.json no longer carries 304,427,008 or a 243 KB average",
    "supersedes=" + synth.get("supersedes", "")[:60],
    "304427008" not in json.dumps(synth) and "243" not in json.dumps(synth["measured_amplification"]),
    "rewritten synthesis")

# --------------------------------------------------------------------------
# ROW 6 -- the v0.1.6 comparison figures
# --------------------------------------------------------------------------
cpu16 = v016_r0["initialization_user_cpu_ns"] + v016_r0["initialization_system_cpu_ns"]
chk("R6.layerstack_init_ns", "944,880,958", str(v016_r0["layerstack_init_ns"]),
    v016_r0["layerstack_init_ns"] == 944880958, "perf.jsonl line 2 records[0]")
chk("R6.cpu", "1,788,767,834 (user+system)", str(cpu16), cpu16 == 1788767834,
    "1,036,730,875 + 752,036,959")
chk("R6.transactions", "73 initialize_admission_transactions", str(v016_r0["initialize_admission_transactions"]),
    v016_r0["initialize_admission_transactions"] == 73, "")
chk("R6.canonical_objects", "25,158 store_canonical_objects", str(v016_r0["store_canonical_objects"]),
    v016_r0["store_canonical_objects"] == 25158, "")
chk("R6.canonical_bytes", "302,182,831 store_canonical_bytes", str(v016_r0["store_canonical_bytes"]),
    v016_r0["store_canonical_bytes"] == 302182831, "")
ratio16 = cpu16 / v016_r0["layerstack_init_ns"]
chk("R6.cpu_per_wall", "1.8934", "%.4f" % ratio16, "%.4f" % ratio16 == "1.8934",
    "1,788,767,834 / 944,880,958 = %.4f, not 1.8934" % ratio16)
chk("R6.max_transaction", "4,149,860 max transaction bytes", str(v016_r0["initialize_max_transaction_bytes"]),
    v016_r0["initialize_max_transaction_bytes"] == 4149860, "handoff claim checked in section 2.1")
ratio_geo = v016_r0["initialize_max_transaction_bytes"] / (c1["pipeline.content_bytes"] / c1["pipeline.commits"])
chk("R6.txn_size_ratio", "~239x transaction-size ratio", "%.2f" % ratio_geo,
    abs(ratio_geo - 239) < 1.0, "4,149,860 / 17,330.6")
chk("R6.commit_ratio", "238.05x commit-count ratio", "%.2f" % (17378 / 73), abs(17378 / 73 - 238.05) < 0.01, "")
chk("R6.wall_ratio", "3.745x operation wall ratio", "%.4f" % (OPERATION / v016_r0["layerstack_init_ns"]),
    abs(OPERATION / v016_r0["layerstack_init_ns"] - 3.745) < 0.001, "diagnostic operation / v0.1.6 init")
chk("R6.cpu_ratio", "1.837x CPU ratio", "%.4f" % (3286170000 / cpu16),
    abs(3286170000 / cpu16 - 1.837) < 0.001, "diagnostic CPU / v0.1.6 CPU")
chk("R6.objects_ratio", "1.0035x canonical objects", "%.5f" % (25245 / 25158),
    "%.4f" % (25245 / 25158) == "1.0035", "")
chk("R6.bytes_ratio", "1.0002x canonical bytes", "%.5f" % (302231057 / 302182831),
    "%.4f" % (302231057 / 302182831) == "1.0002", "")
chk("R6.matrix_cells_v016", "v0.1.6 matrix cells: preparation 4,200.8 / setup 250.0 / teardown 249.1 / "
    "cleanup 319.1 / command 1,457.4 / wall 6,152.5 ms, scanned 300,000,000 / 10,000, disk read 729,088",
    "%.1f / %.1f / %.1f / %.1f / %.1f / %.1f / %d / %d / %d" % (
        v016["preparation_wall_ns"] / 1e6, v016_r0["setup_ns"] / 1e6, v016_r0["teardown_ns"] / 1e6,
        v016["cleanup"]["wall_ns"] / 1e6, v016["command_wall_ns"] / 1e6, v016["wall_ns"] / 1e6,
        v016_r0["scanned_bytes"], v016_r0["scanned_files"], v016_r0["initialization_disk_read_bytes"]),
    abs(v016["preparation_wall_ns"] / 1e6 - 4200.8) < 0.05
    and abs(v016_r0["setup_ns"] / 1e6 - 250.0) < 0.05
    and abs(v016_r0["teardown_ns"] / 1e6 - 249.1) < 0.05
    and abs(v016["cleanup"]["wall_ns"] / 1e6 - 319.1) < 0.05
    and abs(v016["command_wall_ns"] / 1e6 - 1457.4) < 0.05
    and abs(v016["wall_ns"] / 1e6 - 6152.5) < 0.05
    and v016_r0["scanned_bytes"] == 300000000 and v016_r0["scanned_files"] == 10000
    and v016_r0["initialization_disk_read_bytes"] == 729088, "every v0.1.6 cell of the section 3 matrix")
chk("R6.fixture_cells", "fixture generate 2,733,511,125 / plan+manifest 17.8 ms / worker_count 1",
    "%d / %.1f ms / %d" % (v016["preparation"]["fixture"]["fixture_generate_ns"],
                           (v016["preparation"]["fixture"]["fixture_plan_ns"]
                            + v016["preparation"]["fixture"]["fixture_manifest_ns"]) / 1e6,
                           v016["preparation"]["fixture"]["fixture_worker_count"]),
    v016["preparation"]["fixture"]["fixture_generate_ns"] == 2733511125
    and abs((v016["preparation"]["fixture"]["fixture_plan_ns"]
             + v016["preparation"]["fixture"]["fixture_manifest_ns"]) / 1e6 - 17.8) < 0.05
    and v016["preparation"]["fixture"]["fixture_worker_count"] == 1, "")

# --------------------------------------------------------------------------
# ROW 7 -- the seven corrections to the handoff's premises
# --------------------------------------------------------------------------
chk("R7.1a.source_commit", "handoff: both arms b0260df3a dirty=false",
    "v0.1.6 %s dirty=%s; v0.1.7 %s dirty=%s" % (
        v016_hdr["identities"]["host_executor"]["LAYERFS_SOURCE_COMMIT"],
        v016_hdr["identities"]["host_executor"]["LAYERFS_SOURCE_DIRTY"],
        pinrun["identity"]["source_commit"], pinrun["identity"]["source_dirty"]),
    v016_hdr["identities"]["host_executor"]["LAYERFS_SOURCE_COMMIT"] == "b0260df3a2ffc371773cd062feafd4b5e435bf1e"
    and v016_hdr["identities"]["host_executor"]["LAYERFS_SOURCE_DIRTY"] == "false"
    and pinrun["identity"]["source_commit"] == "2a63aff0d0a24743deebb9129ab14e098d9e24ed"
    and pinrun["identity"]["source_dirty"] is True,
    "correction is right: the v0.1.7 row is a different, dirty commit")
chk("R7.1b.dirty_files", "correction says five dirty files on the pinned row",
    str(len(pinrun["identity"]["source_dirty_files"])), len(pinrun["identity"]["source_dirty_files"]) == 5,
    "run.json identity.source_dirty_files")
chk("R7.1c.seal_and_image", "correction's last sentence: 'The product seal b3e3cb7453... and the image "
    "sha256:d152ced8... do match'",
    "seal/image fields in either v0.1.7 row: %s" % ("none"
        if not any(k in json.dumps(pin["identity"]) + json.dumps(diag["identity"])
                   for k in ("seal", "image")) else "present"),
    False,
    "no v0.1.7 artifact records a product seal or an image; b3e3cb7453 occurs only in the v0.1.6 "
    "(root crates/) family and in prose -- this half of the correction is an assertion, not a raw field")
chk("R7.2.content_bytes", "handoff's 301,171,810 is pipeline.content_bytes, not the canonical total",
    str(c1["pipeline.content_bytes"]), c1["pipeline.content_bytes"] == 301171810, "counters.tsv")
chk("R7.2.canonical_total", "the row's canonical total is 302,231,057",
    str(diag["resources"]["space"]["canonical_bytes_total"]),
    diag["resources"]["space"]["canonical_bytes_total"] == 302231057, "resources.space")
byte_delta = 302231057 - 302182831
chk("R7.2.delta", "48,226 bytes above v0.1.6, in the opposite direction", str(byte_delta),
    byte_delta == 48226 and byte_delta > 0, "302,231,057 - 302,182,831")
pct_delta = 100.0 * byte_delta / 302182831
chk("R7.2.pct", "0.016%", "%.4f%%" % pct_delta, "%.3f" % pct_delta == "0.016",
    "48,226 / 302,182,831")
pct_handoff = 100.0 * abs(301171810 - 302182831) / 302182831
chk("R7.2.sixteen_times", "correction says the true gap is 'sixteen times closer' than the handoff's 0.33%",
    "%.2fx (%.4f%% vs %.4f%%)" % (pct_handoff / pct_delta, pct_handoff, pct_delta),
    abs(pct_handoff / pct_delta - 16) < 0.5,
    "the ratio is 20.96x, not 16x; the 0.334% and 0.0160% figures themselves are right")
chk("R7.3.v016_page_cache", "page-cache evidence is on the v0.1.6 row (543,040 / 34,604,032 / 33,554,432)",
    "%d / %d / %d" % (v016_r0["sqlite_t1_page_cache_overflow_peak_bytes"],
                      v016_r0["sqlite_t1_connection_cache_used_bytes"],
                      v016_r0["sqlite_connection_cache_target_bytes"]),
    v016_r0["sqlite_t1_page_cache_overflow_peak_bytes"] == 543040
    and v016_r0["sqlite_t1_connection_cache_used_bytes"] == 34604032
    and v016_r0["sqlite_connection_cache_target_bytes"] == 33554432, "perf.jsonl records[0]")
chk("R7.3.v017_absent", "the v0.1.7 receipts publish neither field",
    "pinned has sqlite keys: %s; diagnostic has: %s" % (
        any("sqlite" in k for k in json.dumps(pin)), any("sqlite" in k for k in json.dumps(diag))),
    not any(k in json.dumps(pin) or k in json.dumps(diag)
            for k in ("sqlite_t1_page_cache_overflow_peak_bytes", "sqlite_t1_connection_cache_used_bytes")),
    "both v0.1.7 receipts, whole-file JSON scan")
chk("R7.4.profile_empty", "profile:'' is the registry fixture-profile column",
    "pinned receipt profile=%r; main.rs:504 = %s" % (pin.get("profile"), mainrs.splitlines()[503].strip()[:60]),
    pin.get("profile") == "" and '("profile", case.profile.to_string())' in mainrs.splitlines()[503],
    "src/main.rs:504 writes the registry row's profile into the trace header")
chk("R7.4.registry_def", "the column is defined as the fixture-profile variant, empty when the row has none",
    registryrs.splitlines()[398].strip()[:70] + " | " + registryrs.splitlines()[399].strip()[:60],
    "Fixture-profile variant, empty when the row has none" in registryrs, "src/registry.rs:399-400")
chk("R7.5.registry_self_check", "pinned run.json records registry_self_check FAIL, exit_code 1, [...,2,4] vs [...,2,5]",
    "status=%s exit=%s expected_tail=%s actual_tail=%s" % (
        pinrun["registry_self_check"]["status"], pinrun["registry_self_check"]["exit_code"],
        pinrun["registry_self_check"]["MISMATCH frozen cardinality array"].split("actual")[0].strip()[-12:],
        pinrun["registry_self_check"]["MISMATCH frozen cardinality array"].split("actual")[1].strip()[-6:]),
    pinrun["registry_self_check"]["status"] == "FAIL"
    and pinrun["registry_self_check"]["exit_code"] == 1
    and pinrun["registry_self_check"]["MISMATCH frozen cardinality array"].endswith("expected [4, 4, 12, 12, 32, 7, 20, 12, 4, 12, 8, 5, 10, 20, 21, 14, 6, 4, 4, 2, 4], actual [4, 4, 12, 12, 32, 7, 20, 12, 4, 12, 8, 5, 10, 20, 21, 14, 6, 4, 4, 2, 5]"),
    "the pinned row itself still passed (tally PASS 1)")
chk("R7.5.row_passed_despite", "the row itself PASSed", "tally=%s gates=%s" % (
        pinrun["tally"], sorted({g["status"] for g in pin["gates"]})),
    pinrun["tally"] == {"PASS": 1} and all(g["status"] == "PASS" for g in pin["gates"]), "")
t1903 = 1788.8 / 0.94
t1926 = 1788.8 / 0.9286
chk("R7.6.handoff_arithmetic", "handoff: 1,788.8/0.94 = ~1,903 ms",
    "%.2f ms" % t1903, abs(t1903 - 1903) < 0.5, "the handoff's own target arithmetic reproduces")
chk("R7.6.diagnostic_divisor", "correction: at the diagnostic row's 0.9286 the same arithmetic gives 1,926.5 ms",
    "%.2f ms" % t1926, abs(t1926 - 1926.5) < 0.05,
    "1,788.8 / 0.9286 = %.2f ms, not 1,926.5; the correction's point (use the row's own ratio) stands" % t1926)
chk("R7.7.different_rows", "different harnesses, receipt schemas, cache contracts and timer sets",
    "v0.1.6 schema=%s identity=%s; v0.1.7 schema=%s harness=%s" % (
        v016_hdr["schema"], v016_hdr["identities"]["harness_identity"][:8],
        pin["schema"], pin["identity"]["harness_root"].rsplit("/", 1)[-1]),
    v016_hdr["schema"] == "layerfs-perf-v1" and pin["schema"] == "layerfs-core-receipt-v1"
    and v016_hdr["cache_contract"] is None
    and pin["identity"]["harness_root"].endswith("fs-bench-pro-storage-content"),
    "v0.1.6 root benchmark/fs-bench-pro, v0.1.7 core/.../fs-bench-pro-storage-content")

# --------------------------------------------------------------------------
# ROW 8 -- Squad D: the bands are weights, not byte sizes
# --------------------------------------------------------------------------
wl = workload.splitlines()
chk("R8.weight_fn", "workload/main.rs:424-430 declares the triples inside a weight function",
    "line 424: %s | 427: %s | 428: %s | 429: %s" % (
        wl[423].strip()[:40], wl[426].strip(), wl[427].strip(), wl[428].strip()),
    wl[423].startswith("fn namespace_relative_weight")
    and "1_u64, 8_u64" in wl[426] and "32, 256" in wl[427] and "1_024, 8_192" in wl[428],
    "the triples are (lower, upper) bounds of a weight")
chk("R8.weight_return", "the function returns lower + floor((2*role+1)*width/(2*count))",
    wl[445].strip()[:64] + " | " + wl[446].strip()[:56],
    "lower" in wl[445] and "checked_add" in wl[446] and "numerator / denominator" in wl[446],
    "main.rs:446-448")
chk("R8.size_formula", "the byte size is a later Hamilton pass: size = 1 + floor(distributable*weight/weight_sum)",
    wl[357].strip()[:70] + " | " + wl[362].strip()[:70],
    "distributable" in wl[357] and "relative_weight" in wl[358] and "weight_sum" in wl[360]
    and "1_u64.checked_add(floor)" in wl[362], "main.rs:358-363")
# independent re-implementation of the ladder from the source formula
def wgt(kind, role, count):
    if kind in ("empty", "anchor"):
        return 0
    lo, hi = {"tiny": (1, 8), "small": (32, 256), "medium": (1024, 8192)}[kind]
    width = hi - lo + 1
    return lo + ((role * 2 + 1) * width) // (count * 2)


classes = [("tiny", 7899), ("small", 1500), ("medium", 500)]
weights = [wgt(k, r, n) for k, n in classes for r in range(n)]
weight_sum = sum(weights)
positive = 7899 + 1500 + 500
distributable = 300000000 - 100000000 - positive
floors = [(distributable * x) // weight_sum for x in weights]
sizes = [1 + f for f in floors]
extra = distributable - sum(floors)
rem = sorted([((distributable * x) % weight_sum, i) for i, x in enumerate(weights)],
             key=lambda t: (-t[0], t[1]))
for _, i in rem[:extra]:
    sizes[i] += 1
chk("R8.weight_sum", "2,555,546", str(weight_sum), weight_sum == 2555546, "recomputed from the weight formula")
chk("R8.distributable", "199,990,101", str(distributable), distributable == 199990101,
    "300,000,000 - 100,000,000 - 9,899")
chk("R8.extra", "4,214 largest-remainder top-ups", str(extra), extra == 4214, "")
bands = []
i = 0
for k, n in classes:
    seg = sizes[i:i + n]
    i += n
    bands.append((k, min(seg), max(seg), sum(seg)))
chk("R8.bands", "tiny 79..627, small 2,505..20,035, medium 80,684..640,537",
    str(bands), bands == [("tiny", 79, 627, 2789651), ("small", 2505, 20035, 16905060),
                          ("medium", 80684, 640537, 180305289)],
    "no file is 1-8 / 32-256 / 1,024-8,192 bytes; the bands are weights")
cut = 131072
ge = [s for s in sizes if s >= cut]
whole = 9900 - (len(ge) + 1)
chk("R8.chunk_partition", "456 chunked files, 9,444 whole-file, 275,564,964 chunked source bytes",
    "%d / %d / %d" % (len(ge) + 1, whole, sum(ge) + 100000000),
    (len(ge) + 1, whole, sum(ge) + 100000000) == (456, 9444, 275564964),
    "exclusive 131,072-byte cutoff, anchor included")
fs = fin["resources"]["space"]
chk("R8.canonical_closure", "role canonical bytes close to the byte against the v0.1.7 receipt",
    "chunk %d vs %d, whole-file %d vs %d, file-state %d vs %d" % (
        sum(ge) + 100000000 + 14466 * 21, fs["canonical_bytes"]["chunk"],
        (sum(sizes) - sum(ge)) + whole * 23, fs["canonical_bytes"]["whole-file"],
        len(ge) + 1, fs["canonical_objects"]["file-state"]),
    sum(ge) + 100000000 + 14466 * 21 == fs["canonical_bytes"]["chunk"] == 275868750
    and (sum(sizes) - sum(ge)) + whole * 23 == fs["canonical_bytes"]["whole-file"] == 24652248
    and len(ge) + 1 == fs["canonical_objects"]["file-state"] == 456,
    "receipt ns17-final-20260921T031259Z (the row Squad D used)")
chk("R8.v017_same_formula", "the v0.1.7 generator uses the same weight band triples",
    "namespace_content.rs contains TINY/SMALL/MEDIUM triples: %s" % (
        "yes" if all(s in read(NSCONTENT) for s in ("7_899", "1_500", "500", "1_024", "8_192")) else "no"),
    all(s in read(NSCONTENT) for s in ("7_899", "1_500", "500", "1_024", "8_192")),
    "src/ops/namespace_content.rs constants")

# --------------------------------------------------------------------------
# ROW 9 -- arithmetic falsified in the reports (collected, not dropped)
# --------------------------------------------------------------------------
falsified = []
manual = [
    ("A section 9 / cadence-calibration.json", "middle-segment fixed cost 7,959 ns",
     "recomputed 8,014.74 ns from the file's own arms"),
    ("cadence-calibration.json derived block", "applied_to_17378_commits_ns [138293022, 227393512]",
     "its own 7,959/13,084 x 17,378 are 138,311,502 / 227,373,752"),
    ("A section 3 matrix", "build/encode 349.9 ms", "sum of the five buckets is 349.767 ms"),
    ("A section 4", "v0.1.6 CPU/wall 1.8934", "1,788,767,834 / 944,880,958 = 1.8931"),
    ("A section 7.6", "1,788.8/0.9286 = 1,926.5 ms", "= 1,926.34 ms"),
    ("A section 7.2", "sixteen times closer", "0.3345% / 0.0160% = 20.96x"),
    ("A section 12.2", "synthetic 51,279 ns/txn, 1.52x the product", "51,279 is in no raw file in the corpus"),
    ("A section 12.1", "scaled floor 446,106,000 ns", "blob-calibration.json's own derived_floor_scaled_ns is 446,111,419"),
    ("A section 13.1", "14.44 writes per pack", "16,802/1,250 = 13.4416"),
    ("A section 13.1", "7.72x / ~2.34 GB", "counts the 1,250 creates as rewrites; measured 7.5917x"),
    ("A section 13.1", "16,802 UPDATE calls", "ranked_table.tsv row 2 is UPDATE + INSERT"),
    ("A section 14.3", "amplification band 6.72-7.22x", "the measured value 7.5917x lies outside the band"),
    ("B section 12(e)(4)", "576 = 207 + 207 + 1 + 1 + 367", "that sum is 783; 576 = 367 + 207 + 1 + 1"),
    ("B sections 4/13.1", "operation_ns = 3,585.8 ms", "pinned2 operation_ns = 3,575,832,667; 3,585.5 ms is ns17-final"),
    ("B ranked_table", "sum of the 11 exact rows 1,956.7 ms", "the same 11 rows sum to 1,956.6 ms"),
]
scaled_floor = 396227057 * 302231057 / 268435456
chk("R9.scaled_floor", "section 12.1 floor 446,106,000 ns", "%.0f" % scaled_floor,
    abs(scaled_floor - 446106000) < 1, "exact = %.0f; blob-calibration.json says 446,111,419" % scaled_floor)
_51279_hits = []
for _d in (A, B, C, D):
    for _root, _dirs, _files in os.walk(_d):
        _dirs[:] = [x for x in _dirs if x not in ("__pycache__",)]
        for _f in _files:
            _t = read(os.path.join(_root, _f))
            if "51279" in _t or "51,279" in _t:
                _51279_hits.append(os.path.relpath(os.path.join(_root, _f), EV))
_51279_raw = [h for h in _51279_hits if not h.endswith("README.md")]
chk("R9.synthetic_51279", "section 12.2's 'synthetic 51,279 ns/txn' is reproducible from a raw file",
    "files in the four #219 squad directories containing 51279 or 51,279: %s" % (_51279_hits or "none"),
    len(_51279_raw) > 0,
    "the number that makes 'per-commit engine cost not anomalous' occurs only in Squad A's README "
    "prose; no raw artifact in the #219 corpus carries it")
print()
print("--- R9: arithmetic falsified, collected (report text -> recomputed) ---")
for _i, (where, claim, raw) in enumerate(manual, 1):
    print("FAIL %-18s %-42s | %s | %s" % ("R9.%02d" % _i, where, claim, raw))
    ROWS.append({"id": "R9.%02d" % _i, "claim": where + ": " + claim, "raw": raw, "verdict": "FAIL",
                 "note": "falsified item"})

# --------------------------------------------------------------------------
# ROW 10 -- section 14.2: two methods agree to the unit
# --------------------------------------------------------------------------
chk("R10.agree_calls", "section 14.2/16.5: Squad B's 16,802 and the product's 1,250+15,552 agree to the unit",
    "ranked_table.tsv %s; counters %d+%d=%d" % (ranked["2"][2], packs, appends, packs + appends),
    int(ranked["2"][2]) == packs + appends == 16802, "both raw files")
chk("R10.agree_statements", "section 14.2: Squad B's 16,595 and pipeline.statements agree to the unit",
    "ranked_table.tsv %s; counters %d" % (ranked["5"][2], statements),
    int(ranked["5"][2]) == statements == 16595, "both raw files")
chk("R10.independence", "section 14.2's stronger claim: 'Two methods that share no input agree exactly'",
    "ranked_table.tsv row 2 basis: " + ranked["2"][3][:60],
    not ranked["2"][3].upper().startswith("EXACT: 16,802 GROUPS PARSED"),
    "Squad B counts GROUPS in the pinned store and multiplies by the one-group-one-write invariant; the "
    "product counter counts WRITES in a different (work-identical) run. They validate the invariant, they "
    "do not share no input, and they are different runs")
chk("R10.label", "Squad B's own label is preserved where it is quoted",
    "README section 13.1: " + "DERIVED-EXACT" if "DERIVED-EXACT" in read(B + "/README.md") else "EXACT",
    "DERIVED-EXACT" in read(B + "/README.md"),
    "Squad A quotes the label correctly in section 16.5, but its section 13 table is copied from the "
    "stale ranked_table.txt")

# --------------------------------------------------------------------------
# ROW 11 -- sections 15/16 honesty about the campaign's own errors
# --------------------------------------------------------------------------
chk("R11.self_correction", "section 15.0 records the superseded first version and why it was wrong",
    "15.0 present: %s" % ("yes" if "A correction to this report's own first version" in read(A + "/README.md") else "no"),
    "A correction to this report's own first version" in read(A + "/README.md")
    and "That reasoning was wrong" in read(A + "/README.md")
    and "is recorded here rather than deleted" in read(A + "/README.md"),
    "the wrong 'sql only, not commit' reasoning is kept in the record")
chk("R11.prior_art_admitted", "section 16.3 admits the phenomenon is prior art and corrects section 13's framing",
    "16.3 present: %s" % ("yes" if "Prior art: this is not a discovery" in read(A + "/README.md") else "no"),
    "Prior art: this is not a discovery" in read(A + "/README.md")
    and "already established the same phenomenon" in read(A + "/README.md"),
    "quotes stage-6-history-209-rca")
chk("R11.prior_art_raw", "the prior-art claims reproduce from that round's own README",
    "45,794 appends / 177,640 avg / 262,112 max / 58.0 us COMMIT",
    all(s in read(S6 + "/README.md") for s in ("45,794", "177,640", "262,112", "58.0")),
    "stage-6-history-209-rca-20260920T191016Z/README.md")
_knob = 0
import os as _os
for _root in (WT + "/core", WT + "/crates"):
    for _dir, _dirs, _files in _os.walk(_root):
        _dirs[:] = [d for d in _dirs if d not in ("target", ".git")]
        for _f in _files:
            if _f.endswith((".rs", ".py", ".sh", ".toml", ".sql")):
                try:
                    _knob += read(_os.path.join(_dir, _f)).count("LAYERFS_STORAGE_COMMIT_EVERY")
                except Exception:
                    pass
chk("R11.knob_absent", "LAYERFS_STORAGE_COMMIT_EVERY does not exist in core/ or crates/ at HEAD 9c46930b8",
    "occurrences under core/ + crates/ (target/ excluded): %d" % _knob, _knob == 0,
    "walked every .rs/.py/.sh/.toml/.sql under both trees")
_marked = isinstance(recon.get("SUPERSEDED"), str) and "RETRACTED" in recon["SUPERSEDED"]
chk("R11.superseded_marking", "every superseded artifact in the directory is marked as superseded",
    "reconciliation.json sha %s, SUPERSEDED key present and retracting: %s" % (
        sha256_file(A + "/reconciliation.json")[:16], _marked),
    _marked,
    "at 04:49Z this file was unmarked and contradicted section 15; it now carries an explicit "
    "SUPERSEDED marker that retracts the verdict and names the mechanism")
chk("R11.novelty_overclaim", "the report no longer overclaims novelty anywhere",
    "section 13.4: 'the only candidate this campaign has found'; section 12.2 still names per-row "
    "metadata as 'the surviving candidate'",
    False,
    "sections 12.2, 13 and 13.4 are unchanged and still frame the amplification as this campaign's "
    "finding/next target; section 15.3 and 16.4 supersede them only in later text")

# --------------------------------------------------------------------------
# ROW 12 -- Squad A section 3 matrix sourcing
# --------------------------------------------------------------------------
chk("R12.matrix_row_mix", "section 3 declares 'v0.1.7 = the diagnostic row above' and every v0.1.7 cell "
    "comes from it",
    "acquisition 8.1 vs 5.2; preparation 913.2 vs 898.8; setup 3.6 vs 2.7; verification 338.7 vs 338.1; "
    "cleanup 459 vs 541; command 4,841.6 vs 4,791.6; run wall 5,045.4 vs 4,986.1; handoff 41.7 vs 44.3; "
    "timing children 11.2 vs 11.7",
    not (diagrun["phases"]["admission"]["acquisition_wall_ns"] / 1e6 < 8.0
         and diagrun["phases"]["admission"]["preparation_wall_ns"] / 1e6 < 900.0),
    "those nine cells are pinned2 values (ns17-pinned2), not the declared diagnostic row; the README "
    "does not say the column is mixed")
chk("R12.matrix_children_mix", "the cell 'product timing children 11.2 ms of 3,538.9 ms = 0.32%' is "
    "self-consistent",
    "diagnostic children %.1f ms; pinned2 children %.1f ms; 11.2/3538.9 = %.2f%%" % (
        sum(ch["elapsed_ns"] for ch in json.load(open(DIAG + "/pipeline-namespace-10000/timing.json"))["children"]) / 1e6,
        sum(ch["elapsed_ns"] for ch in json.load(open(PIN + "/pipeline-namespace-10000/timing.json"))["children"]) / 1e6,
        100.0 * 11.2 / 3538.935458),
    False,
    "the numerator is pinned2's 11.23 ms and the denominator is the diagnostic's 3,538.9 ms")

# --------------------------------------------------------------------------
# ROW 13 -- Squad C: the cadence pre-registration
# --------------------------------------------------------------------------
import os
GEO = C + "/store-geometry.txt"
c_files = sorted(os.listdir(C)) if os.path.isdir(C) else []
if not os.path.exists(C + "/README.md"):
    inc("R13.squadC", "Squad C cadence pre-registration",
        "no README.md in " + C.rsplit("/", 1)[-1] + " (contains only: " + ", ".join(c_files)
        + "); the pre-registration row cannot be reviewed and is not invented here")
else:
    c_readme = read(C + "/README.md")
    c_pre = read(C + "/pre-registration.md")
    c_geo = read(GEO)
    c_dec = read(C + "/commit-decomposition.txt")
    chk("R13.pre_registration", "the pre-registration names one difference, an identity, expected "
        "movement in the instrument units and refutations",
        "pre-registration.md %d bytes" % len(c_pre),
        all(s in c_pre for s in ("The single difference", "The identity it is compared against",
                                 "Expected movement", "What would refute it")))
    newest_receipt = 0.0
    for root, dirs, files in os.walk(WT + "/core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219"):
        for f in files:
            if f in ("receipt.json", "run.json"):
                newest_receipt = max(newest_receipt, os.path.getmtime(os.path.join(root, f)))
    pre_mtime = os.path.getmtime(C + "/pre-registration.md")
    chk("R13.not_run", "no benchmark ran after the pre-registration was written",
        "newest receipt mtime %.0f vs pre-registration %.0f" % (newest_receipt, pre_mtime),
        newest_receipt < pre_mtime and not any(f.endswith(".json") for f in c_files),
        "no receipt exists in the squad directory and no row is newer than the registration")
    chk("R13.identity_block", "the identity block reproduces from the diagnostic receipt",
        "operation %d span %d sql %d commit %d total %d commits %d inserted %d CPU %d" % (
            diag["phases"]["operation_ns"], c1["pipeline.accept_span_ns"], c1["pipeline.profile_sql_ns"],
            c1["pipeline.profile_commit_ns"], c1["pipeline.profile_total_ns"], c1["pipeline.commits"],
            c1["pipeline.inserted"], 3286170000),
        diag["phases"]["operation_ns"] == 3538935458 and c1["pipeline.accept_span_ns"] == 3536155750
        and c1["pipeline.profile_sql_ns"] == 781237539 and c1["pipeline.profile_commit_ns"] == 1351360518
        and c1["pipeline.profile_total_ns"] == 2482364769 and c1["pipeline.commits"] == 17378
        and c1["pipeline.inserted"] == 25245, "matched field by field")
    decomp = [1, 16595, 207, 207, 367, 1]
    chk("R13.decomposition", "1 + 16,595 + 207 + 207 + 367 + 1 = 17,378 commits",
        "%d" % sum(decomp), sum(decomp) == c1["pipeline.commits"] == 17378,
        "Squad B section 12(e)(4) prints the same terms as 207+207+1+1+367 = 783 against a 576 "
        "remainder; Squad C sums them correctly")
    hist = {}
    for line in c_geo.splitlines():
        if line.startswith("records_per_group_hist|"):
            _, recs, n = line.split("|")
            hist[int(recs)] = int(n)
    chk("R13.group_histogram", "the single-record/two-record group counts sum to 16,595",
        "1-record %d, 2-record %d, groups %d" % (hist.get(1, 0), hist.get(2, 0), sum(hist.values())),
        hist.get(1) == 10081 and hist.get(2) == 5662 and sum(hist.values()) == 16595,
        "store-geometry.txt")
    chk("R13.geometry", "geometry: 1,250 packs / 302,023,232 pack bytes / 207 value groups / max stamp 9,444",
        [l for l in c_geo.splitlines() if l.startswith(("packs_total", "value_groups_total", "content_signatures|"))],
        "packs_total|1250|302023232" in c_geo and "value_groups_total|207" in c_geo
        and "content_signatures|1253|9444|8192|8192" in c_geo,
        "the same store whose SUM(length(data)) I re-derived is 302,023,232")
    fixed = [7959 * 17378, 13084 * 17378]
    overflow = [886153042, 962824311]
    resid = [161162455, 326895974]
    chk("R13.split_sums", "fixed + pack-overflow + residual == commit_ns at both ends",
        "%d and %d" % (fixed[0] + overflow[0] + resid[1], fixed[1] + overflow[1] + resid[0]),
        fixed[0] + overflow[0] + resid[1] == 1351360518
        and fixed[1] + overflow[1] + resid[0] == 1351360518, "commit-decomposition.txt")
    chk("R13.per_commit", "77.76 us = 8.0-13.1 fixed + 51.0-55.4 rewritten + 9.3-18.8 not measured",
        "%.1f = %.1f-%.1f + %.1f-%.1f + %.1f-%.1f" % (
            c1["pipeline.profile_commit_ns"] / 17378, fixed[0] / 17378, fixed[1] / 17378,
            overflow[0] / 17378, overflow[1] / 17378, resid[0] / 17378, resid[1] / 17378),
        abs(fixed[0] / 17378 - 7959) < 1 and abs(overflow[0] / 17378 - 50992) < 2
        and abs(resid[1] / 17378 - 18811) < 2, "per-commit division of the split")
    chk("R13.page_arithmetic", "537,062 / 559,782 / 73,736 pages at 4 KiB",
        "%.0f / %.0f / %.0f" % (2199807795 / 4096, 2292865337 / 4096, 302023232 / 4096),
        round(2199807795 / 4096) == 537062 and round(2292865337 / 4096) == 559782
        and round(302023232 / 4096) == 73736, "pack-rewrite-estimate-2.txt vs pack_stats.json volumes")
    chk("R13.readme_factor", "README section 3: per byte the pager is handed (DERIVED, 7.28x) it pays 0.589 ns",
        "4.474/7.2843 = %.4f and 4.474/7.5917 = %.4f" % (4.4744 / 7.2843, 4.4744 / 7.5917),
        False,
        "0.589 ns is commit_ns / 2,292,865,337, i.e. Squad B 7.5917x, not this squad 7.284x; the raw "
        "commit-decomposition.txt labels it correctly as Squad B, the README sentence does not")
    chk("R13.prc1_prediction", "PR-C1 upper bound of 2,800 commits follows from its stated mechanism",
        "5 lane-tail steps/wave x 577 waves = 2,885 plus 783 fixed = 3,668 commits",
        5 * 577 + 783 <= 2800,
        "the registered 1,600-2,800 range is a prediction the stated mechanism does not bound; it is "
        "labelled a prediction, so this is a loose bound rather than a false measurement")
    chk("R13.band_vs_prc1", "PR-C1 refutation threshold 300 ms is under 15% of 2,132,598,057",
        "300 ms = %.2f%% of 2,132,598,057" % (100.0 * 3e8 / 2132598057),
        abs(100.0 * 3e8 / 2132598057 - 14.07) < 0.05, "section 5")
    chk("R13.state_alignment", "Squad C reads the 3.9-6.4% bound the same way Squad A section 13.2 does",
        "replica wrote every blob exactly once and never issued an append_pack",
        "never issued an" in c_readme and "append_pack" in c_readme.split("never issued an")[1][:40],
        "independent agreement on the same limitation")

# --------------------------------------------------------------------------
# ROW 14 -- Squad A section 17 (added after this review started)
# --------------------------------------------------------------------------
c_life = read(WT + "/core/crates/layerfs-storage/src/cas/lifecycle.rs").splitlines()
begin_write = "\n".join(c_life[128:139])
chk("R14.charge_sites", "Squad C section 2 / Squad A 17.2: BEGIN IMMEDIATE is charged nowhere; the two "
    "SaveProfile::charge calls sit on commit_ns at :170 and :257, after write::commit at :169 and :256",
    "charge in begin_write (129-139): %d; line 169 is write::commit: %s; line 170 is charge: %s; "
    "line 256 is write::commit: %s; line 257 is charge: %s" % (
        begin_write.count("charge"), "write::commit" in c_life[168], "SaveProfile::charge" in c_life[169],
        "write::commit" in c_life[255], "SaveProfile::charge" in c_life[256]),
    begin_write.count("charge") == 0 and "write::commit" in c_life[168] and "SaveProfile::charge" in c_life[169]
    and "write::commit" in c_life[255] and "SaveProfile::charge" in c_life[256],
    "read from core/crates/layerfs-storage/src/cas/lifecycle.rs; the only other charge is sql_ns at :247")
diff246 = 1351360518 - 1104900000
chk("R14.diff_246", "section 17.2: profile_commit_ns minus Squad B COMMIT total = 246,460,518",
    str(diff246), diff246 == 246460518, "1,351,360,518 - 1,104,900,000")
chk("R14.diff_attribution", "section 17.2 attributes that 246.5 ms to advance_pack_if_moved (1,250 statements)",
    "1,250 x ~1 us = 1.25 ms can carry at most 0.5%% of the 246.5 ms",
    (1250 * 1000.0 / diff246) > 0.5,
    "Squad C states the same residual as NOT_MEASURED; Squad A s cell says both, which is a "
    "contradiction rather than a measurement")
rate_row2 = 2292865337 / 1941851139
rate_row1 = 2292865337 / 2132598057
rate_est = 2199807795 / 2132598057
chk("R14.rate_range", "section 17.3: 1.18 GB/s over row 2 s term, 1.03-1.07 GB/s over the full row-1 term",
    "%.4f (row2, %.2f GB/s) / %.4f-%.4f (row1)" % (rate_row2, rate_row2, rate_est, rate_row1),
    abs(rate_row2 - 1.18) < 0.005 and abs(rate_est - 1.03) < 0.005 and abs(rate_row1 - 1.08) < 0.006,
    "both are the same volume over different denominators; the correction to quote a range is right")
c_pre = read(C + "/pre-registration.md")
chk("R14.prc1_table", "section 17.4 reproduces Squad C s PR-C1 predictions unchanged",
    "commits 1,600-2,800 / pack writes 1,300-2,600 / bytes 0.30-0.50 GB / sql 330-470 / commit 250-560 / op 2.0-2.5 s",
    all(s in c_pre for s in ("1,600", "2,800", "1,300", "2,600", "0.30", "0.50", "330", "470", "250", "560", "2.0", "2.5")),
    "pre-registration.md (written before any arm)")
wf_measured = 100.0 * 1260350294 / 2292865337
wf_estimate = 100.0 * 1169771205 / 2199807795
chk("R14.wf_share", "section 17.4: 102 packs (8%) carry 53% of the rewritten bytes",
    "measured parse %.1f%%, equal-growth estimate %.1f%%" % (wf_measured, wf_estimate),
    abs(wf_estimate - 53.0) < 1.0,
    "53% is Squad C s equal-growth estimate (1,169,771,205 / 2,199,807,795); the measured pack parse "
    "puts the WholeFile share at 55.0%% of 2,292,865,337")
import subprocess
_git = lambda *a: subprocess.run(["git", "-C", WT] + list(a), capture_output=True, text=True).stdout
_stat216 = _git("show", "--stat", "--format=", "7075f338db36b209b59031e3e55ae11cf87eed57")
_parent = _git("show", "7075f338db36b209b59031e3e55ae11cf87eed57^:core/crates/layerfs-storage/src/cas/lifecycle.rs")
_logS = _git("log", "-S", "never outlives the step", "--oneline", "--",
             "core/crates/layerfs-storage/src/cas/lifecycle.rs")
chk("R14.provenance_git", "section 17.5 / Squad C section 4: #216 s 7075f338 does not carry the step-scoped commit",
    "7075f338 touches lifecycle/placement/pool_lane: %d files; parent already has the rule: %s; log -S finds: %s" % (
        sum(1 for l in _stat216.splitlines() if any(s in l for s in ("cas/lifecycle.rs", "cas/placement.rs", "cas/pool_lane.rs"))),
        "yes" if "never outlives the step" in _parent else "no",
        _logS.split()[0] if _logS.split() else "none"),
    sum(1 for l in _stat216.splitlines() if any(s in l for s in ("cas/lifecycle.rs", "cas/placement.rs", "cas/pool_lane.rs"))) == 0
    and "never outlives the step" in _parent and _logS.startswith("eb319aaa9"),
    "git show / git show ^: / git log -S, all read-only")

# --------------------------------------------------------------------------
# ROW 15 -- Squad A section 18 (the errata written in response to this review)
# --------------------------------------------------------------------------
a_readme = read(A + "/README.md")
_i18 = a_readme.find("## 18. ERRATA")
errata = a_readme[_i18:] if _i18 >= 0 else ""
chk("R15.errata_exists", "section 18 records this review and declares itself to win on disagreement",
    "section 18 present: %s" % ("yes" if errata else "no"),
    bool(errata) and "this section wins" in errata,
    "written at 05:08:57Z, after this review directory appeared")
_corrections = {"13.4416": "14.44 -> 13.4416", "15,552": "UPDATE 15,552 + INSERT 1,250",
                "7.5917x": "7.72x -> 7.5917x", "8,014.74": "7,959 -> 8,014.74",
                "1.8931": "1.8934 -> 1.8931"}
chk("R15.errata_values", "every value section 18 corrects matches my own re-derivation",
    ", ".join("%s (%s)" % (k, v) for k, v in _corrections.items()),
    all(k in errata for k in _corrections),
    "13.4416 = 16,802/1,250; 15,552 = 16,802-1,250; 7.5917 = my pack re-parse; 8,014.74 = the "
    "cadence middle segment; 1.8931 = 1,788,767,834/944,880,958")
_diag_cells = [(diagrun["phases"]["admission"]["acquisition_wall_ns"] / 1e6, "5.2"),
               (diagrun["phases"]["admission"]["preparation_wall_ns"] / 1e6, "898.8"),
               ((1445750 + 1289416) / 1e6, "2.7"),
               (diagrun["phases"]["admission"]["verification_wall_ns"] / 1e6, "338.1"),
               (diagrun["phases"]["admission"]["cleanup_wall_ns"], "541"),
               (diagrun["phases"]["admission"]["complete_command_ns"] / 1e6, "4,791.6"),
               (diagrun["wall_ns"] / 1e6, "4,986.1"),
               (diagrun["phases"]["admission"]["handoff_ns"] / 1e6, "44.3"),
               (11712083 / 1e6, "11.7")]
_pin_cells = [(pinrun["phases"]["admission"]["acquisition_wall_ns"] / 1e6, "8.1"),
              (pinrun["phases"]["admission"]["preparation_wall_ns"] / 1e6, "913.2"),
              ((2321708 + 1280625) / 1e6, "3.6"),
              (pinrun["phases"]["admission"]["verification_wall_ns"] / 1e6, "338.7"),
              (pinrun["phases"]["admission"]["cleanup_wall_ns"], "459"),
              (pinrun["phases"]["admission"]["complete_command_ns"] / 1e6, "4,841.6"),
              (pinrun["wall_ns"] / 1e6, "5,045.4"),
              (pinrun["phases"]["admission"]["handoff_ns"] / 1e6, "41.7"),
              (11229208 / 1e6, "11.2")]
chk("R15.nine_cells", "section 18.1 s nine-cell table matches the two rows cell by cell",
    "diagnostic %s ; pinned2 %s" % ([c[1] for c in _diag_cells], [c[1] for c in _pin_cells]),
    all(abs(v - float(t.replace(",", ""))) < 0.05 for v, t in _diag_cells + _pin_cells)
    and all(t in errata for _, t in _diag_cells + _pin_cells),
    "each printed value recomputed from the two run.json files; the mixed timing-children cell is "
    "listed as mixed")
_r9fails = len([r for r in ROWS if r["id"].startswith("R9") and r["verdict"] == "FAIL"])
chk("R15.item_count", "section 18 says row 9 FAILs with 18 arithmetic items",
    "%d R9 items in this review FAIL" % _r9fails, _r9fails == 18,
    "the review has 17 R9 items (15 collected + 2 standalone, before this section is counted); "
    "Squad A repeated the reviewer s own miscount in its opening sentence")
chk("R15.row_count", "section 18 says the review covered fifteen rows",
    "this review defines rows 1-14", False,
    "cosmetic: the brief listed nine required rows and this review added five")
chk("R15.seal_not_corrected", "section 18 corrects the one claim this review could not verify (the "
    "7.1 product seal / image sentence)",
    "seal or image mentioned in section 18: %s" % ("yes" if ("seal" in errata or "image" in errata) else "no"),
    "seal" in errata or "image" in errata,
    "section 18.6 defers to this review instead of correcting section 7.1, so the assertion still "
    "stands in the body with no raw field behind it")
chk("R15.body_unchanged", "the corrected numbers were also fixed where they appear in the body",
    "13.1 still reads 14.44/7.72x; 14.3 still bands 6.72-7.22x; 12.1 still prints 446,106,000; "
    "12.2 still prints 51,279; 4 still prints 1.8934; 7.6 still prints 1,926.5; 2.2 still prints 349.9",
    not all(s in a_readme for s in ("14.44 writes per pack", "6.72x", "446,106,000", "51,279",
                                     "1.8934", "1,926.5", "349.9 ms")),
    "section 18 declares itself to win on disagreement, so this is a documentation-style choice "
    "rather than a live numeric error for a reader who reaches section 18")

# --------------------------------------------------------------------------
# corpus digest -- the exact revision this review is pinned to
# --------------------------------------------------------------------------
print()
print("--- corpus digest (sha256 + mtime of every file this review read) ---")
import time
DIGEST = [
    A + "/README.md", A + "/counters.tsv", A + "/shares.tsv", A + "/diagnostic-receipt.json",
    A + "/diagnostic-run.json", A + "/cadence-calibration.json", A + "/journal-calibration.json",
    A + "/cache-calibration.json", A + "/blob-calibration.json", A + "/pack-append-synthesis.json",
    A + "/reconciliation.json", A + "/counters-packcounters.tsv", A + "/packcounters-analysis.txt",
    B + "/README.md", B + "/ranked_table.tsv", B + "/ranked_table.txt", B + "/pack_stats.json",
    B + "/row2_defence.json", B + "/pack_parse.py",
    C + "/README.md", C + "/pre-registration.md", C + "/store-geometry.txt",
    C + "/commit-decomposition.txt", C + "/pack-rewrite-estimate-2.txt",
    D + "/README.md", D + "/raw/03-v017-ladder-output.txt", D + "/raw/08-verdict.txt",
    PIN + "/pipeline-namespace-10000/receipt.json", PIN + "/run.json",
    DIAG + "/pipeline-namespace-10000/receipt.json", DIAG + "/run.json",
    PCK + "/pipeline-namespace-10000/receipt.json", FIN + "/pipeline-namespace-10000/receipt.json",
    V016, HANDOFF, WORKLOAD, LAYOUTRS, S6 + "/README.md",
]
for _p in DIGEST:
    if os.path.exists(_p):
        print("  %s  %s  %s" % (sha256_file(_p)[:16], time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(os.path.getmtime(_p))), _p.replace(WT + "/", "")))
    else:
        print("  MISSING %s" % _p.replace(WT + "/", ""))
print("  sample.sqlite sha256 %s (pinned row, matches manifest.json)" % got_sha[:16])

# --------------------------------------------------------------------------
# verdicts
# --------------------------------------------------------------------------
print()
print("=" * 100)
fails = [r for r in ROWS if r["verdict"] == "FAIL"]
incompletes = [r for r in ROWS if r["verdict"] == "INCOMPLETE"]
passes = [r for r in ROWS if r["verdict"] == "PASS"]
print("checks: %d PASS, %d FAIL, %d INCOMPLETE" % (len(passes), len(fails), len(incompletes)))
if fails:
    print()
    print("FAILING CHECKS")
    for r in fails:
        print("  FAIL %-24s claim=%s | raw=%s" % (r["id"], r["claim"], r["raw"]))
if incompletes:
    print()
    print("INCOMPLETE CHECKS")
    for r in incompletes:
        print("  INCOMPLETE %-20s %s" % (r["id"], r["note"]))
sys.exit(1 if fails else 0)
