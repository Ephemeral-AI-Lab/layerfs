#!/usr/bin/env python3
"""Test-only launch profile: ignore SIGXFSZ in the owned daemon, never change limits.

All Docker operations except the uniquely labeled daemon launch run the real CLI.
"""
import os
import sys


def main():
    args = sys.argv[1:]
    docker = os.environ['LAYERFS_REAL_DOCKER']
    if len(args) > 10 and args[:2] == ['run', '-d'] and 'io.layerfs.owner=agent-sdk' in args:
        labels = [arg for i, arg in enumerate(args) if i > 0 and args[i - 1] == '--label']
        assert any(label.startswith('io.layerfs.sandbox-name=view-c5-') for label in labels)
        index = args.index('--entrypoint')
        assert args[index + 1] == '/layerfs-daemon'
        args[index + 1] = '/bin/sh'
        # The image remains unchanged. The shell establishes a real Linux
        # signal disposition inherited by exec, not a product-side fault hook.
        image = next(i for i, arg in enumerate(args) if arg.startswith('sha256:'))
        assert args[image + 1] == '--idle-sandbox'
        args[image + 1:image + 1] = ['-c', 'trap "" XFSZ; exec /layerfs-daemon "$@"', 'sh']
    os.execv(docker, [docker, *args])


if __name__ == '__main__':
    main()
