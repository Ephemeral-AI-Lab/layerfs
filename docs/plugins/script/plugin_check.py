#!/usr/bin/env python3
"""Check plugin listings in plugins/ before a pull request.

A listing is plugins/<id>/ with four files: plugin.json, README.md (the display
page), install.md (the install prompt) and logo.svg or logo.png. The rules are
those of docs/plugins/README.md and of the website, which refuses a listing that
breaks one (rules() in layerfs-website src/components/plugins/schema.ts; keep
the two in step). This script also catches what the site cannot: template
blanks left in, unknown fields and unknown placeholders.

    python3 docs/plugins/script/plugin_check.py                    every listing in plugins/
    python3 docs/plugins/script/plugin_check.py plugins/<id>       one or more listings
    python3 docs/plugins/script/plugin_check.py --online ...       also ask GitHub that the
                                                                   repository is public, each
                                                                   plugin commit exists and each
                                                                   LayerFS commit is on main

--online makes one unauthenticated GitHub API request per fact (60 an hour);
set GITHUB_TOKEN to raise that. Exit status 0 when every listing passes.
"""

import argparse
import json
import os
import re
import struct
import sys
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
LAYERFS_REPO = "Ephemeral-AI-Lab/layerfs"

PLATFORMS = {"linux", "macos", "windows-wsl2"}
USES = {"agent", "backend", "runtime", "daemon"}  # the api/ packages, and layerfs-daemon run directly
STATUS = {"preview", "available"}
LAYERFS_VERSIONS = {"v1.1.0", "v1.0.0", "v0.1.7"}
PLACEHOLDERS = {"{{version}}", "{{commit}}", "{{layerfs.version}}", "{{layerfs.commit}}", "{{repository}}"}
RESERVED = {"submit"}  # a page of its own on the site
SUMMARY_MAX = 90
TAGS_MAX = 6
LOGO_MAX = 64 * 1024

FIELDS = {"schema", "id", "name", "summary", "authors", "license", "repository", "tags",
          "platforms", "uses", "host", "status", "versions", "draft"}
REQUIRED = FIELDS - {"host", "draft"}
VERSION_FIELDS = {"version", "commit", "date", "layerfs", "notes"}

ID_RE = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,38}[a-z0-9])?$")
TAG_RE = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,22}[a-z0-9])?$")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
REPO_RE = re.compile(r"^https://github\.com/([A-Za-z0-9-]+)/([A-Za-z0-9._-]+)$")
# A template blank: <…>, or <Words with spaces> starting with a capital letter.
BLANK_RE = re.compile(r"<(?:…|[A-Z][^<>\n]*\s[^<>\n]*)>")


def newer(a, b):
    """a is a newer x.y.z than b; a pre-release is older than its release."""
    def key(s):
        core, _, pre = s.partition("-")
        return [int(x) for x in core.split(".")] + [0 if pre else 1]
    return key(a) > key(b)


def logo_square(path):
    """True or False for a PNG or an SVG with a readable size; None when unknown."""
    data = path.read_bytes()
    if path.suffix == ".png":
        if data[:8] != b"\x89PNG\r\n\x1a\n" or len(data) < 24:
            return False
        width, height = struct.unpack(">II", data[16:24])
        return width == height
    text = data.decode("utf-8", "replace")
    box = re.search(r'viewBox="\s*[-\d.]+[\s,]+[-\d.]+[\s,]+([\d.]+)[\s,]+([\d.]+)\s*"', text)
    if box:
        return float(box.group(1)) == float(box.group(2))
    size = re.search(r'<svg[^>]*\bwidth="([\d.]+)(?:px)?"[^>]*\bheight="([\d.]+)(?:px)?"', text)
    return float(size.group(1)) == float(size.group(2)) if size else None


def check_listing(folder):
    """Every broken rule in one listing folder, as 'part: message' lines."""
    errors = []
    add = lambda part, message: errors.append(f"{part}: {message}")
    folder = Path(folder)

    files = {name: folder / name for name in ("plugin.json", "README.md", "install.md")}
    for name, path in files.items():
        if not path.is_file():
            add("Files", f"{name} is missing")
    logos = [folder / n for n in ("logo.svg", "logo.png") if (folder / n).is_file()]
    if not logos:
        add("Logo", "logo.svg or logo.png is missing")
    elif len(logos) > 1:
        add("Logo", "keep one logo: logo.svg or logo.png, not both")
    extra = sorted(p.name for p in folder.iterdir() if p.name not in {*files, "logo.svg", "logo.png"})
    if extra:
        add("Files", f"only the four listing files belong here, not {', '.join(extra)}")

    listing = None
    if files["plugin.json"].is_file():
        try:
            listing = json.loads(files["plugin.json"].read_text(encoding="utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError) as error:
            add("plugin.json", f"is not valid JSON ({error})")
    if isinstance(listing, dict):
        errors += check_metadata(listing, folder.name)
    elif listing is not None:
        add("plugin.json", "must be one JSON object")

    readme = files["README.md"].read_text(encoding="utf-8") if files["README.md"].is_file() else None
    if readme is not None:
        if len(readme.strip()) < 80:
            add("Display page", "README.md is nearly empty")
        if re.search(r"^#\s", readme, re.M):
            add("Display page", "README.md starts at ## headings; the page shows the name")
        for blank in BLANK_RE.findall(readme):
            add("Display page", f"template blank left in README.md: {blank}")
        for image in re.findall(r"!\[[^\]]*\]\(([^)\s]+)", readme):
            if not image.startswith("https://"):
                add("Display page", f"image {image} must be an https:// link to a file in your repository at a pinned commit")

    install = files["install.md"].read_text(encoding="utf-8") if files["install.md"].is_file() else None
    if install is not None:
        if len(install.strip()) < 80:
            add("Install prompt", "install.md is nearly empty")
        if "{{layerfs.commit}}" not in install:
            add("Install prompt", "install.md must name {{layerfs.commit}} so it installs the tested LayerFS")
        for unknown in sorted(set(re.findall(r"\{\{[^}]*\}\}", install)) - PLACEHOLDERS):
            add("Install prompt", f"unknown placeholder {unknown}; use {', '.join(sorted(PLACEHOLDERS))}")
        for blank in BLANK_RE.findall(install):
            add("Install prompt", f"template blank left in install.md: {blank}")

    template_logo = ROOT / "docs" / "plugins" / "template" / "logo.svg"
    for logo in logos:
        if template_logo.is_file() and folder.resolve() != template_logo.parent.resolve() and logo.read_bytes() == template_logo.read_bytes():
            add("Logo", "this is the template's placeholder logo; use your plugin's own")
        if logo.stat().st_size > LOGO_MAX:
            add("Logo", f"{logo.name} is {logo.stat().st_size // 1024} KB; keep it under 64 KB")
        if logo_square(logo) is False:
            add("Logo", f"{logo.name} must be square")
    return errors


def check_metadata(l, folder_name):
    errors = []
    add = lambda part, message: errors.append(f"{part}: {message}")
    text = lambda key: l.get(key).strip() if isinstance(l.get(key), str) else ""

    for key in sorted(REQUIRED - l.keys()):
        add("plugin.json", f"{key} is missing")
    for key in sorted(l.keys() - FIELDS):
        add("plugin.json", f"unknown field {key}")
    for blank in BLANK_RE.findall(json.dumps(l, ensure_ascii=False)) + re.findall(r"<[a-z][a-z-]*>", json.dumps(l)):
        add("plugin.json", f"template blank left in: {blank}")

    if l.get("schema") != 1:
        add("About", "schema must be 1")
    if l.get("id") != folder_name:
        add("About", f'id is "{l.get("id")}"; it must be the folder\'s name, "{folder_name}"')
    if not ID_RE.match(folder_name) or folder_name in RESERVED:
        add("About", 'id: a–z, 0–9 and dashes, at most 40 characters, not "submit"')
    if not text("name"):
        add("About", "name is empty")
    summary = l.get("summary") if isinstance(l.get("summary"), str) else ""
    if not summary.strip() or len(summary) > SUMMARY_MAX or "\n" in summary:
        add("About", f"summary: one line, {SUMMARY_MAX} characters at most (it is {len(summary)})")
    authors = l.get("authors")
    if not isinstance(authors, list) or not authors or not all(isinstance(a, dict) and isinstance(a.get("name"), str) and a["name"].strip() for a in authors):
        add("About", "authors: at least one, each with a name")
    if not text("license"):
        add("About", "license is empty")
    if not REPO_RE.match(l.get("repository") or ""):
        add("About", "repository: https://github.com/<owner>/<repo>")
    for key, allowed in (("platforms", PLATFORMS), ("uses", USES)):
        value = l.get(key)
        if not isinstance(value, list) or not value or not set(value) <= allowed:
            add("About", f"{key}: one or more of {', '.join(sorted(allowed))}")
    if l.get("status") not in STATUS:
        add("About", "status: preview or available")
    host = l.get("host")
    if host is not None and not (isinstance(host, dict) and isinstance(host.get("name"), str) and host["name"].strip()):
        add("About", "host: { \"name\": …, \"url\": … }, or leave it out")

    tags = l.get("tags")
    if not isinstance(tags, list) or not 1 <= len(tags) <= TAGS_MAX:
        add("Tags", f"1 to {TAGS_MAX} tags")
    elif not all(isinstance(t, str) and TAG_RE.match(t) for t in tags) or len(set(tags)) != len(tags):
        add("Tags", "lowercase a–z, 0–9 and dashes, no repeats")

    versions = l.get("versions")
    if not isinstance(versions, list) or not versions:
        add("Versions", "at least one version")
        return errors
    draft = bool(l.get("draft"))
    for i, v in enumerate(versions):
        where = f"versions[{i}]"
        if not isinstance(v, dict):
            add("Versions", f"{where} must be an object")
            continue
        for key in sorted(v.keys() - VERSION_FIELDS):
            add("Versions", f"{where}: unknown field {key}")
        if not SEMVER_RE.match(str(v.get("version", ""))):
            add("Versions", f"{where}: version must be x.y.z")
        if not (draft and v.get("commit") is None) and not SHA_RE.match(str(v.get("commit", ""))):
            add("Versions", f"{where}: commit must be the full 40-character plugin commit SHA")
        if not DATE_RE.match(str(v.get("date", ""))):
            add("Versions", f"{where}: date must be YYYY-MM-DD")
        lfs = v.get("layerfs") if isinstance(v.get("layerfs"), dict) else {}
        if lfs.get("version") not in LAYERFS_VERSIONS:
            add("Versions", f"{where}: layerfs.version must be one of {', '.join(sorted(LAYERFS_VERSIONS))}")
        if not SHA_RE.match(str(lfs.get("commit", ""))):
            add("Versions", f"{where}: layerfs.commit must be the full 40-character LayerFS commit SHA")
    numbers = [v.get("version") for v in versions if isinstance(v, dict) and SEMVER_RE.match(str(v.get("version", "")))]
    if len(numbers) == len(versions) and not all(newer(a, b) for a, b in zip(numbers, numbers[1:])):
        add("Versions", "newest first, no repeats")
    return errors


def github(path):
    """GET a GitHub API path: (status, JSON body or None)."""
    request = urllib.request.Request(f"https://api.github.com/{path}", headers={"Accept": "application/vnd.github+json", "User-Agent": "layerfs-plugin-check"})
    if os.environ.get("GITHUB_TOKEN"):
        request.add_header("Authorization", f"Bearer {os.environ['GITHUB_TOKEN']}")
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, None


def check_online(folder):
    """The facts only GitHub knows: public repository, existing commits, LayerFS commits on main."""
    try:
        l = json.loads((Path(folder) / "plugin.json").read_text(encoding="utf-8"))
        match = REPO_RE.match(l["repository"])
        versions = [v for v in l["versions"] if isinstance(v, dict)]
    except (OSError, ValueError, KeyError, TypeError):
        return []  # the offline check already reports it
    if not match:
        return []
    errors = []
    repo = f"{match.group(1)}/{match.group(2)}"
    status, body = github(f"repos/{repo}")
    if status != 200 or (body or {}).get("private"):
        errors.append(f"Online: {repo} is not a public GitHub repository (HTTP {status})")
    for i, v in enumerate(versions):
        if status == 200 and SHA_RE.match(str(v.get("commit", ""))):
            code, _ = github(f"repos/{repo}/commits/{v['commit']}")
            if code != 200:
                errors.append(f"Online: versions[{i}]: commit {v['commit'][:10]} is not in {repo} (HTTP {code}); push it first")
        sha = str((v.get("layerfs") or {}).get("commit", ""))
        if SHA_RE.match(sha):
            code, body = github(f"repos/{LAYERFS_REPO}/compare/main...{sha}")
            if code != 200 or (body or {}).get("status") not in {"behind", "identical"}:
                errors.append(f"Online: versions[{i}]: layerfs.commit {sha[:10]} is not on LayerFS main")
    return errors


def listings(paths):
    if paths:
        return [Path(p) for p in paths]
    root = ROOT / "plugins"
    return sorted(p for p in root.iterdir() if p.is_dir()) if root.is_dir() else []


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("paths", nargs="*", help="listing folders (default: every plugins/<id>/)")
    parser.add_argument("--online", action="store_true", help="also check the repository and commits on GitHub")
    args = parser.parse_args(argv)
    failed = 0
    folders = listings(args.paths)
    if not folders:
        print("plugins/: no listings yet")
    for folder in folders:
        if not folder.is_dir():
            print(f"{folder}: not a folder")
            failed += 1
            continue
        errors = check_listing(folder)
        if args.online:
            errors += check_online(folder)
        print(f"{folder}: {'ok' if not errors else f'{len(errors)} problem' + ('s' if len(errors) > 1 else '')}")
        for error in errors:
            print(f"  ✗ {error}")
        failed += bool(errors)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
