#!/usr/bin/env python3
"""Lock parity between the product workspace and the harness workspace.

Owner decision D3 (`core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md`
section 5) makes this harness its own Cargo workspace, which means its own
`Cargo.lock`. That lock can resolve different transitive versions than
`core/Cargo.lock` does, and `--locked` would then pin the harness to a *different
dependency graph than the one the product seal names*. This test is the mandatory
mitigation and it must exist before the first receipt.

The rule is deliberately narrow and one-directional:

    every package present in BOTH locks must match on version and checksum.

Packages present in only one lock are reported, never failed: the harness may
legitimately carry a development dependency the product does not, and the product
carries members the harness does not depend on. What may never happen is a shared
package resolving differently, because that is the case where the harness links a
different `rusqlite`/`libsqlite3-sys`/`blake3`/`zstd-sys` than the seal names.

Checksums are absent for path/workspace packages. When either side lacks a
checksum the pair is compared on version alone and counted separately, so a
comparison that fell back is visible in the count rather than hidden in a pass.

    python3 core/benchmark/fs-bench-pro-storage-content/shared/test_lock_parity.py
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

HARNESS_ROOT = Path(__file__).resolve().parent.parent
REPO_ROOT = HARNESS_ROOT.parents[2]
HARNESS_LOCK = HARNESS_ROOT / "Cargo.lock"
PRODUCT_LOCK = REPO_ROOT / "core" / "Cargo.lock"


class ParityError(Exception):
    """A shared package resolves differently in the two locks."""


def load_lock(path: Path) -> dict[str, list[dict[str, object]]]:
    """Returns `name -> [entry, ...]` for one lockfile.

    A Cargo lock legitimately holds one name at several versions (`hashbrown` at
    0.14 and 0.15 in this tree), so the value is the whole list of entries for
    that name, sorted by version. Collapsing a name to one entry would hide
    exactly the case this test exists to catch: a shared package that resolved to
    a second, different version on one side.
    """
    with path.open("rb") as handle:
        document = tomllib.load(handle)
    packages: dict[str, list[dict[str, object]]] = {}
    for entry in document.get("package", []):
        name = entry["name"]
        packages.setdefault(name, []).append(
            {
                "version": entry["version"],
                "checksum": entry.get("checksum"),
                "source": entry.get("source"),
            }
        )
    if not packages:
        raise ParityError(f"{path}: no [[package]] entries")
    for entries in packages.values():
        entries.sort(key=lambda item: str(item["version"]))
    return packages


def compare(
    product: dict[str, list[dict[str, object]]],
    harness: dict[str, list[dict[str, object]]],
) -> dict[str, object]:
    """Compares the two locks and returns the counts plus every mismatch.

    Direction of the rule: the harness may not link anything the product seal
    does not name. Concretely, for every package entry in the harness lock:

      * if the name is absent from the product lock and the entry is a registry
        package, that is a mismatch - the harness would link a crate the seal has
        never seen;
      * if the name is present, an identical (version, checksum) entry must exist
        in the product lock.

    Product entries the harness does not link are reported, never failed: `core/`
    has workspace members and transitive edges the harness has no reason to carry.
    """
    mismatches: list[str] = []
    compared_with_checksum = 0
    compared_version_only: list[str] = []
    product_only_versions: list[str] = []
    harness_only_registry: list[str] = []
    shared_names = sorted(set(product) & set(harness))
    for name in sorted(harness):
        left = product.get(name)
        if left is None:
            for entry in harness[name]:
                if entry["source"] is not None:
                    harness_only_registry.append(f"{name}@{entry['version']}")
            continue
        for right_entry in harness[name]:
            matching = [
                item
                for item in left
                if item["version"] == right_entry["version"]
                and item["checksum"] == right_entry["checksum"]
            ]
            if not matching:
                same_version = [item for item in left if item["version"] == right_entry["version"]]
                if same_version:
                    mismatches.append(
                        f"{name} {right_entry['version']}: checksum "
                        f"{same_version[0]['checksum']} (core/Cargo.lock) != "
                        f"{right_entry['checksum']} (harness Cargo.lock)"
                    )
                else:
                    mismatches.append(
                        f"{name}: harness links {right_entry['version']}; "
                        f"core/Cargo.lock has "
                        f"{[item['version'] for item in left]}"
                    )
                continue
            if right_entry["checksum"] is None:
                compared_version_only.append(f"{name}@{right_entry['version']}")
            else:
                compared_with_checksum += 1
        for left_entry in left:
            if not any(
                item["version"] == left_entry["version"]
                and item["checksum"] == left_entry["checksum"]
                for item in harness[name]
            ):
                product_only_versions.append(f"{name}@{left_entry['version']}")
    for entry in harness_only_registry:
        mismatches.append(f"harness-only registry package: {entry}")
    return {
        "product_packages": sum(len(entries) for entries in product.values()),
        "harness_packages": sum(len(entries) for entries in harness.values()),
        "shared_names": len(shared_names),
        "shared_entries": sum(len(harness[name]) for name in shared_names),
        "compared_with_checksum": compared_with_checksum,
        "compared_version_only": compared_version_only,
        "product_only_versions": sorted(product_only_versions),
        "harness_only": sorted(set(harness) - set(product)),
        "mismatches": mismatches,
        "rules": {
            "harness_entries_must_match_product": True,
            "checksum_when_present": True,
            "harness_only_registry_package": "refused",
            "extra_product_entries": "reported, not failed",
        },
    }


def report(result: dict[str, object]) -> None:
    """Prints the counts. Never prints a bare `ok`."""
    print("lock parity: core/Cargo.lock vs harness Cargo.lock")
    print(f"  product packages         : {result['product_packages']}")
    print(f"  harness packages         : {result['harness_packages']}")
    print(f"  shared names / entries   : {result['shared_names']} / {result['shared_entries']}")
    print(
        "  product entries the harness does not link : "
        f"{len(result['product_only_versions'])}"
    )
    print(f"    version + checksum     : {result['compared_with_checksum']}")
    print(
        "    version only (no sums) : "
        f"{len(result['compared_version_only'])}"
        f" {sorted(result['compared_version_only'])}"
    )
    print(f"  harness only             : {sorted(result['harness_only'])}")
    print(f"  mismatches               : {len(result['mismatches'])}")
    for line in result["mismatches"]:
        print(f"    MISMATCH {line}")


def main() -> int:
    """Returns 0 only when every shared package matches."""
    try:
        product = load_lock(PRODUCT_LOCK)
        harness = load_lock(HARNESS_LOCK)
    except (OSError, tomllib.TOMLDecodeError, ParityError) as error:
        print(f"lock parity: FAIL: {error}", file=sys.stderr)
        return 1
    result = compare(product, harness)
    report(result)
    if result["mismatches"]:
        print("lock parity: FAIL", file=sys.stderr)
        return 1
    print(
        "lock parity: PASS "
        f"({result['shared_entries']} shared package entries compared)"
    )
    return 0


def test_lock_parity() -> None:
    """`python3 -m unittest` entry point for this file."""
    product = load_lock(PRODUCT_LOCK)
    harness = load_lock(HARNESS_LOCK)
    result = compare(product, harness)
    assert not result["mismatches"], result["mismatches"]
    assert result["shared_entries"] > 0, "no shared packages: the comparison is vacuous"


if __name__ == "__main__":
    raise SystemExit(main())
