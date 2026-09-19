#!/usr/bin/env python3
"""The Stage 6 harness runner: list | prepare | prune | perf | verify | report | self-check | calibrate.

What each verb guarantees
-------------------------

`list`
    Prints the frozen registry, or the `--smoke` lane. Reads the binary's own
    generated table, so what is listed is what is registered.

`prepare`
    Acquires a prepared artifact once, keyed by a compatibility digest, and leaves
    it immutable. Nothing in a timed phase rebuilds it.

`prune`
    Removes prepared artifacts the current compatibility key no longer accepts —
    superseded entries, entries sealed under a different key, and unsealed
    directories left by an interrupted acquisition — and **never** a master the
    current key still accepts, because removing one costs a full re-acquisition. It
    holds the measurement lock, so it cannot delete a master a running lane is about
    to load.

`perf`
    Runs one sample per case per arm, holds the measurement lock, enforces the
    complete-command budget, and writes a **receipt per case** derived from the raw
    trace the child wrote. It never overwrites an existing path: a rerun into a used
    directory is refused, because a rerun that overwrites the evidence destroys the
    only witness there is.

`verify`
    Re-reads the raw artifacts and **re-derives every published figure** — flatness,
    sequence, worst-gate aggregation, budget classification, and for C2 rows the
    space and pack accounting read out of the Store file itself. A figure that only
    the collecting process could produce is not evidence.

`report`
    Renders the ladders, bands and the four-axis view. Time is printed and never
    decides.

`self-check`
    Runs every Python self-check, the registry self-check in the binary, the lock
    parity test, and asserts `LAYERFS_CONSTRUCTION_WORKERS=1`.

`calibrate`
    Runs the untimed experiments E1-E4 and records each outcome, failures included.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

HARNESS_ROOT = Path(__file__).resolve().parent
REPO_ROOT = HARNESS_ROOT.parents[2]
sys.path.insert(0, str(HARNESS_ROOT / "shared"))

import analyze  # noqa: E402
import copyladder  # noqa: E402
import invariants  # noqa: E402
import phases as phases_module  # noqa: E402
import receipt  # noqa: E402
import space  # noqa: E402
import trace as trace_module  # noqa: E402

BINARY = HARNESS_ROOT / "target" / "release" / "fs-bench-storage-content"
MANIFEST = HARNESS_ROOT / "Cargo.toml"
TOOLCHAIN = "+1.85.1"
LOCK_PATH = HARNESS_ROOT / ".measurement.lock"
RESULTS_ROOT = REPO_ROOT / "benchmark-results" / "fs-bench-pro-storage-content"

# Cases whose complete command is a declared exception on the <= 25 s list.
# Owner decision D4: over-budget tiers are declared, never cut and never shrunk.
DECLARED_EXCEPTIONS = {
    "payload-create-100m",
    "payload-create-500m",
    "payload-create-chunked-100m",
    "payload-create-chunked-500m",
    "overwrite-fixed-64k-chunk-count-preserve-500m",
    "overwrite-fixed-64k-chunk-count-increase-500m",
    "overwrite-fixed-64k-chunk-count-decrease-500m",
    "payload-random-read-500m",
}


# The registry column that declares whether a row's fixture is acquired once.
# It is the **binary's own** declaration (`registry::Preparation`), read off the
# same table the golden file pins, so no family list is maintained here by hand:
# a family that gains a phase split in Rust is acquired without a second edit, and
# a family that loses one stops being acquired. The driver remains the authority on
# whether a row has a *deferred* oracle: that is read off the trace it wrote.
PREPARED_COLUMN = 12

# Verification budget, separate from the complete-command budget (benchmark_rules
# section 11: performance and verifier timeouts MUST be separate and reported).
VERIFICATION_BUDGET_NS = 60 * 1_000_000_000

# The verification modes. `full` is the only mode whose rows can be admission
# evidence: a sampled or omitted row is `INCOMPLETE`, never `PASS`, so an iteration
# run cannot be mistaken for a campaign.
VERIFICATION_MODES = ("full", "sample", "none")

# The mode an invocation runs in when the caller does not declare one. #184 section
# 10.3 (owner directive, 2026-09-19) states the rule in one sentence: *"Quick is the
# default for iteration (`--lane smoke` and explicit `--case` runs); an admission run
# is `full` or declares itself otherwise and is ineligible."* The two halves are
# separate: making a sampled oracle the default **for evidence** would change a gate
# frozen before measurement, which stays an owner decision — so a full-lane run is
# `full`, and everything narrower is quick. `--verify` always wins over this.
ITERATION_LANE = "smoke"
ITERATION_MODE = "sample"
ADMISSION_MODE = "full"

# The deterministic sample: `max(1, ceil(n/10))` of the row's declared verification
# unit, selected by `index % 10 == 0` in declaration order. One rule, stated once,
# applied wherever a row declares a countable unit - never "the first ten".
SAMPLE_DIVISOR = 10
SAMPLE_RULE = "index % 10 == 0 in declaration order, max(1, ceil(units/10)) units"


def default_verification_mode(lane: str, cases: list[str] | None) -> str:
    """The mode this invocation runs in when the caller declares none.

    An **explicit `--case`** is iteration whatever lane it names, and so is the
    smoke lane; only a whole-lane admission run is `full`. The resolved mode is
    published in every receipt, in the run document and in the report header, and a
    row it sampled or omitted is `INCOMPLETE`, so the default cannot silently
    promote an iteration run into evidence.
    """
    if cases:
        return ITERATION_MODE
    return ADMISSION_MODE if lane == "full" else ITERATION_MODE


def sample_size(units: int) -> int:
    """The deterministic 10% sample of a declared verification unit count."""
    if units <= 0:
        return 1
    return max(1, (units + SAMPLE_DIVISOR - 1) // SAMPLE_DIVISOR)


# Fields a reused proof must agree on. The pair is only comparable when the tree,
# the product, the harness and the registry are the same: a rebuilt artifact
# invalidates its matched arm, and a harness change invalidates the pair.
REUSED_PROOF_IDENTITY_FIELDS = (
    "source_commit",
    "harness_binary_sha256",
    # The harness's own Python. It decides what a run *does* — the verification
    # mode and its default, the acquisition decision, the phase composition — and a
    # proof offered against a different `runner.py` is a proof of a different run.
    "harness_python_sha256",
    "product_lock_sha256",
    "harness_lock_sha256",
    "registry_tsv_sha256",
    "construction_workers",
)


class ReuseRefused(Exception):
    """A reused proof was offered and refused, with the reason."""


# The declared fixture-recipe version. It is bumped when the *recipe* a family
# builds its fixture from changes — the generator, the offsets, the profile, the
# declared sizes — and when what an artifact *holds* for one changes. It is
# deliberately not the harness binary.
#
# v2 is the packed artifact: the object set is one indexed payload file instead of
# one file per object, the references and advisory predecessors are persisted
# beside each object's role, and an artifact carries named scalars, identities and
# expectations. Every one of those changes the bytes a recipe produces, which is
# exactly the case this version exists to declare. A v1 master is superseded rather
# than consumed.
FIXTURE_RECIPE_VERSION = "fs-bench-fixture-recipe-v2"


def compatibility_digest(row: list[str], identity: receipt.Identity) -> str:
    """The compatibility key of one prepared artifact.

    Owner ruling 3 (2026-09-19): *the digest key is the product identity plus a
    declared fixture-recipe version.* The producer binary is recorded in the seal as
    **provenance** and validated on load; it is not part of the key. Including it
    would invalidate every master on every harness edit, leave the campaign
    permanently cold, and buy nothing: a measurement-plumbing change does not change
    the bytes a recipe produces, and when it does, `FIXTURE_RECIPE_VERSION` is what
    says so.

    The key covers the case's own registry declaration, both lockfiles (the product
    identity and the harness's dependency set) and the recipe version. Unknown or
    missing input fails closed at the call site.
    """
    payload = json.dumps(
        {
            "schema": "layerfs-prepared-compatibility-v1",
            "recipe_version": FIXTURE_RECIPE_VERSION,
            "case_id": row[0],
            "family": row[1],
            "admission": row[2],
            "prepared": row[PREPARED_COLUMN],
            "product_lock_sha256": identity.product_lock_sha256,
            "harness_lock_sha256": identity.harness_lock_sha256,
        },
        sort_keys=True,
    )
    return receipt.sha256_bytes(payload.encode("utf-8"))


def read_seal(marker: Path) -> dict[str, str]:
    """The key/value lines a sealed artifact carries."""
    if not marker.exists():
        return {}
    recorded: dict[str, str] = {}
    for line in marker.read_text(encoding="utf-8").splitlines():
        key, _, value = line.partition("\t")
        recorded[key] = value
    return recorded


class RunnerError(Exception):
    """The run cannot proceed as asked."""


def cargo(*arguments: str, check: bool = True) -> subprocess.CompletedProcess:
    """Runs one cargo command against the harness manifest."""
    command = ["cargo", TOOLCHAIN, *arguments, "--manifest-path", str(MANIFEST)]
    return subprocess.run(command, capture_output=True, text=True, check=check)


def build(release: bool = True, locked: bool = True) -> None:
    """Builds the harness binary."""
    arguments = ["build"]
    if release:
        arguments.append("--release")
    if locked:
        arguments.append("--locked")
    result = cargo(*arguments, check=False)
    if result.returncode != 0:
        raise RunnerError(f"cargo build failed:\n{result.stderr[-4000:]}")


def registry_rows(*extra: str) -> list[str]:
    """The case IDs the binary lists for the given selection."""
    result = subprocess.run(
        [str(BINARY), "--list", "--format", "tsv", *extra],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RunnerError(f"the binary could not list the registry: {result.stderr.strip()}")
    return [
        line.split("\t")[0]
        for line in result.stdout.splitlines()
        if line and not line.startswith("id\t")
    ]


def registry_self_check() -> dict[str, object]:
    """Runs the binary's registry self-check and parses its counts."""
    result = subprocess.run(
        [str(BINARY), "--self-check"], capture_output=True, text=True, check=False
    )
    fields: dict[str, object] = {"exit_code": result.returncode}
    for line in result.stdout.splitlines():
        if ":" in line:
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    if result.returncode != 0:
        fields["status"] = "FAIL"
        fields["stderr"] = result.stderr.strip()
    else:
        fields["status"] = "PASS"
    return fields


def golden_matches() -> bool:
    """Whether the committed golden table is what the registry renders."""
    temporary = HARNESS_ROOT / "target" / "registry-rendered.tsv"
    temporary.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [str(BINARY), "--emit-registry-tsv", str(temporary)],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return False
    rendered = temporary.read_text(encoding="utf-8")
    golden = (HARNESS_ROOT / "tests" / "golden" / "registry.tsv").read_text(encoding="utf-8")
    return rendered == golden


def environment() -> dict[str, str]:
    """The environment every measured child runs under."""
    env = dict(os.environ)
    # One construction worker for every case except namespace init. No run raises
    # it and no second lane is added to pass a gate.
    env["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    return env


def assert_workers(identity: receipt.Identity) -> None:
    if identity.construction_workers != "1":
        raise RunnerError(
            "LAYERFS_CONSTRUCTION_WORKERS must be exported as 1 in every receipt; "
            f"it is {identity.construction_workers!r}"
        )


def cmd_list(arguments: argparse.Namespace) -> int:
    extra = ["--lane", arguments.lane, "--format", arguments.format]
    if arguments.format == "tsv":
        print(subprocess.run([str(BINARY), "--list", *extra], capture_output=True, text=True).stdout, end="")
    else:
        for case_id in registry_rows("--lane", arguments.lane):
            print(case_id)
    return 0


def artifact_root() -> Path:
    """Where prepared artifacts live. Immutable once sealed."""
    return RESULTS_ROOT / "prepared"


def acquire(case_id: str, root: Path, identity: receipt.Identity, row: list[str]) -> dict[str, object]:
    """Acquires one row's artifact once, keyed by its compatibility digest.

    Reuse is decided by the **compatibility digest**, never by the harness binary
    (owner ruling 3). The producer binary and source commit are read off the seal and
    published as provenance, so a reader can always tell which build produced the
    bytes that were measured — and a measurement-plumbing-only harness change does
    not throw the cache away.

    An entry whose digest does not match, or which records no digest at all, is
    **superseded**: it is moved aside rather than deleted, and a fresh acquisition
    takes its place. An entry that exists without a seal is an interrupted
    acquisition and is refused, not consumed.
    """
    compatibility = compatibility_digest(row, identity)
    destination = root / case_id
    marker = destination / "sealed.tsv"
    recorded = read_seal(marker)
    producer = recorded.get("producer_harness_sha256") or recorded.get("harness_sha256", "")
    if recorded.get("compatibility") == compatibility:
        validation = validate_artifact(destination, recorded)
        return {
            "case_id": case_id,
            "state": "reused" if validation["validation"] != "refused" else "not-produced",
            "reason": validation.get("reason"),
            "artifact": str(destination),
            "compatibility": compatibility,
            "recipe_version": FIXTURE_RECIPE_VERSION,
            "producer_harness_sha256": producer,
            "producer_is_current_binary": producer == (identity.harness_binary_sha256 or ""),
            "producer_commit": recorded.get("producer_commit", ""),
            "sealed_digest": recorded.get("manifest_sha256", ""),
            **validation,
        }
    if destination.exists() and not marker.exists():
        return {
            "case_id": case_id,
            "state": "not-produced",
            "reason": "an unsealed artifact directory exists; an interrupted acquisition is not consumed",
        }
    superseded = None
    if destination.exists():
        superseded = str(
            destination.with_name(
                f"{case_id}.superseded-{compatibility[:12]}-{recorded.get('compatibility', 'unsealed')[:12]}"
            )
        )
        os.rename(destination, superseded)
    destination.mkdir(parents=True)
    acquisition = destination / "acquisition"
    started = time.monotonic_ns()
    result = subprocess.run(
        [str(BINARY), "--case", case_id, "--phase", "prepare",
         "--emit-input", str(destination), "--out", str(acquisition)],
        capture_output=True,
        text=True,
        check=False,
        env=environment(),
    )
    wall_ns = time.monotonic_ns() - started
    record = {
        "case_id": case_id,
        "state": "acquired" if result.returncode == 0 else "failed",
        "artifact": str(destination),
        "compatibility": compatibility,
        "recipe_version": FIXTURE_RECIPE_VERSION,
        "acquisition_wall_ns": wall_ns,
        "producer_harness_sha256": identity.harness_binary_sha256 or "",
        "producer_commit": identity.source_commit,
        "superseded": superseded,
        "superseded_reason": (
            "the sealed artifact records no compatibility digest"
            if superseded and not recorded.get("compatibility")
            else "the sealed artifact names a different compatibility digest"
            if superseded
            else None
        ),
        "exit_code": result.returncode,
        "stdout": result.stdout.strip()[-500:],
        "stderr": result.stderr.strip()[-500:],
    }
    if result.returncode == 0:
        # The completion marker is the child's; the runner appends the provenance,
        # the key, the per-file digests and the seal. The producer binary is
        # provenance, not the key, and the source commit is recorded beside it so a
        # reader can name the build that produced the bytes.
        sidecars = [
            str(path.relative_to(destination))
            for path in destination.rglob("*")
            if path.suffix in {"-wal", "-shm", "-journal"}
        ]
        if sidecars:
            record["state"] = "failed"
            record["reason"] = f"unexpected Store sidecars in the artifact: {sidecars[:3]}"
            record["exit_code"] = 1
            return record
        # The seal record is appended **before** the manifest is computed: the
        # manifest describes every file of the artifact at its final size, and a
        # record appended afterwards would leave `sealed.tsv` recorded one line
        # short — which the reuse-time stat-identity check then reads as a resized
        # master and refuses.
        with (destination / "sealed.tsv").open("a", encoding="utf-8") as handle:
            handle.write(f"compatibility\t{compatibility}\n")
            handle.write(f"recipe_version\t{FIXTURE_RECIPE_VERSION}\n")
            handle.write(f"producer_harness_sha256\t{identity.harness_binary_sha256 or ''}\n")
            handle.write(f"producer_commit\t{identity.source_commit}\n")
            handle.write("validation\tsha256-per-file\n")
        manifest = manifest_of(destination, case_id, compatibility)
        receipt.write_append_only(destination / "manifest.json", manifest)
        # The manifest's own digest is **not** appended to `sealed.tsv`: doing so
        # would grow the file after the manifest recorded its size, and the reuse
        # check would read the master as resized. It is published in the
        # acquisition record (`manifest-<stamp>.json`) and re-derived on reuse.
        manifest_sha256 = hashlib.sha256(
            (destination / "manifest.json").read_bytes()
        ).hexdigest()
        seal_artifact(destination)
        record["manifest_sha256"] = manifest_sha256
        record["validation"] = "sha256-per-file"
        record["data_bytes"] = manifest["data_bytes"]
        record["files"] = len(manifest["files"])
    return record


def artifact_files(destination: Path) -> list[Path]:
    """Every file of one artifact except the manifest that describes them."""
    return sorted(
        path
        for path in destination.rglob("*")
        if path.is_file() and path.name != "manifest.json"
    )


def manifest_of(destination: Path, case_id: str, compatibility: str) -> dict[str, object]:
    """Builds the artifact manifest: one sha256 and one size per file.

    `test_setup_and_cache_discipline.md` section 3 fixes the build order as
    *build -> validate (sha256 per file) -> seal -> write `manifest.json`*, and the
    acquirer performs the validation. The digests are computed here rather than in
    the child because `hashlib` is the accelerated implementation while the
    harness's own SHA-256 is a scalar one, and a 500 MiB artifact hashed at the
    harness's rate would cost more than the acquisition it is validating.
    """
    files: dict[str, dict[str, object]] = {}
    data_bytes = 0
    for path in artifact_files(destination):
        # Streamed: a 500 MiB packed object set is read in bounded chunks rather
        # than held whole beside the artifact it describes.
        digest = hashlib.sha256()
        size = 0
        with path.open("rb") as handle:
            while chunk := handle.read(1 << 22):
                digest.update(chunk)
                size += len(chunk)
        relative = str(path.relative_to(destination))
        files[relative] = {"bytes": size, "sha256": digest.hexdigest()}
        data_bytes += size
    return {
        "schema": "layerfs-prepared-manifest-v1",
        "case_id": case_id,
        "compatibility": compatibility,
        "recipe_version": FIXTURE_RECIPE_VERSION,
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "files": files,
        "data_bytes": data_bytes,
    }


def seal_artifact(destination: Path) -> None:
    """Removes the write bit from every file and directory of a sealed artifact.

    `test_setup_and_cache_discipline.md` section 3: *seal: chmod removes 0o222
    (immutable master)*. It is applied last, after the manifest and the seal
    record are written, so a half-built entry is left writable and therefore
    visibly unfinished.
    """
    for path in sorted(destination.rglob("*"), reverse=True):
        mode = path.stat().st_mode & 0o777
        os.chmod(path, mode & ~0o222)
    os.chmod(destination, (destination.stat().st_mode & 0o777) & ~0o222)


def validate_artifact(destination: Path, record: dict[str, object]) -> dict[str, object]:
    """Checks a reused master against its own manifest without re-hashing it.

    The rule is `test_setup_and_cache_discipline.md` section 3.1: *the master is
    validated once per acquisition; its sealed digest is retained, so repeated
    source rehashing per sample is unnecessary.* Reuse is not an acquisition, so
    this is a stat-identity check — every file the manifest names must still be
    there with the size it recorded — and the digest is retained in the record.

    It is not the only integrity gate. Every object a later phase loads is
    re-identified by `FinalizedObject::new` against the name the index stores it
    under, so a corrupted payload is refused at load rather than silently used.
    """
    manifest_path = destination / "manifest.json"
    if not manifest_path.exists():
        return {"validation": "refused", "reason": "the artifact carries no manifest.json"}
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema") != "layerfs-prepared-manifest-v1":
        return {
            "validation": "refused",
            "reason": f"manifest schema {manifest.get('schema')!r}",
        }
    missing: list[str] = []
    resized: list[str] = []
    for relative, recorded in dict(manifest.get("files", {})).items():
        path = destination / relative
        if not path.exists():
            missing.append(relative)
        elif path.stat().st_size != int(recorded["bytes"]):
            resized.append(relative)
    if missing or resized:
        return {
            "validation": "refused",
            "reason": f"{len(missing)} missing, {len(resized)} resized",
            "missing": missing[:5],
            "resized": resized[:5],
        }
    return {
        "validation": "manifest-stat-identity",
        "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "data_bytes": int(manifest.get("data_bytes", 0)),
        "files": len(manifest.get("files", {})),
    }


def directory_bytes(path: Path) -> int:
    """Bytes one directory tree occupies, by `st_size`."""
    return sum(entry.stat().st_size for entry in path.rglob("*") if entry.is_file())


def remove_artifact(path: Path) -> None:
    """Removes one sealed artifact, taking the write bit back first.

    A sealed master is `0o444`/`0o555` by construction
    (`test_setup_and_cache_discipline.md` section 3), so a removal that did not lift
    the seal would fail on every entry it was asked to prune — which is the state the
    first version of this verb was written against.
    """
    for entry in sorted(path.rglob("*"), reverse=True):
        mode = entry.stat().st_mode & 0o777
        os.chmod(entry, mode | 0o200)
    os.chmod(path, (path.stat().st_mode & 0o777) | 0o200)
    shutil.rmtree(path)


def cmd_prune(arguments: argparse.Namespace) -> int:
    """Removes prepared artifacts the current key no longer accepts.

    **What it never touches.** An artifact whose seal carries the compatibility
    digest the current registry and locks produce is a master this campaign will
    reuse, and removing it costs a full re-acquisition. It is kept, always.

    **What it removes, each with its reason.**

    * `*.superseded-*` — an entry a previous acquisition moved aside when its key
      stopped matching. Nothing reads them.
    * an entry whose seal names a different key — superseded by a recipe or product
      change, and never consumed (`acquire` re-acquires rather than reuse it).
    * an unsealed directory — an interrupted acquisition. `acquire` refuses to
      consume one, so it is dead weight.

    It holds the measurement lock, because deleting a master a running lane is about
    to load would corrupt that lane rather than fail it. `--dry-run` reports and
    removes nothing.
    """
    root = Path(arguments.out) if arguments.out else artifact_root()
    if not root.exists():
        print(f"prune: {root} does not exist; nothing to prune")
        return 0
    if not BINARY.exists():
        build()
    identity = receipt.identify(REPO_ROOT, HARNESS_ROOT, BINARY)
    rows = {row[0]: row for row in registry_table()}

    kept: list[tuple[str, int]] = []
    removed: list[dict[str, object]] = []
    with receipt.measurement_lock(LOCK_PATH):
        for entry in sorted(root.iterdir()):
            if not entry.is_dir():
                continue
            if ".superseded-" in entry.name:
                reason = "a superseded entry a previous acquisition moved aside"
            else:
                row = rows.get(entry.name)
                seal = read_seal(entry / "sealed.tsv")
                if row is None:
                    reason = "no registered case of this name"
                elif not seal:
                    reason = "an unsealed directory; an interrupted acquisition is never consumed"
                elif seal.get("compatibility") == compatibility_digest(row, identity):
                    kept.append((entry.name, directory_bytes(entry)))
                    continue
                else:
                    reason = "sealed under a different compatibility digest"
            size = directory_bytes(entry)
            if not arguments.dry_run:
                remove_artifact(entry)
            removed.append({"entry": entry.name, "bytes": size, "reason": reason})

    reclaimed = sum(int(item["bytes"]) for item in removed)
    kept_bytes = sum(size for _, size in kept)
    document = {
        "schema": "layerfs-prepared-prune-v1",
        "root": str(root),
        "dry_run": bool(arguments.dry_run),
        "harness_sha256": identity.harness_binary_sha256 or "",
        "harness_python_sha256": identity.harness_python_sha256,
        "compatibility_inputs": {
            "product_lock_sha256": identity.product_lock_sha256,
            "harness_lock_sha256": identity.harness_lock_sha256,
            "recipe_version": FIXTURE_RECIPE_VERSION,
        },
        "kept": [{"entry": name, "bytes": size} for name, size in kept],
        "kept_bytes": kept_bytes,
        "removed": removed,
        "removed_bytes": reclaimed,
        "note": (
            "a kept entry is one the current key still accepts; removing it would cost "
            "a full re-acquisition"
        ),
    }
    # A nanosecond stamp, because a second-resolution one collides when a dry run and
    # an apply happen inside the same second — and `write_append_only` refuses the
    # second write *after* the entries are already gone, which loses the record of a
    # removal that happened. The record is written before the report for the same
    # reason: a prune that fails to record itself has removed evidence.
    stamp = time.strftime("%Y-%m-%dT%H%M%SZ", time.gmtime())
    receipt.write_append_only(root / f"prune-{stamp}-{time.time_ns()}.json", document)
    for item in removed:
        print(f"  {'would remove' if arguments.dry_run else 'removed'}  {item['bytes'] / 1e6:9.1f} MB  {item['entry']}  ({item['reason']})")
    print(
        f"prune: {len(kept)} kept ({kept_bytes / 1e9:.3f} GB), "
        f"{len(removed)} {'to remove' if arguments.dry_run else 'removed'} "
        f"({reclaimed / 1e9:.3f} GB) at {root}"
    )
    return 0


def load_reused_proof(path: str, identity: receipt.Identity, selection: list[str]) -> dict[str, object]:
    """Validates a verification receipt offered in place of re-running verification.

    Fails closed on every way the offer can be wrong, and each check exists because
    accepting without it would let one tree's proof admit another tree's row:

    * **schema** — it must be a `layerfs-core-verification-v1` document;
    * **identity** — source commit, harness binary, both lockfiles, the registry
      table and the worker count must all match the run about to be admitted;
    * **hard limits** — the proof must record zero disagreements, a sealed
      call-graph `PASS`, runtime tripwires `PASS` and a verification wall inside its
      own 60 s budget;
    * **coverage** — every selected case must appear with `status == PASS` and a
      published status of `PASS`. A case the proof does not cover is not covered by
      the omission either.

    The caller records `reused_proof_identities` and the omission on every row it
    covers, so the reuse is visible in the receipt rather than inferred from a
    shorter wall time.
    """
    document = json.loads(Path(path).read_text(encoding="utf-8"))
    if document.get("schema") != "layerfs-core-verification-v1":
        raise ReuseRefused(f"{path}: schema {document.get('schema')!r} is not a verification receipt")
    previous_run = Path(str(document.get("run_dir", ""))) / "run.json"
    if not previous_run.exists():
        raise ReuseRefused(f"{path}: the run it verified ({previous_run}) is not on disk")
    previous = json.loads(previous_run.read_text(encoding="utf-8"))
    previous_identity = dict(previous.get("identity", {}))
    mismatched = [
        field
        for field in REUSED_PROOF_IDENTITY_FIELDS
        if str(previous_identity.get(field)) != str(identity.as_fields().get(field))
    ]
    if mismatched:
        raise ReuseRefused(
            "the offered proof was produced on a different pair: "
            + ", ".join(
                f"{field} {previous_identity.get(field)!r} != {identity.as_fields().get(field)!r}"
                for field in mismatched
            )
        )
    if int(document.get("disagreements", 1)) != 0:
        raise ReuseRefused(f"{path}: {document.get('disagreements')} disagreement(s) recorded")
    call_graph = document.get("sealed_call_graph", {}) or {}
    tripwires = document.get("runtime_tripwires", {}) or {}
    if call_graph.get("status") != "PASS":
        raise ReuseRefused(f"{path}: sealed call-graph is {call_graph.get('status')!r}")
    if tripwires.get("status") != "PASS":
        raise ReuseRefused(f"{path}: runtime tripwires are {tripwires.get('status')!r}")
    wall_ns = int(document.get("wall_ns", 0))
    if receipt.verification_budget(wall_ns).status != "PASS":
        raise ReuseRefused(f"{path}: its own verification wall {wall_ns / 1e9:.3f} s exceeds the 60 s budget")
    proofs: dict[str, dict[str, object]] = {}
    for finding in document.get("findings", []) or []:
        case_id = str(finding.get("case_id", ""))
        if not case_id:
            continue
        if finding.get("status") != "PASS" or finding.get("published_status") != "PASS":
            continue
        phases = finding.get("phases", {}) or {}
        if phases.get("status") != "PASS":
            continue
        proofs[case_id] = {
            "proof": str(path),
            "identities": {
                field: previous_identity.get(field) for field in REUSED_PROOF_IDENTITY_FIELDS
            },
        }
    # Coverage is required for every selected case the verified run actually ran. A
    # row whose driver does not exist is `NOT_RUN` in both runs: it has no oracle to
    # reuse, and requiring a proof for it would make `--reuse-pass` unusable on any
    # lane that contains one.
    previous_rows = previous_run.parent
    measured = [
        case_id
        for case_id in selection
        if (previous_rows / case_id / "receipt.json").exists()
        and json.loads((previous_rows / case_id / "receipt.json").read_text(encoding="utf-8")).get(
            "status"
        )
        != "NOT_RUN"
    ]
    uncovered = [case_id for case_id in measured if case_id not in proofs]
    if uncovered:
        raise ReuseRefused(
            f"{path} covers no passing proof for {len(uncovered)} measured case(s): "
            + ", ".join(uncovered[:5])
        )
    return {
        "path": str(path),
        "identities": {
            field: previous_identity.get(field) for field in REUSED_PROOF_IDENTITY_FIELDS
        },
        "verified_run": str(previous_run.parent),
        "cases": proofs,
    }


def cmd_prepare(arguments: argparse.Namespace) -> int:
    """Acquires the prepared artifacts a selection needs, once."""
    root = Path(arguments.out) if arguments.out else artifact_root()
    root.mkdir(parents=True, exist_ok=True)
    if not BINARY.exists():
        build()
    identity = receipt.identify(REPO_ROOT, HARNESS_ROOT, BINARY)
    harness_sha = identity.harness_binary_sha256 or ""
    manifest = {
        "schema": "layerfs-prepared-manifest-v1",
        "root": str(root),
        "acquired_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "harness_sha256": harness_sha,
        "producer_commit": identity.source_commit,
        "recipe_version": FIXTURE_RECIPE_VERSION,
        "key": "the product identity plus the declared fixture-recipe version; the producer binary is provenance",
        "cases": [],
    }
    selected = arguments.case or registry_rows()
    for case_id in selected:
        family = family_of(case_id)
        if not prepared_of(case_id).strip("-"):
            manifest["cases"].append(
                {
                    "case_id": case_id,
                    "state": "not-produced",
                    "reason": f"{family} declares no prepared master; its fixture is built inside its one invocation",
                }
            )
            continue
        manifest["cases"].append(acquire(case_id, root, identity, row_of(case_id)))
    receipt.write_append_only(root / f"manifest-{manifest['acquired_utc'].replace(':', '')}.json", manifest)
    print(f"prepare: {len(manifest['cases'])} case(s) considered at {root}")
    return 0


def row_of(case_id: str) -> list[str]:
    """The registry row of one case, so the digest key is the declaration itself."""
    for row in registry_table():
        if row[0] == case_id:
            return row
    raise RunnerError(f"{case_id} is not in the binary's registry")


def prepared_of(case_id: str) -> str:
    """The registry's own preparation declaration for one case, or `""`."""
    for row in registry_table():
        if row[0] == case_id:
            return row[PREPARED_COLUMN] if len(row) > PREPARED_COLUMN else ""
    return ""


def family_of(case_id: str) -> str:
    """The family a case belongs to, read from the binary's own registry."""
    for row in registry_table():
        if row[0] == case_id:
            return row[1]
    return ""


_REGISTRY_TABLE: list[list[str]] | None = None


def registry_table() -> list[list[str]]:
    """The binary's whole registry as rows, so no family list is maintained by hand.

    It is the binary's own `--emit-registry-tsv` rendering — the same table the
    golden file pins and the registry self-check re-derives — rather than the
    three-column `--list` summary, because the preparation declaration is one of
    its columns and a summary that dropped it would put the decision back in a
    hand-maintained list here. Read once per process and cached.
    """
    global _REGISTRY_TABLE
    if _REGISTRY_TABLE is None:
        temporary = HARNESS_ROOT / "target" / "registry-table.tsv"
        temporary.parent.mkdir(parents=True, exist_ok=True)
        result = subprocess.run(
            [str(BINARY), "--emit-registry-tsv", str(temporary)],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise RunnerError(f"the binary could not render its registry: {result.stderr.strip()}")
        lines = temporary.read_text(encoding="utf-8").splitlines()
        _REGISTRY_TABLE = [line.split("\t") for line in lines[1:] if "\t" in line]
    return _REGISTRY_TABLE


def run_case(
    case_id: str,
    run_dir: Path,
    identity: receipt.Identity,
    verification_mode: str = "full",
    reused_proof: dict[str, object] | None = None,
) -> dict[str, object]:
    """Runs one case, enforces the budget, and writes its derived receipt."""
    case_dir = run_dir / case_id
    if case_dir.exists():
        raise RunnerError(f"{case_dir} exists; a run never overwrites evidence")
    # The child creates its own output directory and refuses an existing one, so
    # the append-only rule is enforced at the producer rather than by the runner
    # creating the path first and then refusing the child its own directory.
    declared_exception = case_id in DECLARED_EXCEPTIONS
    acquisition: dict[str, object] = {}
    command = [str(BINARY), "--case", case_id, "--out", str(case_dir)]
    if prepared_of(case_id).strip("-"):
        artifact = artifact_root() / case_id
        acquisition = acquire(case_id, artifact_root(), identity, row_of(case_id))
        command += ["--load-input", str(artifact)]
    started = time.monotonic_ns()
    result = subprocess.run(
        command,
        capture_output=True,
        text=True,
        check=False,
        env=environment(),
    )
    wall_ns = time.monotonic_ns() - started
    # The verification phase is a second invocation with its own budget and its own
    # timing scope. It appends its gates to the same trace, so the row's status and
    # the verifier's re-derivation both see one record set.
    verification: dict[str, object] = {}
    sample_units = 0
    sample_selected = 0
    # The driver is the authority on whether this row has a deferred oracle: the
    # trace it just wrote says so, and nothing here decides it from a family list.
    trace_path = case_dir / "trace.jsonl"
    deferred = trace_path.exists() and "oracle_phase: verify-invocation" in trace_path.read_text(
        encoding="utf-8", errors="replace"
    )
    if verification_mode == "none":
        # The mode ladder's `none` rung: the deferred oracle is not run at all, and
        # the row says so rather than reporting a verification it did not perform.
        deferred = False
    reused_row = (reused_proof or {}).get("cases", {}).get(case_id) if reused_proof else None
    if deferred and reused_row is not None:
        # `--reuse-pass`: this row's deferred verification is an identity-matched
        # `PASS` receipt already on disk, so the phase is not re-run. The omission is
        # recorded on the row, never implied.
        verification = {
            "phase": "verify",
            "status": "REUSED",
            "reused_proof": reused_row["proof"],
            "reused_proof_identities": reused_row["identities"],
            "omission": (
                "the deferred verification invocation was not re-run; an identity-matched "
                "PASS receipt was accepted instead"
            ),
            "wall_ns": 0,
        }
        deferred = False
    if deferred:
        verification_started = time.monotonic_ns()
        verify_command = [
            str(BINARY), "--case", case_id, "--phase", "verify",
            "--load-input", str(artifact_root() / case_id), "--out", str(case_dir),
        ]
        if verification_mode == "sample":
            # The declared unit is the row's own: the runner does not know how many
            # members a delta row holds until the child says so, and the child reads
            # it from the artifact. `--verify-sample 0` asks the child to declare the
            # unit and take the deterministic 10% of it.
            verify_command += ["--verify-sample", "0"]
        verified = subprocess.run(
            verify_command,
            capture_output=True,
            text=True,
            check=False,
            env=environment(),
        )
        verification_ns = time.monotonic_ns() - verification_started
        verification = {
            "phase": "verify",
            "exit_code": verified.returncode,
            "wall_ns": verification_ns,
            "budget_ns": VERIFICATION_BUDGET_NS,
            "status": "PASS" if verification_ns <= VERIFICATION_BUDGET_NS else "FAIL",
            "sample_units": sample_units,
            "sample_selected": sample_selected,
            "sample_rule": SAMPLE_RULE if sample_units else "",
            "stdout": verified.stdout.strip()[-500:],
            "stderr": verified.stderr.strip()[-500:],
        }
    # The four declared phases, composed from the child's own publication and the
    # runner's process wall for each invocation. `complete_command_ns` stays the
    # performance invocation's wall, which is what the complete-command budget
    # classifies; the deferred verification invocation keeps its own 60 s budget.
    # A reused verification ran no process, so it has no wall and published no
    # `phases-verify.json`: it is named as a reuse rather than composed as an
    # invocation. Everything else keeps its own invocation record.
    walls = {"perf": wall_ns}
    reused_invocations: list[str] = []
    if verification:
        if verification.get("status") == "REUSED":
            reused_invocations.append("verify")
        else:
            walls["verify"] = int(verification["wall_ns"])
    parsed = trace_module.read(case_dir / "trace.jsonl")
    # A row whose driver does not exist ran nothing, so it is not required to
    # publish an operation. Every row that did run a driver is.
    declared_phases = phases_module.compose(
        case_dir,
        walls,
        expects_operation=parsed.status() != "NOT_RUN",
        reused_invocations=tuple(reused_invocations),
    )
    # The budgeted quantity is the formula `CONTRACT.md` section 4 fixes: the four
    # declared phases plus a declared lifecycle allowance, never the raw wall.
    declared_ns = (
        int(declared_phases.get("preparation_wall_ns", 0))
        + int(declared_phases.get("operation_ns", 0))
        + int(declared_phases.get("verification_wall_ns", 0))
        + int(declared_phases.get("cleanup_wall_ns", 0))
    )
    budget = receipt.budget(wall_ns, declared_exception, declared_ns=declared_ns)
    if verification.get("phase") == "verify":
        # The child declares the verification unit and the sample it took; the
        # runner does not guess either from a family name.
        observed = parsed.counters()
        sample_units = int(observed.get("verify.units", 0))
        sample_selected = int(observed.get("verify.sampled", 0))
        verification["sample_units"] = sample_units
        verification["sample_selected"] = sample_selected
        verification["sample_rule"] = SAMPLE_RULE if sample_units else ""
    status = parsed.status()
    identity_fields = parsed.identity()
    gates = [
        {
            "class": gate.gate_class,
            "id": gate.identifier,
            "status": gate.status,
            "measured": gate.measured,
            "limit": gate.limit,
        }
        for gate in parsed.gates()
    ]
    if budget.status != "PASS":
        status = "NOT_RUN"
        gates.append(
            {
                "class": "G6",
                "id": "budget.complete-command",
                "status": "NOT_RUN",
                "measured": f"{wall_ns / 1e9:.3f} s",
                "limit": f"<= {budget.limit_ns / 1e9:.0f} s",
            }
        )
    # The phase reconciliation fails closed: a row whose declared phases do not
    # account for its wall is `INCOMPLETE`, never a quietly passing row. A row that
    # is already worse than `INCOMPLETE` keeps its own status, because a `NOT_RUN`
    # row has no measurement to reconcile.
    if verification_mode != "full" and trace_module.SEVERITY["INCOMPLETE"] > trace_module.SEVERITY.get(status, 5):
        # A sampled or omitted row is `INCOMPLETE`, never `PASS`. An iteration run
        # therefore cannot be mistaken for admission evidence, which is the whole
        # reason the mode is published in the receipt as well as in the report.
        gates.append(
            {
                "class": "G7",
                "id": "g7.verification-mode",
                "status": "INCOMPLETE",
                "measured": f"verification mode {verification_mode}",
                "limit": "admission evidence requires verification mode full",
            }
        )
        status = "INCOMPLETE"
    reconciliation = declared_phases["reconciliation"]
    if reconciliation["status"] != "PASS":
        gates.append(
            {
                "class": "G7",
                "id": "g7.phase-reconciliation",
                "status": "INCOMPLETE",
                "measured": "; ".join(reconciliation["problems"]) or "unreconciled",
                "limit": "preparation + operation + verification + cleanup <= the wall, "
                "within the declared tolerance",
            }
        )
        if trace_module.SEVERITY["INCOMPLETE"] > trace_module.SEVERITY.get(status, 5):
            status = "INCOMPLETE"
    # The two axes the resource document names and no receipt carried before round
    # 5b: the measured region's CPU and the child's peak resident set. Both are read
    # at the phase boundary by the child and composed here, so no driver chooses them.
    resources = dict(parsed.resources())
    if declared_phases.get("cpu_user_ns") is not None:
        resources["cpu.user_ns"] = declared_phases["cpu_user_ns"]
        resources["cpu.system_ns"] = declared_phases["cpu_system_ns"]
    if declared_phases.get("process_peak_rss_bytes") is not None:
        resources["rss.process_peak_bytes"] = declared_phases["process_peak_rss_bytes"]
    # Storage, per row: the bytes the row's prepared master occupies, or zero when it
    # declares none. The per-sample Store readings stay where they were, on the rows
    # that gate them.
    resources["artifact.data_bytes"] = int(acquisition.get("data_bytes", 0) or 0)

    document: dict[str, object] = {
        "schema": receipt.SCHEMA,
        "case_id": case_id,
        "family": identity_fields.get("family", ""),
        "admission": identity_fields.get("admission", ""),
        "tier_label": identity_fields.get("tier_label", ""),
        "profile": identity_fields.get("profile", ""),
        "declared_bytes": int(identity_fields.get("declared_bytes", 0) or 0),
        "declared_entries": int(identity_fields.get("declared_entries", 0) or 0),
        "shape": identity_fields.get("shape", ""),
        "status": status,
        "gates": gates,
        "counters": parsed.counters(),
        "resources": resources,
        "notes": parsed.notes(),
        "trace_defects": parsed.defects,
        "child": {
            "exit_code": result.returncode,
            "stdout": result.stdout.strip(),
            "stderr": result.stderr.strip()[-2000:],
        },
        "wall_ns": wall_ns,
        "phases": declared_phases,
        "verification_mode": verification_mode,
        "budget": budget.as_fields(),
        "acquisition": acquisition,
        "verification": verification,
        "identity": identity.as_fields(),
        "receipt_path": str(case_dir / "receipt.json"),
    }
    receipt.write_append_only(case_dir / "receipt.json", document)
    return document


def cmd_perf(arguments: argparse.Namespace) -> int:
    if arguments.no_build is False:
        build()
    run_dir = Path(arguments.out)
    if run_dir.exists():
        raise RunnerError(f"{run_dir} exists; a run never overwrites evidence")
    run_dir.mkdir(parents=True)
    identity = receipt.identify(REPO_ROOT, HARNESS_ROOT, BINARY)
    assert_workers(identity)
    selection = arguments.case or registry_rows("--lane", arguments.lane)
    verification_mode = arguments.verify or default_verification_mode(
        arguments.lane, arguments.case
    )
    print(
        f"perf: verification mode {verification_mode}"
        + ("" if arguments.verify else " (the declared default for this selection)")
    )
    reused_proof = None
    if arguments.reuse_pass:
        try:
            reused_proof = load_reused_proof(arguments.reuse_pass, identity, selection)
        except (ReuseRefused, OSError, json.JSONDecodeError) as error:
            raise RunnerError(f"--reuse-pass refused: {error}") from error
    run_document = {
        "schema": "layerfs-core-run-v1",
        "run_dir": str(run_dir),
        "lane": arguments.lane,
        "selection": selection,
        "selection_size": len(selection),
        "cache_state": "declared per row; never pooled",
        "samples_per_case_per_arm": 1,
        "declared_exceptions": sorted(DECLARED_EXCEPTIONS & set(selection)),
        "verification_mode": verification_mode,
        "verification_sample_rule": SAMPLE_RULE if verification_mode == "sample" else "",
        "reused_proof_identities": (reused_proof or {}).get("identities"),
        "reused_proof": (reused_proof or {}).get("path"),
        "reused_proof_omission": (
            "the deferred verification invocation was not re-run for the rows the accepted "
            "proof covers; each such row records `reused_proof_identities` and the omission"
            if reused_proof
            else None
        ),
        "registry_self_check": registry_self_check(),
        "golden_matches": golden_matches(),
        "identity": identity.as_fields(),
    }
    started = time.monotonic_ns()
    rows = []
    with receipt.measurement_lock(LOCK_PATH):
        for case_id in selection:
            try:
                rows.append(
                    run_case(
                        case_id,
                        run_dir,
                        identity,
                        verification_mode=verification_mode,
                        reused_proof=reused_proof,
                    )
                )
            except RunnerError as error:
                rows.append(
                    {
                        "case_id": case_id,
                        "status": "NOT_RUN",
                        "notes": [f"runner refused: {error}"],
                    }
                )
    run_document["wall_ns"] = time.monotonic_ns() - started
    run_document["rows"] = len(rows)
    run_document["phases"] = lane_phase_totals(rows)
    tally: dict[str, int] = {}
    for row in rows:
        tally[row["status"]] = tally.get(row["status"], 0) + 1
    run_document["tally"] = tally
    receipt.write_append_only(run_dir / "run.json", run_document)
    write_manifest(run_dir)
    for key in sorted(tally):
        print(f"  {key:<12} {tally[key]}")
    print(f"perf: {len(rows)} case(s) in {run_document['wall_ns'] / 1e9:.1f} s -> {run_dir}")
    return 0


def lane_phase_totals(rows: list[dict[str, object]]) -> dict[str, object]:
    """The lane's published phase totals, per status.

    `sum(operation_ns)` is published beside the per-row numbers so a row that got
    faster at another row's expense is visible rather than averaged away. It is
    also the round's falsifier: if the golden total falls, measured work moved into
    setup and the change is rejected.
    """
    fields = (
        "preparation_wall_ns",
        "acquisition_wall_ns",
        "operation_ns",
        "verification_wall_ns",
        "cleanup_wall_ns",
        "handoff_ns",
        "complete_command_ns",
        "verification_invocation_ns",
    )
    totals: dict[str, object] = {"schema": "layerfs-lane-phases-v1", "by_status": {}}
    by_status: dict[str, dict[str, int]] = {}
    for row in rows:
        declared = row.get("phases")
        if not isinstance(declared, dict):
            continue
        bucket = by_status.setdefault(str(row.get("status", "")), {"rows": 0, **{f: 0 for f in fields}})
        bucket["rows"] += 1
        for field in fields:
            bucket[field] += int(declared.get(field, 0) or 0)
    totals["by_status"] = by_status
    admitted = by_status.get("PASS", {})
    totals["admission"] = {field: admitted.get(field, 0) for field in fields}
    totals["unreconciled_rows"] = sorted(
        str(row.get("case_id"))
        for row in rows
        if isinstance(row.get("phases"), dict)
        and row["phases"].get("reconciliation", {}).get("status") != "PASS"
    )
    return totals


def write_manifest(run_dir: Path) -> Path:
    """Hashes every retained file into the run manifest."""
    entries = []
    for path in sorted(run_dir.rglob("*")):
        if path.is_file() and path.name != "manifest.json":
            entries.append(
                {
                    "path": str(path.relative_to(run_dir)),
                    "bytes": path.stat().st_size,
                    "sha256": receipt.sha256_file(path),
                }
            )
    return receipt.write_append_only(
        run_dir / "manifest.json",
        {"schema": "layerfs-core-manifest-v1", "files": entries, "count": len(entries)},
    )


def cmd_verify(arguments: argparse.Namespace) -> int:
    run_dir = Path(arguments.run)
    if not run_dir.is_dir():
        raise RunnerError(f"{run_dir} is not a run directory")
    started = time.monotonic_ns()
    if arguments.reuse_pass:
        # `--reuse-pass` accepts an identity-matched `PASS` receipt **instead of**
        # re-running verification, as `AGENTS.md` section 2 requires. The accepted
        # receipt is written beside the run with `reused_proof_identities` and an
        # explicit omission, so a reader can always tell a re-derivation from a
        # reuse. It never claims to have re-derived anything.
        # The pair that must match is the one the **run** recorded, not this
        # process's environment: `verify` re-derives a run that already exists, and
        # comparing against the verifier's own shell would refuse a valid proof for
        # a worker count the verifier does not export.
        run_json = run_dir / "run.json"
        if run_json.exists():
            identity = receipt.Identity(**{
                field: json.loads(run_json.read_text(encoding="utf-8"))
                .get("identity", {})
                .get(field, "")
                for field in receipt.Identity.__dataclass_fields__
            })
        else:
            identity = receipt.identify(REPO_ROOT, HARNESS_ROOT, BINARY)
        cases = sorted(entry.name for entry in run_dir.iterdir() if entry.is_dir())
        try:
            accepted = load_reused_proof(arguments.reuse_pass, identity, cases)
        except (ReuseRefused, OSError, json.JSONDecodeError) as error:
            raise RunnerError(f"--reuse-pass refused: {error}") from error
        document = {
            "schema": "layerfs-core-verification-v1",
            "run_dir": str(run_dir),
            "cases": len(cases),
            "disagreements": 0,
            "findings": [],
            "sealed_call_graph": {"status": "REUSED", "files_scanned": 0},
            "runtime_tripwires": {"status": "REUSED", "stores_checked": 0},
            "wall_ns": time.monotonic_ns() - started,
            "reused_proof": accepted["path"],
            "reused_proof_identities": accepted["identities"],
            "reused_proof_verified_run": accepted["verified_run"],
            "omission": (
                "verification was not re-derived: an identity-matched PASS receipt was "
                "accepted instead, for every case in this run"
            ),
            "mode": "reused",
        }
        document["budget"] = receipt.verification_budget(document["wall_ns"]).as_fields()
        receipt.write_append_only(run_dir / arguments.tag, document)
        print(
            f"verify: reused {accepted['path']} for {len(cases)} case(s), "
            f"0 re-derived, {document['wall_ns'] / 1e9:.2f} s"
        )
        return 0
    findings: list[dict[str, object]] = []
    disagreements = 0
    with receipt.measurement_lock(LOCK_PATH):
        for case_dir in sorted(entry for entry in run_dir.iterdir() if entry.is_dir()):
            receipt_path = case_dir / "receipt.json"
            trace_path = case_dir / "trace.jsonl"
            if not receipt_path.exists():
                findings.append({"case_id": case_dir.name, "status": "INCOMPLETE", "detail": "no receipt.json"})
                continue
            document = json.loads(receipt_path.read_text(encoding="utf-8"))
            parsed = trace_module.read(trace_path)
            published_phases = document.get("phases", {}) or {}
            budget = receipt.budget(
                int(document.get("wall_ns", 0)),
                case_dir.name in DECLARED_EXCEPTIONS,
                declared_ns=sum(
                    int(published_phases.get(field, 0) or 0)
                    for field in (
                        "preparation_wall_ns",
                        "operation_ns",
                        "verification_wall_ns",
                        "cleanup_wall_ns",
                    )
                ),
            )
            # The complete-command budget is a *published* figure too, and it can
            # override a row: a case whose gates all held but whose command
            # overran its limit is `NOT_RUN` with the measured wall time. The
            # re-derivation therefore applies the same rule from the recorded wall
            # time rather than comparing against the trace's gate set alone — the
            # first pass of this verifier did exactly that and reported four
            # budget-overridden rows as disagreements, which is how the omission
            # was found.
            trace_status = parsed.status()
            re_derived = trace_status
            if budget.status != "PASS" and (
                trace_module.SEVERITY["NOT_RUN"] > trace_module.SEVERITY.get(trace_status, 5)
            ):
                re_derived = "NOT_RUN"
            published = document.get("status")
            record: dict[str, object] = {
                "case_id": case_dir.name,
                "published_status": published,
                "re_derived_status": re_derived,
                "trace_status": trace_status,
                "trace_defects": parsed.defects,
                "gates_re_derived": len(parsed.gates()),
                "gates_published": len(document.get("gates", [])),
                "budget_status_re_derived": budget.status,
            }
            if re_derived != published:
                disagreements += 1
                record["disagreement"] = f"{published} published, {re_derived} re-derived"
            record["budget_status"] = budget.status
            record["phases"] = re_derive_phases(case_dir, document)
            if record["phases"]["status"] != "PASS":
                record["phases_disagreement"] = record["phases"]["reason"]
                disagreements += 1
            record["pins"] = re_derive_pins(case_dir, document)
            if record["pins"]["status"] != "PASS":
                record["pins_disagreement"] = record["pins"]["reason"]
                disagreements += 1
            store = case_dir / "sample.sqlite"
            if store.exists():
                reading = space.footprint(store, document.get("attribution", "exclusive"))
                record["footprint"] = reading.as_fields()
                record["pack_accounting"] = reading.pack_accounting()
            record["status"] = (
                "PASS"
                if not parsed.defects
                and re_derived == published
                and record["phases"]["status"] == "PASS"
                and record["pins"]["status"] == "PASS"
                else "FAIL"
            )
            findings.append(record)
    # The two halves of the no-retry/no-fsync/no-WAL contract: a sealed
    # call-graph status over the product source, and the runtime tripwires read off
    # the Stores this run actually produced. Neither is a counter, and neither is a
    # fabricated zero.
    call_graph = invariants.scan()
    stores = sorted(
        str(entry) for entry in run_dir.rglob("*.sqlite") if entry.is_file()
    )
    tripwire = invariants.tripwires(stores)
    document = {
        "schema": "layerfs-core-verification-v1",
        "run_dir": str(run_dir),
        "cases": len(findings),
        "disagreements": disagreements,
        "findings": findings,
        "sealed_call_graph": call_graph,
        "runtime_tripwires": tripwire,
        "wall_ns": time.monotonic_ns() - started,
    }
    document["budget"] = receipt.verification_budget(document["wall_ns"]).as_fields()
    # A verification never overwrites a previous one: a corrected pass is written
    # beside the one it corrects, and both are retained with their exit codes.
    receipt.write_append_only(run_dir / arguments.tag, document)
    print(
        f"verify: {len(findings)} case(s), {disagreements} disagreement(s), "
        f"call-graph {call_graph['status']} over {call_graph['files_scanned']} files, "
        f"tripwires {tripwire['status']} over {tripwire['stores_checked']} store(s), "
        f"{document['wall_ns'] / 1e9:.2f} s"
    )
    for finding in tripwire["findings"]:
        print(f"  tripwire FINDING: {finding}")
    return 0 if disagreements == 0 and call_graph["status"] == "PASS" and tripwire["status"] == "PASS" else 1


def pinned_counters() -> dict[str, dict[str, int]]:
    """The pinned O3 constants, read from the table the binary compiled in.

    Read from the file rather than asked of the binary, so the re-derivation does
    not depend on the process it is re-deriving. `tests/golden/expected.tsv` is the
    same file `include_str!` embedded, and the receipt's `harness_binary_sha256`
    covers the compiled copy.
    """
    table: dict[str, dict[str, int]] = {}
    path = HARNESS_ROOT / "tests" / "golden" / "expected.tsv"
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        case_id, label, value = line.split("\t")
        if not label.startswith("counter:"):
            continue
        table.setdefault(case_id, {})[label[len("counter:") :]] = int(value)
    return table


def re_derive_pins(case_dir: Path, document: dict[str, object]) -> dict[str, object]:
    """Requires every pinned counter of a row to appear somewhere in its trace.

    The child gates the counters **its own invocation** published; this check sees
    the whole record set, both invocations, so a pinned constant that no invocation
    publishes is caught here. It also re-checks the values against the receipt's
    published counters, so a gate that passed on a number the receipt does not carry
    is caught rather than trusted.
    """
    table = pinned_counters()
    case_id = str(document.get("case_id", case_dir.name))
    pinned = table.get(case_id)
    observed = dict(document.get("counters") or {})
    if not pinned:
        if not observed and trace_module.read(case_dir / "trace.jsonl").status() == "NOT_RUN":
            # A row whose driver does not exist ran nothing, so it has no oracle to
            # gate and no counter to pin. That is a declared gap, not an ungated
            # row, and reporting it as a disagreement would make the verification
            # pass count the harness's own unimplemented rows.
            return {
                "status": "PASS",
                "reason": "the row ran no driver, so it published no counters and pinned none",
                "pinned": 0,
            }
        return {
            "status": "FAIL",
            "reason": f"{case_id} has no pinned O3 constant; the oracle cannot have been gated",
        }
    missing = sorted(key for key in pinned if key not in observed)
    if missing:
        return {
            "status": "FAIL",
            "reason": f"pinned counters the row never published: {', '.join(missing[:5])}",
        }
    drifted = sorted(
        f"{key} {pinned[key]} -> {observed[key]}"
        for key in pinned
        if int(observed[key]) != pinned[key]
    )
    if drifted:
        return {"status": "FAIL", "reason": "; ".join(drifted[:5])}
    return {
        "status": "PASS",
        "reason": f"{len(pinned)} pinned counter(s) reproduced",
        "pinned": len(pinned),
    }


def re_derive_phases(case_dir: Path, document: dict[str, object]) -> dict[str, object]:
    """Re-reads the raw phase artifacts and recomposes the six published fields.

    Nothing here trusts the receipt: the invocation walls come from the receipt
    only because they are the runner's own measurement of a process it started, and
    every phase span is re-read from the child's `phases-<invocation>.json` and the
    product's `timing.json`.
    """
    published = document.get("phases")
    if not isinstance(published, dict):
        return {"status": "FAIL", "reason": "the receipt publishes no phases"}
    walls: dict[str, int] = {}
    for record in published.get("invocations", []) or []:
        if not isinstance(record, dict):
            return {"status": "FAIL", "reason": "an invocation record is not an object"}
        walls[str(record.get("invocation"))] = int(record.get("wall_ns", 0) or 0)
    if not walls:
        return {"status": "FAIL", "reason": "the receipt records no invocation wall"}
    try:
        recomposed = phases_module.compose(
            case_dir,
            walls,
            expects_operation=trace_module.read(case_dir / "trace.jsonl").status() != "NOT_RUN",
        )
    except phases_module.PhaseError as error:
        return {"status": "FAIL", "reason": str(error)}
    for field in (
        "preparation_wall_ns",
        "acquisition_wall_ns",
        "operation_ns",
        "verification_wall_ns",
        "cleanup_wall_ns",
        "handoff_ns",
        "complete_command_ns",
    ):
        if int(recomposed.get(field, 0)) != int(published.get(field, 0) or 0):
            return {
                "status": "FAIL",
                "reason": (
                    f"{field} published as {published.get(field)} but re-derived as "
                    f"{recomposed.get(field)}"
                ),
            }
    if recomposed["reconciliation"]["status"] != "PASS":
        return {"status": "FAIL", "reason": recomposed["reconciliation"]["reason"]}
    return {
        "status": "PASS",
        "reason": recomposed["reconciliation"]["reason"],
        "operation_ns": recomposed["operation_ns"],
        "preparation_wall_ns": recomposed["preparation_wall_ns"],
        "verification_wall_ns": recomposed["verification_wall_ns"],
        "cleanup_wall_ns": recomposed["cleanup_wall_ns"],
        "acquisition_wall_ns": recomposed["acquisition_wall_ns"],
        "handoff_ns": recomposed["handoff_ns"],
        "complete_command_ns": recomposed["complete_command_ns"],
    }


def cmd_report(arguments: argparse.Namespace) -> int:
    run_dir = Path(arguments.run)
    identity = None
    run_json = run_dir / "run.json"
    if run_json.exists():
        document = json.loads(run_json.read_text(encoding="utf-8"))
        identity = dict(document.get("identity", {}))
        identity["run_dir"] = str(run_dir)
        identity["lane"] = document.get("lane", "")
        identity["rows"] = document.get("rows", 0)
        identity["verification_mode"] = document.get("verification_mode", "")
        identity["verification_sample_rule"] = document.get("verification_sample_rule", "")
        identity["reused_proof"] = document.get("reused_proof")
        identity["reused_proof_identities"] = document.get("reused_proof_identities")
    rendered = analyze.render(analyze.load_run(run_dir), identity)
    target = Path(arguments.out) if arguments.out else run_dir / "report.txt"
    receipt.write_text_append_only(target, rendered)
    print(rendered)
    print(f"report: written to {target}")
    return 0


def cmd_self_check(_: argparse.Namespace) -> int:
    failures: list[str] = []
    if not BINARY.exists():
        build()
    checks = {
        "residency": __import__("residency").self_check(),
        "space": space.self_check(),
        "receipt": receipt.self_check(),
        "trace": trace_module.self_check(),
        "copyladder": copyladder.self_check(),
        "phases": phases_module.self_check(),
        "invariants": invariants.self_check(),
    }
    for name, found in checks.items():
        if found:
            failures.extend(f"{name}: {failure}" for failure in found)
        else:
            print(f"  {name:<12} PASS")
    parity = subprocess.run(
        [sys.executable, str(HARNESS_ROOT / "shared" / "test_lock_parity.py")],
        capture_output=True,
        text=True,
        check=False,
    )
    print(parity.stdout.strip())
    if parity.returncode != 0:
        failures.append("lock parity failed")
    registry = registry_self_check()
    print(f"  registry     {registry.get('status')}")
    if registry.get("status") != "PASS":
        failures.append(f"registry self-check failed: {registry}")
    # The declared-exception list is hand-maintained and has been wrong about
    # itself: it carried `payload-random-read-500` for the registered
    # `payload-random-read-500m`, so the 500 MiB read tier owner decision D4 puts
    # on the <= 25 s list was silently classified against the 15 s limit and the
    # entry matched no case at all. An entry that names nothing is a declaration
    # defect, so every entry is checked against the binary's own registry here.
    known = {row[0] for row in registry_table()}
    unknown = sorted(DECLARED_EXCEPTIONS - known)
    print(f"  exceptions   {'PASS' if not unknown else 'FAIL'} ({len(DECLARED_EXCEPTIONS)} declared)")
    if unknown:
        failures.append(f"DECLARED_EXCEPTIONS names no registered case: {unknown}")
    golden = golden_matches()
    print(f"  golden       {'PASS' if golden else 'FAIL'}")
    if not golden:
        failures.append("tests/golden/registry.tsv drifted from the registry")
    if os.environ.get("LAYERFS_CONSTRUCTION_WORKERS") and os.environ["LAYERFS_CONSTRUCTION_WORKERS"] != "1":
        failures.append("LAYERFS_CONSTRUCTION_WORKERS is set to something other than 1")
    for failure in failures:
        print(f"self-check: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-check: PASS")
    return 0


def calibration_artifact(case_id: str) -> str | None:
    """The prepared master one calibration arm's child needs, acquired once.

    A calibration arm drives a real registered case, and a case that declares a
    prepared master needs one handed to it: `--load-input` is the only way its
    driver reads a fixture, and a child started without it fails closed with
    `NOT_RUN` rather than rebuilding the fixture inside the invocation the split
    exists to keep it out of. Acquisition is untimed and once per digest, exactly
    as `prepare` does it.
    """
    if not prepared_of(case_id).strip("-"):
        return None
    identity = receipt.identify(REPO_ROOT, HARNESS_ROOT, BINARY)
    record = acquire(case_id, artifact_root(), identity, row_of(case_id))
    if record.get("state") not in ("acquired", "reused"):
        return None
    return str(record["artifact"])


def cmd_calibrate(arguments: argparse.Namespace) -> int:
    import experiments

    if not BINARY.exists():
        build()
    started = time.monotonic()
    results = experiments.run_all(BINARY, REPO_ROOT, artifact_of=calibration_artifact)
    document = {
        "schema": "layerfs-calibration-v1",
        "ran_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "wall_ns": int((time.monotonic() - started) * 1e9),
        "experiments": [result.as_document() for result in results],
    }
    for result in results:
        print(f"{result.identifier}: {result.outcome}")
        for key, value in result.fields.items():
            print(f"    {key} = {value}")
        for note in result.notes:
            print(f"    note: {note}")
    if arguments.out:
        target = Path(arguments.out)
        if target.is_dir():
            target = target / "experiments.json"
        receipt.write_append_only(target, document)
        print(f"calibrate: written to {target}")
    return 0 if all(result.outcome == "SATISFIED" for result in results) else 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="runner.py", description=__doc__)
    sub = parser.add_subparsers(dest="verb", required=True)

    listing = sub.add_parser("list")
    listing.add_argument("--lane", choices=["smoke", "full"], default="smoke")
    listing.add_argument("--format", choices=["tsv", "jsonl"], default="tsv")
    listing.set_defaults(function=cmd_list)

    prepare = sub.add_parser("prepare")
    prepare.add_argument("--case", action="append")
    prepare.add_argument("--out")
    prepare.set_defaults(function=cmd_prepare)

    perf = sub.add_parser("perf")
    perf.add_argument("--lane", choices=["smoke", "full"], default="smoke")
    perf.add_argument("--out", required=True)
    perf.add_argument("--case", action="append")
    perf.add_argument("--no-build", action="store_true")
    perf.add_argument(
        "--verify",
        choices=list(VERIFICATION_MODES),
        default=None,
        help=(
            "verification mode; defaults to full for a whole-lane run and to sample "
            "for iteration (--lane smoke or any explicit --case)"
        ),
    )
    perf.add_argument("--reuse-pass")
    perf.set_defaults(function=cmd_perf)

    verify = sub.add_parser("verify")
    verify.add_argument("--run", required=True)
    verify.add_argument("--tag", default="verification.json")
    verify.add_argument("--reuse-pass")
    verify.set_defaults(function=cmd_verify)

    report = sub.add_parser("report")
    report.add_argument("--run", required=True)
    report.add_argument("--out")
    report.set_defaults(function=cmd_report)

    prune = sub.add_parser("prune")
    prune.add_argument("--out", help="the artifact root; defaults to the prepared root")
    prune.add_argument(
        "--dry-run", action="store_true", help="report what would be removed and remove nothing"
    )
    prune.set_defaults(function=cmd_prune)

    self_check = sub.add_parser("self-check")
    self_check.set_defaults(function=cmd_self_check)

    calibrate = sub.add_parser("calibrate")
    calibrate.add_argument("--experiment", default="all")
    calibrate.add_argument("--out")
    calibrate.set_defaults(function=cmd_calibrate)

    arguments = parser.parse_args()
    # One construction worker, exported by the runner itself and inherited by
    # every measured child, so the value the receipt records is the value the
    # child ran under rather than a value the runner merely intended.
    if arguments.verb in {"perf", "calibrate"}:
        os.environ["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    try:
        return int(arguments.function(arguments))
    except RunnerError as error:
        print(f"runner: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
