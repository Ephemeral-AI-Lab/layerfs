"""Sealed expected-versus-observed recipes, separate from performance."""
import importlib.util
import json
from pathlib import Path
import stat
import subprocess
import time

from . import deployment, git_index_oracle, git_queries, oracle, registry, registry_variant, workloads

GIT_CASES = {"E04", "E10", "E11", "E18", "C12"}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def sha(path):
    return deployment.file_hash(Path(path))[0]


def member(root, name):
    path = Path(name)
    require(bool(name) and not path.is_absolute() and ".." not in path.parts and str(path) == name,
            "oracle member escapes bundle")
    candidate = root / path
    require(not any(parent.is_symlink() for parent in [candidate, *candidate.parents] if parent != root and root in parent.parents),
            "oracle member symlink is not a closed regular file")
    actual = (root / path).resolve(strict=True)
    require(root in actual.parents and stat.S_ISREG(actual.lstat().st_mode), "oracle member outside closed bundle")
    return actual


def helper_paths(case=None):
    base = Path(__file__).resolve().parent
    result = {"oracle.py": base / "oracle.py", "workload.py": base / "workload.py",
            "r7_deployment.py": Path(deployment.__file__),
            "oracle_prepare.py": base.parents[1] / "r7-tools/oracle_prepare.py"}
    if case in GIT_CASES:
        result.update({name: base / name for name in ("git_queries.py", "git_index_oracle.py", "workloads.py")})
    return result


def canonical_helpers(case=None):
    return {name: sha(path) for name, path in helper_paths(case).items()}


def git_bundle(root, value, config):
    context = value.get("git_context", {})
    require(value.get("oracle_variant") == config.get("oracle_variant") == registry_variant.NAME and
            value.get("registry_variant_identity") == registry_variant.identity() and
            value.get("registry_variant_source_sha256") == sha(registry_variant.__file__), "paired Git registry variant differs")
    require(context.get("oracle_schema") == git_index_oracle.SCHEMA and context.get("pin") == "git-queries/pin.json" and
            context.get("queries") == "git-queries/queries.json" and context.get("policy") == "git-default-policy.json",
            "closed Git context paths/schema differ")
    require(context.get("query_position") == ("after original canonical body" if value["case_id"] == "C12" else "before original canonical body"),
            "closed actual Git query position differs from original body")
    pin = git_index_oracle.load_sealed(member(root, context["pin"]), context["pin_file_sha256"])
    policy = git_index_oracle.load_sealed(member(root, context["policy"]), context["policy_sha256"])
    queries = git_index_oracle.load_sealed(member(root, context["queries"]), context["queries_sha256"])
    git_index_oracle.validate_pin(pin)
    git_queries.policy_shape(policy)
    require(context.get("actual_pin") == pin and pin["git"]["sha256"] == policy["git_binary_sha256"] and
            pin["git"]["version"] == policy["version"], "closed Git pin/policy actual binary/version differ")
    require(queries.get("schema") == git_queries.SCHEMA and queries.get("status") == "OBSERVED" and queries.get("case_id") == value["case_id"] and
            (queries.get("uid"), queries.get("gid")) == (501, 20) and queries.get("environment") == workloads.ENV and
            queries.get("qualified_policy") == policy and queries.get("effective_config") == pin["effective_config"] and
            queries.get("actual_git_binary") == pin["git"]["binary"] and queries.get("actual_git_binary_sha256") == pin["git"]["sha256"] and
            queries.get("actual_git_version") == pin["git"]["version"] and queries.get("actual_object_format") == pin["object_format"] and
            queries.get("command_override") == ({"core.fsmonitor": "false"} if git_queries.query_override(value["case_id"]) else {}) and
            queries.get("wall_stop_seconds") == git_queries.WALL_SECONDS and
            pin["effective_config_receipt_sha256"] == context["queries_sha256"], "closed original Git query context differs")
    rows = queries.get("queries", [])
    labels = ["version", "object-format", *[key.replace(".", "-") for key in [*git_index_oracle.EFFECTIVE_KEYS, "feature.manyfiles"]]]
    require(len(rows) == 13 and [row.get("label") for row in rows] == labels and
            all(row.get("attempts") == 1 and row.get("status") == "ORIGINAL_COMPLETED" and
                type(row.get("pid")) is int and row["pid"] > 0 and type(row.get("exit_code")) is int and
                row["exit_code"] in {0, 1} for row in rows),
            "closed original Git query attempts/coverage/custody differ")
    artifacts, provenance = {}, {}
    for position, row in enumerate(rows):
        key = None if position < 2 else [*git_index_oracle.EFFECTIVE_KEYS, "feature.manyfiles"][position - 2]
        argv = [pin["git"]["binary"], "--version"] if position == 0 else [pin["git"]["binary"], *git_queries.query_override(value["case_id"])]
        if position == 1:
            argv.extend(["rev-parse", "--show-object-format"])
        elif key is not None:
            argv.extend(["config", "--get"])
            if key in git_queries.BOOLS:
                argv.append("--type=bool")
            argv.append(key)
        require(row.get("argv") == argv and row.get("cwd") == queries["root"] == value["inputs"]["roots"][0]["path"],
                "closed original Git query body/root differs")
        data = {}
        for stream in ("stdout", "stderr"):
            name = "git-queries/" + row["label"] + "." + stream
            require(Path(row[stream]).name == row["label"] + "." + stream and
                    sha(member(root, name)) == row[stream + "_sha256"], "closed original Git query stream differs")
            artifacts[name] = row[stream + "_sha256"]
            with oracle.FileOwner(member(root, name), "rb", "closed_git_query_stream_read") as source:
                data[stream] = source.read(65537)
        require(len(data["stdout"]) <= 65536 and not data["stderr"] and
                (row["exit_code"] == 0 or key is not None and not data["stdout"]), "closed original query absence/output differs")
        observed = data["stdout"].decode("utf-8").strip() if row["exit_code"] == 0 else policy["defaults"][key]
        wanted = pin["git"]["version"] if position == 0 else pin["object_format"] if position == 1 else (
            "false" if key == "feature.manyfiles" else pin["effective_config"][key])
        require(observed == wanted, "closed original queried Git value differs from pin/default policy")
        if key is not None:
            provenance[key] = dict(original="ABSENT" if row["exit_code"] == 1 else observed,
                                   exit_code=row["exit_code"])
            if row["exit_code"] == 1:
                provenance[key]["qualified_literal_default"] = observed
    require(queries.get("original_values") == provenance, "closed literal queried/default provenance differs")
    require(pin["git"]["version_receipt_sha256"] == rows[0]["stdout_sha256"], "closed original version witness differs")
    policy_input = value["inputs"].get("git_default_policy", {})
    require(Path(policy_input.get("path", "")).parent == Path("/code") and
            policy_input.get("sha256") == context["policy_sha256"] and
            value["inputs"]["code_assets"].get(Path(policy_input["path"]).name) == context["policy_sha256"],
            "qualified policy semantic input differs")
    return dict(pin=pin, policy=policy, queries=queries, query_artifacts=artifacts)


def canonical_author():
    path = helper_paths()["oracle_prepare.py"]
    spec = importlib.util.spec_from_file_location("r7_closed_oracle_author", path)
    author = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(author)
    return author


def coverage(case):
    name = case["case_id"]
    if name in {"E05", "E06"}:
        return []
    if name == "W01":
        return [("scan", "scan"), ("writer", "writer")]
    if name == "W02":
        return [("shared", "primary")]
    if name.startswith("K"):
        return [("checkpoint-" + str(index + 1), "primary") for index in range(len(case["workload"]["commands"]))]
    return [("expected", "primary")]


def tree_inventory(path, digest, expected_set, wanted=(), *, empty=False):
    """Check the complete closed set while retaining only requested code hashes."""
    path = Path(path)
    require(sha(path) == digest, "sealed fixture/deployment inventory bytes changed")
    fingerprint, selected, seen, footer = deployment.SetDigest(), {}, set(), None
    with oracle.FileOwner(path, "r", "comparison_inventory_read") as stream:
        first = stream.readline()
        require(bool(first), "complete inventory header missing")
        header = json.loads(first)
        require(header.get("schema") == deployment.TREE_SCHEMA, "complete fixture/deployment inventory required")
        for line in stream:
            row = json.loads(line)
            if row.get("event") == "completed":
                require(footer is None and row.get("status") == "SEALED_SETUP_ONLY", "inventory completion invalid")
                footer = row
                continue
            require(footer is None and row["path"] not in seen, "inventory duplicate or unsealed trailing row")
            seen.add(row["path"])
            fingerprint.add(row["path"], row["metadata"])
            if row["path"] in wanted:
                require(row["metadata"].get("kind") == "file", "required deployment operand must be regular")
                selected[row["path"]] = row["metadata"].get("content_sha256")
    require(footer is not None and "." in seen and footer["entries"] == len(seen) and
            fingerprint.finish() == footer["content_metadata_set_sha256"] == expected_set,
            "complete inventory content/metadata seal differs")
    require(not empty or seen == {"."}, "canonical Git init base inventory is not exactly empty")
    return selected


def bind_inputs(value, config, selection, prepared):
    inputs, fixture = value["inputs"], value["fixture_identity"]
    require(isinstance(inputs, dict) and isinstance(fixture, dict), "closed fixture/input mapping required")
    require(inputs.get("fixture_identity") == fixture, "closed input fixture identity differs")
    require(fixture.get("prepared_store_sha256") == sha(config["prepared_store_file"]), "actual prepared Store differs from oracle fixture")
    require(fixture.get("installed_manifest_sha256") == sha(config["manifest"]), "actual installed manifest differs from oracle fixture")
    roots = inputs.get("roots")
    roles = ["scan", "writer"] if selection["case_id"] == "W01" else ["primary"]
    require(isinstance(roots, list) and [row.get("role") for row in roots] == roles, "closed reference root role coverage differs")
    originals = value.get("native_verifications", [])
    for row in roots:
        original = [item for item in originals if item.get("root") == row["path"]]
        require(len(original) == 1 and original[0].get("inventory_sha256") == row.get("inventory_sha256") and
                original[0].get("content_metadata_set_sha256") == row.get("content_metadata_set_sha256"),
                "closed reference root has no exact original input verification")
    if selection["arm"] == "L":
        require(not prepared["native"], "L oracle must not materialize native roots")
        base = config["base_fixture_inventory"]
        tree_inventory(base["path"], base["sha256"], base["content_metadata_set_sha256"], empty=selection["case_id"] == "C12")
        require(all(row.get("content_metadata_set_sha256") == base["content_metadata_set_sha256"] for row in roots),
                "L base fixture complete content/metadata set differs from reference")
    else:
        require(len(prepared["native"]) == len(roots), "selected native root population differs from reference")
        for root, item in zip(roots, prepared["native"]):
            require(item["target"] == root["path"] and item["content_metadata_set_sha256"] == root["content_metadata_set_sha256"],
                    "selected native root role/content set differs from reference")
            tree_inventory(item["inventory"], item["inventory_sha256"], item["content_metadata_set_sha256"], empty=selection["case_id"] == "C12")
    external = inputs.get("external", [])
    assets = [item for item in prepared["assets"] if item["target"] != "/code"]
    require(len(assets) == len(external), "selected external input population differs from reference")
    for row, item in zip(external, assets):
        require(row["path"] == item["target"] and row["content_metadata_set_sha256"] == item["content_metadata_set_sha256"],
                "selected external input content set differs from reference")
        tree_inventory(item["inventory"], item["inventory_sha256"], item["content_metadata_set_sha256"])
    required_assets = {"largest-path"} if selection["case_id"] == "E15" else {"tracked-paths.json"} if selection["case_id"] == "E11" else {"node-roots.json"} if selection["case_id"] in {"E12", "E13", "E14", "K02", "K03"} else set()
    require(selection["case_id"] != "C12" or fixture.get("fixture_kind") == "empty", "canonical init requires declared empty base fixture")
    code = inputs.get("code_assets", {})
    require(isinstance(code, dict) and required_assets <= set(code) and all(Path(name).name == name for name in code),
            "selected semantic code-data input seals missing or invalid")
    require(not (set(code) & set(value["helpers"])), "semantic code-data inputs cannot replace comparison helpers")
    require(selection["case_id"] not in {"E12", "E13", "K02", "K03"} or [item["path"] for item in external] == ["/replay"],
            "selected replay input seal missing")


def load(config, selection):
    declared = config.get("oracle_recipe")
    if not declared:
        return None
    root = deployment.owned_input(declared["bundle"])
    require(stat.S_ISDIR(root.lstat().st_mode), "oracle bundle must be an actual owned directory")
    closed = member(root, "closed.json")
    require(sha(closed) == declared["closed_sha256"], "closed oracle recipe bytes changed")
    value = json.loads(closed.read_text())
    require(value.get("schema") == "r7-closed-oracle-author-v1" and value.get("status") == "CLOSED_EXPECTED_SETUP_ONLY",
            "oracle expectation is not closed")
    require(value.get("case_id") == selection["case_id"] and value.get("cache_class") == selection["cache_class"],
            "oracle case/cache treatment differs")
    require(value.get("fixture_identity") == config.get("oracle_fixture_identity"), "oracle fixture/cut identity differs")
    require(value.get("image_id") == config["identities"]["image_id"] and (value.get("uid"), value.get("gid")) == (config["uid"], config["gid"]),
            "oracle image/user differs")
    require(value.get("verifier_wall_stop_ns") == 9_500_000_000, "oracle verifier budget changed")
    case = next(item for item in registry.cases() if item["case_id"] == selection["case_id"])
    require(selection["cache_class"] in case["classes"], "selected oracle cache class is not registered for this case")
    require(value.get("environment") == workloads.ENV, "closed oracle environment differs")
    require(value.get("selected_workload") == case["workload"], "closed selected workload differs")
    require(value.get("registry_sha256") == sha(registry.__file__) and value.get("workload_source_sha256") == sha(workloads.__file__),
            "closed registry/workload actual source SHA differs")
    require(value.get("helpers") == canonical_helpers(selection["case_id"]), "closed helper actual source SHA closure differs")
    author = canonical_author()
    require(not author.not_run(selection["case_id"], selection["cache_class"]), "selected oracle contract remains unavailable")
    rows = value.get("expected")
    require(isinstance(rows, list) and all(isinstance(row, dict) for row in rows) and
            [(row.get("label"), row.get("role")) for row in rows] == coverage(case),
            "required checkpoint/role coverage differs")
    asset = value.get("asset_root")
    require(isinstance(asset, str) and asset.startswith("/code/oracles/") and str(Path(asset)) == asset and
            ".." not in Path(asset).parts and not any(c in asset for c in "\n\r\t"), "closed oracle asset root invalid")
    git = git_bundle(root, value, config) if selection["case_id"] in GIT_CASES else None
    for expected in value["expected"]:
        label = expected["label"]
        observer = author.observer_for(selection["case_id"], expected["role"])
        require(expected.get("observer") == observer, "canonical selected observer scope differs")
        require(expected["manifest"] == label + ".jsonl", "expected manifest path differs from checkpoint")
        require(sha(member(root, expected["manifest"])) == expected["sha256"], "closed expected manifest changed")
        if git is not None:
            operand = expected.get("git_index", {})
            context = value["git_context"]
            require(operand.get("manifest") == label + ".index.json" and operand.get("tree_sha256") == expected["sha256"] and
                    all(operand.get(name) == context[name] for name in ("pin", "queries", "policy")) and
                    operand.get("pin_sha256") == context["pin_file_sha256"] and
                    all(operand.get(name + "_sha256") == context[name + "_sha256"] for name in ("queries", "policy")),
                    "closed expected Git operands differ from paired context")
            if selection["case_id"] == "E11":
                require(operand.get("tracked") == "/code/tracked-paths.json" and
                        operand.get("tracked_sha256") == value["inputs"]["code_assets"].get("tracked-paths.json"), "E11 tracked semantic input differs")
            else:
                require(not operand.get("tracked") and not operand.get("tracked_sha256"), "unselected Git tracked input forbidden")
            observation = git_index_oracle.load_sealed(member(root, operand["manifest"]), operand["manifest_sha256"])
            require(observation.get("pin") == git["pin"] and observation.get("pin_sha256") == git_index_oracle.sha256(git_index_oracle.canonical(git["pin"])),
                    "expected index observation comparison pin differs")
            require(git_index_oracle.compare_tree(member(root, expected["manifest"]), member(root, expected["manifest"]),
                    selection["case_id"], observation, observation)["status"] == "PASS", "expected tree/index raw metadata binding differs")
            contract = config.get("git_index_oracle", {})
            require(contract.get("helper_sha256") == value["helpers"]["git_index_oracle.py"] and
                    contract.get("object_format") == git["pin"]["object_format"] and
                    contract.get("index_version") == observation["semantic"]["version"] and
                    contract.get("effective_config_sha256") == git["pin"]["effective_config_sha256"] and
                    contract.get("git_binary_sha256") == git["pin"]["git"]["sha256"] and
                    contract.get("git_version") == git["pin"]["git"]["version"], "configured Git context differs from sealed expected context")
        script = expected["verifier"]
        require(script.get("path") == asset + "/" + label + ".verify.sh" and script.get("expected") == expected["manifest"] and
                script.get("root") == "original selected mounted cwd", "canonical comparison script path/root differs")
        require(script.get("performs_comparison") is True and script.get("wall_stop_ns") == 9_500_000_000,
                "oracle script must perform comparison under the frozen bound")
        source = member(root, Path(script["path"]).name)
        require(sha(source) == script["sha256"], "comparison script changed")
        require(source.read_bytes() == author.verifier_body(asset, observer, label, expected.get("git_index")).encode(), "canonical comparison body differs")
    stdout = value.get("host_stdout_recipe")
    require(stdout and stdout.get("required") is True and
            sha(member(root, stdout["expected_bundle_relative"])) == stdout["expected_sha256"], "closed stdout expectation changed")
    require(stdout.get("expected_asset") == asset + "/" + stdout["expected_bundle_relative"] and
            stdout.get("kind") == ("unordered-complete-lines" if selection["case_id"].startswith("W") else "exact-file-sha256"),
            "closed stdout scope/path differs")
    prepared = deployment.load_plan(config, selection)
    bind_inputs(value, config, selection, prepared)
    plan = {"root": root, "closed": closed, "value": value, "sha256": declared["closed_sha256"]}
    if git is not None:
        plan["git"] = git
    bind_deployment(plan, config, selection)
    return plan


def required_deployment_hashes(plan):
    value = plan["value"]
    wanted = dict(value["helpers"], **value["inputs"].get("code_assets", {}))
    for row in value["expected"]:
        prefix = value["asset_root"][len("/code/"):] + "/"
        wanted[prefix + row["manifest"]] = row["sha256"]
        wanted[row["verifier"]["path"][len("/code/"):]] = row["verifier"]["sha256"]
        if row.get("git_index"):
            for name in ("manifest", "pin", "queries", "policy"):
                wanted[prefix + row["git_index"][name]] = row["git_index"][name + "_sha256"]
    if plan.get("git"):
        wanted.update({prefix + name: digest for name, digest in plan["git"]["query_artifacts"].items()})
    stdout = value["host_stdout_recipe"]
    wanted[stdout["expected_asset"][len("/code/"):]] = stdout["expected_sha256"]
    return wanted


def bind_deployment(plan, config, selection, staged=None):
    """Bind closed operands before setup and their actual deployed bytes after it."""
    prepared = deployment.load_plan(config, selection)
    wanted = required_deployment_hashes(plan)
    code = [item for item in prepared["assets"] if item["target"] == "/code"]
    require(len(code) == 1, "one sealed deployment code tree required")
    item = code[0]
    actual = tree_inventory(item["inventory"], item["inventory_sha256"], item["content_metadata_set_sha256"], wanted)
    require(actual == wanted, "sealed deployed expected/helper/script operands differ from closed recipe")
    if staged is not None:
        require(staged.get("schema") == "r7-container-staging-v1" and staged.get("status") == "PASS",
                "actual staged operands need original successful deployment receipt")
        require(all(staged.get("helper_sha256", {}).get(name) == digest for name, digest in wanted.items()),
                "actual staged expected/helper/script operands differ from closed recipe")
    return wanted


def expected(plan, *, label=None, role=None):
    selected = [row for row in plan["value"]["expected"]
                if (label is None or row["label"] == label) and (role is None or row["role"] == role)]
    require(len(selected) == 1, "exact selected comparison checkpoint/role missing")
    return selected[0]


def compare_event(plan, row, event):
    fields = event.get("fields", {})
    require(event.get("event") == "verify" and fields.get("exit_code") == "0" and fields.get("registered_execs") == "0",
            "original unregistered known-zero verifier event required")
    require(fields.get("command_sha256") == row["verifier"]["sha256"], "original executed verifier body differs from sealed script")
    with oracle.FileOwner(fields["stdout"], "rb", "comparison_result_read") as stream:
        raw = stream.read(65537)
        require(len(raw) <= 65536, "comparison result exceeds declared scalar frame")
        result = json.loads(raw)
        is_git = row["observer"]["kind"] == "git"
        schema = git_queries.COMPARISON_SCHEMA if is_git else "r7-scoped-oracle-comparison-v1"
        require(result.get("schema") == schema and
                result.get("case_id") == row["observer"]["compare_case"] and result.get("status") == "PASS" and
                result.get("differences") == [], "actual expected-versus-observed comparison refused PASS")
        if is_git:
            operand = row["git_index"]
            wanted = dict(tree_sha256=operand["tree_sha256"], index_sha256=operand["manifest_sha256"],
                          pin_file_sha256=operand["pin_sha256"], policy_file_sha256=operand["policy_sha256"])
            if row["observer"]["case"] == "E11":
                wanted["tracked_sha256"] = operand["tracked_sha256"]
            require(result.get("oracle_schema") == git_index_oracle.SCHEMA and result.get("expected_operands") ==
                    wanted and
                    result.get("paired_comparison_pin_sha256") == git_index_oracle.sha256(git_index_oracle.canonical(plan["git"]["pin"])),
                    "actual Git comparison operands/paired pin differ")
    return {"expected_label": row["label"], "expected_sha256": row["sha256"], "original_event": event,
            "comparison": result, "recipe_sha256": plan["sha256"]}


def compare_stdout(plan, event, output, *, deadline):
    if time.monotonic() >= deadline:
        raise TimeoutError("host stdout comparison not launched after verifier stop")
    output = Path(output)
    event_path = output.with_suffix(".event.json")
    with oracle.FileOwner(event_path, "x", "host_stdout_event_write") as stream:
        json.dump(event, stream, sort_keys=True)
    helper = Path(__file__).resolve().parents[2] / "r7-tools/oracle_prepare.py"
    require(sha(plan["closed"]) == plan["sha256"] and sha(helper) == plan["value"]["helpers"]["oracle_prepare.py"],
            "closed stdout recipe or actual host helper changed")
    argv = ["python3", "-B", str(helper), "compare-stdout", "--closed", str(plan["closed"]),
            "--bundle", str(plan["root"]), "--actual-event", str(event_path)]
    left = deadline - time.monotonic()
    if left <= 0:
        raise TimeoutError("host stdout comparison preparation exceeded verifier stop")
    with oracle.FileOwner(output.with_suffix(".stdout"), "xb", "host_stdout_capture_stdout") as stdout, \
            oracle.FileOwner(output.with_suffix(".stderr"), "xb", "host_stdout_capture_stderr") as stderr:
        try:
            completed = subprocess.run(argv, stdout=stdout, stderr=stderr, timeout=left)
        except BaseException as original:
            if not hasattr(original, "original_phase"):
                original.original_phase = "host_stdout_comparator_run"
            raise
        if completed.returncode != 0:
            original = ValueError("original host stdout comparison failed")
            original.original_phase = "host_stdout_comparator_exit"
            raise original
    with oracle.FileOwner(output.with_suffix(".stdout"), "r", "host_stdout_result_read") as stream:
        result = json.load(stream)
        require(result.get("schema") == "r7-original-host-stdout-oracle-v1" and result.get("status") == "PASS",
                "original host stdout comparison did not establish PASS")
        require(result.get("expected_sha256") == plan["value"]["host_stdout_recipe"]["expected_sha256"],
                "original host stdout expected operand differs")
    return result
