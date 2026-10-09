"""Append-only timing oracle variant authorized by the prospective plan119.

The original registry/oracle and all prior receipts retain their identities.
Bodies, environment and matrix population remain unchanged. E18:C stays
NOT_RUN because its required warmup would refresh the unrefreshed index.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
from . import registry

NAME = "r7-git-index-scoped-v2"
E18_C_REASON = "unrefreshed index conflicts with identical-command class C warmup; no reset or refresh authorized"
GIT_TREE_CASES = {"E04","E10","E11","E18","E19","C12"}
CONTRACT = {"oracle_schema":NAME,"source_plan":"core/docs/issues/307/checks/r7-optimization-20261009/119-git-index-timing-oracle-plan.md",
            "comparison":"complete ordered path bytes, staged mode, full object ID, stage, persisted semantic flags and supported extension semantics",
            "excluded_from_cross_filesystem_comparison":"only index filesystem stat-cache fields; raw fields/index bytes retained separately",
            "required":"actual trailing digest and explicitly pinned Git object format/version/configuration; same verifier in matched arms",
            "limitations":"scoped timing oracle; no raw native/L index equality or stat-cache correctness claim",
            "separate_L_proof":"exact index hash/length across live mount, ordinary Commit, terminal unmount and fresh mount before Git refresh",
            "selection_restrictions":{"E18:C":E18_C_REASON}}


def document():
    value = copy.deepcopy(registry.registration())
    value["schema"] = "r7-optimization-registry-git-index-scoped-v2"
    value["base_registry_identity"] = registry.registry_identity()
    value["oracle_variant"] = CONTRACT
    for case in value["cases"]:
        if case["case_id"] in GIT_TREE_CASES:
            case["timing_oracle_variant"] = NAME
    for selection in value["selections"]:
        if selection["case_id"] == "E18" and selection["cache_class"] == "C":
            selection["reason"] = E18_C_REASON
    return value


def identity():
    return hashlib.sha256(json.dumps(document(),sort_keys=True,separators=(",",":")).encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,required=True)
    args = parser.parse_args()
    value = document()
    value["registry_identity"] = identity()
    with args.output.open("x") as stream:
        json.dump(value,stream,sort_keys=True,indent=2)
        stream.write("\n")
    print(json.dumps(dict(registry_identity=identity(),rows=len(value["selections"]),oracle_variant=NAME)))


if __name__ == "__main__":
    main()
