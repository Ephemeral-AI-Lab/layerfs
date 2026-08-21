#!/usr/bin/env python3
"""Read-only machine-code gate for the corrected FastCDC screen topology."""

import json
import re
import subprocess
import sys
from pathlib import Path

TIMED = "fastcdc_region_screen10timed_scan"
SCAN = "layerfs_core3cdc7FastCdc4scan"
REGION = "layerfs_core3cdc11scan_region"


def run(command):
    return subprocess.run(command, check=True, capture_output=True, text=True).stdout


def symbols(binary):
    output = run(["nm", "-nm", binary])
    parsed = []
    for line in output.splitlines():
        match = re.match(r"^([0-9a-f]{16}) .* (\S+)$", line)
        if match:
            parsed.append((int(match.group(1), 16), match.group(2)))
    return output, parsed


def named(parsed, fragment):
    matches = [(address, name) for address, name in parsed if fragment in name]
    if len(matches) != 1:
        raise RuntimeError(f"expected one symbol containing {fragment}, got {len(matches)}")
    return matches[0]


def disassemble(binary, parsed, fragment):
    start, name = named(parsed, fragment)
    stop = min(address for address, _ in parsed if address > start)
    output = run([
        "objdump",
        "-d",
        f"--start-address={start:#x}",
        f"--stop-address={stop:#x}",
        binary,
    ])
    return output, {"name": name, "start": f"{start:#x}", "stop": f"{stop:#x}", "size": stop - start}


def mnemonics(disassembly):
    return re.findall(r"^\s*[0-9a-f]+:\s+[0-9a-f]{8}\s+([a-z.]+)", disassembly, re.MULTILINE)


def main():
    control = Path(sys.argv[1]).resolve()
    candidate = Path(sys.argv[2]).resolve()
    candidate_source = Path(sys.argv[3]).resolve()
    output = Path(sys.argv[4]).resolve()
    output.mkdir(parents=True, exist_ok=False)

    control_nm, control_symbols = symbols(control)
    candidate_nm, candidate_symbols = symbols(candidate)
    control_timed, control_timed_meta = disassemble(control, control_symbols, TIMED)
    candidate_timed, candidate_timed_meta = disassemble(candidate, candidate_symbols, TIMED)
    control_scan, control_scan_meta = disassemble(control, control_symbols, SCAN)
    candidate_scan, candidate_scan_meta = disassemble(candidate, candidate_symbols, SCAN)
    candidate_region, candidate_region_meta = disassemble(candidate, candidate_symbols, REGION)

    source = candidate_source.read_text()
    scanner_fields = source.split("struct Scanner {", 1)[1].split("}", 1)[0]
    checks = {
        "timed_wrappers_same_size": control_timed_meta["size"] == candidate_timed_meta["size"] == 216,
        "timed_wrappers_same_instruction_topology": mnemonics(control_timed) == mnemonics(candidate_timed),
        "control_scan_out_of_line": SCAN in control_timed and named(control_symbols, SCAN)[0] != named(control_symbols, TIMED)[0],
        "candidate_scan_out_of_line": SCAN in candidate_timed and named(candidate_symbols, SCAN)[0] != named(candidate_symbols, TIMED)[0],
        "timer_wraps_scan_control": control_timed.index("Instant3now") < control_timed.index(SCAN) < control_timed.index("Instant7elapsed"),
        "timer_wraps_scan_candidate": candidate_timed.index("Instant3now") < candidate_timed.index(SCAN) < candidate_timed.index("Instant7elapsed"),
        "control_stack_probe_after_timer": all(token in control_scan for token in ("sub\tsp, sp, #0x1, lsl #12", "str\txzr, [sp]", "cmp\tsp, x9", "b.ne")),
        "candidate_stack_probe_after_timer": all(token in candidate_scan for token in ("sub\tsp, sp, #0x1, lsl #12", "str\txzr, [sp]", "cmp\tsp, x9", "b.ne")),
        "candidate_has_two_region_calls": candidate_scan.count(REGION) == 2,
        "region_masks_are_register_arguments": "tst\tx11, x3" in candidate_region and "tst\tx2, x4" in candidate_region,
        "region_has_no_target_comparison": "#0x4000" not in candidate_region and "#16384" not in candidate_region,
        "region_has_no_mutable_mask_loads": not re.search(r"ldr\s+x[34]", candidate_region),
        "source_has_no_cached_mask_fields": "mask" not in scanner_fields,
        "source_has_no_next_even_field": "next_even" not in source,
        "source_calls_fixed_small_region": bool(re.search(r"SHIFTED_SMALL_MASK,\s*SMALL_MASK", source)),
        "source_calls_fixed_large_region": bool(re.search(r"SHIFTED_LARGE_MASK,\s*LARGE_MASK", source)),
    }
    if not all(checks.values()):
        raise RuntimeError("machine-code preflight failed: " + ",".join(name for name, passed in checks.items() if not passed))

    files = {
        "control-nm.txt": control_nm,
        "candidate-nm.txt": candidate_nm,
        "control-timed-scan.asm": control_timed,
        "candidate-timed-scan.asm": candidate_timed,
        "control-fastcdc-scan.asm": control_scan,
        "candidate-fastcdc-scan.asm": candidate_scan,
        "candidate-scan-region.asm": candidate_region,
    }
    for name, contents in files.items():
        (output / name).write_text(contents)
    report = {
        "status": "PASS",
        "checks": checks,
        "control_timed": control_timed_meta,
        "candidate_timed": candidate_timed_meta,
        "control_scan": control_scan_meta,
        "candidate_scan": candidate_scan_meta,
        "candidate_region": candidate_region_meta,
    }
    (output / "CODEGEN-PREFLIGHT-v1.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    main()
