"""Small source-text guard for core/AGENTS.md; semantic review is still required."""

from pathlib import Path
import re
import sys


ATTR = re.compile(r"#\s*!?\s*\[([^\]]*)\]", re.DOTALL)
TEST_ATTR = re.compile(r"^(?:\w+\s*::\s*)*(?:test|bench)\b")
TEST_CFG = re.compile(r'\btest\b|"[^"\n]*(?:test|bench|mock|fixture|fault)[^"\n]*"')
CFG_MACRO = re.compile(r"\bcfg\s*!\s*\(([^;]*)\)", re.DOTALL)
IMPL = re.compile(r"\b(?:struct|enum|trait|impl|union|static|const|let|if|else|match|loop|while|for)\b|\bmacro_rules\s*!")
DOC_CODE = re.compile(r"^\s*(?:///|//!)\s*```", re.MULTILINE)


def violations(path, source):
    """Return line/reason pairs; intentionally reject marker text in comments too."""
    found = []
    lines = source.splitlines()
    entry = path.name in ("lib.rs", "mod.rs")
    limit = 200 if entry else 999
    if len(lines) > limit:
        kind = "entry file" if entry else "production file"
        found.append((limit + 1, f"{kind} has {len(lines)} physical lines; maximum is {limit}"))
    if path.suffix != ".rs":
        return found
    for match in ATTR.finditer(source):
        body = match[1].strip()
        if TEST_ATTR.match(body) or (
            re.match(r"cfg(?:_attr)?\b", body) and TEST_CFG.search(body)
        ):
            found.append((source.count("\n", 0, match.start()) + 1, "test-only attribute in product source"))
    for match in CFG_MACRO.finditer(source):
        if TEST_CFG.search(match[1]):
            found.append((source.count("\n", 0, match.start()) + 1, "test-only cfg! in product source"))
    if DOC_CODE.search(source):
        found.append((1, "put fenced executable documentation examples outside src/"))
    if entry:
        for number, line in enumerate(lines, 1):
            code = line.split("//", 1)[0].strip()
            if IMPL.search(code):
                found.append((number, "implementation construct in declaration/delegation entry file"))
    return found


def production_files(core):
    """Known product inputs; tests/examples/tools are deliberately outside this scope."""
    files = set()
    for package in (core / "crates").glob("*"):
        if not package.is_dir():
            continue
        files.update(path for path in (package / "src").rglob("*")
                     if path.is_file() and path.suffix in (".rs", ".sql"))
        files.update(path for path in (package / "sql").rglob("*.sql") if path.is_file())
    return sorted(files)


def main():
    core = Path(__file__).resolve().parents[1]
    files = production_files(core)
    failures = 0
    for path in files:
        for line, reason in violations(path, path.read_text()):
            print(f"{path.relative_to(core)}:{line}: {reason}")
            failures += 1
    if failures:
        print(f"FAIL: {failures} product-source boundary violations")
        return 1
    print(f"PASS: scanned {len(files)} production Rust/SQL files; semantic review still required")
    if not files:
        print("No product source exists yet; this is only a policy setup check.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
