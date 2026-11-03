#!/bin/bash
# firecracker processes and huge pages on the nested host
limactl shell lfs-66cf6bc0-lfs535l3h -- sh -c 'echo "firecracker processes: $(pgrep -c firecracker)"; grep -E "HugePages_(Total|Free)" /proc/meminfo'
