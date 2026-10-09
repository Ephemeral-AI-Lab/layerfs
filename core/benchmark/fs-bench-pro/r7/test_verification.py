"""External sealed-recipe checks; no product, Docker or measurement execution."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import deployment, git_index_oracle as index, git_queries, registry, registry_variant, verification, workloads


class RecipeContract(unittest.TestCase):
    def test_expired_stdout_proof_does_not_launch_or_create_outputs(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-expired-", dir="/tmp") as folder:
            output = Path(folder) / "proof"
            with mock.patch.object(verification.time, "monotonic", return_value=100):
                with mock.patch.object(verification.subprocess, "run") as launched:
                    with self.assertRaisesRegex(TimeoutError, "not launched"):
                        verification.compare_stdout({}, {}, output, deadline=99)
            launched.assert_not_called()
            self.assertEqual(list(Path(folder).iterdir()), [])

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="layerfs-r7-recipe-test-", dir="/tmp")
        self.root = Path(self.temporary.name).resolve()
        self.bundle = self.root / "bundle"
        self.bundle.mkdir()
        self.store = self.root / "store"
        self.store.write_bytes(b"sealed store")
        self.manifest = self.root / "installed"
        self.manifest.write_bytes(b"sealed installed manifest")
        self.source = self.root / "base"
        self.source.mkdir()
        self.base = deployment.seal_tree(self.source, self.root / "base.jsonl")
        self.config = dict(oracle_recipe=dict(bundle=str(self.bundle)),
                           identities=dict(image_id=registry.IMAGE), uid=501, gid=20,
                           prepared_store_file=str(self.store), manifest=str(self.manifest),
                           base_fixture_inventory=dict(path=self.base["inventory"], sha256=self.base["sha256"],
                               content_metadata_set_sha256=self.base["content_metadata_set_sha256"]))
        self.author = verification.canonical_author()
        self.preparations = 0

    def tearDown(self):
        self.temporary.cleanup()

    def git_data(self, label, case="E04"):
        directory = self.bundle / "git-queries"
        directory.mkdir(exist_ok=True)
        effective = {key: git_queries.DEFAULTS[key] for key in index.EFFECTIVE_KEYS}
        policy = dict(schema="r7-git-default-policy-v1", defaults=git_queries.DEFAULTS,
                      version="git version 2.39.5", git_binary_sha256="a" * 64,
                      primary_sources=[dict(url="https://raw.githubusercontent.com/git/git/v2.39.5/config.c", sha256="b" * 64)],
                      review_receipt_sha256="c" * 64)
        rows, provenance = [], {}
        labels = ["version", "object-format", *[key.replace(".", "-") for key in [*index.EFFECTIVE_KEYS, "feature.manyfiles"]]]
        for position, name in enumerate(labels):
            key = None if position < 2 else [*index.EFFECTIVE_KEYS, "feature.manyfiles"][position-2]
            argv = ["/usr/bin/git", "--version"] if position == 0 else ["/usr/bin/git", *git_queries.query_override(case)]
            if position == 1:
                argv.extend(["rev-parse", "--show-object-format"])
            elif key is not None:
                argv.extend(["config", "--get"])
                if key in git_queries.BOOLS:
                    argv.append("--type=bool")
                argv.append(key)
            data = b"git version 2.39.5\n" if position == 0 else b"sha1\n" if position == 1 else b"false\n" if key == "core.fsmonitor" else b""
            code = 0 if position < 2 or key == "core.fsmonitor" else 1
            if key is not None:
                provenance[key] = dict(original="ABSENT" if code else "false", exit_code=code)
                if code:
                    provenance[key]["qualified_literal_default"] = policy["defaults"][key]
            stdout, stderr = directory / (name + ".stdout"), directory / (name + ".stderr")
            stdout.write_bytes(data)
            stderr.write_bytes(b"")
            rows.append(dict(label=name, argv=argv, cwd="/native-primary", attempts=1, pid=1000+position,
                             status="ORIGINAL_COMPLETED", exit_code=code,
                             stdout="/tmp/layerfs-r7-query/"+stdout.name, stderr="/tmp/layerfs-r7-query/"+stderr.name,
                             stdout_sha256=verification.sha(stdout), stderr_sha256=verification.sha(stderr)))
        queries = dict(schema=git_queries.SCHEMA, status="OBSERVED", case_id=case, root="/native-primary", uid=501, gid=20,
                       queries=rows, environment=workloads.ENV, qualified_policy=policy, effective_config=effective,
                       actual_git_binary="/usr/bin/git", actual_git_binary_sha256="a" * 64,
                       actual_git_version=policy["version"], actual_object_format="sha1", command_override={"core.fsmonitor":"false"} if git_queries.query_override(case) else {},
                       original_values=provenance, wall_stop_seconds=git_queries.WALL_SECONDS)
        query_path = directory / "queries.json"
        query_path.write_text(json.dumps(queries))
        pin = dict(schema=index.PIN_SCHEMA, object_format="sha1", index_versions=[2,3],
                   git=dict(binary="/usr/bin/git", sha256="a"*64, version=policy["version"], version_receipt_sha256=rows[0]["stdout_sha256"]),
                   config_sha256="f"*64, worktree_config_sha256=None, effective_config=effective,
                   effective_config_sha256=index.sha256(index.canonical(effective)), effective_config_receipt_sha256=verification.sha(query_path))
        pin_path = directory / "pin.json"
        pin_path.write_text(json.dumps(pin))
        policy_path = self.bundle / "git-default-policy.json"
        policy_path.write_text(json.dumps(policy))
        metadata = dict(kind="regular", mode=0o644, uid=501, gid=20, size=32, nlink=1, device=1, inode=2)
        expected = self.bundle / (label + ".jsonl")
        expected.write_text(json.dumps(dict(path=".git/index", sha256="d"*64, **metadata))+"\n")
        observation = dict(schema=index.SCHEMA, status="OBSERVED", root="/native-primary", pin=pin,
                           pin_sha256=index.sha256(index.canonical(pin)), excluded_cross_filesystem_fields=list(index.STAT_FIELDS),
                           checksum=dict(verified=True), semantic=dict(version=2, object_format="sha1", entry_count=0, entries=[], extensions=[]),
                           filesystem_metadata=metadata, raw_index_sha256="d"*64, raw_index_bytes=32)
        observed = self.bundle / (label + ".index.json")
        observed.write_text(json.dumps(observation))
        context = dict(oracle_schema=index.SCHEMA, actual_pin=pin, pin="git-queries/pin.json", pin_file_sha256=verification.sha(pin_path),
                       queries="git-queries/queries.json", queries_sha256=verification.sha(query_path),
                       policy=policy_path.name, policy_sha256=verification.sha(policy_path),
                       query_position="after original canonical body" if case == "C12" else "before original canonical body")
        operand = dict(manifest=observed.name, manifest_sha256=verification.sha(observed), tree_sha256=verification.sha(expected),
                       pin=context["pin"], pin_sha256=context["pin_file_sha256"], queries=context["queries"], queries_sha256=context["queries_sha256"],
                       policy=context["policy"], policy_sha256=context["policy_sha256"])
        return context, operand

    def recipe(self, case="C01", arm="L", cache="B"):
        selected = next(item for item in registry.cases() if item["case_id"] == case)
        asset = "/code/oracles/closed"
        rows = []
        for label, role in verification.coverage(selected):
            observer = self.author.observer_for(case, role)
            expected = self.bundle / (label + ".jsonl")
            expected.write_text("{}\n")
            git_operand = None
            if case in verification.GIT_CASES:
                context, git_operand = self.git_data(label, case)
                if case == "E11":
                    tracked = self.bundle / "tracked-paths.json"
                    tracked.write_text('["packages/one.txt", "packages/two.txt"]')
                    git_operand.update(tracked="/code/tracked-paths.json",tracked_sha256=verification.sha(tracked))
            script = self.bundle / (label + ".verify.sh")
            script.write_text(self.author.verifier_body(asset, observer, label, git_operand))
            rows.append(dict(label=label, role=role, manifest=expected.name, sha256=verification.sha(expected),
                             observer=observer, verifier=dict(path=asset + "/" + script.name,
                                 sha256=verification.sha(script), performs_comparison=True, wall_stop_ns=9_500_000_000,
                                 root="original selected mounted cwd", expected=expected.name)))
            if git_operand is not None:
                rows[-1]["git_index"] = git_operand
        stdout = self.bundle / "stdout"
        stdout.write_bytes(b"")
        fixture = dict(prepared_store_sha256=verification.sha(self.store),
                       installed_manifest_sha256=verification.sha(self.manifest))
        if case == "C12":
            fixture["fixture_kind"] = "empty"
        roles = ["scan", "writer"] if case == "W01" else ["primary"]
        native = [dict(path="/native-" + role, role=role, inventory=self.base["inventory"],
                       inventory_sha256=self.base["sha256"], content_metadata_set_sha256=self.base["content_metadata_set_sha256"])
                  for role in roles]
        value = dict(schema="r7-closed-oracle-author-v1", status="CLOSED_EXPECTED_SETUP_ONLY", case_id=case,
                     cache_class=cache, fixture_identity=fixture, environment=workloads.ENV,
                     image_id=registry.IMAGE, uid=501, gid=20, verifier_wall_stop_ns=9_500_000_000,
                     expected=rows, asset_root=asset, helpers=verification.canonical_helpers(case),
                     registry_sha256=verification.sha(registry.__file__), workload_source_sha256=verification.sha(workloads.__file__),
                     selected_workload=selected["workload"], inputs=dict(fixture_identity=fixture, roots=native, external=[], code_assets={}),
                     native_verifications=[dict(root=row["path"], inventory_sha256=row["inventory_sha256"],
                                                content_metadata_set_sha256=row["content_metadata_set_sha256"]) for row in native],
                     host_stdout_recipe=dict(required=True, kind="unordered-complete-lines" if case.startswith("W") else "exact-file-sha256",
                         expected_asset=asset + "/stdout", expected_bundle_relative="stdout", expected_sha256=verification.sha(stdout)))
        if case in verification.GIT_CASES:
            value.update(git_context=context, oracle_variant=registry_variant.NAME, registry_variant_identity=registry_variant.identity(),
                         registry_variant_source_sha256=verification.sha(registry_variant.__file__))
            value["inputs"].update(git_default_policy=dict(path="/code/git-policy.json",sha256=context["policy_sha256"]),
                                   code_assets={"git-policy.json":context["policy_sha256"]})
            if case == "E11":
                value["inputs"]["code_assets"]["tracked-paths.json"] = git_operand["tracked_sha256"]
            self.config.update(oracle_variant=registry_variant.NAME, git_index_oracle=dict(helper_sha256=value["helpers"]["git_index_oracle.py"],
                object_format="sha1", index_version=2, effective_config_sha256=context["actual_pin"]["effective_config_sha256"],
                git_binary_sha256="a"*64, git_version=context["actual_pin"]["git"]["version"]))
        self.config["oracle_fixture_identity"] = fixture
        self.selection = dict(case_id=case, arm=arm, cache_class=cache)
        return value

    def load(self, value):
        closed = self.bundle / "closed.json"
        closed.write_text(json.dumps(value))
        self.config["oracle_recipe"]["closed_sha256"] = verification.sha(closed)
        self.preparations += 1
        code = self.root / ("code-" + str(self.preparations))
        code.mkdir()
        for name, source in verification.helper_paths(value["case_id"]).items():
            (code / name).write_bytes(source.read_bytes())
        prefix = value["asset_root"][len("/code/"):]
        for row in value["expected"]:
            for name in (row["manifest"], Path(row["verifier"]["path"]).name):
                target = code / prefix / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((self.bundle / name).read_bytes())
        if value.get("git_context"):
            for source in (self.bundle / "git-queries").iterdir():
                target = code / prefix / "git-queries" / source.name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(source.read_bytes())
            for row in value["expected"]:
                for name in (row["git_index"]["manifest"], row["git_index"]["policy"]):
                    (code / prefix / name).write_bytes((self.bundle / name).read_bytes())
            (code / "git-policy.json").write_bytes((self.bundle / "git-default-policy.json").read_bytes())
            if value["case_id"] == "E11":
                (code / "tracked-paths.json").write_bytes((self.bundle / "tracked-paths.json").read_bytes())
        stdout = value["host_stdout_recipe"]["expected_bundle_relative"]
        target = code / prefix / stdout
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((self.bundle / stdout).read_bytes())
        sealed = deployment.seal_tree(code, self.root / ("code-" + str(self.preparations) + ".jsonl"))
        self.prepared = dict(assets=[dict(target="/code", inventory=sealed["inventory"], inventory_sha256=sealed["sha256"],
                                        content_metadata_set_sha256=sealed["content_metadata_set_sha256"])], native=[])
        if self.selection["arm"] != "L":
            self.prepared["native"] = [dict(target=row["path"], inventory=self.base["inventory"], inventory_sha256=self.base["sha256"],
                                            content_metadata_set_sha256=self.base["content_metadata_set_sha256"])
                                       for row in value["inputs"]["roots"]]
        with mock.patch.object(deployment, "load_plan", return_value=self.prepared):
            return verification.load(self.config, self.selection)

    def test_matching_closed_recipe_and_stdout_only_coverage(self):
        self.assertIsNotNone(self.load(self.recipe()))
        self.assertEqual(verification.coverage(next(row for row in registry.cases() if row["case_id"] == "E05")), [])

    def test_git_recipe_closes_seven_helpers_and_nested_query_pin_policy_operands(self):
        plan = self.load(self.recipe(case="E04", cache="A"))
        self.assertEqual(len(plan["value"]["helpers"]), 7)
        wanted = verification.required_deployment_hashes(plan)
        for name in ("git_queries.py", "workloads.py", "oracles/closed/expected.index.json",
                     "oracles/closed/git-queries/pin.json", "oracles/closed/git-queries/queries.json",
                     "oracles/closed/git-default-policy.json", "oracles/closed/git-queries/version.stdout"):
            self.assertIn(name, wanted)
        self.assertEqual(plan["git"]["pin"]["effective_config"]["core.untrackedcache"], "keep")

    def test_git_variant_context_and_reference_stream_mutations_refuse(self):
        for field, replacement in (("oracle_variant", "other"), ("registry_variant_identity", "0"*64),
                                   ("registry_variant_source_sha256", "0"*64)):
            value = self.recipe(case="E04", cache="A")
            value[field] = replacement
            with self.assertRaisesRegex(ValueError, "variant"):
                self.load(value)
        value = self.recipe(case="E04", cache="A")
        (self.bundle / "git-queries/version.stdout").write_bytes(b"different original version\n")
        with self.assertRaisesRegex(ValueError, "query stream"):
            self.load(value)

    def test_git_expected_index_pin_and_tree_binding_are_not_arbitrary_metadata(self):
        value = self.recipe(case="E04", cache="A")
        row = value["expected"][0]
        row["git_index"]["pin_sha256"] = "0"*64
        with self.assertRaisesRegex(ValueError, "paired context"):
            self.load(value)
        value = self.recipe(case="E04", cache="A")
        row = value["expected"][0]
        path = self.bundle / row["git_index"]["manifest"]
        observed = json.loads(path.read_text())
        observed["raw_index_sha256"] = "e"*64
        path.write_text(json.dumps(observed))
        row["git_index"]["manifest_sha256"] = verification.sha(path)
        script = self.bundle / "expected.verify.sh"
        script.write_text(self.author.verifier_body(value["asset_root"], row["observer"], "expected", row["git_index"]))
        row["verifier"]["sha256"] = verification.sha(script)
        with self.assertRaisesRegex(ValueError, "raw metadata binding"):
            self.load(value)

    def test_e18_class_c_is_explicitly_unavailable_even_with_closed_recipe(self):
        with self.assertRaisesRegex(ValueError, "unavailable"):
            self.load(self.recipe(case="E18", cache="C"))

    def test_mutable_git_and_post_init_recipes_close_actual_case_context(self):
        for case in ("E10", "E11", "C12"):
            plan = self.load(self.recipe(case=case,cache="B"))
            self.assertEqual(plan["git"]["queries"]["command_override"],{})
            self.assertEqual(plan["value"]["git_context"]["query_position"],
                             "after original canonical body" if case == "C12" else "before original canonical body")
            if case == "E11":
                self.assertIn("tracked-paths.json",verification.required_deployment_hashes(plan))

    def test_c12_nonrepeatable_warmup_and_nonempty_base_inventory_refuse(self):
        with self.assertRaisesRegex(ValueError,"unavailable"):
            self.load(self.recipe(case="C12",cache="C"))
        value = self.recipe(case="C12",cache="B")
        (self.source / "unexpected").write_bytes(b"not empty")
        self.base = deployment.seal_tree(self.source,self.root / "nonempty.jsonl")
        self.config["base_fixture_inventory"] = dict(path=self.base["inventory"],sha256=self.base["sha256"],
                                                     content_metadata_set_sha256=self.base["content_metadata_set_sha256"])
        value["inputs"]["roots"][0].update(inventory=self.base["inventory"],inventory_sha256=self.base["sha256"],
                                           content_metadata_set_sha256=self.base["content_metadata_set_sha256"])
        value["native_verifications"][0].update(inventory_sha256=self.base["sha256"],content_metadata_set_sha256=self.base["content_metadata_set_sha256"])
        with self.assertRaisesRegex(ValueError,"not exactly empty"):
            self.load(value)

    def test_mutable_query_position_and_fsmonitor_override_cannot_be_forged(self):
        value = self.recipe(case="C12",cache="B")
        value["git_context"]["query_position"] = "before original canonical body"
        with self.assertRaisesRegex(ValueError,"query position"):
            self.load(value)
        value = self.recipe(case="E10",cache="B")
        context = value["git_context"]
        path = self.bundle / context["queries"]
        queries = json.loads(path.read_text())
        queries["command_override"] = {"core.fsmonitor":"false"}
        path.write_text(json.dumps(queries))
        context["queries_sha256"] = verification.sha(path)
        context["actual_pin"]["effective_config_receipt_sha256"] = context["queries_sha256"]
        pin = self.bundle / context["pin"]
        pin.write_text(json.dumps(context["actual_pin"]))
        context["pin_file_sha256"] = verification.sha(pin)
        with self.assertRaisesRegex(ValueError,"query context"):
            self.load(value)

    def test_e11_tracked_body_and_selected_static_asset_are_paired(self):
        value = self.recipe(case="E11",cache="B")
        row = value["expected"][0]
        row["git_index"]["tracked_sha256"] = "0"*64
        script = self.bundle / "expected.verify.sh"
        script.write_text(self.author.verifier_body(value["asset_root"],row["observer"],row["label"],row["git_index"]))
        row["verifier"]["sha256"] = verification.sha(script)
        with self.assertRaisesRegex(ValueError,"tracked semantic input"):
            self.load(value)

    def test_e11_result_requires_fifth_tracked_operand(self):
        plan = self.load(self.recipe(case="E11",cache="B"))
        row, output = plan["value"]["expected"][0], self.root / "tracked-result.json"
        operand = row["git_index"]
        result = dict(schema=git_queries.COMPARISON_SCHEMA,status="PASS",case_id="E11",oracle_schema=index.SCHEMA,differences=[],
            expected_operands=dict(tree_sha256=operand["tree_sha256"],index_sha256=operand["manifest_sha256"],
                pin_file_sha256=operand["pin_sha256"],policy_file_sha256=operand["policy_sha256"],tracked_sha256=operand["tracked_sha256"]),
            paired_comparison_pin_sha256=index.sha256(index.canonical(plan["git"]["pin"])))
        event = dict(event="verify",fields=dict(exit_code="0",registered_execs="0",stdout=str(output),command_sha256=row["verifier"]["sha256"]))
        output.write_text(json.dumps(result))
        verification.compare_event(plan,row,event)
        del result["expected_operands"]["tracked_sha256"]
        output.write_text(json.dumps(result))
        with self.assertRaisesRegex(ValueError,"operands/paired pin"):
            verification.compare_event(plan,row,event)

    def test_fabricated_mutable_class_a_is_refused_before_bundle_acceptance(self):
        for case in ("E10","E11","C12"):
            with self.assertRaisesRegex(ValueError,"cache class is not registered"):
                self.load(self.recipe(case=case,cache="A"))

    def test_git_result_requires_original_script_every_operand_and_shared_pin(self):
        plan = self.load(self.recipe(case="E18", cache="A"))
        row = plan["value"]["expected"][0]
        operand = row["git_index"]
        result = dict(schema=git_queries.COMPARISON_SCHEMA, status="PASS", case_id="E18", oracle_schema=index.SCHEMA,
                      differences=[], expected_operands=dict(tree_sha256=operand["tree_sha256"], index_sha256=operand["manifest_sha256"],
                          pin_file_sha256=operand["pin_sha256"], policy_file_sha256=operand["policy_sha256"]),
                      paired_comparison_pin_sha256=index.sha256(index.canonical(plan["git"]["pin"])))
        output = self.root / "actual.git.result.json"
        event = dict(event="verify", fields=dict(exit_code="0",registered_execs="0",stdout=str(output),command_sha256=row["verifier"]["sha256"]))
        output.write_text(json.dumps(result))
        verification.compare_event(plan, row, event)
        for field, replacement in (("oracle_schema", "other"), ("paired_comparison_pin_sha256", "0"*64), ("expected_operands", {})):
            output.write_text(json.dumps(dict(result, **{field: replacement})))
            with self.assertRaisesRegex(ValueError, "operands/paired pin"):
                verification.compare_event(plan, row, event)

    def test_matched_native_roots_and_changed_role_or_base_set(self):
        value = self.recipe(arm="N")
        self.assertIsNotNone(self.load(value))
        plan = copy.deepcopy(self.prepared)
        plan["native"][0]["target"] = "/different-root"
        with self.assertRaisesRegex(ValueError, "native root role"):
            verification.bind_inputs(value, self.config, self.selection, plan)
        value["inputs"]["roots"][0]["role"] = "writer"
        with self.assertRaisesRegex(ValueError, "role coverage"):
            verification.bind_inputs(value, self.config, self.selection, self.prepared)

    def test_l_base_inventory_cannot_be_replaced_by_fixture_name(self):
        value = self.recipe()
        self.load(value)
        value["inputs"]["roots"][0]["content_metadata_set_sha256"] = "0" * 64
        value["native_verifications"][0]["content_metadata_set_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "L base fixture"):
            verification.bind_inputs(value, self.config, self.selection, self.prepared)

    def test_stdout_only_recipe_really_has_zero_tree_rows(self):
        value = self.recipe(case="E05")
        self.assertEqual(self.load(value)["value"]["expected"], [])

    def test_required_tree_cannot_be_omitted_or_duplicated(self):
        value = self.recipe()
        for rows in ([], value["expected"] * 2):
            with self.assertRaisesRegex(ValueError, "coverage"):
                self.load(dict(value, expected=rows))

    def test_checkpoint_and_roles_are_exact(self):
        for case, count in (("K04", 5), ("W01", 2)):
            selected = next(row for row in registry.cases() if row["case_id"] == case)
            self.assertEqual(len(verification.coverage(selected)), count)
        value = self.recipe()
        value["expected"][0]["observer"]["compare_case"] = "E02"
        with self.assertRaisesRegex(ValueError, "observer"):
            self.load(value)

    def test_changed_environment_workload_helpers_and_sources_refuse(self):
        value = self.recipe()
        for key, replacement in (("environment", {}), ("selected_workload", {}),
                                 ("helpers", {}), ("registry_sha256", "0" * 64),
                                 ("workload_source_sha256", "0" * 64)):
            with self.assertRaises(ValueError):
                self.load(dict(value, **{key: replacement}))

    def test_script_path_body_and_leaf_symlink_refuse(self):
        value = self.recipe()
        other = copy.deepcopy(value)
        other["expected"][0]["verifier"]["path"] = "/code/other/expected.verify.sh"
        with self.assertRaisesRegex(ValueError, "script path"):
            self.load(other)
        script = self.bundle / "expected.verify.sh"
        script.write_text("printf fake-PASS\n")
        value["expected"][0]["verifier"]["sha256"] = verification.sha(script)
        with self.assertRaisesRegex(ValueError, "canonical comparison body"):
            self.load(value)
        script.unlink()
        script.symlink_to(self.store)
        with self.assertRaisesRegex(ValueError, "symlink"):
            verification.member(self.bundle, script.name)

    def test_bundle_root_symlink_refuses_before_recipe_read(self):
        value = self.recipe()
        self.load(value)
        alias = self.root / "bundle-alias"
        alias.symlink_to(self.bundle, target_is_directory=True)
        self.config["oracle_recipe"]["bundle"] = str(alias)
        with self.assertRaisesRegex(ValueError, "symlink"):
            verification.load(self.config, self.selection)

    def test_large_commit_requires_node_roots_and_replay_operands(self):
        for case in ("K02", "K03"):
            value = self.recipe(case, cache=None)
            prepared = {"assets": [], "native": []}
            with self.assertRaisesRegex(ValueError, "semantic code-data"):
                verification.bind_inputs(value, self.config, self.selection, prepared)
            value["inputs"]["code_assets"] = {"node-roots.json": "0" * 64}
            with self.assertRaisesRegex(ValueError, "replay input"):
                verification.bind_inputs(value, self.config, self.selection, prepared)

    def test_fixture_requires_actual_store_and_manifest(self):
        value = self.recipe()
        self.store.write_bytes(b"different store")
        with self.assertRaisesRegex(ValueError, "prepared Store"):
            self.load(value)

    def test_deployment_expected_and_helper_bytes_must_match_closed_recipe(self):
        plan = self.load(self.recipe())
        wanted = verification.required_deployment_hashes(plan)
        with mock.patch.object(deployment, "load_plan", return_value=self.prepared):
            staged = dict(schema="r7-container-staging-v1", status="PASS", helper_sha256=wanted)
            verification.bind_deployment(plan, self.config, self.selection, staged)
            for name in ("oracle.py", "oracles/closed/expected.jsonl"):
                altered = dict(wanted, **{name: "0" * 64})
                with self.assertRaisesRegex(ValueError, "actual staged"):
                    verification.bind_deployment(plan, self.config, self.selection, dict(staged, helper_sha256=altered))
        code = self.root / "code-1"
        (code / "oracles/closed/expected.jsonl").write_bytes(b"different expected\n")
        changed = deployment.seal_tree(code, self.root / "changed-code.jsonl")
        item = self.prepared["assets"][0]
        item.update(inventory=changed["inventory"], inventory_sha256=changed["sha256"],
                    content_metadata_set_sha256=changed["content_metadata_set_sha256"])
        with mock.patch.object(deployment, "load_plan", return_value=self.prepared):
            with self.assertRaisesRegex(ValueError, "sealed deployed"):
                verification.bind_deployment(plan, self.config, self.selection)

    def test_original_result_requires_matching_executed_script_and_comparison(self):
        plan = self.load(self.recipe())
        row = plan["value"]["expected"][0]
        stdout = self.root / "result"
        result = dict(schema="r7-scoped-oracle-comparison-v1", case_id="C01", status="PASS", differences=[])
        stdout.write_text(json.dumps(result))
        event = dict(event="verify", fields=dict(exit_code="0", registered_execs="0", stdout=str(stdout), command_sha256=row["verifier"]["sha256"]))
        verification.compare_event(plan, row, event)
        for name, wrong in (("exit_code", "1"), ("registered_execs", "1")):
            with self.assertRaisesRegex(ValueError, "known-zero"):
                verification.compare_event(plan, row, dict(event, fields=dict(event["fields"], **{name: wrong})))
        event["fields"]["command_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "executed verifier"):
            verification.compare_event(plan, row, event)
        event["fields"]["command_sha256"] = row["verifier"]["sha256"]
        stdout.write_text(json.dumps(dict(result, status="FAIL")))
        with self.assertRaisesRegex(ValueError, "comparison refused"):
            verification.compare_event(plan, row, event)


class SelectedComparisonCustody(unittest.TestCase):
    def stream(self, closing=None):
        stream = mock.MagicMock()
        if closing:
            stream.close.side_effect = OSError(closing)
        return stream

    def host_plan(self):
        return dict(closed=Path("closed"), root=Path("bundle"), sha256="sealed",
                    value=dict(helpers={"oracle_prepare.py": "sealed"},
                               host_stdout_recipe=dict(expected_sha256="expected")))

    def test_inventory_read_error_preserves_original_and_prior_close_failures(self):
        original = OSError("original inventory read")
        original.independent_close_failures = ["prior close"]
        stream = self.stream("independent inventory close")
        stream.readline.side_effect = original
        with mock.patch.object(verification, "sha", return_value="sealed"), \
                mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(OSError) as caught:
                verification.tree_inventory(Path("inventory"), "sealed", "set")
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "comparison_inventory_read")
        self.assertEqual(original.independent_close_failures, ["prior close", "independent inventory close"])
        stream.readline.assert_called_once()
        stream.close.assert_called_once_with()

    def test_inventory_decode_error_survives_independent_close(self):
        stream = self.stream("independent inventory close")
        stream.readline.return_value = "malformed JSON"
        with mock.patch.object(verification, "sha", return_value="sealed"), \
                mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(json.JSONDecodeError) as caught:
                verification.tree_inventory(Path("inventory"), "sealed", "set")
        self.assertEqual(caught.exception.independent_close_failures, ["independent inventory close"])

    def test_original_comparison_read_failure_keeps_exact_cause_and_phase(self):
        original = OSError("original comparison read")
        original.original_phase = "earlier original read phase"
        stream = self.stream("independent comparison close")
        stream.read.side_effect = original
        row = dict(verifier=dict(sha256="body"), observer=dict(compare_case="C01"))
        event = dict(event="verify", fields=dict(exit_code="0", registered_execs="0", stdout="original.stdout", command_sha256="body"))
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(OSError) as caught:
                verification.compare_event({}, row, event)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "earlier original read phase")
        self.assertEqual(original.independent_close_failures, ["independent comparison close"])
        stream.read.assert_called_once_with(65537)

    def test_comparison_decode_failure_precedes_independent_close(self):
        stream = self.stream("independent comparison close")
        stream.read.return_value = b"malformed comparison JSON"
        row = dict(verifier=dict(sha256="body"), observer=dict(compare_case="C01"))
        event = dict(event="verify", fields=dict(exit_code="0", registered_execs="0", stdout="original.stdout", command_sha256="body"))
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(json.JSONDecodeError) as caught:
                verification.compare_event({}, row, event)
        self.assertEqual(caught.exception.original_phase, "comparison_result_read")
        self.assertEqual(caught.exception.independent_close_failures, ["independent comparison close"])

    def test_host_event_write_failure_and_close_keep_first_original(self):
        original = OSError("original host event write")
        stream = self.stream("independent event close")
        stream.write.side_effect = original
        with mock.patch.object(Path, "open", return_value=stream), \
                mock.patch.object(verification.time, "monotonic", return_value=1), \
                mock.patch.object(verification.subprocess, "run") as invoked:
            with self.assertRaises(OSError) as caught:
                verification.compare_stdout(self.host_plan(), {}, Path("output"), deadline=10)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.independent_close_failures, ["independent event close"])
        invoked.assert_not_called()

    def test_host_process_failure_survives_both_output_closes_without_replay(self):
        original = OSError("original comparator run failure")
        event, stdout, stderr = self.stream(), self.stream("stdout close"), self.stream("stderr close")
        with mock.patch.object(Path, "open", side_effect=[event, stdout, stderr]), \
                mock.patch.object(verification, "sha", return_value="sealed"), \
                mock.patch.object(verification.time, "monotonic", return_value=1), \
                mock.patch.object(verification.subprocess, "run", side_effect=original) as invoked:
            with self.assertRaises(OSError) as caught:
                verification.compare_stdout(self.host_plan(), {}, Path("output"), deadline=10)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "host_stdout_comparator_run")
        self.assertEqual(original.independent_close_failures, ["stderr close", "stdout close"])
        invoked.assert_called_once()

    def test_second_output_open_failure_precedes_first_output_close(self):
        original = OSError("original stderr open failed")
        with mock.patch.object(Path, "open", side_effect=[self.stream(), self.stream("stdout close"), original]), \
                mock.patch.object(verification, "sha", return_value="sealed"), \
                mock.patch.object(verification.time, "monotonic", return_value=1), \
                mock.patch.object(verification.subprocess, "run") as invoked:
            with self.assertRaises(OSError) as caught:
                verification.compare_stdout(self.host_plan(), {}, Path("output"), deadline=10)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "host_stdout_capture_stderr_open")
        self.assertEqual(original.independent_close_failures, ["stdout close"])
        invoked.assert_not_called()

    def test_known_nonzero_comparator_exit_precedes_output_close_errors(self):
        event, stdout, stderr = self.stream(), self.stream("stdout close"), self.stream("stderr close")
        with mock.patch.object(Path, "open", side_effect=[event, stdout, stderr]), \
                mock.patch.object(verification, "sha", return_value="sealed"), \
                mock.patch.object(verification.time, "monotonic", return_value=1), \
                mock.patch.object(verification.subprocess, "run", return_value=SimpleNamespace(returncode=1)) as invoked:
            with self.assertRaisesRegex(ValueError, "original host stdout comparison failed") as caught:
                verification.compare_stdout(self.host_plan(), {}, Path("output"), deadline=10)
        self.assertEqual(caught.exception.original_phase, "host_stdout_comparator_exit")
        self.assertEqual(caught.exception.independent_close_failures, ["stderr close", "stdout close"])
        invoked.assert_called_once()

    def test_host_result_read_failure_survives_independent_result_close(self):
        original = OSError("original host result read")
        result = self.stream("result close")
        result.read.side_effect = original
        with mock.patch.object(Path, "open", side_effect=[self.stream(), self.stream(), self.stream(), result]), \
                mock.patch.object(verification, "sha", return_value="sealed"), \
                mock.patch.object(verification.time, "monotonic", return_value=1), \
                mock.patch.object(verification.subprocess, "run", return_value=SimpleNamespace(returncode=0)):
            with self.assertRaises(OSError) as caught:
                verification.compare_stdout(self.host_plan(), {}, Path("output"), deadline=10)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "host_stdout_result_read")
        self.assertEqual(original.independent_close_failures, ["result close"])


if __name__ == "__main__":
    unittest.main()
