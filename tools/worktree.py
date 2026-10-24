#!/usr/bin/env python3
"""Private worktree Cargo resources and pinned, independently writable Docker E2E runs."""

import argparse
from pathlib import Path
import subprocess
import sys

from cleanup_state import Refused
import worktree_build
import worktree_resources
import worktree_run


def parser():
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest="action", required=True)
    register = commands.add_parser("register", help="Register a checkout and resolve installed image identities")
    register.add_argument("--worktree", default=str(Path(__file__).resolve().parents[1]))
    register.add_argument("--root", default=str(Path.home() / ".cache/layerfs-worktrees"))
    register.add_argument("--owner", required=True)
    for kind in ("build", "runtime", "sandbox"):
        register.add_argument("--" + kind + "-image", required=True)
    for name in ("prepare", "seed", "cargo", "build", "run", "teardown", "cleanup-scope"):
        command = commands.add_parser(name)
        command.add_argument("--resources", required=True, help="Absolute registration resources.json")
        if name in {"prepare", "seed", "cargo", "build"}:
            command.add_argument("--platform", choices=("macos", "linux"), required=True)
        if name in {"seed", "build"}:
            command.add_argument("--profile", choices=("debug", "release"), default="debug")
        if name == "prepare":
            command.add_argument(
                "--dependency-seed", help="Closed Cargo home; copies registry/git only, never credentials"
            )
        if name == "seed":
            command.add_argument("--source-resources", required=True)
        if name == "build":
            command.add_argument("--jobs", type=int, default=2)
        if name in {"cargo", "run"}:
            command.add_argument("--timeout", type=float, default=90)
            command.add_argument("arguments", nargs=argparse.REMAINDER)
        if name == "run":
            command.add_argument("--bundle", required=True)
        if name == "teardown":
            command.add_argument("--run-receipt", required=True)
            command.add_argument("--release-failed", action="store_true")
            command.add_argument("--reason")
        if name == "cleanup-scope":
            command.add_argument("--out", required=True)
            command.add_argument("--release-caches", action="store_true")
            command.add_argument("--reason")
    return cli


def main():
    args = parser().parse_args()
    try:
        if args.action == "register":
            worktree_resources.register(args)
        elif args.action in {"prepare", "seed", "cargo", "build"}:
            getattr(worktree_build, args.action)(args)
        elif args.action == "run":
            return worktree_run.launch(args)
        elif args.action == "teardown":
            worktree_run.teardown(args)
        else:
            worktree_run.cleanup_scope(args)
        return 0
    except (Refused, OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        print(f"Worktree operation refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
