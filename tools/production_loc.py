#!/usr/bin/env python3
"""Count first-party production source lines for the per-commit LOC rule.

Production code is nonblank, non-comment first-party product implementation:
the product under core/crates/*/src and engine/crates/*/src (layerfs-api and the
layerfs-node binding were api/layerfs-api and api/binding before the api/
regrouping, and are counted there too), and the reference product under crates/*/src, plus shipped runtime SQL under
<crate>/sql/. Tests, examples, benches, fixtures, benchmark harnesses,
development tools, docs, manifests and generated artifacts are excluded. Legacy
inline #[cfg(test)] items are removed before counting, because the reference tree
keeps tests inside src/.

Rust crates are assigned by name to core or engine in every layout: the
layerfs-api method surface and the layerfs-node binding are engine crates
wherever they sit. Unknown historical crates remain counted in core.

The `api` scope is the published JavaScript (#428): the src/ JavaScript of
each package under api/ (api/agent is @layerfs/agent, api/cli is @layerfs/cli,
api/backend and api/runtime; api/agent/sdk and api/agent/cli before the
regrouping), one crate per package, named by its package.json. Tests and
node_modules are excluded. The layouts before the move to api/ are still
counted, so trees on either side compare: sdk/js/native/src, sdk/js/packages/<name>/src, the
older single package @layerfs/sdk under sdk/js/src and sdk/js/operator, and
the layerfs command under cli/src. Those were the separate `sdk` and `cli`
scopes. JavaScript comments are blanked by a scanner that knows line and
block comments and single, double and template strings. Current regex
literals contain no comment delimiters or string quotes.

The deepseek-harness plugin is its own scope, `dsh`: the JavaScript and
TypeScript under dsh/packages/<package>/src, one crate per package
(@dsh-layerfs/<package>, and the dsh-layerfs bundle). Its tests, build output
and the untracked dsh/assets art are excluded. The plugin moved to the
dsh-layerfs repository on 2026-10-23 (#428); the scope stays so trees before
the move still count it, and reads 0 after it. TypeScript comments are blanked
by the same JavaScript scanner.

Runtime SQL is counted for every product scope: a candidate package that ships schema or
query text under <crate>/sql/ contributes it exactly like the reference schema.
Per-file reporting uses the same classification as the totals, and prints the
physical line count separately so the two measures are never confused.

Method: comments are blanked by a Rust-aware scanner (line/nested block comments,
normal/raw/byte strings, char literals and lifetimes) and a line counts once when
it still holds non-whitespace. See AGENTS.md, "Production LOC comparison for
every commit".
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys

SKIP_DIRS = {"target", "tests", "examples", "benches", "node_modules"}
DSH_SKIP_DIRS = SKIP_DIRS | {"dist", "assets"}
JS_SUFFIXES = (".js", ".ts", ".tsx")
CODE_SUFFIXES = (".rs",)


def blank_rust(source: str) -> str:
    """Blank comments, keeping every newline so line numbering is unchanged."""
    out = list(source)
    index = 0
    length = len(source)
    block_depth = 0
    while index < length:
        char = source[index]
        if block_depth:
            if source.startswith("/*", index):
                block_depth += 1
                out[index] = out[index + 1] = " "
                index += 2
                continue
            if source.startswith("*/", index):
                block_depth -= 1
                out[index] = out[index + 1] = " "
                index += 2
                continue
            if char != "\n":
                out[index] = " "
            index += 1
            continue
        if source.startswith("//", index):
            while index < length and source[index] != "\n":
                out[index] = " "
                index += 1
            continue
        if source.startswith("/*", index):
            block_depth = 1
            out[index] = out[index + 1] = " "
            index += 2
            continue
        if char == "r" and is_raw_string_start(source, index):
            index = blank_raw_string(source, out, index)
            continue
        if char == '"':
            index = blank_quoted(source, out, index, '"')
            continue
        if char == "'":
            index = skip_char_literal_or_lifetime(source, out, index)
            continue
        index += 1
    return "".join(out)


def is_raw_string_start(source: str, index: int) -> bool:
    if index > 0 and (source[index - 1].isalnum() or source[index - 1] == "_"):
        return False
    cursor = index + 1
    while cursor < len(source) and source[cursor] == "#":
        cursor += 1
    return cursor < len(source) and source[cursor] == '"'


def blank_quoted(source: str, out: list, index: int, quote: str) -> int:
    """Keep literal text, skip escapes, and return the index after the quote."""
    cursor = index + 1
    while cursor < len(source):
        char = source[cursor]
        if char == "\\":
            cursor += 2
            continue
        if char == quote:
            return cursor + 1
        cursor += 1
    return cursor


def blank_raw_string(source: str, out: list, index: int) -> int:
    cursor = index + 1
    hashes = 0
    while source[cursor] == "#":
        hashes += 1
        cursor += 1
    closing = '"' + "#" * hashes
    end = source.find(closing, cursor + 1)
    return len(source) if end < 0 else end + len(closing)


def skip_char_literal_or_lifetime(source: str, out: list, index: int) -> int:
    if source.startswith("'\\", index):
        return blank_quoted(source, out, index, "'")
    if index + 2 < len(source) and source[index + 2] == "'" and source[index + 1] != "'":
        return index + 3
    return index + 1


def split_top_level(expression: str) -> list:
    """Splits one cfg expression on the commas that are not inside parentheses."""
    parts = []
    depth = 0
    start = 0
    for index, char in enumerate(expression):
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
        elif char == "," and depth == 0:
            parts.append(expression[start:index])
            start = index + 1
    parts.append(expression[start:])
    return [part for part in parts if part]


def implies_test(predicate: str) -> bool:
    """True when a cfg predicate can only hold while `cfg(test)` does.

    `test` implies it. `all(a, b)` implies it when any part does, because every
    part must hold. `any(a, b)` implies it only when every part does, because one
    non-test alternative is enough to reach the item in a production build.
    `not(...)` never implies it - a negated test is exactly the production branch -
    and neither does a feature, target or debug-assertion predicate on its own.
    """
    predicate = predicate.replace(" ", "")
    if predicate == "test":
        return True
    for combinator, every_part_must_imply in (("all(", False), ("any(", True)):
        if predicate.startswith(combinator) and predicate.endswith(")"):
            parts = split_top_level(predicate[len(combinator) : -1])
            if not parts:
                return False
            implied = [implies_test(part) for part in parts]
            return all(implied) if every_part_must_imply else any(implied)
    return False


def blank_inline_tests(code: str) -> str:
    """Blank test-only `#[cfg(...)]` items and their bodies.

    Only an item that cannot be reached outside a test build is removed: plain
    `#[cfg(test)]`, `#[cfg(all(test, ...))]` and `#[cfg(any(test))]`. A predicate
    that merely mentions test is production code - `cfg(not(test))`,
    `cfg(any(test, feature = "x"))` and `cfg(any(debug_assertions,
    feature = "test-instrumentation"))` all compile for something other than a
    test build, so removing them would delete shipped lines.
    """
    out = list(code)
    cursor = 0
    while True:
        found = code.find("#[cfg(", cursor)
        if found < 0:
            break
        end = code.find(")]", found)
        if end < 0:
            break
        attribute = code[found : end + 2]
        cursor = end + 2
        if not implies_test(attribute[len("#[cfg(") : -2]):
            continue
        body = skip_attributes(code, cursor)
        stop = item_end(code, body)
        for position in range(found, stop):
            if out[position] != "\n":
                out[position] = " "
        cursor = stop
    return "".join(out)


def declared_modules(path: Path, text: str) -> list:
    """Modules `path` declares, with whether a test build is the only way in.

    Returns `(test_only, target)` pairs. A declaration inside an already test-only
    file is itself test only, which is what makes the exclusion transitive.
    """
    declarations = []
    pattern = re.compile(r"(?P<attributes>(?:#\[[^\]]*\]\s*)*)mod\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*;")
    directory = path.parent if path.stem in {"mod", "lib", "main"} else path.parent / path.stem
    for match in pattern.finditer(text):
        attributes = match.group("attributes")
        test_only = any(
            implies_test(attribute[attribute.find("(") + 1 : -2])
            for attribute in re.findall(r"#\[cfg\([^\]]*\)\]", attributes)
        ) or any(
            "test" in attribute and "cfg(" not in attribute for attribute in re.findall(r"#\[[^\]]*\]", attributes)
        )
        explicit = re.search(r'#\[path\s*=\s*"([^"]+)"\]', attributes)
        if explicit:
            target = path.parent / explicit.group(1)
        else:
            candidate = directory / match.group("name")
            target = candidate.with_suffix(".rs") if (candidate.with_suffix(".rs")).exists() else candidate / "mod.rs"
        declarations.append((test_only, target))
    return declarations


def test_only_files(files: list) -> set:
    """Files under `src/` that only a test build can reach.

    A `#[cfg(test)] mod x;` declaration makes `x.rs` test-only, and so does a
    declaration inside a file that is itself test-only. Such a file is a test
    module that happens to live under `src/`; counting it as product code inflates
    the reference subtotal by thousands of lines.
    """
    known = {path.resolve() for path in files}
    excluded = set()
    changed = True
    while changed:
        changed = False
        for path in files:
            text = blank_rust(path.read_text(encoding="utf-8"))
            # A declaration inside a file that is itself test-only is test-only too,
            # which is how a nested diagnostic beside its parent test module is
            # reached.
            declaring_file_is_test_only = path.resolve() in excluded
            for test_only, target in declared_modules(path, text):
                resolved = target.resolve()
                if resolved not in known or resolved in excluded:
                    continue
                if test_only or declaring_file_is_test_only:
                    excluded.add(resolved)
                    changed = True
    return {path for path in files if path.resolve() in excluded}


def skip_attributes(code: str, cursor: int) -> int:
    while True:
        while cursor < len(code) and code[cursor].isspace():
            cursor += 1
        if not code.startswith("#[", cursor):
            return cursor
        end = code.find("]", cursor)
        if end < 0:
            return cursor
        cursor = end + 1


def item_end(code: str, start: int) -> int:
    depth = 0
    for position in range(start, len(code)):
        char = code[position]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth <= 0:
                return position + 1
        elif char == ";" and depth == 0:
            return position + 1
    return len(code)


def blank_js(source: str) -> str:
    """Blank JavaScript comments, keeping every newline."""
    out = list(source)
    index = 0
    length = len(source)
    while index < length:
        if source.startswith("//", index):
            while index < length and source[index] != "\n":
                out[index] = " "
                index += 1
            continue
        if source.startswith("/*", index):
            end = source.find("*/", index + 2)
            end = length if end < 0 else end + 2
            for position in range(index, end):
                if source[position] != "\n":
                    out[position] = " "
            index = end
            continue
        if source[index] in "'\"`":
            quote = source[index]
            index += 1
            while index < length and source[index] != quote:
                index += 2 if source[index] == "\\" else 1
            index += 1
            continue
        index += 1
    return "".join(out)


def blank_sql(source: str) -> str:
    """Blank -- line comments and block comments, keeping line numbering."""
    lines = []
    for line in source.splitlines():
        stripped = line.lstrip()
        lines.append("" if stripped.startswith("--") else line)
    return "\n".join(lines)


def counted_lines(path: Path) -> int:
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".rs":
        text = blank_inline_tests(blank_rust(text))
    elif path.suffix == ".sql":
        text = blank_sql(text)
    elif path.suffix in JS_SUFFIXES:
        text = blank_js(text)
    return sum(1 for line in text.splitlines() if line.strip())


SCOPES = ("core", "engine", "reference", "api", "dsh")
ENGINE_CRATES = {
    "layerfs-bridge",
    "layerfs-daemon",
    "layerfs-fuse",
    "layerfs-overlay",
    "layerfs-persistence",
    "layerfs-project",
    "layerfs-sandbox",
    "layerfs-store-service",
    "layerfs-workspace",
    "layerfs-api",
    "layerfs-node",
}
# Crate folders directly under api/ before the regrouping, by folder name.
OLD_API_CRATES = {"layerfs-api": "layerfs-api", "binding": "layerfs-node"}


def crate_layer(name: str) -> str:
    if name in ENGINE_CRATES:
        return "engine"
    # Historical and synthetic crates remain counted rather than disappearing.
    return "core"


def replacement_packages(root: Path) -> list:
    return sorted(
        [p for base in (root / "core/crates", root / "engine/crates") for p in base.glob("*") if p.is_dir()]
        + [root / "api" / folder for folder in OLD_API_CRATES if (root / "api" / folder).is_dir()]
    )


def package_crate(root: Path, package: Path) -> str:
    """The crate name of a package folder; api/binding was layerfs-node."""
    return OLD_API_CRATES[package.name] if package.parent == root / "api" else package.name


def scope_base(root: Path, scope: str) -> Path:
    if scope in {"core", "engine"}:
        return root
    if scope == "api":
        return root
    if scope == "dsh":
        return root / "dsh" / "packages"
    return root / "crates"


def kept(paths: list, base: Path) -> list:
    return [path for path in paths if not any(part in SKIP_DIRS for part in path.relative_to(base).parts)]


def api_packages(api: Path) -> list:
    """Each directory under api/ holding a package.json and a src/ folder."""
    found = []
    for base, dirs, files in os.walk(api):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS and d != "src")
        if "package.json" in files and (Path(base) / "src").is_dir():
            found.append(Path(base))
    return found


def api_files(root: Path) -> list:
    """The binding's Rust and each package's src/ JavaScript, in either layout."""
    files = []
    api = root / "api"
    if api.is_dir():
        for package in api_packages(api):
            files += kept(sorted((package / "src").rglob("*.js")), package)
    # Before the move to api/ (#428): the `sdk` and `cli` scopes.
    sdk = root / "sdk" / "js"
    if (sdk / "native" / "src").is_dir():
        files += kept(sorted((sdk / "native" / "src").rglob("*.rs")), sdk)
    for folder in ("src", "operator"):
        if (sdk / folder).is_dir():
            files += kept(sorted((sdk / folder).rglob("*.js")), sdk)
    if (sdk / "packages").is_dir():
        for package in sorted(p for p in (sdk / "packages").iterdir() if (p / "src").is_dir()):
            files += kept(sorted((package / "src").rglob("*.js")), sdk)
    if (root / "cli" / "src").is_dir():
        files += kept(sorted((root / "cli" / "src").rglob("*.js")), root / "cli")
    return files


def api_crate(root: Path, path: Path) -> str:
    parts = path.relative_to(root).parts
    if parts[:3] == ("sdk", "js", "native"):
        return "layerfs-node"
    if parts[0] == "cli":
        return "@layerfs/cli"
    if parts[:2] == ("sdk", "js"):
        return f"@layerfs/{parts[3]}" if parts[2] == "packages" else "@layerfs/sdk"
    package = path.parent
    while not ((package / "package.json").is_file() and (package / "src").is_dir()):
        package = package.parent
    return json.loads((package / "package.json").read_text(encoding="utf-8"))["name"]


def dsh_files(base: Path) -> list:
    """Each plugin package's src/ JavaScript and TypeScript."""
    files = []
    for package in sorted(p for p in base.iterdir() if p.is_dir()):
        if (package / "src").is_dir():
            files += sorted(f for f in (package / "src").rglob("*") if f.is_file() and f.suffix in JS_SUFFIXES)
    return [path for path in files if not any(part in DSH_SKIP_DIRS for part in path.relative_to(base).parts)]


def dsh_crate(base: Path, path: Path) -> str:
    package = path.relative_to(base).parts[0]
    return package if package == "dsh-layerfs" else f"@dsh-layerfs/{package}"


def crate_of(root: Path, scope: str, path: Path) -> str:
    base = scope_base(root, scope)
    if scope == "api":
        return api_crate(root, path)
    if scope == "dsh":
        return dsh_crate(base, path)
    parts = path.relative_to(root).parts
    if scope in {"core", "engine"}:
        return OLD_API_CRATES[parts[1]] if parts[0] == "api" else parts[2]
    return parts[1]


def rust_files(base: Path) -> list:
    """Product Rust/runtime SQL under a crate collection, including old API nesting."""
    files = []
    for path in sorted(base.rglob("*")):
        if not path.is_file():
            continue
        parts = path.relative_to(base).parts
        if any(part in SKIP_DIRS for part in parts):
            continue
        source = (len(parts) > 2 and parts[1] == "src") or (
            len(parts) > 3 and parts[0] == "layerfs-api" and parts[1] in ("core", "sdk") and parts[2] == "src"
        )
        sql = (len(parts) > 2 and parts[1] == "sql") or (
            len(parts) > 3 and parts[0] == "layerfs-api" and parts[1] in ("core", "sdk") and parts[2] == "sql"
        )
        if source and path.suffix in CODE_SUFFIXES:
            files.append(path)
        elif sql and path.suffix == ".sql":
            # Runtime SQL is shipped implementation for either product scope.
            files.append(path)
    excluded = test_only_files(files)
    return [path for path in files if path not in excluded]


def scope_files(root: Path, scope: str) -> list:
    if scope in {"core", "engine", "api"}:
        files = []
        packages = replacement_packages(root)
        for base in sorted({package.parent for package in packages}):
            names = {
                package.name
                for package in packages
                if package.parent == base and crate_layer(package_crate(root, package)) == scope
            }
            files.extend(path for path in rust_files(base) if path.relative_to(base).parts[0] in names)
        if scope == "api":
            files.extend(api_files(root))
        return sorted(set(files))
    base = scope_base(root, scope)
    if not base.is_dir():
        return []
    return dsh_files(base) if scope == "dsh" else rust_files(base)


def physical_lines(path: Path) -> int:
    """Every physical line, including comments and blanks; a ceiling measure."""
    with path.open(encoding="utf-8") as handle:
        return sum(1 for _ in handle)


def per_file(root: Path) -> dict:
    """Per-file production LOC and physical lines for both scopes."""
    result = {"root": str(root), "files": []}
    for scope in SCOPES:
        for path in scope_files(root, scope):
            crate = crate_of(root, scope, path)
            result["files"].append(
                {
                    "path": str(path.relative_to(root)),
                    "scope": scope,
                    "crate": crate,
                    "production_loc": counted_lines(path),
                    "physical_lines": physical_lines(path),
                }
            )
    return result


def scan(root: Path) -> dict:
    result = {"root": str(root), "scopes": {}, "combined": 0}
    for scope in SCOPES:
        per_crate = {}
        total = 0
        for path in scope_files(root, scope):
            lines = counted_lines(path)
            total += lines
            crate = crate_of(root, scope, path)
            per_crate[crate] = per_crate.get(crate, 0) + lines
        result["scopes"][scope] = {
            "files": len(scope_files(root, scope)),
            "lines": total,
            "crates": dict(sorted(per_crate.items())),
        }
        result["combined"] += total
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".", help="tree to count (default: current directory)")
    parser.add_argument("--detail", action="store_true", help="print per-crate totals")
    parser.add_argument("--json", action="store_true", help="print the full result as JSON")
    parser.add_argument(
        "--files",
        action="store_true",
        help="print per-file production LOC and physical lines",
    )
    args = parser.parse_args()
    if args.files:
        report = per_file(Path(args.root).resolve())
        if args.json:
            print(json.dumps(report, indent=2, sort_keys=True))
            return 0
        print(f"{'path':<78} {'scope':<9} {'prod':>7} {'physical':>9}")
        for entry in report["files"]:
            print(f"{entry['path']:<78} {entry['scope']:<9} {entry['production_loc']:>7} {entry['physical_lines']:>9}")
        return 0
    result = scan(Path(args.root).resolve())
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    for scope in SCOPES:
        entry = result["scopes"][scope]
        print(f"{scope:9s} {entry['lines']:7d} production lines in {entry['files']:4d} files")
        if args.detail:
            for crate, lines in entry["crates"].items():
                print(f"          {crate:48s} {lines:7d}")
    print(f"{'combined':9s} {result['combined']:7d} production lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
