#!/usr/bin/env python3
import json
import os
import sys
import time

pid = int(sys.argv[1])
status_path = f"/proc/{pid}/status"
stop = "/var/tmp/layerfs-owned/stop-thread-sampler"
output = "/var/tmp/layerfs-owned/thread-hwm.json"
samples = 0
threads_hwm = 0
rss_hwm_kib = 0

while not os.path.exists(stop):
    with open(status_path) as source:
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

with open(output, "x") as target:
    json.dump(
        {
            "schema": "layerfs-stage2-daemon-thread-hwm-v1",
            "status": "PASS" if samples and threads_hwm <= 8 else "FAIL",
            "samples": samples,
            "threads_high_water": threads_hwm,
            "daemon_rss_high_water_kib": rss_hwm_kib,
        },
        target,
        indent=2,
        sort_keys=True,
    )
    target.write("\n")
