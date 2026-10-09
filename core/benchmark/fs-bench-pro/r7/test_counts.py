"""Exact endpoint/cost algebra; no product or timed execution."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import counts

DAEMON, SCOPE = [1,2,3,4], [5,6,7,8]


def snapshot(call, *, admitted=0, read=0, mutation=0, startup=0, operation=3):
    def entry(section, values, index=0, available=True):
        return {"daemon": DAEMON, "scope": SCOPE, "call": call, "slot": 0,
                "section": section, "index": index, "available": available, "values": values}
    owner = [0] * 32
    owner[0], owner[14], owner[15] = admitted, read, mutation
    owner[1], owner[2], owner[4] = 64, 128, 8192
    rows = [entry(0,[operation,2,8388608],index=1), entry(1,owner)]
    for family in range(14):
        values = [0] * 17
        if family == 0:
            values[0] = values[1] = values[2] = startup
            values[7] = startup * 10
        rows.append(entry(2,values,index=family))
    cost = [1,1] + [0] * 33
    cost[2] = cost[3] = cost[4] = 3
    cost[9] = 30
    rows.append(entry(30,cost))
    return rows


def groups(*rows):
    return {str(index): row for index,row in enumerate(rows)}


def event(sequence, name, before, after):
    return {"sequence": sequence, "event": name, "fields": {
        "control_send_records_before": before, "control_send_records_after": after}}


class CounterViews(unittest.TestCase):
    def test_real_schema_cardinalities(self):
        self.assertEqual(len(counts.STATEMENT), 17)
        self.assertEqual(len(counts.PAYLOAD), 7)
        self.assertEqual(len(counts.ALLOCATION), 6)
        self.assertEqual(len(counts.STORAGE), 42)
        self.assertEqual(len(counts.STORED), 17)
        self.assertEqual(len(counts.CONSTRUCTION), 31)

    def test_resources_actual_field_slices(self):
        rows = snapshot(2)
        rows.append(dict(rows[0],section=28,index=0,values=list(range(31))))
        decoded = counts.decode(rows)
        self.assertEqual(decoded["stored_counts"]["wait_refs"], 0)
        self.assertEqual(decoded["stored_counts"]["ready_targets"], 16)
        self.assertEqual(decoded["allocation_state"]["logical_bytes"], 17)
        self.assertEqual(decoded["pages"]["debt_upper_bytes"], 30)

    def test_unavailable_cost_has_no_fabricated_work(self):
        rows = snapshot(2)
        rows[-1]["values"] = [1,0]
        decoded = counts.decode(rows)
        self.assertFalse(decoded["observer"]["completion_available"])
        self.assertNotIn("sql", decoded["observer"])

    def test_reader_storage_index_is_preserved(self):
        rows = snapshot(2)
        rows.append(dict(rows[0],section=29,index=1,values=list(range(42))))
        decoded = counts.decode(rows)
        self.assertEqual(decoded["reader_storage"][1]["forced_seals"], 41)

    def test_cardinality_cannot_be_silently_truncated(self):
        rows = snapshot(2)
        rows[1]["values"].pop()
        with self.assertRaisesRegex(ValueError,"cardinality"):
            counts.decode(rows)


class Correlation(unittest.TestCase):
    def origin(self):
        return {"event": "control_origin", "fields": {"control_send_records": 1}}

    def test_mount_exactly_two_original_control_calls(self):
        values = groups(snapshot(2,operation=1),snapshot(3,operation=9))
        result = counts.correlate([self.origin(),event(2,"mount",1,3)],values)
        self.assertEqual([item[2] for item in result[0]["identities"]],[2,3])
        self.assertIn("instrumented complete",result[0]["span_scope"])

    def test_ambiguous_call_ids_across_scopes_rejected(self):
        first, second = snapshot(2), snapshot(2)
        for row in second:
            row["scope"] = [9,9,9,9]
        with self.assertRaisesRegex(ValueError,"ambiguity"):
            counts.correlate([self.origin(),event(2,"workspace_status",1,2)],groups(first,second))

    def test_missing_native_group_rejected(self):
        with self.assertRaisesRegex(ValueError,"missing"):
            counts.correlate([self.origin(),event(2,"mount",1,3)],groups(snapshot(2,operation=1)))

    def test_send_counts_are_actual_numbers(self):
        forged = event(2,"workspace_status","1","2")
        with self.assertRaisesRegex(ValueError,"send records"):
            counts.correlate([self.origin(),forged],groups(snapshot(2)))

    def test_wrong_operation_is_not_relabelled(self):
        with self.assertRaisesRegex(ValueError,"operation mismatch"):
            counts.correlate([self.origin(),event(2,"commit",1,2)],groups(snapshot(2,operation=3)))

    def test_original_typed_status_outcome_required(self):
        for available in (None,0):
            original = event(2,"workspace_status",1,2)
            if available is not None:
                original["fields"]["status_state_attribution_available"] = available
            result = counts.correlate([self.origin(),original],groups(snapshot(2)))
            self.assertEqual(result[0]["status_state"]["status"],"UNAVAILABLE")
            self.assertNotIn("Lifecycle_jobs",result[0]["status_state"])
        original["fields"].update(status_state_attribution_available=1,status_state_lifecycle_jobs=1)
        result = counts.correlate([self.origin(),original],groups(snapshot(2)))
        self.assertEqual(result[0]["status_state"]["Lifecycle_jobs"],1)
        original["fields"]["status_state_attribution_available"] = 0
        with self.assertRaisesRegex(ValueError,"fabricate"):
            counts.correlate([self.origin(),original],groups(snapshot(2)))

    def test_prospective_endpoint_labels_resolve_only_original_ranges(self):
        values = groups(snapshot(2,operation=1),snapshot(3,operation=9),snapshot(4,operation=3))
        correlated = counts.correlate([self.origin(),event(2,"mount",1,3),event(3,"workspace_status",3,4)],values)
        bindings = {"mount_attach":{"event_sequence":2,"event":"mount","position":"last"},
                    "command_end_status":{"event_sequence":3,"event":"workspace_status","position":"last"}}
        selected = [{"label":"first command","before":"mount_attach","end":"command_end_status"}]
        result = counts.resolve_selectors(selected,bindings,correlated)[0]
        self.assertEqual(result["status"],"RESOLVED")
        self.assertEqual(result["before_identity"][2],3)
        self.assertEqual(result["end_identity"][2],4)
        bindings["command_end_status"]["event_sequence"] = 99
        self.assertEqual(counts.resolve_selectors(selected,bindings,correlated)[0]["status"],"UNAVAILABLE")


class ExactSubtraction(unittest.TestCase):
    def fixtures(self):
        before = snapshot(1,admitted=100,read=50,mutation=20,startup=100)
        middle = snapshot(2,admitted=102,read=52,mutation=20,startup=103)
        end = snapshot(3,admitted=107,read=54,mutation=23,startup=110)
        return groups(before,middle,end), counts.identity(before), counts.identity(end)

    def test_subtract_only_costs_after_before_through_end(self):
        values, before, end = self.fixtures()
        result = counts.phase_counts(values,before,end,exclusive_observer_scope=True)
        self.assertEqual(result["status"],"AVAILABLE")
        self.assertEqual(result["raw"]["owner"]["admitted"],7)
        self.assertEqual(result["observer"]["jobs"],2)
        self.assertEqual(result["adjusted"]["owner"]["admitted"],5)
        self.assertEqual(result["adjusted"]["completed"]["Read"],2)
        self.assertEqual(result["adjusted"]["completed"]["Mutation"],3)
        self.assertEqual(result["adjusted"]["sql_foreground"][0]["executions"],4)
        self.assertNotIn("elapsed_ns",result["adjusted"]["sql_foreground"][0])
        self.assertNotIn("peak_credited_bytes",result["adjusted"]["owner"])
        self.assertIn("publisher",result["observer_credit"])
        self.assertEqual(result["status_state_attribution"]["status"],"UNAVAILABLE")

    def test_typed_state_job_floor_separate_from_sql_and_resources_view(self):
        values,before,end = self.fixtures()
        # Two known original Status State completions within the interval.
        values["2"][1]["values"][17] = 2
        correlation = [{"event":"workspace_status", "identities":[counts.identity(values[key])],
                        "status_state":{"status":"AVAILABLE","Lifecycle_jobs":1}} for key in ("1","2")]
        result = counts.phase_counts(values,before,end,exclusive_observer_scope=True,control_correlation=correlation)
        self.assertEqual(result["status"],"AVAILABLE")
        self.assertEqual(result["adjusted"]["owner"]["admitted"],5)
        self.assertEqual(result["adjusted_without_status_state_jobs"]["owner"]["admitted"],3)
        self.assertEqual(result["adjusted"]["completed"]["Lifecycle"],2)
        self.assertEqual(result["adjusted_without_status_state_jobs"]["completed"]["Lifecycle"],0)
        self.assertEqual(result["adjusted"]["sql_foreground"][0]["executions"],4)
        self.assertIn("UNAVAILABLE",result["status_state_attribution"]["sql_work"])

    def test_missing_completion_is_unavailable_not_zero(self):
        values,before,end = self.fixtures()
        values["1"][-1]["values"] = [1,0]
        result = counts.phase_counts(values,before,end,exclusive_observer_scope=True)
        self.assertEqual(result["status"],"UNAVAILABLE")
        self.assertIn("exact observer work",result["reason"])

    def test_missing_call_cannot_skip_observer_charge(self):
        values,before,end = self.fixtures()
        values.pop("1")
        result = counts.phase_counts(values,before,end,exclusive_observer_scope=True)
        self.assertEqual(result["status"],"UNAVAILABLE")
        self.assertIn("observer call missing",result["reason"])

    def test_owner_saturation_is_not_a_difference(self):
        values,before,end = self.fixtures()
        values["2"][1]["values"][0] = 2**64-1
        self.assertEqual(counts.phase_counts(values,before,end,exclusive_observer_scope=True)["status"],"UNAVAILABLE")

    def test_concurrent_other_observers_not_assumed_absent(self):
        values,before,end = self.fixtures()
        result = counts.phase_counts(values,before,end)
        self.assertEqual(result["status"],"UNAVAILABLE")
        self.assertIn("exclusivity",result["reason"])

    def test_decreased_counter_not_clamped_to_zero(self):
        values,before,end = self.fixtures()
        values["2"][1]["values"][0] = 99
        self.assertEqual(counts.phase_counts(values,before,end,exclusive_observer_scope=True)["status"],"UNAVAILABLE")

    def test_original_counts_independent_of_observer_count_at_two_sizes(self):
        adjusted = []
        for last in (2,4):
            before = snapshot(1,admitted=100,read=50,startup=100)
            rows = [before]
            for call in range(2,last+1):
                observed = call-1
                rows.append(snapshot(call,admitted=100+observed+5,read=50+observed+2,startup=100+observed*3+4))
            result = counts.phase_counts(groups(*rows),counts.identity(before),counts.identity(rows[-1]),exclusive_observer_scope=True)
            self.assertEqual(result["status"],"AVAILABLE")
            adjusted.append((result["adjusted"]["owner"]["admitted"],result["adjusted"]["sql_foreground"][0]["executions"]))
        self.assertEqual(adjusted,[(5,4),(5,4)])


if __name__ == "__main__":
    unittest.main()
