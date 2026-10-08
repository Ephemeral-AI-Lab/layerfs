"""Prospective R7 command bodies, recovered from the pinned #305 source.

These are workload definitions, not reusable measurements or product claims.
The fixture checkout is never an execution root. Helpers are installed at their
declared /code paths in the pinned image, using an owned copy of fixture bytes.
"""
import hashlib
import json

HISTORICAL_COMMIT = "1451b68a720bbe2175a103dd9b35693ad05e2be1"
HISTORICAL_SOURCE = "core/experiment/real-tree/runner.py"
FIXTURE_COMMIT = "639ed01539"
ENV = {
    "PATH": "/opt/node/bin:/usr/local/bin:/usr/bin:/bin", "HOME": "/tmp",
    "LANG": "C", "LC_ALL": "C", "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_AUTHOR_NAME": "Experiment", "GIT_AUTHOR_EMAIL": "experiment@example.invalid",
    "GIT_COMMITTER_NAME": "Experiment", "GIT_COMMITTER_EMAIL": "experiment@example.invalid",
    "GIT_AUTHOR_DATE": "2026-10-03T00:00:00Z", "GIT_COMMITTER_DATE": "2026-10-03T00:00:00Z",
    "LAYERFS_CONSTRUCTION_WORKERS": "1",
}
COMMANDS = {
    "E01": "true",
    "E02": "find . | wc -l",
    "E03": "tar -cf - . | wc -c",
    "E04": "git -c core.fsmonitor=false status --porcelain=v1",
    "E05": "git log -20 --stat && git diff --stat HEAD~1",
    "E06": "git grep -n tool -- packages | wc -l",
    "E07": "node -e \"require('typescript')\"",
    "E08": "node node_modules/typescript/bin/tsc -b tsconfig.client.json",
    "E09": "node --test .github/review-ownership/*.test.mjs",
    "E10": "sed -i '1i// experiment 305' packages/acp/acp/src/codec.ts && git add -A && git commit -m 'experiment 305'",
    "E11": "git checkout -q HEAD~100 && git checkout -q -",
    "E12": "python3 /code/workload.py copy",
    "E13": "python3 /code/workload.py link",
    "E14": "python3 /code/workload.py remove",
    "E15": 'cp "$(cat /code/largest-path)" experiment-large && cmp "$(cat /code/largest-path)" experiment-large',
    "E16": "python3 /code/workload.py append",
    "E17": "python3 /code/workload.py churn",
}
COMMANDS["E18"] = COMMANDS["E04"]
COMMANDS["E19"] = COMMANDS["E04"]
MK1000 = "for i in $(seq 1 1000); do echo $i > f$i; done"
MKTREE = "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; for c in $(seq 1 10); do touch $a/$b/$c; done; done; done"
DD = "dd if=/dev/zero of=big bs=1M count=64 status=none"
CF = {
    "C01": (MK1000, ""),
    "C02": (MK1000 + "; for i in $(seq 1 1000); do stat f$i; done", ""),
    "C03": (MK1000 + "; rm f*", ""),
    "C04": (MKTREE, ""),
    "C05": (MKTREE + "; find . -type f | wc -l", ""),
    "C06": (DD, ""),
    "C07": (DD + "; cp big big2", ""),
    "C08": (DD + "; cat big > /dev/null", ""),
    "C09": ("cat big > /dev/null", DD),
    "C10": ("cp big big2", DD),
    "C11": (DD + " conv=notrunc", DD),
    "C12": ("git init -q; for i in $(seq 1 100); do echo $i > f$i; done; git add -A; git -c user.email=a@b -c user.name=a commit -qm init", ""),
}
COMMANDS.update({case: "{ " + body + "; } >/dev/null" for case, (body, _) in CF.items()})

ORACLES = {
    "E01": "ownership: Ready, unregistered external process, complete terminal drain",
    "E02": "full names and inode-kind manifest",
    "E03": "content digest of tar stream; separate verifier, never byte count alone",
    "E04": "scoped .git tree, stdout and exact exit; index rewrite retained",
    "E05": "stdout digest and exact exit",
    "E06": "stdout digest and exact exit",
    "E07": "stdout digest and exact exit",
    "E08": "NOT_RUN: owner excluded TypeScript build",
    "E09": "NOT_RUN: no authorized normalized oracle",
    "E10": "scoped .git and codec.ts tree; survival through selected Commit",
    "E11": "scoped .git and tracked-file tree",
    "E12": "scoped node_modules tree: full payload digests and metadata",
    "E13": "E12 plus device/inode/nlink equivalence classes, including .experiment-store",
    "E14": "scoped removed-dependency tree",
    "E15": "full bytes of experiment-large, matching sealed largest source file",
    "E16": "experiment.log exactly (x*99 + newline)*10000, 1000000 bytes",
    "E17": "no experiment-temp-* names remain; scoped tree and exact exit",
    "E18": "scoped .git tree and stdout; labelled unrefreshed index",
    "E19": "scoped .git and tracked tree; derive expected output after 282 changes",
}
ORACLES.update({case: "full-tree SHA-256 and metadata; complete for declared empty-base scenario" for case in CF})


def workload(case_id):
    """Return immutable command semantics; no execution or fixture access."""
    command = COMMANDS[case_id]
    oracle_argv = ["python3", "/code/oracle.py", "observe", "--case", case_id,
                   "--root", "{verification_root}", "--output", "{fresh_oracle_output}"]
    oracle_unavailable = None
    if case_id in {"E05", "E06", "E07"}:
        oracle_argv = ["python3", "/code/oracle.py", "stdout", "--expected", "{sealed_stdout_oracle}",
                       "--actual", "{original_stdout}"]
    if case_id in {"E01", "E03", "E08", "E09"}:
        oracle_argv = None
        oracle_unavailable = {
            "E01": "ownership oracle is original SDK/daemon lifecycle receipt, not a tree observer",
            "E03": "wc-only body does not expose the timed tar stream; independent tree/content observer cannot attest that exact stream",
            "E08": "owner NOT_RUN", "E09": "owner NOT_RUN: no normalized oracle",
        }[case_id]
    if case_id in {"E11", "E19"}:
        oracle_argv += ["--tracked", "/code/tracked-paths.json"]
    return {
        "argv": ["/bin/bash", "-o", "pipefail", "-c", command],
        "command_sha256": hashlib.sha256(command.encode()).hexdigest(),
        "environment": dict(ENV), "uid": "declared fixture owner", "gid": "declared fixture owner",
        "historical_uid": 1000, "historical_gid": 1000,
        "preparation_command": CF.get(case_id, (None, ""))[1],
        "related_root_preparation_argv": ["python3", "/code/changes.py", "--root", "{owned_root_or_mount}",
                                          "--tracked", "/code/tracked-paths.json"] if case_id == "E19" else None,
        "oracle": ORACLES[case_id],
        "oracle_argv": oracle_argv, "oracle_unavailable_reason": oracle_unavailable,
        "historical_source": {"commit": HISTORICAL_COMMIT, "path": HISTORICAL_SOURCE},
    }


def workload_identity():
    body = {case: workload(case) for case in COMMANDS}
    return hashlib.sha256(json.dumps(body, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
