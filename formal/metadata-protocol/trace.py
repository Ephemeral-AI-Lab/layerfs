#!/usr/bin/env python3
"""Prints a TLC log's verdict and counterexample compactly: one line per
state with its action and the variables that changed (zero lock levels and
empty pending requests dropped). Usage: trace.py logs/<config>.log"""
import re, sys

text = open(sys.argv[1]).read()
for line in text.splitlines():
    if re.match(r'(Error:|Model checking completed|\d+ states generated|Finished in|The depth)', line):
        print(line)
states = re.split(r'\n(?=State \d+: |\d+: Back to state)', text)
for block in states[1:]:
    head, _, body = block.partition('\n')
    head = re.sub(r' line \d+, col \d+ to line \d+, col \d+ of module \w+', '', head)
    body = body.split('\n\n')[0]
    body = re.sub(r'\n\s+', ' ', body)
    out = []
    for var in re.findall(r'/\\ (.*)', body):
        var = re.sub(r'<<\d+, "\w", "\w+">> :> 0 (@@ )?', '', var)
        var = re.sub(r'<<"-", "-">>,? ?', '', var)
        out.append(var.strip())
    print(head)
    for v in out:
        print('    ' + v)
