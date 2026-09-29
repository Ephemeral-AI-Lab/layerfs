#!/usr/bin/env python3
"""External test-only Docker CLI port adapter; all other Docker commands exec real CLI.

The SDK owner still launches an unchanged immutable daemon and calls its public
WorkspaceApi. Only this test process's PATH resolves this wrapper. Intercept
`docker port` to give the owner a transparent loopback relay for that owned
container; do not intercept another owner's CLI or modify Docker resources.
"""
import os
import socket
import subprocess
import sys


def main():
    args = sys.argv[1:]
    docker = os.environ['LAYERFS_REAL_DOCKER']
    if len(args) == 3 and args[0] == 'port' and args[2] == '23456/tcp':
        label = subprocess.run([docker, 'inspect', '--format',
                                '{{ index .Config.Labels "io.layerfs.sandbox-name" }}', args[1]],
                               capture_output=True, text=True, timeout=12,
                               check=True).stdout.strip()
        assert label.startswith('view-release-loss-'), 'not this test\'s owned sandbox'
        actual = subprocess.run([docker, *args], capture_output=True, text=True,
                                timeout=12, check=True).stdout.strip()
        assert actual.startswith('127.0.0.1:'), actual
        with socket.create_connection(('127.0.0.1', int(os.environ['LAYERFS_RELAY_CONTROL_PORT'])),
                                      timeout=5) as channel:
            channel.sendall(f'MAP {args[1]} {actual}\n'.encode())
            channel.shutdown(socket.SHUT_WR)
            answer = channel.makefile('rb').read(128).decode()
        assert answer == 'MAPPED\n', answer
        print(os.environ['LAYERFS_RELAY_ENDPOINT'])
    else:
        os.execv(docker, [docker, *args])


if __name__ == '__main__':
    main()
