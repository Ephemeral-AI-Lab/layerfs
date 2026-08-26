#!/usr/bin/env python3
import json
import os
import sys
import time

pid = int(sys.argv[1])
proc = f"/proc/{pid}"
stop = "/var/tmp/layerfs-owned/stop-process-sampler"
output = "/var/tmp/layerfs-owned/process-hwm.json"
fd_baseline = len(os.listdir(f"{proc}/fd"))
fd_hwm = fd_baseline
threads_hwm = 0
rss_hwm_kib = 0
samples = 0

while not os.path.exists(stop):
    fd_hwm = max(fd_hwm, len(os.listdir(f"{proc}/fd")))
    with open(f"{proc}/status") as source:
        fields = {
            key.rstrip(":"): value.strip()
            for key, value in (line.split(None, 1) for line in source)
            if key.rstrip(":") in {"Threads", "VmHWM", "VmRSS"}
        }
    threads_hwm = max(threads_hwm, int(fields["Threads"]))
    rss_hwm_kib = max(
        rss_hwm_kib,
        int(fields["VmHWM"].split()[0]),
        int(fields["VmRSS"].split()[0]),
    )
    samples += 1
    time.sleep(0.05)

checks = {
    "fd_high_water_bounded": fd_hwm <= fd_baseline + 64,
    "samples_nonzero": samples > 0,
    "threads_high_water_bounded": threads_hwm <= 8,
}
with open(output, "x") as target:
    json.dump(
        {
            "schema": "layerfs-stage2-process-highwater-v2",
            "status": "PASS" if all(checks.values()) else "FAIL",
            "checks": checks,
            "interval_ms": 50,
            "samples": samples,
            "fd_baseline": fd_baseline,
            "fd_high_water": fd_hwm,
            "fd_limit": fd_baseline + 64,
            "threads_high_water": threads_hwm,
            "daemon_rss_high_water_kib": rss_hwm_kib,
        },
        target,
        indent=2,
        sort_keys=True,
    )
    target.write("\n")
