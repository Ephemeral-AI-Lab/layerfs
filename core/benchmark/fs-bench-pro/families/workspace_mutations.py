"""Family6 mixed ordinary mutations; existing SDK/native runners and full proof."""
from dataclasses import dataclass
import hashlib

from families import workspace_commit_native as native
from families import workspace_namespace as namespace

SCHEMA = "core-workspace-mutations-run-v1"
PROOF_SCHEMA = "core-workspace-mutations-proof-v1"
NATIVE_SCHEMA = "core-workspace-mutations-native-run-v1"
PROFILE = "mixed-workspace-mutations-functional-clone-v1"
FIRST_TIME = 1_577_934_245
SECOND_TIME = FIRST_TIME + 86400
G1_BYTES = b"abXYZf" + bytes(4)
G2_BYTES = b"QQXYZf" + bytes(4)

MIX = ("set -e; umask 022; mkdir work; printf abcdefghij > work/file; "
       "printf XYZ | dd of=work/file bs=1 seek=2 conv=notrunc 2>/dev/null; "
       "truncate -s 6 work/file; truncate -s 10 work/file; chmod 600 work/file; "
       "ln work/file work/alias; mv work/alias work/moved; "
       "ln -s file work/link; mv work/link work/link2; "
       "mkdir work/empty; rmdir work/empty; printf temporary > work/remove; rm work/remove; "
       "printf sibling-new > packages/old/subtree/sibling.txt; "
       "TZ=UTC touch -t 202001020304.05 work/file; "
       'test "$(readlink work/link2)" = file; cmp work/file work/moved; cmp work/file work/link2')
LATER = ("set -e; printf QQ | dd of=work/file bs=1 conv=notrunc 2>/dev/null; "
         "rm work/moved; mv work/link2 work/link3; TZ=UTC touch -t 202001030304.05 work/file; "
         'test "$(readlink work/link3)" = file; cmp work/file work/link3')
REFUSALS = ("set -e; umask 022; mkdir checks; mkdir checks/child; printf keep > checks/child/file; "
            "if rmdir checks/child; then exit 91; fi; "
            "if mv checks checks/child/nested; then exit 92; fi; "
            "if ln checks/child checks/dirlink; then exit 93; fi; "
            'test "$(cat checks/child/file)" = keep; test ! -e checks/child/nested; '
            "test ! -e checks/dirlink; printf success > checks/result")
FAILURE = "set -e; umask 022; mkdir uncommitted; printf private > uncommitted/file; exit 7"


@dataclass(frozen=True)
class Case(namespace.SdkCase):
    role: str = "mixed"


SDK = {case.id: case for case in (
    Case("workspace-mutations-mixed-ordinary-sdk-v1", "small", MIX),
    Case("workspace-mutations-retained-g1-live-g2-sdk-v1", "small", LATER, prelude=MIX, role="retained"),
    Case("workspace-mutations-known-posix-refusals-sdk-v1", "small", REFUSALS, role="refusals"),
    Case("workspace-mutations-shell-exit7-no-commit-sdk-v1", "small", FAILURE, role="failure"),
)}
NATIVE = {case.id: case for case in (
    native.Case("workspace-mutations-mixed-live-g1-g2-native-v1", "phase_b_mutations::mixed_live", budget_ns=15_000_000_000, ignored=True),
    native.Case("workspace-mutations-mixed-known-unknown-custody-native-v1", "phase_b_mutations::mixed_failures", clones=2,
                retained=True, budget_ns=15_000_000_000, ignored=True),
    native.Case("workspace-mutations-mixed-local-c5-resume-native-v1", "phase_b_mutations::mixed_local_resume",
                budget_ns=15_000_000_000, ignored=True),
)}
REUSED = {
    "deterministic-live-sdk-stopping-refusal": {
        "case": "workspace-commit-sdk-stopping-refusal-fixture-release-v2", "round": "r058", "source_commit": "3ea097f7929ec1be7b41fc985cef9022dbe06f0a",
        "compact_receipt_sha256": "20070f8429c1faa23f9c04319079bbb5e009b4bf0aac3f81ce538409cdaa1ee4", "raw_manifest_sha256": "d1420f3cd378cafe29ad7a6a3a7e86092d6d2ec3f291fa664b1ad57cf1105fbd",
        "report": "core/docs/issues/286/experiments/20260930-workspace-commit-sdk-stopping-r058.md",
        "scope": "same live mounted Workspace, stopping/held FD, known Busy/Io refusals, outside fixture release, checked SDK unmount/delete",
        "reason": "production stopping/lease/transport paths unchanged; no repeated performance arm",
    },
    "known-saved-file-root-and-32-lease-authority": {
        "round": "r054", "source_commit": "003ee63224631d48bed3a932b4d07c7a135c9426",
        "compact_receipt_sha256": "e3727e54632373fac91c846d58c37f1cdca37ce919e36846a85a97f8a1e8d684", "raw_manifest_sha256": "47ae87e3a8ded3fb959119fe3b8d4f2a692473e4d312deb816e8f82e3fb4b9a8", "report": "core/docs/issues/286/FAMILY4-CHECKPOINT-20260930.md",
        "scope": "known C1 saved root before metadata failure,32 leases, forged/stale/unissued token/metadata authority and checked release",
        "reason": "production authority and known-save custody paths unchanged; current mixed native failures supplement this scope",
    },
}


def base_tree():
    return {path: ("d", 493, None) if data is None else ("f", 420, data)
            for path, data in {**namespace.tree("small"), ".marker": b"baseline"}.items()}


def mixed_tree():
    result = base_tree()
    result.update({"work": ("d", 493, None), "work/file": ("f", 384, G1_BYTES),
                   "work/moved": ("f", 384, G1_BYTES), "work/link2": ("l", 511, b"file"),
                   "packages/old/subtree/sibling.txt": ("f", 420, b"sibling-new")})
    return result


def manifest(tree):
    return "".join(f"{path or '.'}\t{kind}\t{mode}\t{0 if data is None else len(data)}\t"
                   f"{'-' if data is None else hashlib.sha256(data).hexdigest()}\n"
                   for path, (kind, mode, data) in sorted(tree.items()))


def oracle(case):
    old = mixed_tree() if case.role == "retained" else base_tree()
    new = mixed_tree() if case.role == "mixed" else dict(old)
    if case.role == "retained":
        del new["work/moved"]
        del new["work/link2"]
        new.update({"work/file": ("f", 384, G2_BYTES), "work/link3": ("l", 511, b"file")})
    elif case.role == "refusals":
        new.update({"checks": ("d", 493, None), "checks/child": ("d", 493, None),
                    "checks/child/file": ("f", 420, b"keep"), "checks/result": ("f", 420, b"success")})
    return manifest(old), manifest(new)


def properties(case):
    if case.role == "failure":
        return {"expected_failure": "1", "expected_exit_status": "7"}
    result = {"identity_old_prefix": "packages/old/subtree", "identity_new_prefix": "packages/old/subtree"}
    aliases = "work/file\twork/moved\t2"
    first = f"work/file\t{FIRST_TIME}\t0\nwork/moved\t{FIRST_TIME}\t0"
    if case.role == "mixed":
        result.update(alias_pairs_hex=aliases.encode().hex(), mtime_oracle_hex=first.encode().hex())
    if case.role == "retained":
        result.update(old_alias_pairs_hex=aliases.encode().hex(), old_mtime_oracle_hex=first.encode().hex(),
                      alias_pairs_hex=b"work/file\twork/file\t1".hex(),
                      mtime_oracle_hex=f"work/file\t{SECOND_TIME}\t0".encode().hex())
    return result


def attempt(out, case, master, prepared):
    def route(driver, counts):
        if not driver:
            return False
        if case.role == "failure":
            return driver.get("commit_called") is False and driver.get("exec_exit_status") == 7
        minimum = 3 if case.role == "retained" else 2 if case.role == "mixed" else 0
        return bool(driver.get("commit_called") and driver.get("exec_exit_status") == 0
                    and int(counts.get("rename", 0)) >= minimum)
    return namespace.sdk_attempt(out, case, master, prepared, profile=PROFILE, oracle_builder=oracle,
                                 properties=properties(case), route_validator=route,
                                 pin_path=b"work/file", pin_expected=G1_BYTES)


def run(selection, output, common):
    if selection in NATIVE or selection in ("workspace-mutations-native", "workspace-mutations-native-tail"):
        selected = tuple(NATIVE)[1:] if selection == "workspace-mutations-native-tail" else tuple(NATIVE) if selection not in NATIVE else (selection,)
        return native.run(selection, output, common, cases=NATIVE, selected=selected,
                          profile=PROFILE, schema=NATIVE_SCHEMA, reused_proofs=REUSED)
    return namespace.run(selection, output, common, sdk_cases=SDK, profile=PROFILE, schema=SCHEMA, attempt=attempt, reused_proofs=REUSED)


def prove(run, output, common):
    return namespace.prove(run, output, common, schema=PROOF_SCHEMA)


def report(out):
    return namespace.report(out)
