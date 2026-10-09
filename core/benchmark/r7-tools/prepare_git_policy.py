"""Close primary-source Git default policy evidence; no fixture or Store access."""
import argparse
import hashlib
import json
from pathlib import Path
import time
import urllib.request

from prepare_inputs import owned
from oracle_prepare import OutputFile, ReadFile

VERSION = "git version 2.39.5"
BINARY = "6464b23aabeb8dcb55a67b68c911678041b1b62437eeddf779ac3f201f6a09c9"
DEFAULTS = {
    "core.repositoryformatversion": "0", "extensions.objectformat": "sha1",
    "extensions.worktreeconfig": "false", "core.filemode": "true",
    "core.ignorecase": "false", "index.skiphash": "false",
    "core.splitindex": "false", "core.sparsecheckout": "false",
    "core.untrackedcache": "keep", "core.fsmonitor": "false",
    "feature.manyfiles": "false",
}
SOURCES = ["Documentation/config/core.txt", "Documentation/config/feature.txt",
           "Documentation/config/index.txt", "repo-settings.c", "setup.c", "hash.h"]


def sha(path):
    digest = hashlib.sha256()
    with ReadFile(path, "policy_input_read") as stream:
        for part in iter(lambda: stream.read(65536), b""):
            digest.update(part)
    return digest.hexdigest()


def write(path, value):
    raw = (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()
    with OutputFile(path) as stream:
        if stream.write(raw) != len(raw):
            raise OSError("short original policy record write; no resend")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tool-receipt", type=Path, required=True)
    args = parser.parse_args()
    output = owned(args.output, existing=False)
    receipt = args.tool_receipt.resolve(strict=True)
    with ReadFile(receipt, "tool_identity_receipt_read") as stream:
        tool = json.load(stream)
    if tool.get("version_stdout") != VERSION + "\n" or tool.get("sha256") != BINARY or tool.get("version_exit") != 0:
        raise ValueError("actual pinned-image Git tool receipt differs")
    output.mkdir()
    rows = []
    start = time.monotonic_ns()
    deadline = time.monotonic() + 14
    result = dict(schema="r7-git-default-policy-review-v1", status="INCOMPLETE",
                  version=VERSION, git_binary_sha256=BINARY, defaults=DEFAULTS,
                  tool_identity_receipt=dict(path=str(receipt), sha256=sha(receipt)),
                  primary_sources=rows, source_mutations=0, automatic_retry=False,
                  performance_samples=0, scope="untimed primary-source policy review; no Git/fixture/Store operation")
    try:
        for index, path in enumerate(SOURCES):
            left = deadline - time.monotonic()
            if left <= 0:
                raise TimeoutError("primary-source policy setup wall stop")
            url = "https://raw.githubusercontent.com/git/git/v2.39.5/" + path
            destination = output / (str(index) + "-" + Path(path).name)
            digest = hashlib.sha256()
            count = 0
            with urllib.request.urlopen(url, timeout=left) as source, OutputFile(destination) as target:
                while True:
                    part = source.read(65536)
                    if not part:
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("primary-source policy setup wall stop")
                    if target.write(part) != len(part):
                        raise OSError("short original primary-source write; no resend")
                    digest.update(part)
                    count += len(part)
            rows.append(dict(url=url, sha256=digest.hexdigest(), bytes=count, artifact=str(destination)))
        result.update(status="CLOSED_PRIMARY_SOURCE_POLICY", setup_ns=time.monotonic_ns()-start,
                      interpretation="Absent values use the named Git release defaults; actual config presence and object format are queried in the separate mounted proof. Enabled feature.manyFiles and unsupported actual index extensions refuse.")
        review = output / "review.json"
        write(review, result)
        policy = dict(schema="r7-git-default-policy-v1", version=VERSION,
                      git_binary_sha256=BINARY, defaults=DEFAULTS,
                      primary_sources=[dict(url=row["url"], sha256=row["sha256"]) for row in rows],
                      review_receipt_sha256=sha(review))
        path = output / "policy.json"
        write(path, policy)
        print(json.dumps(dict(status=result["status"], review=str(review), review_sha256=sha(review),
                              policy=str(path), policy_sha256=sha(path), sources=rows), sort_keys=True))
        return 0
    except Exception as error:
        result.update(original_error=str(error), original_error_type=type(error).__name__,
                      independent_close_failures=getattr(error, "independent_close_failures", []),
                      setup_ns=time.monotonic_ns()-start)
        write(output / "failure.json", result)
        print(json.dumps(result, sort_keys=True))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
