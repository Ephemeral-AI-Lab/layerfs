"""Admission/custody tests only; no sample, product build or large fixture."""
import copy
import json
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import cluster_two_evidence as family
from shared.evidence_gates import LIMIT_DEFINITIONS, artifact, calibration, evaluate, numerical_scope, resolve_limit, resource_phase, sha256
from shared.evidence_registration import _construction, _oracle, _source_coverage, _service, _workload, digest_json, validate
from shared.evidence_workload import payload_chunks, write_trace


class Registration(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def sealed(self, name, value):
        path = self.root / name
        path.write_text(json.dumps(value) + "\n")
        return {"path": name, "sha256": sha256(path)}

    def control(self, **updates):
        context = {"profile": "durable", "route": "original", "workload_sha256": "1" * 64,
                   "cache_sha256": "2" * 64, "observer_sha256": "3" * 64,
                   "case": "E01-startup", "limit": "operation_ns"}
        receipt = {"kind": "control", "sample_count": 1, "admission_eligible": True,
                   "status": "PASS", "cache_status": "PASS", "correctness_status": "PASS",
                   "resource_status": "PASS", "cleanup_status": "PASS", "match": context,
                   "metrics": {"operation_ns": {"value": 101, "unit": "ns",
                       "meaning": LIMIT_DEFINITIONS["operation_ns"][1], "scope": numerical_scope(context)}}}
        receipt.update(updates)
        limit = {"kind": "calibration", "control": self.sealed("control.json", receipt),
                 "metric": "operation_ns", "numerator": 11, "denominator": 10, "offset": 0,
                 "untouched_control": True, "candidate_sample_taken": False, "unit": "ns"}
        return context, limit

    def test_catalogue_retains_all_proposals_and_no_measurement(self):
        catalog = family.registry()
        self.assertEqual(len(catalog["cases"]), 27)
        self.assertEqual([sum(case["group"] == group for case in catalog["cases"])
                          for group in ("engine", "runtime", "acquisition", "custody")], [7, 12, 5, 3])
        self.assertTrue(all(case["status"] == "NOT_RUN" and case["sample_count"] == 0 for case in catalog["cases"]))
        self.assertEqual(catalog["preserved_verdicts"]["init"]["speed"], "8 FAIL")
        self.assertEqual(catalog["preserved_verdicts"]["history"]["speed_allocation"], "6 pairs PASS")

    def test_template_cannot_admit_unimplemented_route_or_missing_numeric_constants(self):
        result = validate(family.registry(), family.template(["E04-write-16m"]), self.root)
        self.assertEqual(result["status"], "INCOMPLETE")
        row = next(row for row in result["rows"] if row["case"] == "E04-write-16m")
        self.assertEqual(row["registration_status"], "INCOMPLETE")
        self.assertEqual(row["sample_count"], 0)
        self.assertTrue(any("driver" in error for error in row["errors"]))
        self.assertTrue(any("limit operation_ns" in error for error in row["errors"]))
        self.assertEqual(sum(row["registration_status"] == "NOT_RUN" for row in result["rows"]), 26)

    def test_registry_tamper_and_post_sample_registration_fail_closed(self):
        frozen = family.template(["E01-startup"])
        frozen["registry_sha256"] = "0" * 64
        frozen["sample_count"] = 1
        result = validate(family.registry(), frozen, self.root)
        self.assertEqual(result["status"], "INCOMPLETE")
        self.assertEqual(len(result["global_errors"]), 3)

    def test_unknown_case_is_retained_as_global_error(self):
        frozen = family.template([])
        frozen["rows"]["invented"] = {"selected": True}
        result = validate(family.registry(), frozen, self.root)
        self.assertTrue(any("unknown case IDs" in error for error in result["global_errors"]))

    def test_huge_root_budget_is_unresolved_and_never_inherits_history_exception(self):
        huge = next(case for case in family.registry()["cases"] if case["id"] == "Q05-root-1000000")
        self.assertIsNone(huge["budgets"]["complete_command_ns"])
        self.assertLess(huge["budgets"]["verification_ns"], 10_000_000_000)

    def test_declared_dense_exceptions_are_only_25_seconds(self):
        catalog = family.registry()
        exceptions = [case["id"] for case in catalog["cases"] if case["budgets"]["exception"]]
        self.assertEqual(exceptions, ["Q04-root-100000", "Q06-native-file-5g"])
        self.assertTrue(all(case["budgets"]["functional_ns"] <= 120_000_000_000 for case in catalog["cases"]))

    def test_calibration_uses_exact_ceiling_arithmetic(self):
        context, limit = self.control()
        self.assertEqual(calibration(limit, self.root, context), 112)

    def test_calibration_rejects_ineligible_or_wrong_profile_control(self):
        context, limit = self.control(cache_status="INELIGIBLE")
        with self.assertRaisesRegex(ValueError, "not eligible"):
            calibration(limit, self.root, context)
        context, limit = self.control()
        context["profile"] = "disposable"
        with self.assertRaisesRegex(ValueError, "mismatch"):
            calibration(limit, self.root, context)

    def test_calibration_rejects_candidate_derived_or_boolean_metrics(self):
        context, limit = self.control(metrics={"operation_ns": True})
        with self.assertRaisesRegex(ValueError, "incomplete"):
            calibration(limit, self.root, context)
        context, limit = self.control()
        limit["candidate_sample_taken"] = True
        with self.assertRaisesRegex(ValueError, "precede"):
            calibration(limit, self.root, context)

    def test_artifact_change_cannot_be_reused(self):
        sealed = self.sealed("input.json", {"value": 1})
        (self.root / "input.json").write_text("changed")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            artifact(sealed, self.root)

    def test_unknown_observation_is_not_zero(self):
        gates = [{"id": "retry", "metric": "retries", "operator": "eq", "value": 0}]
        self.assertEqual(evaluate(gates, {}, {})["status"], "INCOMPLETE")
        self.assertEqual(evaluate(gates, {"retries": None}, {})["status"], "INCOMPLETE")
        self.assertEqual(evaluate(gates, {"retries": False}, {})["status"], "INCOMPLETE")
        self.assertEqual(evaluate(gates, {"retries": 0}, {})["status"], "PASS")
        self.assertEqual(evaluate(gates, {"retries": 1}, {})["status"], "FAIL")

    def test_missing_latency_constant_keeps_real_observation_incomplete(self):
        gate = {"id": "latency", "metric": "operation_ns", "operator": "le", "limit": "operation_ns"}
        self.assertEqual(evaluate([gate], {"operation_ns": 12}, {})["status"], "INCOMPLETE")
        self.assertEqual(evaluate([gate], {"operation_ns": 12}, {"operation_ns": 11})["status"], "FAIL")

    def phase(self):
        return {"scope": "phase", "peak_kind": "sampled", "baseline_bytes": 4,
                "peak_bytes": 10, "final_bytes": 5, "first_ns": 0, "opened_ns": 1,
                "closed_ns": 20, "last_ns": 21, "largest_gap_ns": 11,
                "sample_interval_ns": 10, "samples": 3, "sample_timestamps_ns": [0, 10, 21],
                "sample_byte_values": [4, 10, 5],
                "sample_clock": "local-monotonic", "boundary_clock": "local-monotonic",
                "time_uncertainty_ns": 0}

    def test_phase_observation_preserves_sampled_label(self):
        result = resource_phase(self.phase(), sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)
        self.assertEqual(result["peak_kind"], "sampled")

    def test_sample_inventory_cannot_claim_continuous_peak(self):
        phase = self.phase()
        phase["peak_kind"] = "continuous"
        # The same three samples cannot establish absence of a between-sample peak.
        with self.assertRaisesRegex(ValueError, "continuous"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)

    def test_lifetime_peak_and_missed_phase_boundary_cannot_pass(self):
        phase = self.phase()
        phase["scope"] = "lifetime"
        with self.assertRaisesRegex(ValueError, "lifetime"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)
        phase = self.phase()
        phase["first_ns"] = 2
        phase["sample_timestamps_ns"][0] = 2
        with self.assertRaisesRegex(ValueError, "boundary"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)

    def test_observer_gap_is_an_executable_precision_gate(self):
        with self.assertRaisesRegex(ValueError, "gap"):
            resource_phase(self.phase(), sample_interval_ns=10, maximum_gap_ns=9, clock_error_ns=0)

    def test_two_boundary_samples_do_not_cover_an_interior_phase(self):
        phase = self.phase()
        phase.update(samples=2, sample_timestamps_ns=[0, 21], largest_gap_ns=21)
        with self.assertRaisesRegex(ValueError, "guaranteed interior"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=30, clock_error_ns=0)

    def test_sample_inventory_count_and_gap_must_match(self):
        phase = self.phase()
        phase["samples"] = 4
        with self.assertRaisesRegex(ValueError, "inventory/count"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)
        phase = self.phase()
        phase["largest_gap_ns"] = 10
        with self.assertRaisesRegex(ValueError, "disagree with inventory"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)

    def test_uncertainty_can_remove_a_nominally_interior_sample(self):
        phase = self.phase()
        phase.update(opened_ns=20, closed_ns=80, last_ns=100,
                     sample_timestamps_ns=[0, 22, 100], largest_gap_ns=78,
                     time_uncertainty_ns=5)
        with self.assertRaisesRegex(ValueError, "no guaranteed interior"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=100, clock_error_ns=5)

    def test_unattested_different_clock_cannot_cover_boundaries(self):
        phase = self.phase()
        phase["sample_clock"] = "another-host"
        with self.assertRaisesRegex(ValueError, "attested bounded calibration"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)

    def test_calibrated_clock_requires_exact_conversion_inventory(self):
        phase = self.phase()
        phase.update(sample_clock="source", boundary_clock="target", first_ns=100,
                     opened_ns=101, closed_ns=120, last_ns=121,
                     source_timestamps_ns=[0, 10, 21], sample_timestamps_ns=[100, 110, 121],
                     clock_calibration={"schema": "cluster-two-clock-calibration-v1", "status": "PASS",
                         "source_clock": "source", "target_clock": "target", "offset_ns": 100,
                         "maximum_error_ns": 0, "validated_receipt": True})
        self.assertEqual(resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)["status"], "PASS")
        phase["source_timestamps_ns"][1] = 9
        with self.assertRaisesRegex(ValueError, "conversion mismatch"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)

    def test_distinct_clock_receipt_is_hashed_before_phase_validation(self):
        phase = self.phase()
        phase["sample_clock"] = "source"
        phase["clock_calibration"] = {"schema": "cluster-two-clock-calibration-v1", "status": "PASS",
            "source_clock": "source", "target_clock": "local-monotonic", "offset_ns": 0,
            "maximum_error_ns": 0}
        with self.assertRaisesRegex(ValueError, "artifact"):
            family.phase_witness(phase, self.root)
        phase["calibration_artifact"] = self.sealed("clock.json", phase["clock_calibration"])
        checked = family.phase_witness(phase, self.root)
        self.assertTrue(checked["clock_calibration"]["validated_receipt"])

    def observation(self):
        binding = {"identity": {"source_commit": "1" * 40, "source_tree": "2" * 40,
                               "workload_sha256": "3" * 64, "observer_sha256": "4" * 64,
                               "product_sha256": "9" * 64, "cache_sha256": "a" * 64},
                   "boundary_sha256": "5" * 64, "driver": {"binary": {"sha256": "6" * 64}},
                   "source_arm": "candidate", "sample_id": "8" * 64, "selected": True,
                   "profile": "overlay-memory-off",
                   "attempt_contract": {"scope": "declared-public-operation-attempts", "count": 1},
                   "resident_spread_contract": next(case for case in family.registry()["cases"] if case["id"] == "E01-startup")["resident_spread_contract"]}
        manifest = {"registry_sha256": "7" * 64, "rows": {"E01-startup": binding}}
        record = {"schema": "cluster-two-row-observations-v1", "case": "E01-startup",
                  "registration_sha256": digest_json(manifest), "registration_row_sha256": digest_json(binding),
                  "registry_sha256": manifest["registry_sha256"], "boundary_sha256": binding["boundary_sha256"],
                  **binding["identity"], "binary_sha256": "6" * 64,
                  "source_arm": binding["source_arm"], "sample_id": binding["sample_id"], "sample_count": 1,
                  "metrics": {"retry_attempts": 0, "sample_count": 1, "linux_phase_peak_bytes": 10,
                              "phase_resident_spread_bytes": 0}, "resources": {"linux": self.phase()}}
        receipt = {"schema": "cluster-two-original-attempt-v1", "attempt_id": "b" * 64,
                   "operation_index": 0, "scope": binding["attempt_contract"]["scope"],
                   "sample_count": 1, "attempt_count": 1,
                   **{key: record[key] for key in ("case", "source_arm", "sample_id", "source_commit", "source_tree", "binary_sha256")}}
        record["original_attempts"] = [{"attempt_id": receipt["attempt_id"], "receipt": self.sealed("attempt.json", receipt)}]
        record["raw"] = self.sealed("raw.json", record)
        return manifest, record

    def test_retained_observation_requires_exact_registration_and_raw_artifact(self):
        manifest, record = self.observation()
        self.assertEqual(family.observation_binding(manifest, "E01-startup", record, self.root)["metrics"]["retry_attempts"], 0)
        record["metrics"]["retry_attempts"] = 1
        with self.assertRaisesRegex(ValueError, "differ from sealed raw"):
            family.observation_binding(manifest, "E01-startup", record, self.root)

    def test_unrelated_row_or_workload_identity_is_refused(self):
        manifest, record = self.observation()
        record["workload_sha256"] = "9" * 64
        with self.assertRaisesRegex(ValueError, "do not match admitted"):
            family.observation_binding(manifest, "E01-startup", record, self.root)

    def test_service_classes_require_numerical_arrivals_progress_and_drain(self):
        case = next(case for case in family.registry()["cases"] if case["id"] == "Q01-mixed-load")
        schedule = {"schema": "cluster-two-schedule-v1", "case": case["id"],
                    "horizon_ns": 10_000_000_000, "arrivals": []}
        binding = {"schedule_manifest": self.sealed("schedule.json", schedule),
                   "service_classes": {name: {} for name in case["workload"]["classes"]}}
        with self.assertRaisesRegex(ValueError, "numerical service"):
            _service(binding, case, case["workload"], self.root)

    def test_workload_cannot_reduce_original_write_count(self):
        case = next(case for case in family.registry()["cases"] if case["id"] == "E04-write-16m")
        parameters = copy.deepcopy(case["workload"])
        parameters["writes"] = 999
        binding = {"workload_manifest": self.sealed("workload.json", {
                       "schema": "cluster-two-workload-v1", "case": case["id"], "parameters": parameters}),
                   "fixture_manifest": self.sealed("fixture.json", {})}
        with self.assertRaisesRegex(ValueError, "frozen parameter: writes"):
            _workload(binding, case, self.root)

    def test_write_cohorts_expand_exact_offsets_and_independent_payload_identity(self):
        small, large = write_trace(16_777_216), write_trace(1_073_741_824)
        self.assertEqual(len(small), 1000)
        self.assertEqual(small[1]["offset"], 9_539_584)
        self.assertEqual(large[1]["offset"], 4096 * 104729)
        self.assertEqual(len({row["offset"] for row in small}), 1000)
        self.assertEqual([row["replacement_sha256"] for row in small],
                         [row["replacement_sha256"] for row in large])
        self.assertTrue(all(row["length"] == 4096 and row["offset"] % 4096 == 0 for row in small + large))

    def test_fixture_generator_is_length_exact_and_distinct_without_candidate_output(self):
        a = b"".join(payload_chunks(1, 1, 1025))
        b = b"".join(payload_chunks(1, 2, 1025))
        self.assertEqual(len(a), 1025)
        self.assertNotEqual(a, b)
        self.assertEqual(a, b"".join(payload_chunks(1, 1, 1025)))

    def authority(self, **updates):
        context, _ = self.control()
        entry = {"value": 123, "unit": "ns", "meaning": LIMIT_DEFINITIONS["operation_ns"][1],
                 "scope": numerical_scope(context)}
        entry.update(updates)
        value = {"kind": "contract", "value": 123, "unit": "ns", "anchor": "operation",
                 "authority": self.sealed("authority.json", {
                     "schema": "cluster-two-numerical-authority-v1", "limits": {"operation": entry}})}
        return context, value

    def test_numeric_authority_requires_referenced_structured_value(self):
        context, value = self.authority()
        self.assertEqual(resolve_limit(value, self.root, context), 123)
        value["anchor"] = "nonexistent"
        with self.assertRaisesRegex(ValueError, "key/value/unit/meaning/scope"):
            resolve_limit(value, self.root, context)
        value["authority"] = self.sealed("arbitrary.json", {"comment": "not numerical authority"})
        with self.assertRaisesRegex(ValueError, "owning anchor"):
            resolve_limit(value, self.root, context)

    def test_numeric_authority_wrong_unit_meaning_scope_or_value_is_refused(self):
        for change in ({"unit": "bytes"}, {"meaning": "payload-byte-volume"},
                       {"scope": {"case": "another"}}, {"value": 124}):
            context, value = self.authority(**change)
            with self.assertRaisesRegex(ValueError, "key/value/unit/meaning/scope"):
                resolve_limit(value, self.root, context)

    def test_calibration_cannot_use_bytes_or_unrelated_metric_as_latency(self):
        context, value = self.control()
        entry = {"value": 101, "unit": "bytes", "meaning": LIMIT_DEFINITIONS["operation_ns"][1],
                 "scope": numerical_scope(context)}
        context, value = self.control(metrics={"operation_ns": entry})
        with self.assertRaisesRegex(ValueError, "meaning/unit/scope"):
            calibration(value, self.root, context)
        entry.update(unit="ns", meaning="payload-size-v1")
        context, value = self.control(metrics={"operation_ns": entry})
        with self.assertRaisesRegex(ValueError, "meaning/unit/scope"):
            calibration(value, self.root, context)
        context, value = self.control()
        value["metric"] = "some_other_integer"
        with self.assertRaisesRegex(ValueError, "incomplete"):
            calibration(value, self.root, context)

    def test_oracle_command_must_invoke_its_sealed_binary(self):
        binary = self.sealed("oracle.binary", {"fixture": "binary"})
        source = self.sealed("oracle.source", {"fixture": "source"})
        binding = {"oracle": {"binary": binary, "source": source,
                   "uses_candidate_result": False, "coverage": {"expected_bytes": True},
                   "command": ["/usr/bin/true"]}}
        case = {"oracle": {"required_coverage": ["expected_bytes"]}}
        with self.assertRaisesRegex(ValueError, "does not use sealed binary"):
            _oracle(binding, case, self.root)
        binding["oracle"]["command"] = [str(self.root / binary["path"])]
        _oracle(binding, case, self.root)

    def test_oracle_alias_names_the_same_owned_sealed_executable(self):
        binary = self.sealed("oracle.binary", {"fixture": "binary"})
        source = self.sealed("oracle.source", {"fixture": "source"})
        alias = self.root / "checkout-alias"
        alias.symlink_to(self.root.resolve(), target_is_directory=True)
        case = {"oracle": {"required_coverage": ["expected_bytes"]}}
        binding = {"oracle": {"binary": binary, "source": source,
                   "uses_candidate_result": False, "coverage": {"expected_bytes": True},
                   "command": [str(alias / binary["path"])]}}
        _oracle(binding, case, self.root)
        binding["oracle"]["command"] = ["checkout-alias/" + binary["path"]]
        _oracle(binding, case, self.root)
        binding["oracle"]["command"] = ["/usr/bin/true"]
        with self.assertRaisesRegex(ValueError, "does not use sealed binary"):
            _oracle(binding, case, self.root)

    def test_source_inventory_must_cover_registry_pins_and_current_content(self):
        (self.root / "core/crates").mkdir(parents=True)
        pin = self.sealed("core/crates/selected.rs", {"actual_source": 1})
        product = self.sealed("product.json", {"artifacts": [pin]})
        binding = {"identity": {"source_artifacts": [pin], "product_manifest": product}}
        case = {"route": {"sources": [pin]}, "observers": ["observed"]}
        catalog = {"observed": {"sources": [pin]}}
        _source_coverage(binding, case, catalog, self.root)
        binding["identity"]["source_artifacts"] = []
        with self.assertRaisesRegex(ValueError, "omits/mismatches registry pin"):
            _source_coverage(binding, case, catalog, self.root)
        binding["identity"]["source_artifacts"] = [pin]
        (self.root / pin["path"]).write_text("changed source")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            _source_coverage(binding, case, catalog, self.root)

    def test_unimplemented_observer_cannot_gain_admission_from_arbitrary_source(self):
        (self.root / "core/crates").mkdir(parents=True)
        pin = self.sealed("core/crates/selected.rs", {})
        binding = {"identity": {"source_artifacts": [pin],
                   "product_manifest": self.sealed("product.json", {"artifacts": [pin]})}}
        case = {"route": {"sources": [pin]}, "observers": ["missing"]}
        with self.assertRaisesRegex(ValueError, "no implemented pinned source"):
            _source_coverage(binding, case, {"missing": {"sources": []}}, self.root)

    def test_native_init_four_constructors_are_independent_of_environment_one(self):
        catalog = family.registry()
        native = next(case for case in catalog["cases"] if case["id"] == "Q06-native-file-5g")
        binding = {"identity": {"environment": {"LAYERFS_CONSTRUCTION_WORKERS": "1"},
                   "construction_profile": copy.deepcopy(native["construction_profile"])}}
        _construction(binding, native)
        self.assertEqual(native["construction_profile"]["constructor_worker_limit"], 4)
        binding["identity"]["construction_profile"]["constructor_worker_limit"] = 1
        with self.assertRaisesRegex(ValueError, "native Init retains four"):
            _construction(binding, native)
        ordinary = next(case for case in catalog["cases"] if case["id"] == "E04-write-16m")
        binding["identity"]["construction_profile"] = copy.deepcopy(native["construction_profile"])
        with self.assertRaisesRegex(ValueError, "constructor profile mismatched"):
            _construction(binding, ordinary)

    def test_gated_peak_cannot_disagree_with_checked_phase_witness(self):
        case = next(case for case in family.registry()["cases"] if case["id"] == "E01-startup")
        raw = {"metrics": {"linux_phase_peak_bytes": 0}}
        phases = {"linux": {"status": "PASS", "peak_bytes": 1000000}}
        with self.assertRaisesRegex(ValueError, "phase peak disagrees"):
            family.checked_resource_metrics(case, raw, phases)
        raw["metrics"]["linux_phase_peak_bytes"] = 1000000
        self.assertEqual(family.checked_resource_metrics(case, raw, phases)["linux_phase_peak_bytes"], 1000000)

    def reseal(self, record):
        record["raw"] = self.sealed("raw.json", {key: value for key, value in record.items() if key != "raw"})

    def test_retained_aggregate_sample_count_and_wrong_source_arm_are_refused(self):
        manifest, record = self.observation()
        record["sample_count"] = 2
        self.reseal(record)
        with self.assertRaisesRegex(ValueError, "exactly one source-arm sample"):
            family.observation_binding(manifest, "E01-startup", record, self.root)
        manifest, record = self.observation()
        record["source_arm"] = "baseline"
        self.reseal(record)
        with self.assertRaisesRegex(ValueError, "do not match admitted"):
            family.observation_binding(manifest, "E01-startup", record, self.root)

    def test_duplicate_original_attempt_or_resampled_receipt_is_refused(self):
        manifest, record = self.observation()
        binding = manifest["rows"]["E01-startup"]
        binding["attempt_contract"]["count"] = 2
        record["original_attempts"].append(copy.deepcopy(record["original_attempts"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate/missing original"):
            family.original_attempts(binding, record, self.root)
        manifest, record = self.observation()
        entry = record["original_attempts"][0]
        receipt = json.loads((self.root / entry["receipt"]["path"]).read_text())
        receipt["sample_count"] = 2
        entry["receipt"] = self.sealed("resampled.json", receipt)
        with self.assertRaisesRegex(ValueError, "single source-arm sample/attempt"):
            family.original_attempts(manifest["rows"]["E01-startup"], record, self.root)

    def test_resident_spread_single_operand_is_derived_and_missing_cohort_refused(self):
        manifest, record = self.observation()
        case = next(case for case in family.registry()["cases"] if case["id"] == "E01-startup")
        observed = {"rows": {case["id"]: record}}
        admitted = {case["id"]: {"registration_status": "PASS", "resolved_limits": {"observer_interval_ns": 10, "observer_gap_ns": 11, "clock_error_ns": 0}}}
        spread = family.resident_spread(manifest, observed, case, self.root, admitted)
        self.assertEqual(spread["value"], 0)
        self.assertEqual(spread["operands"], [{"case": "E01-startup", "aggregate_phase_peak_bytes": 10}])
        self.assertTrue(spread["singleton"])
        cohort = next(case for case in family.registry()["cases"] if case["id"] == "E04-write-16m")
        with self.assertRaisesRegex(ValueError, "operand unselected"):
            family.resident_spread(manifest, observed, cohort, self.root, admitted)

    def test_spread_refuses_incomplete_operand_despite_valid_numeric_limits(self):
        manifest, record = self.observation()
        case = next(case for case in family.registry()["cases"] if case["id"] == "E01-startup")
        observed = {"rows": {case["id"]: record}}
        for cause in ("route proof failed", "cache registration incomplete"):
            admitted = {case["id"]: {"registration_status": "INCOMPLETE", "errors": [cause],
                "resolved_limits": {"observer_interval_ns": 10, "observer_gap_ns": 11, "clock_error_ns": 0}}}
            with self.assertRaisesRegex(ValueError, "operand registration is not PASS"):
                family.resident_spread(manifest, observed, case, self.root, admitted)

    def test_retained_peak_gate_cannot_pass_a_zero_claim_with_million_byte_witness(self):
        manifest, record = self.observation()
        record["metrics"]["linux_phase_peak_bytes"] = 0
        record["resources"]["linux"]["peak_bytes"] = 1000000
        record["resources"]["linux"]["sample_byte_values"][1] = 1000000
        self.reseal(record)
        observations = {"schema": "cluster-two-observations-v1",
            "registration_sha256": digest_json(manifest), "registry_sha256": manifest["registry_sha256"],
            "rows": {"E01-startup": record}}
        admitted = {"status": "PASS", "rows": [{"case": "E01-startup", "registration_status": "PASS",
            "resolved_limits": {"observer_interval_ns": 10, "observer_gap_ns": 11, "clock_error_ns": 0},
            "gates": [{"id": "memory", "metric": "linux_phase_peak_bytes", "operator": "le", "value": 100}]}]}
        # Registration has separate negative tests; this isolates the reviewed
        # resource-binding defect in the retained evaluator, with sealed inputs.
        with mock.patch.object(family, "admission", return_value=admitted):
            result = family.retained_gates(manifest, observations, self.root)
        self.assertEqual(result["retained_numeric_status"], "FAIL")
        self.assertEqual(result["rows"][0]["gate_evaluation"]["gates"][0]["observed"], 1000000)
        self.assertTrue(any("phase peak disagrees" in error for error in result["rows"][0]["observation_errors"]))

    def test_failed_or_unattempted_receipts_cannot_be_described_as_qualified_evidence(self):
        for outcome in ("failed", "unattempted"):
            manifest, record = self.observation()
            receipt = json.loads((self.root / record["original_attempts"][0]["receipt"]["path"]).read_text())
            receipt["outcome"] = outcome
            record["original_attempts"][0]["receipt"] = self.sealed(outcome + ".json", receipt)
            self.reseal(record)
            observations = {"schema": "cluster-two-observations-v1",
                "registration_sha256": digest_json(manifest), "registry_sha256": manifest["registry_sha256"],
                "rows": {"E01-startup": record}}
            admitted = {"status": "PASS", "measurement_status": "NOT_RUN", "rows": [{
                "case": "E01-startup", "registration_status": "PASS",
                "resolved_limits": {"observer_interval_ns": 10, "observer_gap_ns": 11, "clock_error_ns": 0},
                "gates": [{"id": "retry", "metric": "retry_attempts", "operator": "eq", "value": 0}]}]}
            with mock.patch.object(family, "admission", return_value=admitted):
                result = family.retained_gates(manifest, observations, self.root)
            self.assertEqual(result["retained_numeric_status"], "PASS")
            self.assertEqual(result["qualification_status"], "NOT_EVALUATED")
            self.assertEqual(result["rows"][0]["numeric_gate_status"], "PASS")
            self.assertEqual(result["rows"][0]["qualification_status"], "NOT_EVALUATED")
            self.assertNotIn("evidence_admission", result["rows"][0])
            self.assertNotIn("retained_evidence_status", result)
            self.assertNotIn("status", result)

    def test_phase_peak_is_derived_from_byte_inventory(self):
        phase = self.phase()
        phase["sample_byte_values"][1] = 1000000
        with self.assertRaisesRegex(ValueError, "disagree with actual byte inventory"):
            resource_phase(phase, sample_interval_ns=10, maximum_gap_ns=11, clock_error_ns=0)


if __name__ == "__main__":
    unittest.main()
