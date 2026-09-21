#!/usr/bin/env python3
"""Re-derives every number in this directory's README from the raw receipts.

    python3 attribute.py            # from this directory

The receipts under `receipts/` are byte copies of what the runner wrote; nothing
here is typed in by hand. A number the README quotes and this script does not
print is a number the README got wrong.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
ROWS = {
    "baseline": "ns17-squadA-packcounters-20260921T044814Z",
    "A0": "ns19-A0-baseline-20260921T054800Z",
    "D1a": "ns19-D1a-span-20260921T054548Z",
    "A1": "ns19-D1b-attrib-20260921T054800Z",
    "T1": "ns19-T1-reserved-dir-20260921T061423Z",
    "T1b": "ns19-T1b-packbytes-20260921T061532Z",
    "T1c": "ns19-T1c-final-20260921T061849Z",
}
# The campaign's own directory parse of the pinned store (issue219-squadA-profile).
CAMPAIGN_PACK_BYTES = 2_292_865_337
CAMPAIGN_PERSISTED = 302_023_232
PINS = [
    "pipeline.batches", "pipeline.bindings", "pipeline.chain_objects", "pipeline.commits",
    "pipeline.content_bytes", "pipeline.content_objects", "pipeline.declared_content_bytes",
    "pipeline.declared_directories", "pipeline.declared_files", "pipeline.inserted",
    "pipeline.largest_batch_bindings", "pipeline.metadata_objects", "pipeline.objects_emitted",
    "pipeline.reused",
]
ROOT = "1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847"


def load(name):
    with open(os.path.join(HERE, "receipts", ROWS[name], "receipt.json")) as handle:
        return json.load(handle)


def ms(value):
    return f"{value / 1e6:9.1f}"


def main():
    rows = {name: load(name) for name in ROWS}
    print("row status / gates")
    for name, row in rows.items():
        gates = [g for g in row["gates"] if g["status"] == "PASS"]
        print(f"  {name:9s} {row['status']:5s} {len(gates)}/{len(row['gates'])} gates")

    print("\nphase wall clock, ms")
    keys = ["operation_ns", "cpu_user_ns", "cpu_system_ns", "preparation_wall_ns",
            "verification_wall_ns", "complete_command_ns"]
    print("  " + " " * 22 + "".join(f"{name:>11s}" for name in ROWS))
    for key in keys:
        line = "  " + f"{key:22s}"
        for row in rows.values():
            value = row["phases"].get(key)
            if value is None:
                value = row["phases"]["invocations"][0][key]
            line += f"{ms(value):>11s}"
        print(line)

    a1, t1c = rows["A1"], rows["T1c"]
    print("\nA1 -> T1c movement (same instrument, same work)")
    for key in keys:
        before = a1["phases"].get(key) or a1["phases"]["invocations"][0][key]
        after = t1c["phases"].get(key) or t1c["phases"]["invocations"][0][key]
        print(f"  {key:22s} {ms(before)} -> {ms(after)}  {(after - before) / 1e6:+10.1f} ms  "
              f"{(after - before) / before * 100:+6.2f} %")

    print("\npinned counters: A0 (clean tree) against T1c")
    for key in PINS:
        before = rows["A0"]["counters"][key]
        after = t1c["counters"][key]
        print(f"  {key:34s} {before:>12} {after:>12}  {'same' if before == after else 'MOVED'}")
    for name, row in rows.items():
        digest = [g["measured"] for g in row["gates"] if g["id"] == "g1.o1-pinned-identity"]
        print(f"  root digest {name:9s} {digest[0].split()[-1] if digest else 'n/a'} "
              f"{'OK' if digest and ROOT in digest[0] else 'CHECK'}")

    print("\nthe profile, ns")
    profile = ["pipeline.accept_span_ns", "pipeline.profile_commit_ns", "pipeline.profile_sql_ns",
               "pipeline.profile_full_ns", "pipeline.profile_group_ns", "pipeline.profile_place_ns",
               "pipeline.profile_resolve_ns", "pipeline.profile_total_ns"]
    for key in profile:
        print(f"  {key:34s} A1 {ms(a1['counters'][key])}  T1c {ms(t1c['counters'][key])}")
    span = t1c["counters"]["pipeline.accept_span_ns"]
    total = t1c["counters"]["pipeline.profile_total_ns"]
    print(f"  T1c remainder {ms(span - total)} ms = {(span - total) / span:.1%} of the span")
    span = a1["counters"]["pipeline.accept_span_ns"]
    total = a1["counters"]["pipeline.profile_total_ns"]
    print(f"  A1  remainder {ms(span - total)} ms = {(span - total) / span:.1%} of the span")

    print("\nthe remainder, by name (diagnostic totals; nested, so read the residues)")
    for key in sorted(k for k in t1c["counters"] if k.startswith("pipeline.diag_")
                      or k.startswith("pipeline.span_")):
        print(f"  {key:38s} A1 {ms(a1['counters'].get(key, 0))}  T1c {ms(t1c['counters'][key])}")
    a = a1["counters"]
    t = t1c["counters"]
    print("  residues (A1):")
    print(f"    driver C1 construction inside the timer   {ms(a['pipeline.span_build_ns'])}")
    print(f"    content loop outside flush_batch          "
          f"{ms(a['pipeline.span_content_ns'] - a['pipeline.diag_flush_batch_ns'])}")
    print(f"    flush_batch outside wave + offer          "
          f"{ms(a['pipeline.diag_flush_batch_ns'] - a['pipeline.diag_wave_ns'] - a['pipeline.diag_offer_total_ns'])}")
    print(f"    offer outside full/delta/resolve/seal     "
          f"{ms(a['pipeline.diag_offer_total_ns'] - a['pipeline.profile_full_ns'] - a['pipeline.profile_delta_ns'] - a['pipeline.profile_resolve_ns'] - a['pipeline.diag_seal_total_ns'])}")
    print(f"    BEGIN IMMEDIATE + watermark read          {ms(a['pipeline.diag_begin_ns'])}")
    print(f"    validate_candidates - its queries         "
          f"{ms(a['pipeline.diag_validate_ns'] - a['pipeline.diag_collision_query_ns'])}")
    print(f"    validate_candidates queries               {ms(a['pipeline.diag_collision_query_ns'])}")
    print(f"    per-wave locator + presence seed          {ms(a['pipeline.diag_wave_ns'])}")
    print("    write_pack outside its SQL                see README section 4: sql_ns contains both the")
    print("                                              pack write and the object insert, so the two")
    print("                                              cannot be separated by subtraction alone")
    print(f"    finish span outside finish_inner          "
          f"{ms(a['pipeline.span_finish_ns'] - a['pipeline.diag_finish_total_ns'])}")

    print("\nthe instrument the treatment moves")
    written = t1c["counters"]["pipeline.pack_bytes_written"]
    print(f"  campaign pack bytes written (directory parse)  {CAMPAIGN_PACK_BYTES:>14,}")
    print(f"  T1c pipeline.pack_bytes_written                {written:>14,}")
    print(f"  reduction                                      {CAMPAIGN_PACK_BYTES / written:>14.4f}x")
    print(f"  amplification, campaign                        {CAMPAIGN_PACK_BYTES / CAMPAIGN_PERSISTED:>14.4f}x")
    print(f"  amplification, T1c (against the campaign's persisted figure)"
          f"  {written / CAMPAIGN_PERSISTED:>9.4f}x")
    for key in ("pipeline.packs_created", "pipeline.pack_appends", "pipeline.statements",
                "pipeline.commits", "pipeline.inserted"):
        print(f"  {key:34s} A0 {rows['A0']['counters'][key]:>10}  T1c {t1c['counters'][key]:>10}")

    print("\nstore size and hash (stores.txt; the 300 MB stores are not copied here)")
    with open(os.path.join(HERE, "stores.txt")) as handle:
        for line in handle:
            if line.strip() and not line.startswith("#"):
                print("  " + line.rstrip())


if __name__ == "__main__":
    main()
