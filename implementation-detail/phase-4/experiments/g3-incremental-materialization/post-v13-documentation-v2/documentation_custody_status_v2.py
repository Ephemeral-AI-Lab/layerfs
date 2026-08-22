#!/usr/bin/env python3
"""Execute the frozen v1 custody verifier with its sole whitespace fix."""

import hashlib
from pathlib import Path


HERE = Path(__file__).resolve().parent
V1 = HERE.parent / "post-v13-documentation-v1/documentation_custody_status_v1.py"
EXPECTED_V1_SHA256 = "a124dd9e761efe3fa01bb537ba1f5f0970750b994a360eabc5674ab6c5d131ca"
OLD = 'need("no persistent replayable destination receipt" in all_text, "no-persistent-receipt")'
NEW = 'need("no persistent replayable destination receipt" in re.sub(r"\\s+", " ", all_text), "no-persistent-receipt")'


source = V1.read_bytes()
if hashlib.sha256(source).hexdigest() != EXPECTED_V1_SHA256:
    raise RuntimeError("v1 custody verifier changed")
text = source.decode()
if text.count(OLD) != 1 or NEW in text:
    raise RuntimeError("whitespace repair operand mismatch")
repaired = text.replace(OLD, NEW)
namespace = {"__name__": "__main__", "__file__": str(V1)}
exec(compile(repaired, str(V1), "exec"), namespace)
