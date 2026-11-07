#!/usr/bin/env python3
"""timeout.py SECONDS cmd... : run cmd in its own process group; kill the group at the ceiling (exit 124)."""
import os, signal, subprocess, sys

secs = float(sys.argv[1])
proc = subprocess.Popen(sys.argv[2:], start_new_session=True)
try:
    sys.exit(proc.wait(timeout=secs))
except subprocess.TimeoutExpired:
    os.killpg(proc.pid, signal.SIGTERM)
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.wait()
    print(f"TIMEOUT after {secs}s", file=sys.stderr)
    sys.exit(124)
