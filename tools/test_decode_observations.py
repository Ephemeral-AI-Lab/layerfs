"""Focused tests for the daemon numeric observation decoder."""

import json
import tempfile
import unittest
from pathlib import Path

import decode_observations

SCOPE = [1, 2, 3, 4]
DAEMON = [9, 9, 9, 9]
SCOPE_HEX = "".join(f"{word:016x}" for word in SCOPE)


def row(call, section, index, values, available=True, scope=SCOPE):
    return json.dumps(
        {
            "layerfs_observation": 1,
            "call": call,
            "slot": 0,
            "scope": scope,
            "daemon": DAEMON,
            "section": section,
            "index": index,
            "available": available,
            "values": values if available else [],
        },
        separators=(",", ":"),
    )


def group(call, scope=SCOPE):
    return [
        row(call, 0, 4, [2, 18, 65536], scope=scope),
        row(call, 2, 1, list(range(17)), scope=scope),
        row(call, 14, -2, [0] * 55, scope=scope),
        row(call, 16, 0, list(range(42)), scope=scope),
        row(call, 12, 0, [], available=False, scope=scope),
        row(call, 99, 0, [4, 114, 1], scope=scope),
    ]


class DecoderTests(unittest.TestCase):
    def test_names_sections_and_values_from_the_source_order(self):
        lines = ["daemon starting", *group(3), "unrelated text"]
        [decoded] = list(decode_observations.decode(lines, {}))
        self.assertEqual(decoded["scope"], SCOPE_HEX)
        self.assertEqual(decoded["call"], 3)
        self.assertTrue(decoded["complete"])
        self.assertEqual(decoded["operation"], "Commit")
        sections = decoded["sections"]
        self.assertEqual(sections["operation"]["namespace"], 4)
        self.assertEqual(sections["sql_foreground.begin"]["elapsed_ns"], 16)
        self.assertEqual(sections["reader_sql.closed_sum"]["commit_drop_statement_wall_ns"], 0)
        self.assertEqual(sections["commit_storage"]["forced_seals"], 41)
        self.assertIsNone(sections["kernel_mount"], "unavailable stays null, never zero")
        self.assertEqual(sections["footer"]["records"], 4)
        self.assertNotIn("trace", decoded)

    def test_joins_traced_calls_by_scope_and_leaves_others_unjoined(self):
        other = [5, 6, 7, 8]
        lines = [*group(1), *group(2), *group(1, scope=other)[:-1]]
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            trace.write_text(
                json.dumps({"layerfs_trace": 1, "id": "ab12cd34-7", "scope": SCOPE_HEX}) + "\n" + "not json\n",
                encoding="utf-8",
            )
            decoded = list(decode_observations.decode(lines, decode_observations.trace_scopes(trace)))
        self.assertEqual(
            [(g["call"], g.get("trace")) for g in decoded], [(1, "ab12cd34-7"), (2, "ab12cd34-7"), (1, None)]
        )
        self.assertFalse(decoded[2]["complete"], "no footer means incomplete")

    def test_every_named_section_matches_its_source_width(self):
        widths = {1: 15, 2: 17, 13: 55, 15: 47, 16: 42, 22: 20, 26: 47, 28: 28, 29: 42, 30: 34, 32: 11}
        for section, width in widths.items():
            self.assertEqual(len(decode_observations.SECTIONS[section][1]), width, section)

    def test_kernel_opcodes_decodes_old_and_current_lengths(self):
        # The 45-value layout predates lookup_missing/served_ns (#359); both
        # stay decodable, with old records simply lacking the two new names.
        old = list(range(45))
        new = list(range(47))
        lines = [*group(5), row(5, 26, 0, old), *group(6), row(6, 26, 0, new)]
        decoded = {g["call"]: g["sections"]["kernel_opcodes"] for g in decode_observations.decode(lines, {})}
        self.assertEqual(decoded[5]["store_units"], 44)
        self.assertNotIn("lookup_missing", decoded[5])
        self.assertNotIn("served_ns", decoded[5])
        self.assertEqual(decoded[6]["lookup_missing"], 45)
        self.assertEqual(decoded[6]["served_ns"], 46)

    def test_commit_namespace_decodes_old_and_current_lengths(self):
        # The 31-value layout predates window_jobs and the phase times, the
        # 35-value one the values and update splits (#352 record 033); all stay
        # decodable, older records lacking the newer names.
        old = list(range(31))
        new = list(range(35))
        split = list(range(47))
        lines = [*group(7), row(7, 15, 0, old), *group(8), row(8, 15, 0, new), *group(9), row(9, 15, 0, split)]
        decoded = {g["call"]: g["sections"]["commit_namespace"] for g in decode_observations.decode(lines, {})}
        self.assertEqual(decoded[7]["fresh_points"], 30)
        for name in ["window_jobs", "normalize_ns", "values_ns", "update_ns"]:
            self.assertNotIn(name, decoded[7])
        self.assertNotIn("unnamed", decoded[7])
        self.assertEqual(decoded[8]["fresh_points"], 30)
        self.assertEqual(decoded[8]["window_jobs"], 31)
        self.assertEqual(decoded[8]["update_ns"], 34)
        self.assertNotIn("inode_page_ns", decoded[8])
        self.assertNotIn("unnamed", decoded[8])
        self.assertEqual(decoded[9]["inode_page_ns"], 35)
        self.assertEqual(decoded[9]["backing_ns"], 46)
        self.assertNotIn("unnamed", decoded[9])


if __name__ == "__main__":
    unittest.main()
