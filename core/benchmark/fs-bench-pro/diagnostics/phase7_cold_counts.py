#!/usr/bin/env python3
"""Count-driven cold-contract attribution; never a product speed/admission arm."""
import json
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
from shared import sqlite_contract as contract


def measure(source):
    counts = {}
    ns = {}
    primitive = contract.residency

    def wrap(owner, name, label):
        original = getattr(owner, name)
        def observed(*args, **kwargs):
            start = time.monotonic_ns()
            counts[label] = counts.get(label, 0) + 1
            try:
                return original(*args, **kwargs)
            finally:
                ns[label] = ns.get(label, 0) + time.monotonic_ns() - start
        setattr(owner, name, observed)
        return original

    restored = []
    try:
        # Instrument the actual existing primitive, including nested residency
        # calls. Inclusive observations overlap and are not additive.
        for owner, name, label in (
            (Path, "stat", "path_stat"),
            (primitive, "residency", "residency"),
            (primitive, "de_warm", "de_warm"),
            (primitive._libc, "mmap", "mmap"),
            (primitive._libc, "mincore", "mincore"),
            (primitive._libc, "msync", "msync"),
            (primitive._libc, "munmap", "munmap"),
        ):
            restored.append((owner, name, wrap(owner, name, label)))
        start = time.monotonic_ns()
        result = contract.dewarm_tree(source)
        elapsed = time.monotonic_ns() - start
    finally:
        for owner, name, original in reversed(restored):
            setattr(owner, name, original)
    return {"kind": "count-driven-diagnostic", "product_sample_count": 0,
            "scope": "existing cold-content invalidation and whole-input attestation only",
            "source": str(Path(source).resolve()), "cold_result": result,
            "counts": counts, "inclusive_observation_ns": ns, "diagnostic_wall_ns": elapsed,
            "observer_limit": "wrapper overhead included; inclusive calls overlap; no payload reads or product operation"}


if __name__ == "__main__":
    print(json.dumps(measure(sys.argv[1]), sort_keys=True))
