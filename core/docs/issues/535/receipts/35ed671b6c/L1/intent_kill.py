"""L1 (from K1) item 7 (#536 §12 item 3): run one operation through the Python package
and SIGKILL this process at a chosen point, so the operation's intent is left
as a crash leaves it.

    python3 intent_kill.py <home> push <store> <machine> <point> <events.log>
    python3 intent_kill.py <home> pull <store> -         <point> <events.log>
    python3 intent_kill.py <home> create <project> <name>:<machine> <point> <events.log>

push/pull points: a progress phase name (check, snapshot, send, verify,
install, record, retire): killed inside the progress handler when that
phase's first event arrives (the Python binding calls the handler
synchronously on the calling thread, before the phase is written into the
record's `op`); `send-mid`: at the first `send` event with 0 < done < total;
`record-window`: at the `record` event a thread starts polling the Store
record and kills as soon as it reads `"phase": "record"` (the window
between that write and the record write that drops `op`); `none`: no kill.
create points: `claim` or `start`: a thread polls the Sandbox record and
kills as soon as `op.phase` reads that value; `none`: no kill.
Every event is appended (unbuffered) to events.log with a timestamp.
"""

import json
import os
import signal
import sys
import threading
import time

home, mode, a, b, point, log_path = sys.argv[1:7]
import layerfs  # noqa: E402

lfs = layerfs.client(home=home)
log = open(log_path, "a", buffering=1)


def note(text):
    log.write(f"{time.time():.6f} {text}\n")
    log.flush()
    os.fsync(log.fileno())


def die(why):
    note(f"SIGKILL {why}")
    os.kill(os.getpid(), signal.SIGKILL)


def poll_kill(path, phase, why):
    def run():
        deadline = time.time() + 60
        while time.time() < deadline:
            try:
                with open(path) as f:
                    record = json.load(f)
            except (OSError, ValueError):
                continue
            op = record.get("op") or {}
            if op.get("phase") == phase:
                die(f"{why}: record op.phase={phase}")
        note(f"poll gave up on {path} phase {phase}")

    threading.Thread(target=run, daemon=True).start()


armed = {"done": False}


def on_progress(progress):
    phase = progress.get("phase")
    note(f"progress {json.dumps(progress)}")
    if armed["done"]:
        return
    if point == "send-mid":
        done, total = progress.get("done"), progress.get("total")
        if phase == "send" and done and total and 0 < done < total:
            armed["done"] = True
            die("at send-mid")
    elif point == "record-window":
        if phase == "record":
            armed["done"] = True
            poll_kill(os.path.join(home, "store", a, "record.json"), "record", "record-window")
    elif point == phase:
        armed["done"] = True
        die(f"at progress {phase}")


note(f"begin {mode} {a} {b} point={point} pid={os.getpid()}")
try:
    if mode == "push":
        value = lfs.store.push(a, b, on_progress=on_progress)
    elif mode == "pull":
        value = lfs.store.pull(a, on_progress=on_progress)
    elif mode == "create":
        name, machine = b.split(":")
        if point != "none":
            poll_kill(os.path.join(home, "sandbox", name, "record.json"), point, f"create {point}")
        value = lfs.sandbox.create(a, name, machine)
    else:
        sys.exit("mode is push, pull or create")
    note(f"answered {json.dumps(value)[:500]}")
except layerfs.LayerFSError as error:
    note(f"refused {error.code} {str(error)[:500]}")
    sys.exit(3)
